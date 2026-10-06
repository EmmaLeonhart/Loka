"""How much would citing the neighbour side of each proposal cost?

For every (subject, proposed predicate) pair the candidate selector produces
(`candidate_predicates_with_evidence`, same arguments as inference), count:

- subject-side evidence: the subject's statements that matched a neighbour
  (what `propositionInferredFrom` cites today);
- neighbour-side evidence: for every neighbour s2 that produced a match and
  has the proposed predicate, s2's matching statement plus s2's statements
  with the proposed predicate (what retraction of a neighbour would need).

Reports the distribution of both counts and of the number of neighbours.

Usage:
    python tools/neighbour_evidence_stats.py --endpoint http://127.0.0.1:3035 --output out.json
"""
from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "training"))
import infer_with_citations as iwc  # noqa: E402


def dist(v: list[int]) -> dict:
    v = sorted(v)
    return {"median": statistics.median(v), "p90": v[int(0.9 * (len(v) - 1))],
            "p99": v[int(0.99 * (len(v) - 1))], "max": v[-1], "mean": round(sum(v) / len(v), 2)}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--endpoint", required=True)
    ap.add_argument("--property-cache", default="training/property_label_cache.json")
    ap.add_argument("--max-candidates-per-subject", type=int, default=5)
    ap.add_argument("--output", required=True)
    args = ap.parse_args()

    triples = iwc.fetch_all_triples(args.endpoint)
    labels, subj_facts, pred_usage, _ = iwc.build_inference_state(triples, args.property_cache)

    subj_counts, nb_counts, nb_entities = [], [], []
    for s in subj_facts:
        if s not in labels:
            continue
        cands, evidence, _ = iwc.candidate_predicates_with_evidence(
            s, labels=labels, subj_facts=subj_facts, pred_usage=pred_usage,
            max_candidates_per_subject=args.max_candidates_per_subject)
        for p2 in cands:
            nb_statements, neighbours = set(), set()
            for p, o_term in evidence[p2]:
                ok = iwc.o_key(o_term)
                for s2, o2_term in pred_usage.get(p, []):
                    if s2 == s or iwc.o_key(o2_term) != ok:
                        continue
                    p2_rows = [(s2, q, iwc.o_key(x)) for q, x in subj_facts.get(s2, []) if q == p2]
                    if not p2_rows:
                        continue
                    neighbours.add(s2)
                    nb_statements.add((s2, p, ok))
                    nb_statements.update(p2_rows)
            subj_counts.append(len(evidence[p2]))
            nb_counts.append(len(nb_statements))
            nb_entities.append(len(neighbours))

    result = {
        "endpoint": args.endpoint,
        "proposals": len(subj_counts),
        "subject_side_citations": dist(subj_counts),
        "neighbour_side_citations": dist(nb_counts),
        "neighbours_per_proposal": dist(nb_entities),
        "total_subject_side": sum(subj_counts),
        "total_neighbour_side": sum(nb_counts),
    }
    Path(args.output).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
