"""Pull triples from SutraDB, substitute Wikidata QIDs/PIDs with English labels.

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
USER_AGENT = "SutraDB-Training/0.1 (https://github.com/EmmaLeonhart/SutraDB)"

WIKIDATA_ENTITY_RE = re.compile(r"^http://www\.wikidata\.org/entity/(Q\d+)$")
WIKIDATA_PROP_RE = re.compile(r"^http://www\.wikidata\.org/prop/direct/(P\d+)$")
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"


def fetch_all_triples(endpoint: str) -> list[dict]:
    """Pull every triple from SutraDB. Returns a list of dicts with s/p/o keys.

    Each value is a dict with 'type' (uri/literal/bnode) and 'value' (string).
    Literals may also have 'xml:lang' or 'datatype'.
    """
    query = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }"
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
        if t["o"].get("xml:lang") != "en":
            continue
        if t["s"]["type"] != "uri":
            continue
        labels[t["s"]["value"]] = t["o"]["value"]
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
        resp = requests.get(
            WIKIDATA_SPARQL,
            params={"query": query, "format": "json"},
            headers={"User-Agent": USER_AGENT, "Accept": "application/sparql-results+json"},
            timeout=120,
        )
        resp.raise_for_status()
        for row in resp.json()["results"]["bindings"]:
            wd_iri = row["p"]["value"]
            pid = wd_iri.rsplit("/", 1)[-1]
            full_iri = f"http://www.wikidata.org/prop/direct/{pid}"
            cache[full_iri] = row["label"]["value"]

    cache_path.parent.mkdir(parents=True, exist_ok=True)
    cache_path.write_text(json.dumps(cache, ensure_ascii=False, indent=2), encoding="utf-8")
    return cache


def resolve_term(term: dict, labels: dict[str, str]) -> str | None:
    """Resolve a SPARQL JSON term to its English label. Returns None if unresolvable."""
    if term["type"] == "literal":
        if term.get("xml:lang") and term["xml:lang"] != "en":
            return None
        return term["value"]
    if term["type"] == "uri":
        return labels.get(term["value"])
    return None


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", default="http://localhost:3030", help="SutraDB SPARQL endpoint")
    parser.add_argument("--output", required=True, help="Path to write training file")
    parser.add_argument(
        "--property-cache",
        default="training/property_label_cache.json",
        help="Path to cache resolved Wikidata property labels",
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

    written = 0
    skipped = 0
    with out_path.open("w", encoding="utf-8") as f:
        for t in triples:
            if t["p"]["type"] == "uri" and t["p"]["value"] == RDFS_LABEL:
                continue  # don't train on the label triples themselves
            s = resolve_term(t["s"], labels)
            p = resolve_term(t["p"], labels)
            o = resolve_term(t["o"], labels)
            if not (s and p and o):
                skipped += 1
                continue
            # Tabs would break the format; replace any in labels
            s, p, o = (x.replace("\t", " ").replace("\n", " ") for x in (s, p, o))
            f.write(f"{s}\t{p}\t{o}\n")
            written += 1

    print(f"Wrote {written:,} triples to {out_path}; skipped {skipped:,} (unresolved labels)", file=sys.stderr)


if __name__ == "__main__":
    main()
