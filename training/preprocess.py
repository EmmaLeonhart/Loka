"""Pull triples from Loka, substitute Wikidata QIDs/PIDs with English labels.

Output: a flat training file, one triple per line, tab-separated:
    subject_label\tpredicate_label\tobject_label

Usage:
    python training/preprocess.py --endpoint http://localhost:3030 --output training/data/triples.txt
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

import requests

WIKIDATA_SPARQL = "https://query.wikidata.org/sparql"
USER_AGENT = "Loka-Training/0.1 (https://github.com/EmmaLeonhart/Loka)"

WIKIDATA_ENTITY_RE = re.compile(r"^http://www\.wikidata\.org/entity/(Q\d+)$")
WIKIDATA_PROP_RE = re.compile(r"^http://www\.wikidata\.org/prop/direct/(P\d+)$")
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"

# Annotation predicates from infer_with_citations.py. Their rows are RDF-star
# annotations on model-generated triples — loka-internal provenance, never
# semantic statements the world model should train on.
# All system-reserved provenance predicates live under this prefix. Nothing
# the world model has ever seen, anywhere, will use a predicate IRI starting
# with this string — Wikidata uses wdt:/wd:/wikibase:/, RDFS uses rdfs:/,
# OWL uses owl:/, schema.org uses schema:/, and so on. So:
#
#   * The training corpus drops every row whose predicate is in this namespace.
#   * The inference script refuses to emit a primary triple whose predicate
#     is in this namespace, and refuses to even consider one as a candidate.
#   * The names are deliberately verbose ("propositionGeneratedFrom" not
#     "generatedFrom") so that any human reading raw triples knows these are
#     system-only, and any future collision with a real-world predicate is
#     vanishingly unlikely.
RESERVED_PROVENANCE_PREFIX = "http://loka.dev/provenance/"
LOKA_GENERATED = RESERVED_PROVENANCE_PREFIX + "propositionGenerated"
LOKA_GENERATED_BY = RESERVED_PROVENANCE_PREFIX + "propositionGeneratedBy"
LOKA_CONFIDENCE = RESERVED_PROVENANCE_PREFIX + "propositionConfidence"
LOKA_INFERRED_FROM = RESERVED_PROVENANCE_PREFIX + "propositionInferredFrom"
LOKA_IMPORTED_FROM = RESERVED_PROVENANCE_PREFIX + "propositionImportedFrom"


def is_reserved_predicate(iri: str) -> bool:
    """True if the predicate IRI is in the reserved provenance namespace.

    Used to strip annotation rows from the training corpus AND to guard the
    inference path from ever producing or considering one.
    """
    return iri.startswith(RESERVED_PROVENANCE_PREFIX)


# Wikidata predicates we exclude from training. These are properties whose
# datatype on Wikidata is one of:
#
#   - `external-id`     (~10,200) — Freebase, ISNI, GND, LCCN, Dewey, etc.
#                                   The dominant noise: half the corpus by
#                                   volume teaches the model catalog formats
#                                   that then leak onto unrelated predicates.
#   - `url`             (~120)    — links to external sites (catalog refs).
#   - `commonsMedia`    (~91)     — Wikimedia Commons file names.
#   - `math`            (~36)     — LaTeX formulae.
#   - `wikibase-sense`  (~19)     — lexeme senses (specialised, rare).
#   - `wikibase-lexeme` (~16)     — lexemes (specialised, rare).
#   - `globe-coord`     (~10)     — `Point(lat lon)` strings.
#   - `wikibase-form`   (~10)     — lexeme forms.
#   - `musical-notation`(~6)      — rare.
#   - `tabular-data`    (~6)      — rare.
#   - `geo-shape`       (~3)      — rare.
#   - `wikibase-entity-schema` (~2) — rare.
#
# We KEEP: wikibase-item, wikibase-property, quantity, string, time,
# monolingualtext. (The latter three get value-normalised below.)
#
# The list is built by `tools/refresh_wikidata_external_id_list.py`
# (despite the name, it pulls all the drop-types). Loaded lazily.
EXCLUDED_PREDICATES_PATH = Path(__file__).parent / "wikidata_excluded_predicates.json"
# Legacy path; superseded by the broader file above. Kept for back-compat.
LEGACY_EXTERNAL_ID_PATH = Path(__file__).parent / "wikidata_external_id_predicates.json"

_EXCLUDED_IRIS_CACHE: set[str] | None = None


def load_excluded_predicate_iris() -> set[str]:
    """Wikidata-property IRIs whose datatype falls into the drop list.

    Cached on first call. Returns an empty set if no list file is present
    so callers in environments without it just don't filter.
    """
    global _EXCLUDED_IRIS_CACHE
    if _EXCLUDED_IRIS_CACHE is not None:
        return _EXCLUDED_IRIS_CACHE
    path = EXCLUDED_PREDICATES_PATH if EXCLUDED_PREDICATES_PATH.exists() else (
        LEGACY_EXTERNAL_ID_PATH if LEGACY_EXTERNAL_ID_PATH.exists() else None
    )
    if path is None:
        _EXCLUDED_IRIS_CACHE = set()
        return _EXCLUDED_IRIS_CACHE
    data = json.loads(path.read_text(encoding="utf-8"))
    _EXCLUDED_IRIS_CACHE = {
        f"http://www.wikidata.org/prop/direct/{pid}" for pid in data.get("pids", [])
    }
    return _EXCLUDED_IRIS_CACHE


def is_excluded_predicate(iri: str) -> bool:
    """True if `iri` is a Wikidata predicate in the drop list."""
    return iri in load_excluded_predicate_iris()


# Back-compat aliases — earlier commits referenced the external-id-only names.
load_external_id_iris = load_excluded_predicate_iris
is_external_id_predicate = is_excluded_predicate


# ── Value normalisation ─────────────────────────────────────────────────────
# Wikidata renders time and quantity values in formats with leading sign
# markers and trailing precision suffixes that produce noisy training tokens.
# Normalise them before the value reaches the corpus.
import re as _re

_TIME_RE = _re.compile(
    r"^([+-]?)(\d{1,5})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})Z?$"
)
_QUANTITY_RE = _re.compile(r"^([+-])(\d.*)$")


def normalise_time_value(value: str) -> str:
    """Convert a Wikidata time string to a clean YYYY-MM-DD or full datetime.

    Wikidata format is `+YYYY-MM-DDTHH:MM:SSZ` (with `+` or `-` for era).
    We strip the leading `+` for positive years (the `-` for BCE stays),
    drop the `Z`, and drop the time portion when it's `T00:00:00`.

        +2012-10-15T00:00:00Z   ->  2012-10-15
        +2012-10-15T14:30:00Z   ->  2012-10-15T14:30:00
        -0044-03-15T00:00:00Z   ->  -0044-03-15
    """
    m = _TIME_RE.match(value)
    if not m:
        return value
    sign, year, month, day, hour, minute, second = m.groups()
    sign = "" if sign == "+" else sign  # keep "-" for BCE
    if hour == "00" and minute == "00" and second == "00":
        return f"{sign}{year}-{month}-{day}"
    return f"{sign}{year}-{month}-{day}T{hour}:{minute}:{second}"


def normalise_quantity_value(value: str) -> str:
    """Strip the leading `+` Wikidata adds to positive numeric quantities."""
    m = _QUANTITY_RE.match(value)
    if not m:
        return value
    sign, rest = m.groups()
    return rest if sign == "+" else value

# SPARQL-star query that pulls every triple EXCEPT those flagged
# `<< ?s ?p ?o >> loka:generated <anything>`. This removes the inner generated
# triples (which would otherwise look like normal facts in a plain SPO scan).
# Annotation rows themselves still come through because their `?s` is the
# bnode-rendered quoted-triple ID, not a real interned term — those are
# stripped below via ANNOTATION_PREDICATES.
TRAINING_CORPUS_QUERY = f"""SELECT ?s ?p ?o WHERE {{
  ?s ?p ?o .
  FILTER NOT EXISTS {{
    << ?s ?p ?o >> <{LOKA_GENERATED}> ?_gen .
  }}
}}"""

# Loka currently surfaces tagged literals with the suffix embedded in the
# value string rather than as separate JSON fields. Two flavours:
#   language-tagged   value='Tokyo"@en'
#   datatyped         value='+1966-02-18T00:00:00Z"^^<http://.../dateTime>'
# Both must be stripped before the value reaches training, otherwise tokens
# like "xmlschema", "decimal", "org" leak into the corpus as if they were
# entity content.
LANG_SUFFIX_RE = re.compile(r'^(.*)"@([a-zA-Z-]+)$')
TYPED_SUFFIX_RE = re.compile(r'^(.*)"\^\^<[^>]+>$')


def parse_literal(o: dict) -> tuple[str, str | None]:
    """Return (clean_value, lang) for a literal, handling Loka's embedded suffixes
    AND normalising Wikidata-flavoured time/quantity formats.

    Returns (value-only, None) for datatyped literals (datatype is dropped after
    being inspected for normalisation). Returns (value, lang) for
    language-tagged literals; the @lang suffix never appears in `value`.
    """
    value = o["value"]
    lang = o.get("xml:lang")
    datatype = o.get("datatype")
    if not lang:
        m = LANG_SUFFIX_RE.match(value)
        if m:
            value, lang = m.group(1), m.group(2).lower()
        else:
            m = TYPED_SUFFIX_RE.match(value)
            if m:
                # Pull the datatype back out so we can branch on it.
                end = value.rfind('"^^<')
                if end > 0:
                    datatype = value[end + 4:-1]
                value = m.group(1)
    # Normalise common Wikidata datatype value shapes.
    if datatype:
        dt = datatype.split("#")[-1].split("/")[-1].lower()
        if dt in ("datetime", "date", "time"):
            value = normalise_time_value(value)
        elif dt in ("decimal", "integer", "double", "float", "quantity"):
            value = normalise_quantity_value(value)
    elif value and value[0] in "+-" and value[1:2].isdigit():
        # Plain literals that look like Wikidata-formatted numbers/dates.
        if "T" in value and value.endswith("Z"):
            value = normalise_time_value(value)
        else:
            value = normalise_quantity_value(value)
    return value, lang


def fetch_all_triples(endpoint: str, exclude_generated: bool = True) -> list[dict]:
    """Pull every triple from Loka. Returns a list of dicts with s/p/o keys.

    Each value is a dict with 'type' (uri/literal/bnode) and 'value' (string).
    Literals may also have 'xml:lang' or 'datatype'.

    By default, model-generated triples (flagged via the RDF-star annotation
    `<< s p o >> loka:generated ...`) are excluded so the training corpus
    never feeds back its own outputs.
    """
    query = TRAINING_CORPUS_QUERY if exclude_generated else "SELECT ?s ?p ?o WHERE { ?s ?p ?o }"
    resp = requests.post(
        f"{endpoint}/sparql",
        data=query,
        headers={
            "Content-Type": "application/sparql-query",
            "Accept": "application/sparql-results+json",
        },
        timeout=300,
    )
    resp.raise_for_status()
    return resp.json()["results"]["bindings"]


def build_qid_label_map(triples: list[dict]) -> dict[str, str]:
    """Walk the triples once, build {entity_iri: english_label} from rdfs:label triples."""
    labels: dict[str, str] = {}
    for t in triples:
        if t["p"]["value"] != RDFS_LABEL:
            continue
        if t["s"]["type"] != "uri":
            continue
        if t["o"]["type"] != "literal":
            continue
        value, lang = parse_literal(t["o"])
        if lang != "en":
            continue
        labels[t["s"]["value"]] = value
    return labels


def collect_unlabeled_predicates(triples: list[dict], known_labels: dict[str, str]) -> set[str]:
    """Find Wikidata P-numbers that don't yet have labels in the database."""
    missing: set[str] = set()
    for t in triples:
        if t["p"]["type"] != "uri":
            continue
        piri = t["p"]["value"]
        if piri == RDFS_LABEL:
            continue
        if piri in known_labels:
            continue
        if WIKIDATA_PROP_RE.match(piri):
            missing.add(piri)
    return missing


