"""Link-prediction evaluation (filtered MRR / Hits@k) for a masked-triple checkpoint.

Held-out set: triples in a later normalized-wikidata tier that the model never
trained on, whose subject, predicate and object labels all occur in the training
tier (the transductive setting; see tools/heldout_split_check.py and
planning/arxiv-readiness.md). For v13 that is v14-1M minus v13-500k.

Protocol, for each held-out (s, p, o) as the query (s, p, ?):

- Candidates are the objects seen with predicate p in training ("predicate-
  constrained"). Queries whose true o was never an object of p in training
  cannot be ranked under this constraint and are dropped; the count is reported.
- Filtered setting: every other known true object of (s, p), from training or
  held-out, is removed from the candidates before ranking.
- Rank uses the mean of optimistic and pessimistic ranks, so ties are neither
  rewarded nor punished.
- Model score: the input is [CLS] s [SEP_S] p [SEP_P] [MASK]*L [SEP_O], exactly
  how training masks a role of L tokens, and a candidate of L tokens scores the
  sum of its tokens' log-probabilities at those positions. One forward pass per
  (query, L) therefore scores every candidate of length L. Objects are truncated
  to tokens_per_role tokens, as in training.
- Baseline: rank candidates by how often they occur as an object of p in
  training (predicate-frequency).

Results are reported overall and split by entity-valued objects (the object
label also occurs as a subject in training) versus literal-valued ones.

Usage:
    python training/eval_linkpred.py \\
        --train   .../v13-500k/triples_normalized.txt \\
        --later   .../v14-1M/triples_normalized.txt \\
        --checkpoint .../wikidata_v13.pt \\
        --bpe-tokenizer .../tokenizer_bpe.json \\
        --output training/logs/linkpred_v13.json
"""
from __future__ import annotations

import argparse
import json
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model import TripleTransformer, ROLE_SPECIAL, ROLE_S, ROLE_P, ROLE_O  # noqa: E402
from tokenizer import PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID  # noqa: E402


def read_triples(path: str):
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) == 3:
                yield tuple(parts)


def build_split(train_path: str, later_path: str):
    train = set(read_triples(train_path))
    labels, subjects, preds = set(), set(), set()
    obj_freq: dict[str, Counter] = defaultdict(Counter)
    known: dict[tuple[str, str], set[str]] = defaultdict(set)
    for s, p, o in train:
        labels.add(s); labels.add(o); subjects.add(s); preds.add(p)
        obj_freq[p][o] += 1
        known[(s, p)].add(o)
    heldout = []
    seen = set()
    for t in read_triples(later_path):
        if t in train or t in seen:
            continue
        s, p, o = t
        if s in labels and o in labels and p in preds:
            seen.add(t)
            heldout.append(t)
    for s, p, o in heldout:
        known[(s, p)].add(o)
    return train, heldout, subjects, obj_freq, known


def realistic_rank(scores: torch.Tensor, target_idx: int, keep: torch.Tensor) -> float:
    """Mean of optimistic and pessimistic rank of scores[target_idx] among kept candidates."""
    t = scores[target_idx]
    s = scores[keep]
    higher = int((s > t).sum())
    equal = int((s == t).sum()) - 1  # exclude the target itself
    return 1 + higher + equal / 2


