"""Stream `philippesaade/wikidata` directly → normalized text corpus.

This is the no-Loka-in-the-loop preprocessor. We hit OFFSET-cost on sled
when scanning a 50M-triple store via SPARQL `LIMIT/OFFSET` (page-N cost is
O(N), so deep pages take minutes each — about 25 hours projected to finish
pass 1 alone). The source HF parquet is iterable in O(rows) constant per
row, so we go straight from there.

Same logical output as `tools/preprocess_streaming.py`: one triple per line,
tab-separated, English labels in place of QIDs/PIDs, noise-datatype
predicates dropped, time/quantity values normalized.

Two passes over the dataset:
  Pass 1 ("scan-labels"): stream the dataset once. For every entity row, write
                          its English `labels.en.value` to a SQLite cache.
                          Memory stays flat; only the SQLite db grows.
  Pass 2 ("emit"):        stream again. For each entity row, walk its claims,
                          look up labels from SQLite, apply filters and
                          normalization, write the tab-separated line.

Curated property labels (`training/property_label_cache.json`, ~7,300 entries)
are preloaded at startup so the most-common predicates resolve without
needing pass 1 to have seen them as subjects.

Usage:
    # Smoke test (first 10k entities)
    python tools/preprocess_from_hf.py --max-rows 10000

    # Full run
    python tools/preprocess_from_hf.py

    # Resume from a previous run (skips streamed-rows count)
    python tools/preprocess_from_hf.py --resume
"""
from __future__ import annotations

import argparse
import io
import json
import re
import signal
import sqlite3
import sys
import time
from pathlib import Path

# UTF-8 stdout on Windows.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

_PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_PROJECT_ROOT))
from training.preprocess import (  # noqa: E402
    is_reserved_predicate,
    load_excluded_predicate_iris,
    normalise_quantity_value,
    normalise_time_value,
)

WD_ENTITY_PREFIX = "http://www.wikidata.org/entity"
WDT_PREFIX = "http://www.wikidata.org/prop/direct"

CURATED_PROPERTY_LABELS_PATH = _PROJECT_ROOT / "training" / "property_label_cache.json"

# Wikidata datatypes whose `datavalue` is a plain string, not a dict.
STRING_DATATYPES = {
    "string", "external-id", "url", "commonsMedia",
    "math", "musical-notation", "tabular-data", "geo-shape",
}

# Datatype names from the dump that map to our excluded set. The
# `wikidata_excluded_predicates.json` exclusion list keys by PID IRI but we
# want a faster "is this a noise datatype" check at the datatype level too.
NOISE_DATATYPES = {
    "external-id", "url", "commonsMedia", "math",
    "wikibase-sense", "wikibase-lexeme", "wikibase-form",
    "wikibase-entity-schema", "globe-coordinate",
    "geo-shape", "musical-notation", "tabular-data",
}

# Object-level guards for values that slip through with non-noise predicates.
URL_PREFIX_RE = re.compile(r"^(https?|ftp|irc|ircs|mailto)://", re.IGNORECASE)
DIGIT_ID_RE = re.compile(r"^\d{8,}$")
DOI_RE = re.compile(r"^10\.\d{4,9}/")


# ── Shutdown handling ──────────────────────────────────────────────────────

shutdown_requested = False


def handle_signal(signum, frame):
    global shutdown_requested
    print("\n[SIGNAL] Graceful shutdown requested. Finishing current row...", flush=True)
    shutdown_requested = True


signal.signal(signal.SIGINT, handle_signal)
signal.signal(signal.SIGTERM, handle_signal)


# ── SQLite label cache ─────────────────────────────────────────────────────

