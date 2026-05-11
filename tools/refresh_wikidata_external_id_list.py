"""Refresh `training/wikidata_external_id_predicates.json`.

Pulls every Wikidata property whose datatype is `external-id` (Freebase IDs,
ISNI, GND, LCCN, Dewey, the works — ~10,200 of them as of 2026-05) and
writes the PID + English label list to disk. The training corpus builder
(`training/preprocess.py`) reads this file to filter those predicates out
by default.

Re-run periodically: Wikidata adds new external-id properties continuously.

Usage:
    python tools/refresh_wikidata_external_id_list.py
"""
from __future__ import annotations

import argparse
import io
import json
import sys
import time
from datetime import date
from pathlib import Path

import requests

if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

WIKIDATA_SPARQL = "https://query.wikidata.org/sparql"
USER_AGENT = "Loka-CorpusCleanup/0.1 (https://github.com/EmmaLeonhart/Loka)"

QUERY = """
SELECT ?p ?label WHERE {
  ?p wikibase:propertyType wikibase:ExternalId .
  OPTIONAL { ?p rdfs:label ?label . FILTER(LANG(?label) = "en") }
}
"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        default="training/wikidata_external_id_predicates.json",
        help="Where to write the PID list (default: training/wikidata_external_id_predicates.json)",
    )
    args = parser.parse_args()

    print(f"Querying Wikidata: {WIKIDATA_SPARQL}", file=sys.stderr)
    t0 = time.time()
    r = requests.get(
        WIKIDATA_SPARQL,
        params={"query": QUERY, "format": "json"},
        headers={"User-Agent": USER_AGENT, "Accept": "application/sparql-results+json"},
        timeout=180,
    )
    r.raise_for_status()
    rows = r.json()["results"]["bindings"]
    print(f"  got {len(rows):,} rows in {time.time()-t0:.1f}s", file=sys.stderr)

    pids: list[str] = []
    labels: dict[str, str] = {}
    for row in rows:
        iri = row["p"]["value"]
        pid = iri.rsplit("/", 1)[-1]
        if not pid.startswith("P"):
            continue
        pids.append(pid)
        if "label" in row:
            labels[pid] = row["label"]["value"]

    pids.sort(key=lambda x: int(x[1:]))

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(
            {
                "fetched_at": date.today().isoformat(),
                "source_query": "wikibase:propertyType wikibase:ExternalId",
                "count": len(pids),
                "pids": pids,
                "labels": labels,
            },
            indent=2,
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )
    print(
        f"Wrote {len(pids):,} external-id PIDs ({len(labels):,} with labels) "
        f"-> {out_path}",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