class Metrics:
    def __init__(self) -> None:
        self.n = 0
        self.rr = 0.0
        self.hits = {1: 0, 3: 0, 10: 0}

    def add(self, rank: float) -> None:
        self.n += 1
        self.rr += 1.0 / rank
        for k in self.hits:
            if rank <= k:
                self.hits[k] += 1

    def as_dict(self) -> dict:
        if self.n == 0:
            return {"n": 0}
        return {
            "n": self.n,
            "mrr": self.rr / self.n,
            **{f"hits@{k}": v / self.n for k, v in self.hits.items()},
        }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--train", required=True)
    ap.add_argument("--later", required=True)
    ap.add_argument("--checkpoint", required=True)
    ap.add_argument("--bpe-tokenizer", required=True)
    ap.add_argument("--output", required=True)
    ap.add_argument("--chunk", type=int, default=16, help="queries per forward batch")
    ap.add_argument("--limit", type=int, default=None, help="score only the first N rankable queries (smoke test)")
    ap.add_argument("--score-train", type=int, default=None,
                    help="sanity check: score N random TRAINING triples instead of the held-out set")
    ap.add_argument("--threads", type=int, default=4, help="CPU threads (kept low for the laptop's thermal envelope)")
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = ap.parse_args()
    torch.set_num_threads(args.threads)

    t0 = time.time()
    train, heldout, subjects, obj_freq, known = build_split(args.train, args.later)
    print(f"train unique {len(train):,}  transductive held-out {len(heldout):,}", flush=True)

    rankable = [(s, p, o) for s, p, o in heldout if o in obj_freq[p]]
    if args.score_train is not None:
        import random
        rankable = random.Random(0).sample(sorted(train), args.score_train)
    print(f"rankable under predicate constraint: {len(rankable):,} "
          f"(dropped {len(heldout) - len(rankable):,})", flush=True)
    n_rankable = len(rankable)
    if args.limit is not None:
        rankable = rankable[: args.limit]

    from tokenizers import Tokenizer  # type: ignore
    tok = Tokenizer.from_file(args.bpe_tokenizer)

    ckpt = torch.load(args.checkpoint, map_location=args.device, weights_only=False)
    cfg = ckpt["config"]
    tpr = cfg["tokens_per_role"]
    model = TripleTransformer(
        vocab_size=ckpt["vocab_size"], d_model=cfg["d_model"], nhead=cfg["nhead"],
        num_layers=cfg["num_layers"], max_len=cfg["max_len"],
    ).to(args.device)
    model.load_state_dict(ckpt["model_state"])
    model.eval()

    def enc(text: str) -> list[int]:
        return tok.encode(text, add_special_tokens=False).ids[:tpr]

    # Per predicate: candidate labels, their token ids (padded), lengths, frequency.
    by_pred: dict[str, list[tuple[str, str, str]]] = defaultdict(list)
    for t in rankable:
        by_pred[t[1]].append(t)

    model_m = {"all": Metrics(), "entity": Metrics(), "literal": Metrics()}
    freq_m = {"all": Metrics(), "entity": Metrics(), "literal": Metrics()}
    seq_len = 1 + (tpr + 1) * 3
    done = 0

    with torch.no_grad():
        for p, queries in by_pred.items():
            cands = list(obj_freq[p].keys())
            index = {c: i for i, c in enumerate(cands)}
            ids = [enc(c) for c in cands]
            lens = torch.tensor([len(x) for x in ids], device=args.device)
            pad = torch.zeros((len(cands), tpr), dtype=torch.long, device=args.device)
            for i, x in enumerate(ids):
                if x:
                    pad[i, : len(x)] = torch.tensor(x, device=args.device)
            freq = torch.tensor([obj_freq[p][c] for c in cands], dtype=torch.float, device=args.device)
            p_ids = enc(p)
            lengths = sorted(set(int(v) for v in lens.tolist() if v > 0))

            for start in range(0, len(queries), args.chunk):
                chunk = queries[start : start + args.chunk]
                # One input row per (query, L).
                rows = [(qi, L) for qi in range(len(chunk)) for L in lengths]
                tokens = torch.full((len(rows), seq_len), PAD_ID, dtype=torch.long)
                roles = torch.full((len(rows), seq_len), ROLE_SPECIAL, dtype=torch.long)
                attn = torch.zeros((len(rows), seq_len), dtype=torch.bool)
                for r, (qi, L) in enumerate(rows):
                    s_ids = enc(chunk[qi][0])
                    pos = 0
                    tokens[r, pos] = CLS_ID; attn[r, pos] = True; pos += 1
                    for slot_ids, role, sep in ((s_ids, ROLE_S, SEP_S_ID), (p_ids, ROLE_P, SEP_P_ID), (None, ROLE_O, SEP_O_ID)):
                        n = L if slot_ids is None else len(slot_ids)
                        if slot_ids is None:
                            tokens[r, pos : pos + n] = MASK_ID
                        else:
                            tokens[r, pos : pos + n] = torch.tensor(slot_ids, dtype=torch.long)
                        roles[r, pos : pos + tpr] = role
                        attn[r, pos : pos + n] = True
                        pos += tpr
                        tokens[r, pos] = sep; attn[r, pos] = True; pos += 1
                o_start = 1 + (tpr + 1) * 2
                logits = model(tokens.to(args.device), roles.to(args.device), attn.to(args.device))
                lp = torch.log_softmax(logits[:, o_start : o_start + tpr, :].float(), dim=-1)  # (R, tpr, V)

                row_of = {key: r for r, key in enumerate(rows)}
                for qi, (s, _, o) in enumerate(chunk):
                    scores = torch.full((len(cands),), float("-inf"), device=args.device)
                    for L in lengths:
                        sel = (lens == L).nonzero(as_tuple=True)[0]
                        r = row_of[(qi, L)]
                        g = lp[r, torch.arange(L, device=args.device).unsqueeze(0), pad[sel, :L]]  # (n_sel, L)
                        scores[sel] = g.sum(dim=1)
                    target = index[o]
                    keep = torch.ones(len(cands), dtype=torch.bool, device=args.device)
                    for other in known[(s, p)]:
                        j = index.get(other)
                        if j is not None and other != o:
                            keep[j] = False
                    kind = "entity" if o in subjects else "literal"
                    mr = realistic_rank(scores, target, keep)
                    fr = realistic_rank(freq, target, keep)
                    for m, rank in ((model_m, mr), (freq_m, fr)):
                        m["all"].add(rank); m[kind].add(rank)
                done += len(chunk)
            print(f"  {done:,}/{len(rankable):,} queries  {time.time() - t0:.0f}s  ({p})", flush=True)

    result = {
        "checkpoint": args.checkpoint,
        "train": args.train,
        "later": args.later,
        "train_unique_triples": len(train),
        "transductive_heldout": len(heldout),
        "rankable_queries": n_rankable,
        "scored_queries": len(rankable),
        "protocol": "filtered, predicate-constrained candidates, realistic rank",
        "model": {k: v.as_dict() for k, v in model_m.items()},
        "predicate_frequency_baseline": {k: v.as_dict() for k, v in freq_m.items()},
        "seconds": round(time.time() - t0, 1),
    }
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
