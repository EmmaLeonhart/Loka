"""Streaming preprocessor: Loka → normalized text corpus, with SQLite-backed
label cache and Wikidata API fallback for both QIDs and PIDs.

This is the memory-flat replacement for `training/preprocess.py`. Same logical
output (one triple per line, tab-separated, English labels in place of QIDs/PIDs)
but constant memory regardless of corpus size, and the label cache survives
across runs.

Two passes over Loka:
  Pass 1 ("scan"):  page through every triple; extract rdfs:label rows into the
                    SQLite label cache and collect the set of QIDs/PIDs that
                    appear *anywhere* (subject, predicate, or object position).
  Pass 2 ("emit"):  page through every triple again; resolve labels via SQLite,
                    apply normalization, write the output line. Skip rows that
                    can't be resolved.

Between the passes, an optional Wikidata-API step fetches English labels for
any QIDs/PIDs that appear in the corpus but have no `rdfs:label` row in Loka
— writing them straight to the SQLite cache.

Memory characteristics: the only structure with corpus-scale growth is the
"seen QID/PID" set built in pass 1 (entity-count, not triple-count — typically
1–10% of triple count). At 50M triples this is bounded by a few hundred MB,
not 21 GB.

Usage:
    # Build label cache + run a small smoke test
    python tools/preprocess_streaming.py \\
        --endpoint http://localhost:3030 \\
        --label-db training/data/wikidata_labels.sqlite \\
        --output training/data/triples_normalized.txt \\
        --max-pages 1

    # Full run
    python tools/preprocess_streaming.py \\
        --endpoint http://localhost:3030 \\
        --label-db training/data/wikidata_labels.sqlite \\
        --output training/data/triples_normalized.txt \\
        --fetch-missing-from-wikidata
"""
from __future__ import annotations

import argparse
import io
import json
import re
import sqlite3
import sys
import time
from pathlib import Path

import requests

# UTF-8 stdout on Windows.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

# Import shared logic from the original preprocess so we don't drift on
# normalization rules, datatype filters, and the reserved-provenance namespace.
_PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_PROJECT_ROOT))
from training.preprocess import (  # noqa: E402
    RDFS_LABEL,
    WIKIDATA_ENTITY_RE,
    WIKIDATA_PROP_RE,
    TRAINING_CORPUS_QUERY,
    is_reserved_predicate,
    load_excluded_predicate_iris,
    parse_literal,
)

WIKIDATA_SPARQL = "https://query.wikidata.org/sparql"
USER_AGENT = "Loka-Training/0.2 (https://github.com/EmmaLeonhart/Loka)"

# Path to the curated property-label cache. Used as the authoritative source
# for property labels, preloaded into SQLite at startup. See the comment in
# scan_pass for why corpus-derived property labels are not trustworthy.
CURATED_PROPERTY_LABELS_PATH = Path(__file__).resolve().parent.parent / "training" / "property_label_cache.json"

# Engine bug #2 guard. Loka's executor occasionally surfaces an RDF-star
# inner-triple component (an entity IRI) in the predicate position. The
# `t["p"]["type"] != "uri"` check catches *literals* in the predicate slot but
# not *entity URIs* in the predicate slot. A Wikidata predicate must be either
# under `/prop/direct/` (`P\d+`) or one of the handful of structural vocab
# IRIs the dump uses. Anything in the `/entity/` namespace is wrong by
# construction.
WD_ENTITY_PREDICATE_RE = re.compile(r"^http://www\.wikidata\.org/entity/")

# Property IRI shape, used to identify rdfs:label rows whose subject is a
# property — we deliberately drop those in pass 1 because the corpus's
# RDF-star annotation rows produce a different value for them than the
# property's actual label. See scan_pass.
WD_PROPERTY_SUBJECT_RE = re.compile(r"^http://www\.wikidata\.org/prop/direct/P\d+$")

# URL-shaped values that leak into the object slot when the parent predicate
# wasn't caught by the noise-datatype filter (e.g. when engine bug #2 puts an
# entity in the predicate position). These never carry world-model signal:
# strip them at the object level too.
URL_PREFIX_RE = re.compile(r"^(https?|ftp|irc|ircs|mailto)://", re.IGNORECASE)

# External-identifier shape: long digit-only strings, ISNI-style spaced groups,
# DOI-shape (10.NNNN/...), etc. Object-position fallback for the same reason
# as URL_PREFIX_RE.
DIGIT_ID_RE = re.compile(r"^\d{8,}$")  # GND / VIAF / ISNI / catalog ids
DOI_RE = re.compile(r"^10\.\d{4,9}/")


