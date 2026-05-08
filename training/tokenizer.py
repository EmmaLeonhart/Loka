"""Word-level tokenizer for the training corpus.

Builds a vocabulary from a tab-separated triples file. Tokenization is lowercase
whitespace splitting with punctuation stripped from token edges. Chosen for speed
and simplicity at v0; subword/BPE is a later upgrade.

Special tokens:
    [PAD] — padding (id 0)
    [CLS] — sequence start (id 1)
    [SEP_S], [SEP_P], [SEP_O] — role boundaries
    [MASK] — masked-token marker for training
    [UNK] — out-of-vocabulary fallback

Usage:
    python training/tokenizer.py --input training/data/triples.txt --output training/data/vocab.json
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path

SPECIAL_TOKENS = ["[PAD]", "[CLS]", "[SEP_S]", "[SEP_P]", "[SEP_O]", "[MASK]", "[UNK]"]
PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID, UNK_ID = range(7)

_TOKEN_RE = re.compile(r"[A-Za-z0-9]+(?:'[A-Za-z]+)?|\d+")


def tokenize(text: str) -> list[str]:
    """Lowercase + regex split. Keeps numbers as tokens, splits on punctuation."""
    return _TOKEN_RE.findall(text.lower())


def build_vocab(triples_path: Path, max_vocab: int) -> dict[str, int]:
    counter: Counter[str] = Counter()
    with triples_path.open(encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 3:
                continue
            for role in parts:
                counter.update(tokenize(role))

    vocab: dict[str, int] = {tok: i for i, tok in enumerate(SPECIAL_TOKENS)}
    n_special = len(SPECIAL_TOKENS)
    most_common = counter.most_common(max_vocab - n_special)
    for token, _count in most_common:
        vocab[token] = len(vocab)
    return vocab


def encode(text: str, vocab: dict[str, int]) -> list[int]:
    return [vocab.get(t, UNK_ID) for t in tokenize(text)]


def decode(ids: list[int], inv_vocab: list[str]) -> str:
    return " ".join(inv_vocab[i] for i in ids if i != PAD_ID)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, help="Tab-separated triples file from preprocess.py")
    parser.add_argument("--output", required=True, help="Path to write vocab.json")
    parser.add_argument("--max-vocab", type=int, default=32_000, help="Maximum vocabulary size")
    args = parser.parse_args()

    triples_path = Path(args.input)
    if not triples_path.exists():
        raise SystemExit(f"Input file not found: {triples_path}")

    print(f"Building vocabulary from {triples_path}...", file=sys.stderr)
    vocab = build_vocab(triples_path, args.max_vocab)
    print(f"  vocab size: {len(vocab):,}", file=sys.stderr)

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(vocab, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"Wrote vocabulary to {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
