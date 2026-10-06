"""Knowledge-graph-embedding baselines (TransE, DistMult) for the link-prediction
evaluation in eval_linkpred.py, on the identical split and protocol.

Trains a PyKEEN model on the training tier's unique triples (entities are the
English labels, as in the transformer's corpus), then ranks each held-out query's
predicate-constrained, filtered candidates with the trained model's score_hrt,
using eval_linkpred's split, filtering and realistic-rank code unchanged.

Hyperparameters are fixed, not tuned: there is no validation split, and the
same is true of the transformer being compared.

Usage:
    python training/baseline_kge.py --model TransE \\
        --train .../v13-500k/triples_normalized.txt \\
        --later .../v14-1M/triples_normalized.txt \\
        --output training/logs/linkpred_v13_transe.json
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from eval_linkpred import Metrics, build_split, realistic_rank  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", default="TransE", choices=["TransE", "DistMult"])
    ap.add_argument("--train", required=True)
    ap.add_argument("--later", required=True)
    ap.add_argument("--output", required=True)
    ap.add_argument("--dim", type=int, default=128)
    ap.add_argument("--epochs", type=int, default=30)
    ap.add_argument("--batch-size", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=0.001)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--threads", type=int, default=4, help="CPU threads (kept low for the laptop's thermal envelope)")
    ap.add_argument("--limit", type=int, default=None, help="score only the first N rankable queries (smoke test)")
    args = ap.parse_args()
    torch.set_num_threads(args.threads)
    torch.manual_seed(args.seed)

    from pykeen.models import DistMult, TransE
    from pykeen.training import SLCWATrainingLoop
    from pykeen.triples import TriplesFactory

    t0 = time.time()
    train, heldout, subjects, obj_freq, known = build_split(args.train, args.later)
    rankable = [(s, p, o) for s, p, o in heldout if o in obj_freq[p]]
    n_rankable = len(rankable)
    if args.limit is not None:
        rankable = rankable[: args.limit]
    print(f"train unique {len(train):,}  held-out {len(heldout):,}  rankable {n_rankable:,}", flush=True)

    tf = TriplesFactory.from_labeled_triples(np.array(sorted(train), dtype=str))
    cls = {"TransE": TransE, "DistMult": DistMult}[args.model]
    model = cls(triples_factory=tf, embedding_dim=args.dim, random_seed=args.seed)
    optimizer = torch.optim.Adam(model.parameters(), lr=args.lr)
    loop = SLCWATrainingLoop(model=model, triples_factory=tf, optimizer=optimizer)
    losses = loop.train(triples_factory=tf, num_epochs=args.epochs, batch_size=args.batch_size,
                        use_tqdm=False)
    print(f"trained {args.epochs} epochs in {time.time() - t0:.0f}s; final loss {losses[-1]:.4f}", flush=True)

    e2id, r2id = tf.entity_to_id, tf.relation_to_id
    metrics = {"all": Metrics(), "entity": Metrics(), "literal": Metrics()}
    model.eval()
    by_pred: dict[str, list] = {}
    for t in rankable:
        by_pred.setdefault(t[1], []).append(t)
    with torch.no_grad():
        for p, queries in by_pred.items():
            cands = list(obj_freq[p].keys())
            index = {c: i for i, c in enumerate(cands)}
            cand_ids = torch.tensor([e2id[c] for c in cands], dtype=torch.long)
            rid = r2id[p]
            for s, _, o in queries:
                hrt = torch.stack([
                    torch.full_like(cand_ids, e2id[s]),
                    torch.full_like(cand_ids, rid),
                    cand_ids,
                ], dim=1)
                scores = torch.cat([model.score_hrt(hrt[i : i + 65536]).view(-1)
                                    for i in range(0, len(hrt), 65536)])
                keep = torch.ones(len(cands), dtype=torch.bool)
                for other in known[(s, p)]:
                    j = index.get(other)
                    if j is not None and other != o:
                        keep[j] = False
                rank = realistic_rank(scores, index[o], keep)
                kind = "entity" if o in subjects else "literal"
                metrics["all"].add(rank); metrics[kind].add(rank)

    result = {
        "model": args.model,
        "train": args.train,
        "later": args.later,
        "hyperparameters": {"dim": args.dim, "epochs": args.epochs, "batch_size": args.batch_size,
                            "lr": args.lr, "optimizer": "Adam", "training": "sLCWA, PyKEEN defaults otherwise",
                            "seed": args.seed},
        "final_training_loss": losses[-1],
        "train_unique_triples": len(train),
        "transductive_heldout": len(heldout),
        "rankable_queries": n_rankable,
        "scored_queries": len(rankable),
        "protocol": "filtered, predicate-constrained candidates, realistic rank (same as eval_linkpred.py)",
        "metrics": {k: v.as_dict() for k, v in metrics.items()},
        "seconds": round(time.time() - t0, 1),
    }
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result["metrics"], indent=2))


if __name__ == "__main__":
    main()