# ── SQLite label cache ──────────────────────────────────────────────────────


def open_label_db(path: Path) -> sqlite3.Connection:
    """Open (or create) the label-cache DB. Schema:

      labels(iri TEXT PRIMARY KEY, label TEXT NOT NULL, source TEXT,
             fetched_at REAL)

    `source` is 'loka' (from in-corpus rdfs:label rows) or 'wikidata-api'
    (from the public Wikidata SPARQL endpoint). `fetched_at` is a unix timestamp.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(path))
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS labels (
            iri        TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            source     TEXT NOT NULL,
            fetched_at REAL NOT NULL
        )
        """
    )
    # Index isn't strictly needed (PRIMARY KEY gives one), but helps when we
    # do existence-check scans for the "missing from cache" set.
    conn.execute("CREATE INDEX IF NOT EXISTS idx_labels_source ON labels(source)")
    conn.commit()
    return conn


def label_lookup(conn: sqlite3.Connection, iri: str) -> str | None:
    row = conn.execute("SELECT label FROM labels WHERE iri = ?", (iri,)).fetchone()
    return row[0] if row else None


def label_upsert(conn: sqlite3.Connection, iri: str, label: str, source: str) -> None:
    conn.execute(
        "INSERT OR REPLACE INTO labels (iri, label, source, fetched_at) VALUES (?, ?, ?, ?)",
        (iri, label, source, time.time()),
    )


def label_upsert_many(conn: sqlite3.Connection, rows: list[tuple[str, str, str]]) -> None:
    """rows: list of (iri, label, source). Commits at the end."""
    if not rows:
        return
    now = time.time()
    conn.executemany(
        "INSERT OR REPLACE INTO labels (iri, label, source, fetched_at) VALUES (?, ?, ?, ?)",
        [(iri, label, src, now) for iri, label, src in rows],
    )
    conn.commit()


def preload_curated_property_labels(conn: sqlite3.Connection) -> int:
    """Preload curated property labels from training/property_label_cache.json.

    Why: corpus-derived property labels are systematically wrong. When the
    Wikidata RDF-star dump records `<<S P O>> rdfs:label "..."@en` annotations
    on triples, Loka's SPARQL executor surfaces those rows in a way that maps
    the property IRI to the inner triple's *object* value, not the property's
    real label. E.g. a triple `<Q42> wdt:P20 <Q31>` (place of death = Belgium)
    with an annotation rdfs:label row ends up producing
    `wdt:P20 -> "Belgium"` in the cache instead of
    `wdt:P20 -> "place of death"`.

    We trust the curated property_label_cache.json (built by the manual /
    SPARQL-API path in training/preprocess.py) as the authoritative source.
    Pass 1 *also* drops corpus rdfs:label rows whose subject is a property.

    Curated entries are tagged source='curated' so a future
    --fetch-missing-from-wikidata pass won't try to re-resolve them.
    """
    if not CURATED_PROPERTY_LABELS_PATH.exists():
        return 0
    data = json.loads(CURATED_PROPERTY_LABELS_PATH.read_text(encoding="utf-8"))
    rows = [(iri, label, "curated") for iri, label in data.items()]
    label_upsert_many(conn, rows)
    return len(rows)


def labels_present(conn: sqlite3.Connection, iris: list[str]) -> set[str]:
    """Subset of `iris` that already have a label in the cache."""
    if not iris:
        return set()
    present: set[str] = set()
    # SQLite has a ~999 parameter limit; chunk.
    CHUNK = 500
    for i in range(0, len(iris), CHUNK):
        batch = iris[i : i + CHUNK]
        placeholders = ",".join("?" * len(batch))
        rows = conn.execute(
            f"SELECT iri FROM labels WHERE iri IN ({placeholders})", batch
        ).fetchall()
        present.update(r[0] for r in rows)
    return present


# ── Loka SPARQL paging ──────────────────────────────────────────────────────


def iter_triple_pages(endpoint: str, page_size: int, max_pages: int | None):
    """Yield SPARQL JSON bindings page-by-page from Loka.

    Uses the same TRAINING_CORPUS_QUERY as the canonical preprocess: excludes
    `<< ?s ?p ?o >> loka:propositionGenerated …` annotated rows.
    """
    offset = 0
    page_num = 0
    while True:
        page_num += 1
        if max_pages is not None and page_num > max_pages:
            return
        paged = f"{TRAINING_CORPUS_QUERY} LIMIT {page_size} OFFSET {offset}"
        resp = requests.post(
            f"{endpoint}/sparql",
            data=paged,
            headers={
                "Content-Type": "application/sparql-query",
                "Accept": "application/sparql-results+json",
            },
            timeout=1800,
        )
        resp.raise_for_status()
        bindings = resp.json()["results"]["bindings"]
        if not bindings:
            return
        yield page_num, offset, bindings
        if len(bindings) < page_size:
            return
        offset += page_size