def fetch_wikidata_property_labels(prop_iris: set[str], cache_path: Path) -> dict[str, str]:
    """Fetch English labels for Wikidata P-numbers via the public SPARQL endpoint.

    Caches results to disk so re-runs don't repeat the query.
    """
    cache: dict[str, str] = {}
    if cache_path.exists():
        cache = json.loads(cache_path.read_text(encoding="utf-8"))

    needed = [p for p in prop_iris if p not in cache]
    if not needed:
        return cache

    pids = [WIKIDATA_PROP_RE.match(p).group(1) for p in needed]
    print(f"  fetching {len(pids)} property labels from Wikidata...", file=sys.stderr)

    BATCH = 100
    for i in range(0, len(pids), BATCH):
        batch = pids[i : i + BATCH]
        values = " ".join(f"wd:{pid}" for pid in batch)
        query = f"""
        SELECT ?p ?label WHERE {{
          VALUES ?p {{ {values} }}
          ?p rdfs:label ?label .
          FILTER(LANG(?label) = "en")
        }}
        """
        try:
            resp = requests.get(
                WIKIDATA_SPARQL,
                params={"query": query, "format": "json"},
                headers={"User-Agent": USER_AGENT, "Accept": "application/sparql-results+json"},
                timeout=120,
            )
            resp.raise_for_status()
        except requests.exceptions.HTTPError as e:
            # Wikidata throttles aggressively. Failing here would block the
            # whole pipeline over a handful of unresolved predicate labels;
            # those predicates simply won't be candidates for inference. Log
            # and continue.
            print(f"  [WARN] Wikidata fetch failed ({e}); continuing without {len(batch)} labels", file=sys.stderr)
            continue
        for row in resp.json()["results"]["bindings"]:
            wd_iri = row["p"]["value"]
            pid = wd_iri.rsplit("/", 1)[-1]
            full_iri = f"http://www.wikidata.org/prop/direct/{pid}"
            cache[full_iri] = row["label"]["value"]

    cache_path.parent.mkdir(parents=True, exist_ok=True)
    cache_path.write_text(json.dumps(cache, ensure_ascii=False, indent=2), encoding="utf-8")
    return cache


