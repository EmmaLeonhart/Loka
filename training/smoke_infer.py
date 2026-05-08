"""Offline smoke test for infer_with_citations.py.

Skips the SutraDB SPARQL pull. Reads training/data/triples.txt directly
(already English labels) to build subject_label -> [(predicate_label, object_label)]
and exercises the same prediction + emit code path as the real script. Validates:
- the checkpoint loads
- predict_object returns plausible strings
- the N-Triples-star block is well-formed
- candidate-predicate selection picks sensible suggestions

Usage:
    python training/smoke_infer.py
"""
from __future__ import annotations

import json
import random
import sys
from collections import defaultdict
from pathlib import Path

import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model import TripleTransformer  # noqa: E402
from infer_with_citations import (  # noqa: E402
    predict_object,
    quoted,
    escape_literal,
    SUTRA_GENERATED,
    SUTRA_GENERATED_BY,
    SUTRA_CONFIDENCE,
    SUTRA_INFERRED_FROM,
    XSD_BOOLEAN,
    XSD_DECIMAL,
)

CHECKPOINT = Path("training/checkpoints/wikidata_v2.pt")
VOCAB_PATH = Path("training/data/vocab.json")
DATA_PATH = Path("training/data/triples.txt")

CONF_THRESHOLD = 0.3
MAX_SUBJECTS = 8
MAX_CANDIDATES_PER_SUBJECT = 3
MAX_CITATIONS = 5
MODEL_VERSION = "wikidata_v2"
SEED = 7

# Synthetic URI prefix for the offline smoke test (no real Wikidata IRIs without
# a label->URI map; we just want to validate the emit format).
URN = "urn:smoketest:"


def label_to_urn(label: str, kind: str) -> str:
    safe = label.replace(" ", "_").replace('"', "_")
    return f"<{URN}{kind}/{safe}>"


def main() -> None:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")

    random.seed(SEED)
    torch.manual_seed(SEED)
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    vocab: dict[str, int] = json.loads(VOCAB_PATH.read_text(encoding="utf-8"))
    inv_vocab = [""] * len(vocab)
    for tok, i in vocab.items():
        inv_vocab[i] = tok

    ckpt = torch.load(CHECKPOINT, map_location=device, weights_only=False)
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
    n_params = sum(p.numel() for p in model.parameters())
    print(f"[smoke] {CHECKPOINT}: {n_params:,} params, vocab {ckpt['vocab_size']:,}")

    subj_facts: dict[str, list[tuple[str, str]]] = defaultdict(list)
    pred_usage: dict[str, list[tuple[str, str]]] = defaultdict(list)
    with DATA_PATH.open(encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 3:
                continue
            s_lab, p_lab, o_lab = parts
            subj_facts[s_lab].append((p_lab, o_lab))
            pred_usage[p_lab].append((s_lab, o_lab))

    candidates = [s for s, facts in subj_facts.items() if len(facts) >= 5]
    random.shuffle(candidates)
    candidates = candidates[:MAX_SUBJECTS]
    print(f"[smoke] inferring on {len(candidates)} subjects from {DATA_PATH}")

    out_lines: list[str] = []
    n_emitted = 0
    n_attempted = 0

    for s_lab in candidates:
        s_existing_preds = {p for p, _ in subj_facts[s_lab]}
        neighbor_pred_score: dict[str, int] = defaultdict(int)
        for p, o in subj_facts[s_lab]:
            for s2, o2 in pred_usage.get(p, []):
                if s2 == s_lab or o2 != o:
                    continue
                for p2, _ in subj_facts.get(s2, []):
                    if p2 in s_existing_preds:
                        continue
                    neighbor_pred_score[p2] += 1

        cand_preds = [p for p, _ in sorted(neighbor_pred_score.items(), key=lambda kv: -kv[1])][
            :MAX_CANDIDATES_PER_SUBJECT
        ]

        print(f"\n  subject: {s_lab!r}  (existing facts: {len(subj_facts[s_lab])})")
        if not cand_preds:
            print("    (no candidate predicates)")
            continue
        print(f"    candidate predicates: {cand_preds}")

        for p_lab in cand_preds:
            n_attempted += 1
            res = predict_object(
                model, s_lab, p_lab, vocab, inv_vocab, tokens_per_role, device
            )
            if res is None:
                print(f"    {p_lab!r:35s} -> (no prediction)")
                continue
            o_lab, conf = res
            existing_for_pair = {
                oo.lower() for op, oo in subj_facts[s_lab] if op == p_lab
            }
            note = ""
            if conf < CONF_THRESHOLD:
                note = "  [below threshold]"
            elif o_lab.lower() in existing_for_pair:
                note = "  [duplicate of existing]"
            elif len(o_lab) < 2:
                note = "  [too short]"
            print(f"    {p_lab!r:35s} -> {o_lab!r:30s}  conf={conf:.3f}{note}")

            if note:
                continue

            s_term = label_to_urn(s_lab, "entity")
            p_term = label_to_urn(p_lab, "prop")
            o_term = f'"{escape_literal(o_lab)}"'
            out_lines.append(f"{s_term} {p_term} {o_term} .")
            qt = quoted(s_term, p_term, o_term)
            out_lines.append(f'{qt} <{SUTRA_GENERATED}> "true"^^<{XSD_BOOLEAN}> .')
            out_lines.append(f'{qt} <{SUTRA_GENERATED_BY}> "{escape_literal(MODEL_VERSION)}" .')
            out_lines.append(f'{qt} <{SUTRA_CONFIDENCE}> "{conf:.4f}"^^<{XSD_DECIMAL}> .')
            for cp, co in subj_facts[s_lab][:MAX_CITATIONS]:
                cited = quoted(
                    s_term, label_to_urn(cp, "prop"), f'"{escape_literal(co)}"'
                )
                out_lines.append(f"{qt} <{SUTRA_INFERRED_FROM}> {cited} .")
            n_emitted += 1

    print(f"\n[smoke] {n_emitted}/{n_attempted} predictions met threshold")
    print(f"[smoke] sample output ({min(20, len(out_lines))} of {len(out_lines)} lines):")
    print("-" * 60)
    for line in out_lines[:20]:
        print(line)
    if len(out_lines) > 20:
        print(f"... and {len(out_lines) - 20} more")


if __name__ == "__main__":
    main()