# ── Pass 1: scan corpus → cache rdfs:label rows + collect seen QIDs/PIDs ────


def scan_pass(
    endpoint: str,
    conn: sqlite3.Connection,
    page_size: int,
    max_pages: int | None,
) -> set[str]:
    """Stream all triples once.

    Side effects:
      - Upsert every English `rdfs:label` row into the SQLite cache.
    Returns:
      - The set of unique Wikidata QID/PID IRIs that appear in s/p/o positions.
        Used by `fetch_missing_from_wikidata` to figure out what to query.

    Memory: the returned set is entity-scale, not triple-scale.
    """
    seen: set[str] = set()
    pending_labels: list[tuple[str, str, str]] = []
    LABEL_FLUSH_EVERY = 20_000
    rows_seen = 0
    pages = 0

    for page_num, offset, bindings in iter_triple_pages(endpoint, page_size, max_pages):
        pages += 1
        for t in bindings:
            rows_seen += 1
            # Track QIDs / PIDs that we'll need labels for.
            for slot in ("s", "p", "o"):
                term = t[slot]
                if term.get("type") == "uri":
                    iri = term["value"]
                    if (
                        WIKIDATA_ENTITY_RE.match(iri)
                        or WIKIDATA_PROP_RE.match(iri)
                    ):
                        seen.add(iri)
            # Capture English rdfs:label rows.
            if t["p"].get("type") != "uri":
                continue
            if t["p"]["value"] != RDFS_LABEL:
                continue
            if t["s"].get("type") != "uri":
                continue
            if t["o"].get("type") != "literal":
                continue
            # Skip rdfs:label rows whose subject is a Wikidata property.
            # See preload_curated_property_labels for why corpus property
            # labels are corrupt.
            if WD_PROPERTY_SUBJECT_RE.match(t["s"]["value"]):
                continue
            value, lang = parse_literal(t["o"])
            if lang != "en":
                continue
            pending_labels.append((t["s"]["value"], value, "loka"))
            if len(pending_labels) >= LABEL_FLUSH_EVERY:
                label_upsert_many(conn, pending_labels)
                pending_labels = []
        print(
            f"  scan page {page_num} (offset {offset:,}): {len(bindings):,} rows, "
            f"total {rows_seen:,}, seen-IRIs {len(seen):,}",
            file=sys.stderr,
            flush=True,
        )

    if pending_labels:
        label_upsert_many(conn, pending_labels)
    print(f"Scan pass done: {rows_seen:,} rows over {pages} pages, "
          f"{len(seen):,} unique QID/PID IRIs.", file=sys.stderr, flush=True)
    return seen


# ── Wikidata API: fetch labels for unresolved IRIs ──────────────────────────


def fetch_missing_from_wikidata(
    conn: sqlite3.Connection,
    iris: list[str],
    rate_limit_seconds: float = 1.0,
    batch: int = 50,
) -> int:
    """For each Wikidata IRI without a cached label, query the public SPARQL
    endpoint and upsert. Returns the count of newly-cached labels.

    Polite: sleeps `rate_limit_seconds` between batches.
    """
    if not iris:
        return 0
    new_count = 0
    for i in range(0, len(iris), batch):
        chunk = iris[i : i + batch]
        # Convert IRIs back to wd:Qnn / wd:Pnn tokens
        tokens: list[str] = []
        token_to_iri: dict[str, str] = {}
        for iri in chunk:
            m = WIKIDATA_ENTITY_RE.match(iri) or WIKIDATA_PROP_RE.match(iri)
            if not m:
                continue
            id_ = m.group(1)
            tok = f"wd:{id_}"
            tokens.append(tok)
            # Map back to the IRI we want to key the cache by.
            token_to_iri[f"http://www.wikidata.org/entity/{id_}"] = iri
            token_to_iri[f"http://www.wikidata.org/prop/direct/{id_}"] = iri
            # Properties exposed by wd:P… are at the entity namespace in SPARQL.
            token_to_iri[f"http://www.wikidata.org/entity/{id_}"] = iri
        if not tokens:
            continue
        values = " ".join(tokens)
        query = (
            f"SELECT ?x ?label WHERE {{ VALUES ?x {{ {values} }} "
            f"?x rdfs:label ?label . FILTER(LANG(?label) = \"en\") }}"
        )
        try:
            resp = requests.get(
                WIKIDATA_SPARQL,
                params={"query": query, "format": "json"},
                headers={
                    "User-Agent": USER_AGENT,
                    "Accept": "application/sparql-results+json",
                },
                timeout=120,
            )
            resp.raise_for_status()
        except requests.exceptions.RequestException as e:
            print(f"  [WARN] Wikidata fetch failed at batch {i // batch + 1} "
                  f"({e}); skipping {len(chunk)} IRIs",
                  file=sys.stderr, flush=True)
            time.sleep(rate_limit_seconds * 5)
            continue

        rows = resp.json().get("results", {}).get("bindings", [])
        to_upsert: list[tuple[str, str, str]] = []
        for row in rows:
            wd_iri = row["x"]["value"]
            label = row["label"]["value"]
            target_iri = token_to_iri.get(wd_iri)
            if not target_iri:
                # Could be a property exposed at wd:Pnn — map manually
                target_iri = wd_iri
            to_upsert.append((target_iri, label, "wikidata-api"))
        if to_upsert:
            label_upsert_many(conn, to_upsert)
            new_count += len(to_upsert)
        print(f"  api batch {i // batch + 1}: requested {len(chunk):,}, "
              f"got {len(rows):,} labels, total new {new_count:,}",
              file=sys.stderr, flush=True)
        time.sleep(rate_limit_seconds)
    return new_count


