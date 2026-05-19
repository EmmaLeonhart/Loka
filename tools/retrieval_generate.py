"""Slice 3: base-Qwen continuation over the retrieved sequence.

Pipeline (no fine-tune — the _ft_probe3 result said the base model is
the asset): node -> graph_retrieval.retrieve() -> a relevance-ranked
`s | p | o` sequence -> base Qwen2.5-1.5B-Instruct "continue these RDF
triples" -> parse emitted triples -> provenance-tag (reuse
infer_with_citations' emitter; loka:propositionBaseModel records it was
the base model) -> optionally POST back to Loka.

Reusable `generate(endpoint, node, ...) -> list[dict]` so the sidecar
imports it (no parallel impl).

  python tools/retrieval_generate.py --endpoint http://localhost:3031 \
      --node http://www.wikidata.org/entity/Q42 --post
"""
from __future__ import annotations
import argparse, math, re, sys
from pathlib import Path
import requests

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
sys.path.insert(0, str(ROOT / "training"))
from graph_retrieval import retrieve  # noqa: E402
from infer_with_citations import (  # noqa: E402
    escape_literal, quoted, LOKA_GENERATED, LOKA_GENERATED_BY,
    LOKA_CONFIDENCE, RESERVED_PROVENANCE_PREFIX, XSD_BOOLEAN, XSD_DECIMAL,
)

LOKA_BASE_MODEL = RESERVED_PROVENANCE_PREFIX + "propositionBaseModel"
BASE = "Qwen/Qwen2.5-1.5B-Instruct"
_MODEL = None
_TOK = None


def _load():
    global _MODEL, _TOK
    if _MODEL is None:
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer
        _TOK = AutoTokenizer.from_pretrained(BASE)
        _MODEL = AutoModelForCausalLM.from_pretrained(
            BASE, dtype=torch.float32, device_map=None).eval()
    return _MODEL, _TOK


def generate(endpoint, node, hops=2, budget=50, max_new_tokens=180,
             model_version="qwen2.5-1.5b-base"):
    ctx = retrieve(endpoint, node, hops=hops, budget=budget)
    if not ctx:
        return [], "no context retrieved for node"
    subj_label = next((s for s, _, _ in reversed(ctx)), None) or node
    block = "\n".join(f"{s} | {p} | {o}" for s, p, o in ctx)
    prompt = ("Continue this sequence of RDF triples. Each line is exactly "
              "`subject | predicate | object`. Output only more true triples "
              f"about {subj_label}, in that format, one per line:\n\n{block}\n")

    import torch
    model, tok = _load()
    msgs = [{"role": "user", "content": prompt}]
    ptext = tok.apply_chat_template(msgs, tokenize=False,
                                    add_generation_prompt=True)
    enc = tok(ptext, return_tensors="pt", add_special_tokens=False)
    with torch.no_grad():
        out = model.generate(**enc, max_new_tokens=max_new_tokens,
                              do_sample=False, return_dict_in_generate=True,
                              output_scores=True, pad_token_id=tok.pad_token_id)
    gen_ids = out.sequences[0, enc["input_ids"].shape[1]:]
    txt = tok.decode(gen_ids, skip_special_tokens=True).strip()
    # mean token prob as a coarse confidence
    lps = [torch.log_softmax(s[0], -1)[i].item()
           for i, s in zip(gen_ids, out.scores)]
    conf = math.exp(sum(lps) / len(lps)) if lps else 0.0

    seen = {(s.lower(), p.lower(), str(o).lower()) for s, p, o in ctx}
    triples = []
    for ln in txt.splitlines():
        # strip leading markdown bullets / numbering the model adds
        ln = re.sub(r"^\s*(?:[-*•·]|\d+[.)])\s*", "", ln).strip()
        parts = [x.strip() for x in ln.split("|")]
        if len(parts) != 3 or not all(parts):
            continue
        s, p, o = parts
        if (s.lower(), p.lower(), o.lower()) in seen:
            continue
        # the retrieved context is now label-only; reject any QID that
        # still slipped through so we never emit raw identifiers
        if re.fullmatch(r"Q\d{2,}", o) or re.fullmatch(r"Q\d{2,}", s):
            continue
        triples.append({"s": s, "p": p, "o": o, "confidence": round(conf, 4)})

    nt = []
    for t in triples:
        s_t = f'"{escape_literal(t["s"])}"'
        p_t = f'"{escape_literal(t["p"])}"'
        o_t = f'"{escape_literal(t["o"])}"'
        nt.append(f"{s_t} {p_t} {o_t} .")
        qt = quoted(s_t, p_t, o_t)
        nt.append(f'{qt} <{LOKA_GENERATED}> "true"^^<{XSD_BOOLEAN}> .')
        nt.append(f'{qt} <{LOKA_GENERATED_BY}> "{escape_literal(model_version)}" .')
        nt.append(f'{qt} <{LOKA_BASE_MODEL}> "{escape_literal(BASE)}" .')
        nt.append(f'{qt} <{LOKA_CONFIDENCE}> "{t["confidence"]:.4f}"^^<{XSD_DECIMAL}> .')
    return triples, "\n".join(nt)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--endpoint", default="http://localhost:3031")
    ap.add_argument("--node", required=True)
    ap.add_argument("--hops", type=int, default=2)
    ap.add_argument("--budget", type=int, default=50)
    ap.add_argument("--post", action="store_true")
    args = ap.parse_args()
    triples, nt = generate(args.endpoint, args.node, hops=args.hops,
                           budget=args.budget)
    for t in triples:
        print(f'  + {t["s"]} | {t["p"]} | {t["o"]}  (conf {t["confidence"]})')
    print(f"\n{len(triples)} new triples", file=sys.stderr)
    if args.post and nt.strip():
        r = requests.post(f"{args.endpoint.rstrip('/')}/triples",
                          data=(nt + "\n").encode("utf-8"),
                          headers={"Content-Type": "text/plain; charset=utf-8"},
                          timeout=120)
        print(f"POST /triples: {r.status_code} {r.text[:160]}", file=sys.stderr)


if __name__ == "__main__":
    main()
