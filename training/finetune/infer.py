"""Generate predictions with the fine-tuned (QLoRA) model.

Emits the SAME N-Triples-star provenance schema as the from-scratch
`training/infer_with_citations.py` — it imports that module's emitter
helpers and the shared `candidate_predicates`/`build_inference_state`
rather than re-implementing them, so the two model tracks stay in lock-step
(no parallel implementation). Adds `loka:propositionBaseModel` per
planning/fine-tuning-track.md so a consumer can audit "fine-tune vs
from-scratch".

The sidecar (`tools/infer_server.py`) imports `load_finetuned` and
`generate_for_subject_llm` directly; the CLI below is for batch runs.

    python training/finetune/infer.py \
        --adapter training/finetune/adapters/qwen2.5-1.5b-loka-v1/epoch3 \
        --seed training/data/seed_Q42.nt \
        --output training/finetune/data/gen_qwen_e3.nt
"""
from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

_HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(_HERE))               # sft_common
sys.path.insert(0, str(_HERE.parent))        # infer_with_citations

from sft_common import build_prompt, render_chat  # noqa: E402
from infer_with_citations import (  # noqa: E402
    build_inference_state,
    candidate_predicates,
    escape_literal,
    quoted,
    fmt_term,
    parse_literal,
    is_reserved_predicate,
    RESERVED_PROVENANCE_PREFIX,
    LOKA_GENERATED,
    LOKA_GENERATED_BY,
    LOKA_CONFIDENCE,
    LOKA_INFERRED_FROM,
    XSD_BOOLEAN,
    XSD_DECIMAL,
)

LOKA_BASE_MODEL = RESERVED_PROVENANCE_PREFIX + "propositionBaseModel"


def load_finetuned(adapter_dir, base_model=None, device="cuda"):
    """Load base (4-bit nf4, same as training) + the LoRA adapter. Returns a
    bundle the sidecar/CLI reuse. `base_model` defaults to the adapter's
    recorded base."""
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig
    from peft import PeftConfig, PeftModel

    adapter_dir = str(adapter_dir)
    pcfg = PeftConfig.from_pretrained(adapter_dir)
    base = base_model or pcfg.base_model_name_or_path
    tok = AutoTokenizer.from_pretrained(adapter_dir)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token
    bnb = BitsAndBytesConfig(
        load_in_4bit=True, bnb_4bit_quant_type="nf4",
        bnb_4bit_compute_dtype=torch.bfloat16, bnb_4bit_use_double_quant=True,
    )
    model = AutoModelForCausalLM.from_pretrained(
        base, quantization_config=bnb, device_map={"": 0}, dtype=torch.bfloat16,
    )
    model = PeftModel.from_pretrained(model, adapter_dir)
    model.eval()
    print(f"loaded {base} + adapter {adapter_dir}", file=sys.stderr)
    return {"model": model, "tok": tok, "base_model": base,
            "adapter": adapter_dir}


def _facts_as_context(s_uri, labels, subj_facts, max_ctx=8):
    """The subject's known facts as readable [s,p,o] label triples."""
    s_label = labels.get(s_uri, s_uri)
    ctx = []
    for p_uri, o_term in subj_facts.get(s_uri, [])[:max_ctx]:
        p_label = labels.get(p_uri, p_uri.rsplit("/", 1)[-1])
        if o_term["type"] == "uri":
            o_label = labels.get(o_term["value"], o_term["value"].rsplit("/", 1)[-1])
        else:
            o_label = parse_literal(o_term)[0]
        ctx.append([s_label, p_label, o_label])
    return ctx, s_label


def _generate_answer(bundle, prompt_text, max_new_tokens=24):
    """Greedy-decode the answer; confidence = exp(mean token log-prob)."""
    import torch
    model, tok = bundle["model"], bundle["tok"]
    enc = tok(prompt_text, return_tensors="pt", add_special_tokens=False).to(
        model.device
    )
    with torch.no_grad():
        out = model.generate(
            **enc, max_new_tokens=max_new_tokens, do_sample=False,
            num_beams=1, return_dict_in_generate=True, output_scores=True,
            pad_token_id=tok.pad_token_id,
        )
    gen_ids = out.sequences[0, enc["input_ids"].shape[1]:]
    logps = []
    for tok_id, step in zip(gen_ids, out.scores):
        lp = torch.log_softmax(step[0], dim=-1)[tok_id].item()
        logps.append(lp)
    text = tok.decode(gen_ids, skip_special_tokens=True).strip()
    text = text.splitlines()[0].strip() if text else ""
    conf = math.exp(sum(logps) / len(logps)) if logps else 0.0
    return text, conf