def resolve_term(term: dict, labels: dict[str, str], keep_all_languages: bool = True) -> str | None:
    """Resolve a SPARQL JSON term to a training-corpus token (or None).

    For literals: returns the value with any @lang or ^^<datatype> suffix
    stripped, plus normalisation for time / quantity values. By default we
    keep monolingualtext values in *all* languages (per `--keep-all-languages`,
    on by default in the v7 corpus build) — the @lang tag is dropped from
    the value so the model never sees `Tokyo@en` or `東京@ja`. Set
    `keep_all_languages=False` to restore the v6 behaviour of dropping
    non-English literals.
    """
    if term["type"] == "literal":
        value, lang = parse_literal(term)
        if not keep_all_languages and lang and lang != "en":
            return None
        return value
    if term["type"] == "uri":
        return labels.get(term["value"])
    return None


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", default="http://localhost:3030", help="Loka SPARQL endpoint")
    parser.add_argument("--output", required=True, help="Path to write training file")
    parser.add_argument(
        "--property-cache",
        default="training/property_label_cache.json",
        help="Path to cache resolved Wikidata property labels",
    )
    parser.add_argument(
        "--keep-noise-datatypes",
        action="store_true",
        help="Include predicates of catalog/noise datatypes (external-id, url, "
             "commonsMedia, math, globe-coord, geo-shape, musical-notation, "
             "tabular-data, lexeme/sense/form/entity-schema). Default is to "
             "exclude them — the v6 model demonstrated that catalog formats "
             "leak onto unrelated predicates.",
    )
    parser.add_argument(
        "--english-only",
        action="store_true",
        help="Drop non-English monolingualtext values (v6 behaviour). Default "
             "is to keep all languages, with the @lang tag stripped — so the "
             "model sees 'Tokyo' and '東京' as plain values.",
    )
    args = parser.parse_args()

    print(f"Fetching triples from {args.endpoint}...", file=sys.stderr)
    triples = fetch_all_triples(args.endpoint)
    print(f"  got {len(triples):,} triples", file=sys.stderr)

    print("Building QID → label map from rdfs:label triples...", file=sys.stderr)
    labels = build_qid_label_map(triples)
    print(f"  resolved {len(labels):,} entity labels", file=sys.stderr)

    print("Resolving Wikidata property labels...", file=sys.stderr)
    missing_props = collect_unlabeled_predicates(triples, labels)
    prop_labels = fetch_wikidata_property_labels(missing_props, Path(args.property_cache))
    labels.update(prop_labels)
    print(f"  resolved {len(prop_labels):,} property labels (total {len(labels):,})", file=sys.stderr)

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    # Inner generated triples have already been excluded by TRAINING_CORPUS_QUERY
    # (FILTER NOT EXISTS << ?s ?p ?o >> propositionGenerated). What still needs
    # stripping here is every row whose predicate is in the reserved provenance
    # namespace — that's both the annotation rows on generated/imported triples
    # AND any defense-in-depth guard against ever training on system metadata.
    excl_iris = set() if args.keep_noise_datatypes else load_excluded_predicate_iris()
    if excl_iris:
        print(
            f"Excluding Wikidata catalog/noise-datatype predicates "
            f"({len(excl_iris):,} property IRIs)",
            file=sys.stderr,
        )
    keep_all_languages = not args.english_only

    written = 0
    skipped = 0
    skipped_annotations = 0
    skipped_nonuri_pred = 0
    skipped_excluded = 0
    with out_path.open("w", encoding="utf-8") as f:
        for t in triples:
            # Loka's SPARQL occasionally surfaces RDF-star inner-triple
            # components in the wrong slot, returning literal values where ?p
            # should be a URI. That's invalid RDF and corrupts training, so
            # drop those rows entirely.
            if t["p"]["type"] != "uri":
                skipped_nonuri_pred += 1
                continue
            if t["p"]["value"] == RDFS_LABEL:
                continue  # don't train on the label triples themselves
            if is_reserved_predicate(t["p"]["value"]):
                skipped_annotations += 1
                continue
            if t["p"]["value"] in excl_iris:
                skipped_excluded += 1
                continue
            s = resolve_term(t["s"], labels, keep_all_languages=keep_all_languages)
            p = resolve_term(t["p"], labels, keep_all_languages=True)
            o = resolve_term(t["o"], labels, keep_all_languages=keep_all_languages)
            if not (s and p and o):
                skipped += 1
                continue
            # Tabs would break the format; replace any in labels
            s, p, o = (x.replace("\t", " ").replace("\n", " ") for x in (s, p, o))
            f.write(f"{s}\t{p}\t{o}\n")
            written += 1

    print(
        f"Wrote {written:,} triples to {out_path}; "
        f"skipped {skipped:,} (unresolved labels), "
        f"{skipped_annotations:,} (loka-internal provenance annotations), "
        f"{skipped_excluded:,} (Wikidata catalog/noise-datatype predicates), "
        f"{skipped_nonuri_pred:,} (non-URI predicate; Loka SPARQL quirk)",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