# ── Pass 2: emit the normalized corpus ──────────────────────────────────────


def emit_pass(
    endpoint: str,
    conn: sqlite3.Connection,
    out_path: Path,
    page_size: int,
    max_pages: int | None,
    keep_all_languages: bool,
) -> dict[str, int]:
    """Stream all triples a second time and write the normalized text corpus."""
    excl_iris = load_excluded_predicate_iris()
    stats = {
        "written": 0,
        "skipped_unresolved": 0,
        "skipped_excluded": 0,
        "skipped_provenance": 0,
        "skipped_nonuri_pred": 0,
        "skipped_entity_pred": 0,
        "skipped_url_object": 0,
        "skipped_identifier_object": 0,
        "skipped_property_subject": 0,
        "skipped_property_object": 0,
        "skipped_rdfs_label": 0,
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", encoding="utf-8") as f:
        for page_num, offset, bindings in iter_triple_pages(endpoint, page_size, max_pages):
            for t in bindings:
                # Engine bug #2 guard: drop rows where the predicate slot is
                # not a URI (RDF-star inner-triple components leaking into
                # the wrong slot).
                if t["p"].get("type") != "uri":
                    stats["skipped_nonuri_pred"] += 1
                    continue
                p_iri = t["p"]["value"]
                if p_iri == RDFS_LABEL:
                    stats["skipped_rdfs_label"] += 1
                    continue
                if is_reserved_predicate(p_iri):
                    stats["skipped_provenance"] += 1
                    continue
                if p_iri in excl_iris:
                    stats["skipped_excluded"] += 1
                    continue
                # Engine bug #2 surgical guard: entity IRIs leaking into the
                # predicate position. Wikidata predicates are at /prop/direct/
                # or in structural vocab; nothing under /entity/ is valid here.
                if WD_ENTITY_PREDICATE_RE.match(p_iri):
                    stats["skipped_entity_pred"] += 1
                    continue
                # Property IRIs (wdt:P\d+) appearing in the subject or object
                # slot are also engine-bug-#2 fallout — for world-model
                # training we want s/o to be entities or literals, never
                # property identifiers themselves.
                if t["s"].get("type") == "uri" and WD_PROPERTY_SUBJECT_RE.match(t["s"]["value"]):
                    stats["skipped_property_subject"] = stats.get("skipped_property_subject", 0) + 1
                    continue
                if t["o"].get("type") == "uri" and WD_PROPERTY_SUBJECT_RE.match(t["o"]["value"]):
                    stats["skipped_property_object"] = stats.get("skipped_property_object", 0) + 1
                    continue
                s = resolve_term(t["s"], conn, keep_all_languages)
                p = resolve_term(t["p"], conn, keep_all_languages=True)
                o = resolve_term(t["o"], conn, keep_all_languages)
                if not (s and p and o):
                    stats["skipped_unresolved"] += 1
                    continue
                # Object-level URL / external-identifier guard. Catches values
                # that came through because the parent predicate wasn't caught
                # by the noise-datatype filter (mis-typed predicates, engine
                # bug #2 fallout, etc.).
                if URL_PREFIX_RE.match(o):
                    stats["skipped_url_object"] += 1
                    continue
                if DIGIT_ID_RE.match(o) or DOI_RE.match(o):
                    stats["skipped_identifier_object"] += 1
                    continue
                s = s.replace("\t", " ").replace("\n", " ")
                p = p.replace("\t", " ").replace("\n", " ")
                o = o.replace("\t", " ").replace("\n", " ")
                f.write(f"{s}\t{p}\t{o}\n")
                stats["written"] += 1
            print(f"  emit page {page_num} (offset {offset:,}): "
                  f"{stats['written']:,} written, "
                  f"{stats['skipped_unresolved']:,} unresolved, "
                  f"{stats['skipped_excluded']:,} excluded",
                  file=sys.stderr, flush=True)
    return stats


def resolve_term(term: dict, conn: sqlite3.Connection, keep_all_languages: bool) -> str | None:
    if term["type"] == "literal":
        value, lang = parse_literal(term)
        if not keep_all_languages and lang and lang != "en":
            return None
        return value
    if term["type"] == "uri":
        return label_lookup(conn, term["value"])
    return None


# ── Entrypoint ──────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", default="http://localhost:3030",
                        help="Loka SPARQL endpoint")
    parser.add_argument("--label-db", default="training/data/wikidata_labels.sqlite",
                        help="SQLite file for the label cache (persisted across runs)")
    parser.add_argument("--output", default="training/data/triples_normalized.txt",
                        help="Where to write the normalized text corpus")
    parser.add_argument("--page-size", type=int, default=100_000,
                        help="SPARQL page size for each Loka query")
    parser.add_argument("--max-pages", type=int, default=None,
                        help="Cap the number of pages per pass (smoke testing)")
    parser.add_argument("--skip-scan", action="store_true",
                        help="Skip pass 1; use whatever is already in the label cache")
    parser.add_argument("--skip-emit", action="store_true",
                        help="Skip pass 2; just refresh the label cache")
    parser.add_argument("--fetch-missing-from-wikidata", action="store_true",
                        help="Between passes, query the public Wikidata SPARQL endpoint "
                             "for QIDs/PIDs seen in the corpus but missing from the cache")
    parser.add_argument("--api-rate-seconds", type=float, default=1.0,
                        help="Sleep between Wikidata API batches (politeness)")
    parser.add_argument("--english-only", action="store_true",
                        help="Drop non-English monolingualtext values (v6 behaviour)")
    args = parser.parse_args()

    label_db = Path(args.label_db)
    out_path = Path(args.output)
    conn = open_label_db(label_db)

    print(f"Label cache: {label_db} "
          f"({conn.execute('SELECT COUNT(*) FROM labels').fetchone()[0]:,} entries on open)",
          file=sys.stderr)

    preloaded = preload_curated_property_labels(conn)
    if preloaded:
        print(f"Preloaded {preloaded:,} curated property labels "
              f"from {CURATED_PROPERTY_LABELS_PATH.name}",
              file=sys.stderr)

    seen: set[str] = set()
    if not args.skip_scan:
        print("=== Pass 1: scan corpus + cache rdfs:label rows ===", file=sys.stderr)
        seen = scan_pass(args.endpoint, conn, args.page_size, args.max_pages)

    if args.fetch_missing_from_wikidata:
        print("=== Wikidata API fallback for unresolved QIDs/PIDs ===", file=sys.stderr)
        if not seen:
            print("  (no scan output; nothing to look up — pass --skip-scan only with "
                  "a populated cache and you'll need to provide IRIs another way)",
                  file=sys.stderr)
        else:
            seen_sorted = sorted(seen)
            present = labels_present(conn, seen_sorted)
            missing = [iri for iri in seen_sorted if iri not in present]
            print(f"  {len(seen):,} seen, {len(present):,} cached, "
                  f"{len(missing):,} to fetch from Wikidata",
                  file=sys.stderr)
            n = fetch_missing_from_wikidata(
                conn, missing, rate_limit_seconds=args.api_rate_seconds,
            )
            print(f"  cached {n:,} new labels from Wikidata API", file=sys.stderr)

    if not args.skip_emit:
        print(f"=== Pass 2: emit normalized corpus to {out_path} ===", file=sys.stderr)
        stats = emit_pass(
            args.endpoint, conn, out_path, args.page_size, args.max_pages,
            keep_all_languages=not args.english_only,
        )
        print("Done. Stats:", file=sys.stderr)
        for k, v in stats.items():
            print(f"  {k}: {v:,}", file=sys.stderr)

    conn.close()


if __name__ == "__main__":
    main()
