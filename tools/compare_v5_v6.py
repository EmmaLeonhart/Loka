"""Compare v5 (word-level) vs v6 (BPE) predictions on unicode-name subjects.

This is the queue.md #6 win condition for v6: did the BPE tokenizer actually
help on the cases that motivated it (Saint-Léger, Wikipédia, Curt Meyer-Clason)?
v6's per-token perplexity (194.98) is not directly comparable to v5's (84.85)
because BPE has more mass per position; the qualitative diff is the metric.

No SutraDB server needed. Reads training/data/triples.txt directly to find
subjects with non-ASCII characters in their labels, then runs predict_object
against both checkpoints with their respective tokenizers and prints a
side-by-side table.

Usage:
    python tools/compare_v5_v6.py
"""
from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path

import torch

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "training"))

from model import TripleTransformer  # noqa: E402
from infer_with_citations import predict_object  # noqa: E402

V5_CK = REPO / "training/checkpoints/wikidata_v5.pt"
V6_CK = REPO / "training/checkpoints/wikidata_v6.pt"
WORD_VOCAB = REPO / "training/data/vocab.json"
BPE_VOCAB = REPO / "training/data/vocab_bpe.json"
BPE_TOK_JSON = REPO / "training/data/tokenizer_bpe.json"
TRIPLES = REPO / "training/data/triples.txt"

CONF = 0.30  # below this = "(low)"
N_PER_SUBJECT = 3  # candidate predicates to try per subject


def has_unicode(s: str) -> bool:
    return any(ord(c) > 127 for c in s)


def load_model(ckpt_path: Path, device):
    ckpt = torch.load(str(ckpt_path), map_location=device, weights_only=False)
    cfg = ckpt["config"]
    m = TripleTransformer(
        vocab_size=ckpt["vocab_size"],
        d_model=cfg["d_model"],
        nhead=cfg["nhead"],
        num_layers=cfg["num_layers"],
        max_len=cfg["max_len"],
    ).to(device)
    m.load_state_dict(ckpt["model_state"])
    m.eval()
    return m, cfg["tokens_per_role"]


def main() -> None:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    word_vocab = json.loads(WORD_VOCAB.read_text(encoding="utf-8"))
    word_inv = [""] * len(word_vocab)
    for tok, i in word_vocab.items():
        word_inv[i] = tok

    bpe_vocab = json.loads(BPE_VOCAB.read_text(encoding="utf-8"))
    bpe_inv = [""] * len(bpe_vocab)
    for tok, i in bpe_vocab.items():
        bpe_inv[i] = tok

    from tokenizers import Tokenizer
    bpe_tok = Tokenizer.from_file(str(BPE_TOK_JSON))

    def bpe_encode(text: str) -> list[int]:
        return bpe_tok.encode(text, add_special_tokens=False).ids

    print(f"Loading v5 (word-level) ...", file=sys.stderr)
    v5_model, v5_tpr = load_model(V5_CK, device)
    print(f"Loading v6 (BPE) ...", file=sys.stderr)
    v6_model, v6_tpr = load_model(V6_CK, device)

    print(f"Scanning {TRIPLES} for unicode-name subjects ...", file=sys.stderr)
    subj_facts: dict[str, list[tuple[str, str]]] = defaultdict(list)
    pred_usage: dict[str, list[tuple[str, str]]] = defaultdict(list)
    with TRIPLES.open(encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 3:
                continue
            s, p, o = parts
            subj_facts[s].append((p, o))
            pred_usage[p].append((s, o))

    unicode_subjects = sorted(
        (s for s, facts in subj_facts.items() if has_unicode(s) and len(facts) >= 5),
        key=lambda s: (len(subj_facts[s]), s),
    )
    print(f"  {len(unicode_subjects):,} unicode subjects with >=5 facts", file=sys.stderr)

    # Hand-pick representative cases by name pattern. If a target is missing,
    # just take the next available unicode subject.
    targets = []
    for needle in ("Saint-L", "Wikip", "Meyer-Clason", "Curt", "Léger", "François",
                   "André", "Köln", "München", "São"):
        match = next((s for s in unicode_subjects if needle in s), None)
        if match and match not in targets:
            targets.append(match)
    # Pad with random unicode subjects to reach 12
    for s in unicode_subjects:
        if len(targets) >= 12:
            break
        if s not in targets:
            targets.append(s)

    print()
    print("=" * 100)
    print(f"{'subject / predicate':<48} | {'v5 (word)':<22} | {'v6 (BPE)':<22}")
    print("=" * 100)

    for s in targets:
        existing_preds = {p for p, _ in subj_facts[s]}
        # Pick a few predicates that appear on neighbours-by-shared-object but
        # not yet on this subject — same heuristic as smoke_infer.py.
        score: dict[str, int] = defaultdict(int)
        for p, o in subj_facts[s]:
            for s2, o2 in pred_usage.get(p, []):
                if s2 == s or o2 != o:
                    continue
                for p2, _ in subj_facts.get(s2, []):
                    if p2 in existing_preds:
                        continue
                    score[p2] += 1
        cand_preds = [
            p for p, _ in sorted(score.items(), key=lambda kv: -kv[1])
        ][:N_PER_SUBJECT]
        if not cand_preds:
            print(f"{s[:46]:<48} | (no candidate predicates)")
            continue

        for i, p in enumerate(cand_preds):
            v5_res = predict_object(
                v5_model, s, p, word_vocab, word_inv, v5_tpr, device,
                encode_fn=None,
            )
            v6_res = predict_object(
                v6_model, s, p, bpe_vocab, bpe_inv, v6_tpr, device,
                encode_fn=bpe_encode,
            )

            def fmt(res):
                if res is None:
                    return "(no pred)"
                lab, conf = res
                tag = "" if conf >= CONF else " (low)"
                # decode BPE pieces if present (Ġ prefix etc.)
                lab = lab.replace("Ġ", " ").strip()
                return f"{lab[:14]:<14} {conf:.3f}{tag}"

            label = f"{s[:30]} / {p[:14]}" if i == 0 else f"  / {p[:30]}"
            print(f"{label[:46]:<48} | {fmt(v5_res):<22} | {fmt(v6_res):<22}")
        print("-" * 100)


if __name__ == "__main__":
    main()
