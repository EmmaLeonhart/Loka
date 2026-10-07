"""Time committed cascade retractions on a persistent (sled-backed) server.

Picks `--n` entities from the seed file (seeded random), and for each calls
`POST /retract {"iri": ..., "commit": true}` in turn, timing the call (HTTP
included). The commit path removes each triple from the in-memory store and
the persistent store and flushes the persistent store before replying.

Run it against a COPY of a store: it deletes data. With `--count-before`, the
script records the store's row count first, so the caller can restart the
server and check that the reloaded count equals before minus removed.

Usage:
    python tools/retract_commit_eval.py --endpoint http://127.0.0.1:3041 \\
        --seed seed_Q42.nt --n 30 --output out.json
"""
from __future__ import annotations

import argparse
import json
import random
import statistics
import time
from pathlib import Path

import requests


def count_rows(endpoint: str) -> int:
    r = requests.post(f"{endpoint}/sparql",
                      data="SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }".encode("utf-8"),
                      headers={"Content-Type": "application/sparql-query",
                               "Accept": "application/sparql-results+json"}, timeout=600)
    r.raise_for_status()
    return int(r.json()["results"]["bindings"][0]["n"]["value"])


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--n", type=int, default=30)
    ap.add_argument("--random-seed", type=int, default=0)
    ap.add_argument("--output", required=True)
    args = ap.parse_args()

    roots = sorted({line.split(" ", 1)[0][1:-1] for line in open(args.seed, encoding="utf-8")
                    if line.startswith("<http")})
    picked = random.Random(args.random_seed).sample(roots, args.n)

    before = count_rows(args.endpoint)
    runs = []
    s = requests.Session()
    for iri in picked:
        t0 = time.perf_counter()
        r = s.post(f"{args.endpoint}/retract", json={"iri": iri, "commit": True}, timeout=600)
        ms = (time.perf_counter() - t0) * 1000
        r.raise_for_status()
        body = r.json()
        runs.append({"iri": iri, "ms": round(ms, 2), "removed": body.get("removed", 0),
                     "committed": body.get("committed")})
    after = count_rows(args.endpoint)

    ms = sorted(x["ms"] for x in runs)
    removed = [x["removed"] for x in runs]
    nonzero = [x for x in runs if x["removed"]]
    per_triple = [x["ms"] / x["removed"] for x in nonzero]
    result = {
        "endpoint": args.endpoint,
        "retractions": len(runs),
        "all_committed": all(x["committed"] for x in runs),
        "rows_before": before,
        "rows_after_in_memory": after,
        "total_removed": sum(removed),
        "removed": {"median": statistics.median(removed), "max": max(removed)},
        "latency_ms": {"median": statistics.median(ms), "p95": ms[int(0.95 * (len(ms) - 1))],
                       "max": ms[-1]},
        "ms_per_removed_triple_median": round(statistics.median(per_triple), 4) if per_triple else None,
        "runs": runs,
    }
    Path(args.output).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in result.items() if k != "runs"}, indent=2))


if __name__ == "__main__":
    main()