def generate_for_subject_llm(
    bundle,
    s_uri,
    *,
    labels,
    subj_facts,
    pred_usage,
    model_version="qwen2.5-1.5b-loka",
    confidence=0.0,
    max_candidates_per_subject=6,
    max_citations=10,
    fallback_candidates=True,
):
    """Fine-tune analogue of infer_with_citations.generate_for_subject —
    SAME return shape (out_lines, log) so the sidecar swaps paths cleanly.
    Candidate predicates come from the shared helper."""
    out_lines: list[str] = []
    log: list[str] = []
    if s_uri not in labels or s_uri not in subj_facts:
        return out_lines, log

    ctx, s_label = _facts_as_context(s_uri, labels, subj_facts)
    cands = candidate_predicates(
        s_uri, labels=labels, subj_facts=subj_facts, pred_usage=pred_usage,
        max_candidates_per_subject=max_candidates_per_subject,
        fallback_candidates=fallback_candidates,
    )
    existing: dict[str, set] = {}
    for p, o in subj_facts.get(s_uri, []):
        val = (labels.get(o["value"], "") if o["type"] == "uri"
               else parse_literal(o)[0])
        existing.setdefault(p, set()).add(val.lower())

    for p_uri in cands:
        if is_reserved_predicate(p_uri):
            continue
        p_label = labels[p_uri]
        prompt = build_prompt(ctx, [s_label, p_label, "?"], "object")
        prompt_text, _ = render_chat(bundle["tok"], prompt, None)
        o_label, conf = _generate_answer(bundle, prompt_text)
        if not o_label or len(o_label) < 2 or conf < confidence:
            continue
        if o_label.lower() in existing.get(p_uri, set()):
            continue

        s_t = f"<{s_uri}>"
        p_t = f"<{p_uri}>"
        o_t = f'"{escape_literal(o_label)}"'
        out_lines.append(f"{s_t} {p_t} {o_t} .")
        qt = quoted(s_t, p_t, o_t)
        out_lines.append(f'{qt} <{LOKA_GENERATED}> "true"^^<{XSD_BOOLEAN}> .')
        out_lines.append(f'{qt} <{LOKA_GENERATED_BY}> "{escape_literal(model_version)}" .')
        out_lines.append(f'{qt} <{LOKA_BASE_MODEL}> "{escape_literal(bundle["base_model"])}" .')
        out_lines.append(f'{qt} <{LOKA_CONFIDENCE}> "{conf:.4f}"^^<{XSD_DECIMAL}> .')
        for cp_uri, co_term in subj_facts[s_uri][:max_citations]:
            cited = quoted(s_t, f"<{cp_uri}>", fmt_term(co_term))
            out_lines.append(f"{qt} <{LOKA_INFERRED_FROM}> {cited} .")
        log.append(f"  + {s_label} | {p_label} | {o_label}  (conf={conf:.3f})")
    return out_lines, log


def _nt_to_sparql_terms(path: Path):
    """Parse a simple N-Triples seed file into the SPARQL-JSON-ish term dicts
    build_inference_state expects (subject/predicate uri, object uri|literal)."""
    import re
    tri = []
    rx = re.compile(r'^<([^>]+)>\s+<([^>]+)>\s+(?:<([^>]+)>|"((?:[^"\\]|\\.)*)")')
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("<<"):
            continue
        m = rx.match(line)
        if not m:
            continue
        s, p, o_uri, o_lit = m.groups()
        o = ({"type": "uri", "value": o_uri} if o_uri is not None
             else {"type": "literal", "value": o_lit})
        tri.append({"s": {"type": "uri", "value": s},
                    "p": {"type": "uri", "value": p}, "o": o})
    return tri


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--adapter", required=True, type=Path, help="Adapter directory written by finetune.py")
    ap.add_argument("--seed", required=True, type=Path, help="N-Triples file: seed subjects to generate predictions for")
    ap.add_argument("--output", required=True, type=Path, help="N-Triples-star output")
    ap.add_argument("--n", type=int, default=10, help="(reserved) candidates per (subject, predicate)")
    ap.add_argument("--confidence", type=float, default=0.25, help="Drop candidates below this token-avg prob")
    ap.add_argument("--max-subjects", type=int, default=20)
    ap.add_argument("--model-version", default="qwen2.5-1.5b-loka-v1", help="Tag emitted in loka:generatedBy")
    args = ap.parse_args()

    triples = _nt_to_sparql_terms(args.seed)
    labels, subj_facts, pred_usage, _ = build_inference_state(triples)
    bundle = load_finetuned(args.adapter)

    subs = [s for s in subj_facts if s in labels][: args.max_subjects]
    out: list[str] = []
    for s_uri in subs:
        lines, log = generate_for_subject_llm(
            bundle, s_uri, labels=labels, subj_facts=subj_facts,
            pred_usage=pred_usage, model_version=args.model_version,
            confidence=args.confidence,
        )
        out.extend(lines)
        for m in log:
            print(m, file=sys.stderr)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(out) + "\n", encoding="utf-8")
    print(f"wrote {len(out)} lines -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
