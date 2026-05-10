"""Train the role-aware transformer on label-substituted triples.

Objective: pick one role (S, P, or O) at random for each example, replace its
tokens with [MASK], predict the original tokens. Cross-entropy loss is computed
only on the masked positions.

Usage:
    python training/train.py \\
        --data training/data/triples.txt \\
        --vocab training/data/vocab.json \\
        --checkpoint training/checkpoints/model.pt \\
        --epochs 10
"""
from __future__ import annotations

import argparse
import json
import math
import random
import sys
import time
from pathlib import Path

import torch
import torch.nn as nn
from torch.utils.data import DataLoader, Dataset

# Local imports
sys.path.insert(0, str(Path(__file__).resolve().parent))
from model import TripleTransformer, ROLE_SPECIAL, ROLE_S, ROLE_P, ROLE_O  # noqa: E402
from tokenizer import (  # noqa: E402
    PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID, UNK_ID,
    encode as encode_word,
)


class TripleDataset(Dataset):
    def __init__(
        self,
        path: Path,
        vocab: dict[str, int],
        tokens_per_role: int = 8,
        bpe_tokenizer=None,
    ) -> None:
        """Read triples; encode via word-level vocab OR a BPE tokenizer.

        If `bpe_tokenizer` is provided (a HuggingFace `tokenizers.Tokenizer`
        loaded from `tokenizer_bpe.json`), each role string is encoded with
        BPE. Otherwise the original word-level path runs unchanged. Special
        token IDs (PAD/CLS/SEP_*/MASK/UNK) are stable across both paths
        because both vocabs reserve 0..6 for them.
        """
        self.tokens_per_role = tokens_per_role
        self.examples: list[tuple[list[int], list[int], list[int]]] = []
        if bpe_tokenizer is not None:
            def _enc(text: str) -> list[int]:
                return bpe_tokenizer.encode(text, add_special_tokens=False).ids
        else:
            def _enc(text: str) -> list[int]:
                return encode_word(text, vocab)
        with path.open(encoding="utf-8") as f:
            for line in f:
                parts = line.rstrip("\n").split("\t")
                if len(parts) != 3:
                    continue
                s, p, o = (_enc(x) for x in parts)
                if not s or not p or not o:
                    continue
                self.examples.append((s, p, o))

    def __len__(self) -> int:
        return len(self.examples)

    def __getitem__(self, idx: int) -> tuple[list[int], list[int], list[int]]:
        return self.examples[idx]


