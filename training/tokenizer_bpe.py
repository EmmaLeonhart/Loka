"""BPE tokenizer for the world-model corpus — drop-in replacement for tokenizer.py.

Why this exists: the word-level regex tokenizer chops Unicode entity names into
nonsense pieces. "Saint-Léger" -> ["saint", "l", "ger"] under the regex, losing
"Léger" entirely. The Gemini 3 Flash review (paper/reviews/v1_post2378) flagged
mode collapse on common tokens, partly downstream of vocabulary loss like this.

This module trains a byte-level BPE tokenizer (HuggingFace `tokenizers`) over
the triples corpus, reserving the same first-7 special token IDs that
`model.py` and the masked-prediction training loop already depend on:

    PAD=0, CLS=1, SEP_S=2, SEP_P=3, SEP_O=4, MASK=5, UNK=6

After training, every Unicode codepoint round-trips through subword pieces;
"Saint-Léger" becomes a stable sequence of subword tokens that include the
accented characters as bytes.

Usage:
    # Train a tokenizer (writes a HF tokenizer JSON + a vocab.json compatible
    # with model.py's existing dict[str, int] expectation):
    python training/tokenizer_bpe.py train \\
        --input training/data/triples.txt \\
        --output training/data/tokenizer_bpe.json \\
        --vocab-out training/data/vocab_bpe.json \\
        --vocab-size 50000

    # Smoke-test:
    python training/tokenizer_bpe.py test --tokenizer training/data/tokenizer_bpe.json

In Python (replaces tokenizer.encode):
    from tokenizer_bpe import load_tokenizer, encode_with
    tok = load_tokenizer("training/data/tokenizer_bpe.json")
    ids = encode_with(tok, "Saint-Léger")

Special token IDs are stable; train.py / infer_with_citations.py imports of
PAD_ID / CLS_ID / etc. work unchanged. To wire model.py + train.py to BPE,
replace `from tokenizer import encode` with `from tokenizer_bpe import
load_tokenizer` and pass the loaded tokenizer through.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

SPECIAL_TOKENS = ["[PAD]", "[CLS]", "[SEP_S]", "[SEP_P]", "[SEP_O]", "[MASK]", "[UNK]"]
PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID, UNK_ID = range(7)


def train_tokenizer(input_path: Path, output_path: Path, vocab_out: Path, vocab_size: int) -> None:
    """Train a byte-level BPE tokenizer over the triples file.

    Each line is "subject\\tpredicate\\tobject"; we treat the whole line as
    training text. Tabs are not significant for the BPE — they're just byte
    boundaries, and the encoder will split on them at runtime via three
    encode() calls (one per role).
    """
    from tokenizers import Tokenizer
    from tokenizers.models import BPE
    from tokenizers.pre_tokenizers import ByteLevel
    from tokenizers.decoders import ByteLevel as ByteLevelDecoder
    from tokenizers.trainers import BpeTrainer

    tokenizer = Tokenizer(BPE(unk_token="[UNK]"))
    tokenizer.pre_tokenizer = ByteLevel(add_prefix_space=False)
    tokenizer.decoder = ByteLevelDecoder()

    trainer = BpeTrainer(
        vocab_size=vocab_size,
        special_tokens=SPECIAL_TOKENS,
        initial_alphabet=ByteLevel.alphabet(),
        show_progress=True,
    )

    print(f"Training BPE on {input_path}, vocab={vocab_size}...", file=sys.stderr)
    tokenizer.train(files=[str(input_path)], trainer=trainer)
    tokenizer.save(str(output_path))
    print(f"Saved tokenizer to {output_path}", file=sys.stderr)

    # Sanity: confirm the special-token IDs landed at 0..6.
    for expected_id, tok in enumerate(SPECIAL_TOKENS):
        actual = tokenizer.token_to_id(tok)
        if actual != expected_id:
            raise SystemExit(
                f"[FATAL] special token {tok!r} got id {actual} but {expected_id} was expected. "
                "model.py and the training loop hardcode 0..6 as the special-token range; "
                "train.py would index into the wrong rows. Refusing to save vocab_bpe.json."
            )

    # Also write a model.py-compatible vocab.json (str -> int dict).
    vocab = tokenizer.get_vocab()
    vocab_out.parent.mkdir(parents=True, exist_ok=True)
    vocab_out.write_text(json.dumps(vocab, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"Wrote vocab.json-compatible mapping ({len(vocab):,} tokens) to {vocab_out}", file=sys.stderr)


def load_tokenizer(path: str | Path):
    """Load a saved BPE tokenizer."""
    from tokenizers import Tokenizer
    return Tokenizer.from_file(str(path))


def encode_with(tokenizer, text: str) -> list[int]:
    """Encode a string into token ids using the loaded tokenizer.

    No special tokens added — the masked-prediction loop in train.py adds
    [CLS] / [SEP_*] manually at known positions. This matches the original
    tokenizer.py's encode() contract: returns the token ids for the *content*
    only, not the surrounding special tokens.
    """
    return tokenizer.encode(text, add_special_tokens=False).ids


def decode_with(tokenizer, ids: list[int]) -> str:
    """Decode token ids back to a string. Skips PAD and ignores other special
    tokens for cleaner downstream display."""
    return tokenizer.decode(ids, skip_special_tokens=True)


def smoke_test(tokenizer_path: str) -> None:
    """Confirm Unicode preservation. Runs against a hand-picked set of names
    that the word-level tokenizer would have chopped."""
    tok = load_tokenizer(tokenizer_path)
    samples = [
        "Saint-Léger",
        "Wikipédia",
        "Tokyo",
        "東京",
        "Bora Dağtekin",
        "Comtesse de Die",
        "Liriodendron tulipifera",
        "Engishiki Jinmyōchō",
    ]
    print(f"\n=== Smoke test: {tokenizer_path} ===")
    for s in samples:
        enc = tok.encode(s, add_special_tokens=False)
        decoded = tok.decode(enc.ids)
        print(f"  {s!r:35s} -> {len(enc.ids):2d} tokens  (round-trip: {decoded!r})")
        # Print the actual token strings so we can eyeball the pieces:
        pieces = [tok.id_to_token(i) for i in enc.ids]
        print(f"    pieces: {pieces}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_train = sub.add_parser("train", help="Train a new BPE tokenizer")
    p_train.add_argument("--input", type=Path, required=True, help="Tab-separated triples file")
    p_train.add_argument("--output", type=Path, required=True, help="Path to save the HF tokenizer JSON")
    p_train.add_argument("--vocab-out", type=Path, required=True, help="Path to write the model.py-compatible vocab.json")
    p_train.add_argument("--vocab-size", type=int, default=50000)

    p_test = sub.add_parser("test", help="Smoke-test a saved tokenizer's Unicode handling")
    p_test.add_argument("--tokenizer", required=True, help="Path to a saved tokenizer JSON")

    args = parser.parse_args()

    if args.cmd == "train":
        train_tokenizer(args.input, args.output, args.vocab_out, args.vocab_size)
        smoke_test(str(args.output))
    elif args.cmd == "test":
        smoke_test(args.tokenizer)


if __name__ == "__main__":
    main()