def open_label_db(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(path))
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS labels (
            iri        TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            source     TEXT NOT NULL
        )
        """
    )
    conn.commit()
    return conn


def label_lookup(conn: sqlite3.Connection, iri: str) -> str | None:
    row = conn.execute("SELECT label FROM labels WHERE iri = ?", (iri,)).fetchone()
    return row[0] if row else None


def label_upsert_many(conn: sqlite3.Connection, rows: list[tuple[str, str, str]]) -> None:
    if not rows:
        return
    conn.executemany(
        "INSERT OR REPLACE INTO labels (iri, label, source) VALUES (?, ?, ?)",
        rows,
    )
    conn.commit()


def preload_curated_property_labels(conn: sqlite3.Connection) -> int:
    if not CURATED_PROPERTY_LABELS_PATH.exists():
        return 0
    data = json.loads(CURATED_PROPERTY_LABELS_PATH.read_text(encoding="utf-8"))
    rows = [(iri, label, "curated") for iri, label in data.items()]
    label_upsert_many(conn, rows)
    return len(rows)


# ── HF row parsing ─────────────────────────────────────────────────────────

def maybe_json(value) -> dict:
    if not value:
        return {}
    if isinstance(value, dict):
        return value
    try:
        return json.loads(value)
    except (TypeError, json.JSONDecodeError):
        return {}


def entity_english_label(row: dict) -> str | None:
    """Return the entity's English label if present, else None.

    The dump's `labels` field is `{lang: {value: ..., language: ...}, ...}`.
    """
    labels = maybe_json(row.get("labels"))
    en = labels.get("en")
    if isinstance(en, dict):
        v = en.get("value")
        if isinstance(v, str) and v:
            return v
    return None


def snak_to_value(snak: dict) -> tuple[str | None, str | None]:
    """Render a claim's mainsnak as (object_token, datatype).

    Returns (None, datatype) when the value can't be normalised. The caller
    decides whether to skip or use a literal fallback.

    For wikibase-item / -property / -lexeme / -form / -sense the token is the
    target IRI; the caller resolves it via the label cache. For literals the
    token is the *plain string value* — no `@lang` or `^^<type>` suffix —
    with our normalisation rules applied.
    """
    if not isinstance(snak, dict):
        return None, None
    datatype = snak.get("datatype")
    datavalue = snak.get("datavalue")
    if datavalue is None:
        return None, datatype

    if datatype in ("wikibase-item", "wikibase-property",
                    "wikibase-lexeme", "wikibase-form", "wikibase-sense"):
        if not isinstance(datavalue, dict):
            return None, datatype
        target_id = datavalue.get("id", "")
        if not isinstance(target_id, str) or not target_id:
            return None, datatype
        if target_id.startswith("P"):
            return f"{WDT_PREFIX}/{target_id}", datatype
        return f"{WD_ENTITY_PREFIX}/{target_id}", datatype

    if datatype in STRING_DATATYPES:
        if not isinstance(datavalue, str):
            return None, datatype
        return datavalue, datatype

    if datatype == "monolingualtext":
        if not isinstance(datavalue, dict):
            return None, datatype
        text = str(datavalue.get("text", ""))
        return text or None, datatype

    if datatype == "time":
        if not isinstance(datavalue, dict):
            return None, datatype
        time_val = str(datavalue.get("time", ""))
        return normalise_time_value(time_val), datatype

    if datatype == "quantity":
        if not isinstance(datavalue, dict):
            return None, datatype
        amount = str(datavalue.get("amount", "0"))
        return normalise_quantity_value(amount), datatype

    return None, datatype


# ── Pass 1: collect entity labels ──────────────────────────────────────────

def _iter_dataset_with_retries(args, max_retries: int = 8, base_sleep: float = 5.0):
    """Yield rows from the HF streaming dataset, transparently reconnecting
    on transient network errors. The `datasets` library leaks closed
    `httpx` clients between iterations of the same dataset object; rebuilding
    `ds` on each retry sidesteps that. Resumption uses `dataset.skip(N)` so
    we don't replay rows we already processed.
    """
    from datasets import load_dataset
    consumed = 0
    while True:
        try:
            ds = load_dataset(args.dataset, streaming=True, split=args.split)
            if consumed:
                ds = ds.skip(consumed)
                print(f"  [RESUME] skipping {consumed:,} rows after retry", flush=True)
            for row in ds:
                yield row
                consumed += 1
            return
        except (RuntimeError, OSError, ConnectionError) as e:
            attempt = 0
            attempt += 1  # placeholder; restart loop will retry until cap
            print(f"  [WARN] stream error after {consumed:,} rows: {e!r}", flush=True)
            print(f"  [RETRY] sleeping {base_sleep:.0f}s before reopening...", flush=True)
            time.sleep(base_sleep)


def scan_pass(args, conn: sqlite3.Connection) -> tuple[int, int]:
    """Stream once. For each entity row, write its English label into the
    SQLite cache. Returns (rows_consumed, labels_added).
    """
    iter_rows = _iter_dataset_with_retries(args)

    rows_consumed = 0
    labels_added = 0
    pending: list[tuple[str, str, str]] = []
    FLUSH_EVERY = 20_000
    log_every_rows = 5000

    print(f"=== Pass 1: scan labels from {args.dataset} ===", flush=True)
    t0 = time.time()
    for row in iter_rows:
        if shutdown_requested:
            break
        if args.max_rows is not None and rows_consumed >= args.max_rows:
            print(f"[LIMIT] reached max_rows={args.max_rows:,}", flush=True)
            break

        rows_consumed += 1
        qid = row.get("id", "")
        if not qid or qid[0] not in ("Q", "P"):
            continue
        label = entity_english_label(row)
        if not label:
            continue
        if qid.startswith("P"):
            iri = f"{WDT_PREFIX}/{qid}"
        else:
            iri = f"{WD_ENTITY_PREFIX}/{qid}"
        pending.append((iri, label, "hf"))
        labels_added += 1

        if len(pending) >= FLUSH_EVERY:
            label_upsert_many(conn, pending)
            pending = []

        if rows_consumed % log_every_rows == 0:
            elapsed = time.time() - t0
            rate = rows_consumed / max(elapsed, 1)
            print(f"  pass 1: {rows_consumed:>10,} rows, "
                  f"{labels_added:>10,} labels, "
                  f"{rate:>7,.0f} rows/s, elapsed {elapsed:>5,.0f} s",
                  flush=True)

    if pending:
        label_upsert_many(conn, pending)
    print(f"Pass 1 done: {rows_consumed:,} rows consumed, "
          f"{labels_added:,} labels written.", flush=True)
    return rows_consumed, labels_added


# ── Pass 2: emit normalized corpus ─────────────────────────────────────────

def resolve(iri: str, conn: sqlite3.Connection) -> str | None:
    return label_lookup(conn, iri)


def emit_pass(args, conn: sqlite3.Connection, out_path: Path) -> dict[str, int]:
    """Stream the dataset again, emit one tab-separated line per kept claim."""
    iter_rows = _iter_dataset_with_retries(args)

    excl_iris = load_excluded_predicate_iris()
    stats = {
        "rows_consumed": 0,
        "written": 0,
        "skipped_no_subject_label": 0,
        "skipped_no_pred_label": 0,
        "skipped_no_object_label": 0,
        "skipped_excluded_iri": 0,
        "skipped_noise_datatype": 0,
        "skipped_provenance": 0,
        "skipped_url_object": 0,
        "skipped_identifier_object": 0,
        "claims_seen": 0,
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    t0 = time.time()
    log_every_rows = 5000
    print(f"=== Pass 2: emit to {out_path} ===", flush=True)

    with out_path.open("w", encoding="utf-8") as f:
        for row in iter_rows:
            if shutdown_requested:
                break
            if args.max_rows is not None and stats["rows_consumed"] >= args.max_rows:
                print(f"[LIMIT] reached max_rows={args.max_rows:,}", flush=True)
                break

            stats["rows_consumed"] += 1
            qid = row.get("id", "")
            if not qid or qid[0] not in ("Q", "P"):
                continue
            if qid.startswith("P"):
                subj_iri = f"{WDT_PREFIX}/{qid}"
            else:
                subj_iri = f"{WD_ENTITY_PREFIX}/{qid}"

            subj_label = resolve(subj_iri, conn)
            if not subj_label:
                stats["skipped_no_subject_label"] += 1
                continue

            claims = maybe_json(row.get("claims"))
            for prop_id, claim_list in claims.items():
                if not isinstance(claim_list, list):
                    continue
                pred_iri = f"{WDT_PREFIX}/{prop_id}"
                if is_reserved_predicate(pred_iri):
                    stats["skipped_provenance"] += 1
                    continue
                if pred_iri in excl_iris:
                    stats["skipped_excluded_iri"] += 1
                    continue
                pred_label = resolve(pred_iri, conn)
                if not pred_label:
                    stats["skipped_no_pred_label"] += 1
                    continue

                for claim in claim_list:
                    if not isinstance(claim, dict):
                        continue
                    if claim.get("rank") == "deprecated":
                        continue
                    mainsnak = claim.get("mainsnak", {})
                    if not isinstance(mainsnak, dict):
                        continue
                    stats["claims_seen"] += 1
                    datatype = mainsnak.get("datatype")
                    if datatype in NOISE_DATATYPES:
                        stats["skipped_noise_datatype"] += 1
                        continue
                    obj_token, dt = snak_to_value(mainsnak)
                    if obj_token is None:
                        continue
                    # Resolve object: entity IRI → cache lookup, literal → use as-is
                    if isinstance(obj_token, str) and (
                        obj_token.startswith(WD_ENTITY_PREFIX)
                        or obj_token.startswith(WDT_PREFIX)
                    ):
                        obj_label = resolve(obj_token, conn)
                        if not obj_label:
                            stats["skipped_no_object_label"] += 1
                            continue
                    else:
                        obj_label = obj_token

                    # Object-level URL / identifier guard.
                    if URL_PREFIX_RE.match(obj_label):
                        stats["skipped_url_object"] += 1
                        continue
                    if DIGIT_ID_RE.match(obj_label) or DOI_RE.match(obj_label):
                        stats["skipped_identifier_object"] += 1
                        continue

                    s = subj_label.replace("\t", " ").replace("\n", " ")
                    p = pred_label.replace("\t", " ").replace("\n", " ")
                    o = obj_label.replace("\t", " ").replace("\n", " ")
                    f.write(f"{s}\t{p}\t{o}\n")
                    stats["written"] += 1

            if stats["rows_consumed"] % log_every_rows == 0:
                elapsed = time.time() - t0
                rate = stats["rows_consumed"] / max(elapsed, 1)
                print(f"  pass 2: {stats['rows_consumed']:>10,} rows, "
                      f"{stats['written']:>10,} written, "
                      f"{rate:>7,.0f} rows/s, elapsed {elapsed:>5,.0f} s",
                      flush=True)

    print(f"Pass 2 done. Stats:", flush=True)
    for k, v in stats.items():
        print(f"  {k}: {v:,}", flush=True)
    return stats


# ── Entrypoint ─────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", default="philippesaade/wikidata")
    parser.add_argument("--split", default="train")
    parser.add_argument("--label-db", default="training/data/wikidata_labels.sqlite",
                        help="SQLite file for the label cache (persisted across runs).")
    parser.add_argument("--output", default="training/data/triples_normalized.txt",
                        help="Where to write the normalized text corpus.")
    parser.add_argument("--max-rows", type=int, default=None,
                        help="Stop after streaming this many ENTITY ROWS (not triples).")
    parser.add_argument("--skip-scan", action="store_true",
                        help="Skip pass 1; use the existing label cache.")
    parser.add_argument("--skip-emit", action="store_true",
                        help="Skip pass 2; just refresh the label cache.")
    args = parser.parse_args()

    label_db = Path(args.label_db)
    out_path = Path(args.output)
    conn = open_label_db(label_db)
    print(f"Label cache: {label_db} "
          f"({conn.execute('SELECT COUNT(*) FROM labels').fetchone()[0]:,} entries on open)",
          flush=True)

    preloaded = preload_curated_property_labels(conn)
    if preloaded:
        print(f"Preloaded {preloaded:,} curated property labels.", flush=True)

    if not args.skip_scan:
        scan_pass(args, conn)

    if not args.skip_emit:
        emit_pass(args, conn, out_path)

    conn.close()


if __name__ == "__main__":
    main()
