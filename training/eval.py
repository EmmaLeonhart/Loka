"""Sanity-check the trained model.

For a small sample of triples, mask each role (S, P, O) in turn and show the
model's top-k token predictions. This is a smoke test, not a real evaluation
harness — it answers "did the model learn anything?", not "how well does it
generalize?".

Usage:
    python training/eval.py \\
        --checkpoint training/checkpoints/model.pt \\
        --vocab training/data/vocab.json \\
        --data training/data/triples.txt \\
        --samples 10 \\
        --top-k 5
"""
from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

# Make stdout tolerant of non-cp1252 chars (Japanese, etc.) on Windows.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model import TripleTransformer, ROLE_SPECIAL, ROLE_S, ROLE_P, ROLE_O  # noqa: E402
from tokenizer import (  # noqa: E402
    PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID,
    encode,
)


def build_inputs(
    s: list[int], p: list[int], o: list[int], mask_role: str, tokens_per_role: int
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor, list[int]]:
    """Build (tokens, roles, attention) for a single example with the chosen role masked.

    Returns the masked position indices alongside the tensors.
    """
    seq_len = 1 + (tokens_per_role + 1) * 3
    tokens = torch.full((seq_len,), PAD_ID, dtype=torch.long)
    roles = torch.full((seq_len,), ROLE_SPECIAL, dtype=torch.long)
    attention = torch.zeros((seq_len,), dtype=torch.bool)

    pos = 0
    tokens[pos] = CLS_ID
    attention[pos] = True
    pos += 1

    sep_ids = [SEP_S_ID, SEP_P_ID, SEP_O_ID]
    role_ids = [ROLE_S, ROLE_P, ROLE_O]
    role_names = ["S", "P", "O"]
    slot_ids = [s, p, o]

    masked_positions: list[int] = []
    for ids, role, sep_id, name in zip(slot_ids, role_ids, sep_ids, role_names):
        ids = ids[:tokens_per_role]
        n = len(ids)
        if name == mask_role:
            tokens[pos : pos + n] = MASK_ID
            masked_positions = list(range(pos, pos + n))
        else:
            tokens[pos : pos + n] = torch.tensor(ids, dtype=torch.long)
        roles[pos : pos + tokens_per_role] = role
        attention[pos : pos + n] = True
        pos += tokens_per_role
        tokens[pos] = sep_id
        attention[pos] = True
        pos += 1

    return tokens.unsqueeze(0), roles.unsqueeze(0), attention.unsqueeze(0), masked_positions


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", required=True)
    parser.add_argument("--vocab", required=True)
    parser.add_argument("--data", required=True)
    parser.add_argument("--samples", type=int, default=10)
    parser.add_argument("--top-k", type=int, default=5)
    parser.add_argument("--device", default=None)
    parser.add_argument("--seed", type=int, default=7)
    args = parser.parse_args()

    random.seed(args.seed)

    if args.device is None:
        args.device = "cuda" if torch.cuda.is_available() else "cpu"
    device = torch.device(args.device)

    vocab: dict[str, int] = json.loads(Path(args.vocab).read_text(encoding="utf-8"))
    inv_vocab: list[str] = [""] * len(vocab)
    for tok, i in vocab.items():
        inv_vocab[i] = tok

    ckpt = torch.load(args.checkpoint, map_location=device)
    cfg = ckpt["config"]
    tokens_per_role = cfg["tokens_per_role"]

    model = TripleTransformer(
        vocab_size=ckpt["vocab_size"],
        d_model=cfg["d_model"],
        nhead=cfg["nhead"],
        num_layers=cfg["num_layers"],
        max_len=cfg["max_len"],
    ).to(device)
    model.load_state_dict(ckpt["model_state"])
    model.eval()

    # Sample some triples
    triples_path = Path(args.data)
    all_lines = triples_path.read_text(encoding="utf-8").splitlines()
    sample_lines = random.sample(all_lines, min(args.samples, len(all_lines)))

    print(f"\n=== Eval: {len(sample_lines)} samples, top-{args.top_k} predictions ===\n")
    correct_top1 = {"S": 0, "P": 0, "O": 0}
    total = {"S": 0, "P": 0, "O": 0}

    for line in sample_lines:
        parts = line.split("\t")
        if len(parts) != 3:
            continue
        s_text, p_text, o_text = parts
        s, p, o = encode(s_text, vocab), encode(p_text, vocab), encode(o_text, vocab)
        if not s or not p or not o:
            continue

        print(f"Triple: {s_text} | {p_text} | {o_text}")
        for role_name, role_text, role_ids in [("S", s_text, s), ("P", p_text, p), ("O", o_text, o)]:
            tokens, roles, attention, masked_positions = build_inputs(
                s, p, o, role_name, tokens_per_role
            )
            tokens, roles, attention = tokens.to(device), roles.to(device), attention.to(device)

            with torch.no_grad():
                logits = model(tokens, roles, attention)  # (1, T, V)

            print(f"  Mask {role_name} (was '{role_text}'):")
            first_pos_correct = False
            for j, mp in enumerate(masked_positions):
                topk = logits[0, mp].topk(args.top_k)
                preds = [inv_vocab[idx.item()] for idx in topk.indices]
                expected = inv_vocab[role_ids[j]] if j < len(role_ids) else "(beyond)"
                hit = "[HIT]" if expected == preds[0] else "     "
                print(f"    pos {j}: top-{args.top_k}={preds}  expected='{expected}' {hit}")
                if j == 0 and expected == preds[0]:
                    first_pos_correct = True
            total[role_name] += 1
            if first_pos_correct:
                correct_top1[role_name] += 1
        print()

    print("=== Top-1 accuracy on first masked token ===")
    for role in ("S", "P", "O"):
        n = total[role]
        c = correct_top1[role]
        rate = (c / n * 100) if n else 0.0
        print(f"  {role}: {c}/{n} = {rate:.1f}%")


if __name__ == "__main__":
    main()
