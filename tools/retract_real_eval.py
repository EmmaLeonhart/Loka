"""Cascade retraction on a real graph through the server's preview endpoint.

Given a running Loka server holding curated triples plus generated triples
written by `training/infer_with_citations.py --post`, and the generated
N-Triples-star file, this:

- calls `POST /retract/preview` for every entity IRI that appears as a subject
  in the curated seed file, timing each call (HTTP included);
- checks each answer against the generated file, computing the expected
  closure independently: a generated triple is removed if it touches the root
  (subject or object), if it cites a statement touching the root, or,
  transitively, if it cites a removed generated triple. Every expected triple
  must appear in the preview exactly with all of its annotation rows (recall),
  and no other generated triple may appear (precision);
- reports the per-call latency distribution and the removed-set sizes.

Usage:
    python tools/retract_real_eval.py --endpoint http://127.0.0.1:3031 \\
        --seed seed_Q42.nt --generated generated_v13.nt --output out.json
"""
from __future__ import annotations

import argparse
import json
import re
import statistics
import time
from collections import defaultdict
from pathlib import Path

import requests

INFERRED = "<http://loka.dev/provenance/propositionInferredFrom>"
QUOTED = re.compile(r"<< (\S+) (\S+) (.+?) >>")


def max_hops(removed: set, cites: dict, root_tok: str) -> int:
    """Longest chain of generated-cites-generated links among removed triples."""
    memo: dict = {}

    def depth(g, seen=()):
        if g in memo:
            return memo[g]
        best = 1
        for src in cites.get(g, ()):
            if src in removed and src not in seen:
                best = max(best, 1 + depth(src, seen + (g,)))
        memo[g] = best
        return best

    return max((depth(g) for g in removed), default=0)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--generated", required=True)
    ap.add_argument("--output", required=True)
    args = ap.parse_args()

    roots = sorted({line.split(" ", 1)[0][1:-1] for line in open(args.seed, encoding="utf-8")
                    if line.startswith("<http")})

    # generated triple (s, p, o text) -> set of cited (s, p, o text)
    cites: dict[tuple, set] = defaultdict(set)
    n_annotations: dict[tuple, int] = defaultdict(int)
    for line in open(args.generated, encoding="utf-8"):
        if not line.startswith("<<"):
            continue
        quoted = QUOTED.findall(line)
        g = quoted[0]
        n_annotations[g] += 1
        if INFERRED in line and len(quoted) > 1:
            cites[g].add(quoted[1])

    latencies, totals, checked, mismatches, extras, deepest = [], [], 0, [], [], 0
    session = requests.Session()
    for root in roots:
        t0 = time.perf_counter()
        r = session.post(f"{args.endpoint}/retract/preview", json={"iri": root}, timeout=60)
        latencies.append((time.perf_counter() - t0) * 1000)
        r.raise_for_status()
        body = r.json()
        totals.append(body["total"])
        rows = {(r["s"], r["p"], r["o"]) for d in body["by_depth"] for r in d["triples"]}
        root_tok = f"<{root}>"
        removed = {g for g in n_annotations if g[0] == root_tok or g[2] == root_tok}
        removed |= {g for g, srcs in cites.items()
                    if any(s == root_tok or o == root_tok for s, _, o in srcs)}
        grew = True
        while grew:
            grew = False
            for g, srcs in cites.items():
                if g not in removed and any(src in removed for src in srcs):
                    removed.add(g)
                    grew = True
        depth_hops = max_hops(removed, cites, root_tok)
        deepest = max(deepest, depth_hops)
        for g in removed:
            checked += 1
            g_row = (g[0][1:-1], g[1][1:-1], g[2])
            quoted_g = f"<< {g[0]} {g[1]} {g[2]} >>"
            n_ann = sum(1 for s, _, _ in rows if s == quoted_g)
            if g_row not in rows or n_ann != n_annotations[g]:
                mismatches.append({"root": root, "generated": list(g),
                                   "triple_present": g_row in rows,
                                   "annotations": f"{n_ann}/{n_annotations[g]}"})
        for g in n_annotations:
            if g not in removed and (g[0][1:-1], g[1][1:-1], g[2]) in rows:
                extras.append({"root": root, "generated": list(g)})

    result = {
        "endpoint": args.endpoint,
        "roots": len(roots),
        "generated_triples": len(n_annotations),
        "annotation_rows": sum(n_annotations.values()),
        "cited_statements_per_prediction": round(
            sum(len(v) for v in cites.values()) / max(1, len(cites)), 2),
        "dependency_checks": checked,
        "dependency_mismatches": len(mismatches),
        "unexpected_removals": len(extras),
        "unexpected_examples": extras[:5],
        "deepest_generated_chain": deepest,
        "mismatch_examples": mismatches[:5],
        "latency_ms": {
            "median": round(statistics.median(latencies), 2),
            "p95": round(sorted(latencies)[int(0.95 * (len(latencies) - 1))], 2),
            "max": round(max(latencies), 2),
        },
        "removed_triples": {"median": statistics.median(totals), "max": max(totals)},
    }
    Path(args.output).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
