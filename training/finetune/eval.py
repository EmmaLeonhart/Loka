"""Evaluate a QLoRA-fine-tuned adapter on held-out triple recovery.

Status: STUB. CLI fixed. Body deferred until finetune.py produces a real
adapter.

Implementation plan:

1. Load `--held-out` JSONL (same schema as the training JSONL, separated by
   `prepare_jsonl.py --split-ratio 0.95 0.05` once that's implemented).
2. For each example: build the masked prompt, generate, compare against
   `answer`. Report:
     - top-1 exact match rate
     - top-5 match rate (sample 5 candidates with nucleus sampling)
     - per-slot breakdown (object/subject/predicate)
     - per-predicate breakdown (top 20 most-frequent predicates)
3. Optional smoke-coherence pass: hand-pick 10 entities, generate 5
   candidates each, flag obvious nonsense via simple heuristics (empty
   string, repeated tokens, etc.).
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--adapter", required=True, type=Path)
    ap.add_argument("--held-out", required=True, type=Path, help="JSONL of held-out SFT examples")
    ap.add_argument("--output", type=Path, default=Path("training/finetune/eval_report.json"))
    ap.add_argument("--n", type=int, default=5, help="Candidates for top-5 metric")
    args = ap.parse_args()

    print(
        "training/finetune/eval.py is a stub — see module docstring for the\n"
        "implementation plan. Args parsed successfully:\n"
        f"  adapter  = {args.adapter}\n"
        f"  held_out = {args.held_out}\n"
        f"  output   = {args.output}\n"
        f"  n        = {args.n}\n",
        file=sys.stderr,
    )
    sys.exit(2)


if __name__ == "__main__":
    main()