def collate(
    batch: list[tuple[list[int], list[int], list[int]]],
    tokens_per_role: int = 8,
) -> tuple[torch.Tensor, ...]:
    """Pack each triple into a fixed-length sequence with role embeddings.

    Layout: [CLS] s[:tokens_per_role pad] [SEP_S] p[..] [SEP_P] o[..] [SEP_O]
    Total length = 1 + tokens_per_role + 1 + tokens_per_role + 1 + tokens_per_role + 1
    """
    seq_len = 1 + (tokens_per_role + 1) * 3
    B = len(batch)
    tokens = torch.full((B, seq_len), PAD_ID, dtype=torch.long)
    roles = torch.full((B, seq_len), ROLE_SPECIAL, dtype=torch.long)
    targets = torch.full((B, seq_len), -100, dtype=torch.long)  # -100 = ignored by CE
    attention = torch.zeros((B, seq_len), dtype=torch.bool)

    for i, (s, p, o) in enumerate(batch):
        roles_for_slot = [ROLE_S, ROLE_P, ROLE_O]
        ids_for_slot = [s, p, o]

        # Pick which slot to mask (S, P, or O)
        mask_idx = random.randrange(3)

        pos = 0
        tokens[i, pos] = CLS_ID
        attention[i, pos] = True
        pos += 1

        sep_ids = [SEP_S_ID, SEP_P_ID, SEP_O_ID]
        for slot, (ids, role, sep_id) in enumerate(zip(ids_for_slot, roles_for_slot, sep_ids)):
            ids = ids[:tokens_per_role]
            n = len(ids)
            if slot == mask_idx:
                # Replace tokens with [MASK]; record originals as targets
                tokens[i, pos : pos + n] = MASK_ID
                targets[i, pos : pos + n] = torch.tensor(ids, dtype=torch.long)
            else:
                tokens[i, pos : pos + n] = torch.tensor(ids, dtype=torch.long)
            roles[i, pos : pos + tokens_per_role] = role
            attention[i, pos : pos + n] = True
            pos += tokens_per_role
            tokens[i, pos] = sep_id
            attention[i, pos] = True
            pos += 1

    return tokens, roles, targets, attention


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", required=True)
    parser.add_argument("--vocab", required=True)
    parser.add_argument("--checkpoint", required=True)
    parser.add_argument("--epochs", type=int, default=10)
    parser.add_argument("--batch-size", type=int, default=64)
    parser.add_argument("--tokens-per-role", type=int, default=8)
    parser.add_argument("--d-model", type=int, default=256)
    parser.add_argument("--nhead", type=int, default=8)
    parser.add_argument("--layers", type=int, default=4)
    parser.add_argument("--lr", type=float, default=3e-4)
    parser.add_argument("--weight-decay", type=float, default=0.01)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--device", default=None, help="cuda / cpu / mps; auto-detect if unset")
    parser.add_argument(
        "--bpe-tokenizer",
        default=None,
        help="Optional path to a tokenizer_bpe.json (HuggingFace tokenizers format). "
             "If provided, encodes each role's text with BPE instead of the word-level "
             "vocab. The first 7 special token IDs (PAD/CLS/SEP_S/SEP_P/SEP_O/MASK/UNK) "
             "are stable across both paths so the masked-prediction loop is unchanged.",
    )
    args = parser.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)

    if args.device is None:
        if torch.cuda.is_available():
            args.device = "cuda"
        elif torch.backends.mps.is_available():
            args.device = "mps"
        else:
            args.device = "cpu"
    device = torch.device(args.device)
    print(f"Device: {device}", file=sys.stderr)

    vocab = json.loads(Path(args.vocab).read_text(encoding="utf-8"))
    print(f"Vocabulary: {len(vocab):,} tokens", file=sys.stderr)

    bpe_tokenizer = None
    if args.bpe_tokenizer:
        from tokenizers import Tokenizer  # type: ignore
        bpe_tokenizer = Tokenizer.from_file(args.bpe_tokenizer)
        print(f"BPE tokenizer loaded: {args.bpe_tokenizer}", file=sys.stderr)

    print(f"Loading dataset from {args.data}...", file=sys.stderr)
    dataset = TripleDataset(
        Path(args.data),
        vocab,
        args.tokens_per_role,
        bpe_tokenizer=bpe_tokenizer,
    )
    print(f"  examples: {len(dataset):,}", file=sys.stderr)
    if len(dataset) == 0:
        raise SystemExit("Empty dataset; preprocess.py may have produced no triples.")

    seq_len = 1 + (args.tokens_per_role + 1) * 3
    loader = DataLoader(
        dataset,
        batch_size=args.batch_size,
        shuffle=True,
        collate_fn=lambda b: collate(b, args.tokens_per_role),
        drop_last=False,
    )

    model = TripleTransformer(
        vocab_size=len(vocab),
        d_model=args.d_model,
        nhead=args.nhead,
        num_layers=args.layers,
        max_len=seq_len,
    ).to(device)
    print(f"Model parameters: {model.num_parameters():,}", file=sys.stderr)

    optim = torch.optim.AdamW(
        model.parameters(), lr=args.lr, weight_decay=args.weight_decay, betas=(0.9, 0.95)
    )
    loss_fn = nn.CrossEntropyLoss(ignore_index=-100)

    Path(args.checkpoint).parent.mkdir(parents=True, exist_ok=True)

    model.train()
    for epoch in range(1, args.epochs + 1):
        epoch_loss = 0.0
        n_batches = 0
        t0 = time.time()
        for tokens, roles, targets, attention in loader:
            tokens = tokens.to(device)
            roles = roles.to(device)
            targets = targets.to(device)
            attention = attention.to(device)

            logits = model(tokens, roles, attention)  # (B, T, V)
            loss = loss_fn(logits.reshape(-1, logits.size(-1)), targets.reshape(-1))

            optim.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optim.step()

            epoch_loss += loss.item()
            n_batches += 1

        avg = epoch_loss / max(n_batches, 1)
        ppl = math.exp(min(avg, 20))
        print(
            f"epoch {epoch:3d}/{args.epochs}  loss {avg:.4f}  ppl {ppl:.2f}  "
            f"({time.time() - t0:.1f}s, {n_batches} batches)",
            file=sys.stderr,
        )

        torch.save(
            {
                "model_state": model.state_dict(),
                "vocab_size": len(vocab),
                "config": {
                    "d_model": args.d_model,
                    "nhead": args.nhead,
                    "num_layers": args.layers,
                    "max_len": seq_len,
                    "tokens_per_role": args.tokens_per_role,
                },
                "epoch": epoch,
            },
            args.checkpoint,
        )

    print(f"Saved checkpoint to {args.checkpoint}", file=sys.stderr)


if __name__ == "__main__":
    main()
