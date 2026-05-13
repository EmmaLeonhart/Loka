"""Generate triples from a QLoRA-fine-tuned base model.

Status: STUB. CLI fixed. Body deferred until finetune.py produces a real
adapter.

Implementation plan:

1. Load base model + LoRA adapter via peft.PeftModel.from_pretrained.
2. For each `(subject, predicate)` pair in `--seed`, build the same chat
   template as in finetune.py with the masked slot, generate `--n` candidate
   answers, score by token-prob, filter by `--confidence`.
3. Emit N-Triples-star to `--output`, exactly matching
   `training/infer_with_citations.py` schema:

       <s> <p> <o> .
       << <s> <p> <o> >> loka:generated true .
       << <s> <p> <o> >> loka:confidence "0.42"^^xsd:decimal .
       << <s> <p> <o> >> loka:generatedBy "qwen2.5-1.5b-loka-v1" .

Provenance: refuses to emit primary triples whose predicate starts with
`http://loka.dev/provenance/` (same guard as from-scratch).
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--adapter", required=True, type=Path, help="Adapter directory written by finetune.py")
    ap.add_argument("--seed", required=True, type=Path, help="N-Triples file: seed subjects to generate predictions for")
    ap.add_argument("--output", required=True, type=Path, help="N-Triples-star output")
    ap.add_argument("--n", type=int, default=10, help="Candidates per (subject, predicate)")
    ap.add_argument("--confidence", type=float, default=0.25, help="Drop candidates below this token-avg prob")
    ap.add_argument("--model-version", default="qwen2.5-1.5b-loka-v1", help="Tag emitted in loka:generatedBy")
    args = ap.parse_args()

    print(
        "training/finetune/infer.py is a stub — see module docstring for the\n"
        "implementation plan. Args parsed successfully:\n"
        f"  adapter       = {args.adapter}\n"
        f"  seed          = {args.seed}\n"
        f"  output        = {args.output}\n"
        f"  n             = {args.n}\n"
        f"  confidence    = {args.confidence}\n"
        f"  model_version = {args.model_version}\n",
        file=sys.stderr,
    )
    sys.exit(2)


if __name__ == "__main__":
    main()
