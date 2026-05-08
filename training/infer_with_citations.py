"""Generative-citation inference: predict new triples and write them back to SutraDB.

For each candidate subject, find predicates used by graph-neighbors but missing
from this subject. Mask the object slot, run the trained transformer, decode
top-1 tokens. If the prediction is high-confidence, emit:

    <S>  <P>  "predicted-label" .
    << <S> <P> "predicted-label" >>  sutra:generated     "true"^^xsd:boolean .
    << <S> <P> "predicted-label" >>  sutra:generatedBy   "wikidata_v2" .
    << <S> <P> "predicted-label" >>  sutra:confidence    "0.87"^^xsd:decimal .
    << <S> <P> "predicted-label" >>  sutra:supports      << <S> <p_existing> <o_existing> >> .
    ...

Generated triples are tagged so they can be (a) hidden from default queries and
(b) excluded from future training corpora. preprocess.py honours that filter.

Object is emitted as a plain literal: the model output is text. URI resolution
for predicted objects is the HNSW-decoder milestone (world-model thesis §5.7),
not implemented yet.

Usage:
    python training/infer_with_citations.py --post     # write to SutraDB
    python training/infer_with_citations.py            # dry-run, file only
"""
from __future__ import annotations

import argparse
import io
import json
import random
import sys
from collections import defaultdict
from pathlib import Path

import requests
import torch
import torch.nn.functional as F

# Make stdout tolerant of non-cp1252 chars (Japanese, etc.) on Windows.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model import TripleTransformer, ROLE_SPECIAL, ROLE_S, ROLE_P, ROLE_O  # noqa: E402
from tokenizer import (  # noqa: E402
    PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID, UNK_ID,
    encode,
)
from preprocess import (  # noqa: E402
    fetch_all_triples,
    build_qid_label_map,
    collect_unlabeled_predicates,
    fetch_wikidata_property_labels,
    parse_literal,
    RDFS_LABEL,
)

SUTRA_NS = "http://sutra.dev/"
SUTRA_GENERATED = SUTRA_NS + "generated"
SUTRA_GENERATED_BY = SUTRA_NS + "generatedBy"
SUTRA_CONFIDENCE = SUTRA_NS + "confidence"
SUTRA_SUPPORTS = SUTRA_NS + "supports"

XSD_BOOLEAN = "http://www.w3.org/2001/XMLSchema#boolean"
XSD_DECIMAL = "http://www.w3.org/2001/XMLSchema#decimal"


def escape_literal(s: str) -> str:
    """Escape a string for use as a quoted N-Triples literal."""
    return (
        s.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def fmt_term(term: dict) -> str:
    """Format a SPARQL JSON term as an N-Triples token (IRI / literal / bnode)."""
    if term["type"] == "uri":
        return f"<{term['value']}>"
    if term["type"] == "literal":
        value, lang = parse_literal(term)
        if lang:
            return f'"{escape_literal(value)}"@{lang}'
        if "datatype" in term:
            return f'"{escape_literal(value)}"^^<{term["datatype"]}>'
        return f'"{escape_literal(value)}"'
    if term["type"] == "bnode":
        return f"_:{term['value']}"
    raise ValueError(f"Unknown term type: {term}")


def quoted(s: str, p: str, o: str) -> str:
    """Build an N-Triples-star quoted-triple token: << s p o >>."""
    return f"<< {s} {p} {o} >>"


def build_inference_inputs(s_ids, p_ids, tokens_per_role):
    """Same layout as training, but the O slot is fully masked.

    Attention covers all tokens_per_role positions of the masked slot, since at
    inference we don't know how many tokens the answer will be. This is a soft
    mismatch with training (which only attended over the original token count)
    but works well enough in practice for the leading positions.
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
    role_vals = [ROLE_S, ROLE_P, ROLE_O]
    slots = [s_ids[:tokens_per_role], p_ids[:tokens_per_role], None]
    masked_positions: list[int] = []

    for i, ids in enumerate(slots):
        if ids is None:
            tokens[pos : pos + tokens_per_role] = MASK_ID
            attention[pos : pos + tokens_per_role] = True
            masked_positions = list(range(pos, pos + tokens_per_role))
        else:
            n = len(ids)
            if n:
                tokens[pos : pos + n] = torch.tensor(ids, dtype=torch.long)
                attention[pos : pos + n] = True
        roles[pos : pos + tokens_per_role] = role_vals[i]
        pos += tokens_per_role
        tokens[pos] = sep_ids[i]
        attention[pos] = True
        pos += 1

    return tokens.unsqueeze(0), roles.unsqueeze(0), attention.unsqueeze(0), masked_positions


def predict_object(
    model, s_label, p_label, vocab, inv_vocab, tokens_per_role, device, per_token_floor=0.05
):
    """Return (predicted_label, mean_confidence) or None if no usable prediction."""
    s_ids = encode(s_label, vocab)
    p_ids = encode(p_label, vocab)
    if not s_ids or not p_ids:
        return None
    tokens, roles, attention, masked_positions = build_inference_inputs(
        s_ids, p_ids, tokens_per_role
    )
    tokens = tokens.to(device)
    roles = roles.to(device)
    attention = attention.to(device)
    with torch.no_grad():
        logits = model(tokens, roles, attention)
        probs = F.softmax(logits, dim=-1)

    skip_ids = {PAD_ID, MASK_ID, UNK_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID}
    out_tokens: list[str] = []
    out_probs: list[float] = []
    for mp in masked_positions:
        p = probs[0, mp].clone()
        for sid in skip_ids:
            p[sid] = 0
        idx = int(p.argmax().item())
        prob = float(p[idx].item())
        if prob < per_token_floor:
            break
        out_tokens.append(inv_vocab[idx])
        out_probs.append(prob)

    if not out_tokens:
        return None
    label = " ".join(out_tokens)
    confidence = sum(out_probs) / len(out_probs)
    return label, confidence


def o_key(term: dict) -> str:
    """Stable key for an object term, for dedup/comparison."""
    if term["type"] == "uri":
        return term["value"]
    return parse_literal(term)[0]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkpoint", default="training/checkpoints/wikidata_v2.pt")
    parser.add_argument("--vocab", default="training/data/vocab.json")
    parser.add_argument("--endpoint", default="http://localhost:3030")
    parser.add_argument("--property-cache", default="training/property_label_cache.json")
    parser.add_argument("--max-subjects", type=int, default=20)
    parser.add_argument("--max-candidates-per-subject", type=int, default=5)
    parser.add_argument("--max-citations", type=int, default=10)
    parser.add_argument(
        "--confidence",
        type=float,
        default=0.4,
        help="Mean per-token probability threshold for emitting a triple",
    )
    parser.add_argument("--output", default="training/data/generated.nt")
    parser.add_argument(
        "--post",
        action="store_true",
        help="POST results to SutraDB /triples (in addition to writing the file)",
    )
    parser.add_argument(
        "--model-version",
        default=None,
        help="Tag to record on every generated triple (default: checkpoint stem)",
    )
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--device", default=None)
    args = parser.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)

    if args.device is None:
        args.device = "cuda" if torch.cuda.is_available() else "cpu"
    device = torch.device(args.device)

    if args.model_version is None:
        args.model_version = Path(args.checkpoint).stem

    vocab: dict[str, int] = json.loads(Path(args.vocab).read_text(encoding="utf-8"))
    inv_vocab: list[str] = [""] * len(vocab)
    for tok, i in vocab.items():
        inv_vocab[i] = tok

    ckpt = torch.load(args.checkpoint, map_location=device, weights_only=False)
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
    print(
        f"Loaded {args.checkpoint} ({n_params:,} params, {ckpt['vocab_size']:,} vocab)",
        file=sys.stderr,
    )

    print(f"Fetching triples from {args.endpoint}...", file=sys.stderr)
    triples = fetch_all_triples(args.endpoint)
    print(f"  got {len(triples):,} triples", file=sys.stderr)

    print("Building label maps...", file=sys.stderr)
    labels = build_qid_label_map(triples)
    missing_props = collect_unlabeled_predicates(triples, labels)
    prop_labels = fetch_wikidata_property_labels(missing_props, Path(args.property_cache))
    labels.update(prop_labels)
    print(f"  resolved {len(labels):,} URI -> label mappings", file=sys.stderr)

    # subject_uri -> [(predicate_uri, object_term)]
    subj_facts: dict[str, list[tuple[str, dict]]] = defaultdict(list)
    # predicate_uri -> [(subject_uri, object_term)]
    pred_usage: dict[str, list[tuple[str, dict]]] = defaultdict(list)

    for t in triples:
        if t["s"]["type"] != "uri":
            continue
        p_iri = t["p"]["value"]
        if p_iri == RDFS_LABEL:
            continue
        # Skip RDF-star annotations on already-generated triples (their subjects
        # are quoted triples, which surface as the synthetic <<QUOTED_TRIPLE>>
        # marker, not a real URI). We also skip the annotation predicates
        # themselves so we don't train on our own generated provenance.
        if p_iri in (SUTRA_GENERATED, SUTRA_GENERATED_BY, SUTRA_CONFIDENCE, SUTRA_SUPPORTS):
            continue
        s_uri = t["s"]["value"]
        subj_facts[s_uri].append((p_iri, t["o"]))
        pred_usage[p_iri].append((s_uri, t["o"]))

    candidate_subjects = [
        s for s, facts in subj_facts.items() if len(facts) >= 3 and s in labels
    ]
    random.shuffle(candidate_subjects)
    candidate_subjects = candidate_subjects[: args.max_subjects]
    print(f"Inferring on {len(candidate_subjects)} subjects...", file=sys.stderr)

    out_lines: list[str] = []
    n_emitted = 0
    n_attempted = 0

    for s_uri in candidate_subjects:
        s_label = labels[s_uri]
        s_existing_preds = {p for p, _ in subj_facts[s_uri]}

        # Find graph-neighbors: subjects that share at least one (p, o-key) with S.
        # Their predicates are the candidate predicates for S.
        neighbor_pred_score: dict[str, int] = defaultdict(int)
        for p, o_term in subj_facts[s_uri]:
            ok = o_key(o_term)
            for s2, o2_term in pred_usage.get(p, []):
                if s2 == s_uri:
                    continue
                if o_key(o2_term) != ok:
                    continue
                for p2, _ in subj_facts.get(s2, []):
                    if p2 in s_existing_preds:
                        continue
                    if p2 not in labels:
                        continue
                    neighbor_pred_score[p2] += 1

        candidate_preds = sorted(neighbor_pred_score.items(), key=lambda kv: -kv[1])
        candidate_preds = [p for p, _ in candidate_preds[: args.max_candidates_per_subject]]

        for p_uri in candidate_preds:
            n_attempted += 1
            p_label = labels[p_uri]
            res = predict_object(
                model, s_label, p_label, vocab, inv_vocab, tokens_per_role, device
            )
            if res is None:
                continue
            o_label, confidence = res
            if confidence < args.confidence:
                continue
            if len(o_label) < 2:
                continue
            # Skip if a triple with the same (S, P) already has an object whose
            # label matches the prediction (the model is just memorising).
            existing_labels = set()
            for op, oo in subj_facts[s_uri]:
                if op != p_uri:
                    continue
                if oo["type"] == "uri":
                    existing_labels.add(labels.get(oo["value"], "").lower())
                else:
                    existing_labels.add(parse_literal(oo)[0].lower())
            if o_label.lower() in existing_labels:
                continue

            s_term = f"<{s_uri}>"
            p_term = f"<{p_uri}>"
            o_term = f'"{escape_literal(o_label)}"'

            out_lines.append(f"{s_term} {p_term} {o_term} .")
            qt = quoted(s_term, p_term, o_term)
            out_lines.append(f'{qt} <{SUTRA_GENERATED}> "true"^^<{XSD_BOOLEAN}> .')
            out_lines.append(f'{qt} <{SUTRA_GENERATED_BY}> "{escape_literal(args.model_version)}" .')
            out_lines.append(f'{qt} <{SUTRA_CONFIDENCE}> "{confidence:.4f}"^^<{XSD_DECIMAL}> .')

            for cp_uri, co_term in subj_facts[s_uri][: args.max_citations]:
                cited = quoted(s_term, f"<{cp_uri}>", fmt_term(co_term))
                out_lines.append(f"{qt} <{SUTRA_SUPPORTS}> {cited} .")

            n_emitted += 1
            print(
                f"  + {s_label!s} | {p_label!s} | {o_label!s}  (conf={confidence:.3f})",
                file=sys.stderr,
            )

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(out_lines) + "\n", encoding="utf-8")
    print(
        f"\n{n_emitted}/{n_attempted} predictions met threshold "
        f"-> {len(out_lines)} N-Triples lines written to {out_path}",
        file=sys.stderr,
    )

    if args.post and out_lines:
        print(f"POSTing to {args.endpoint}/triples...", file=sys.stderr)
        body = "\n".join(out_lines)
        resp = requests.post(
            f"{args.endpoint}/triples",
            data=body.encode("utf-8"),
            headers={"Content-Type": "text/plain; charset=utf-8"},
            timeout=60,
        )
        if resp.status_code == 200:
            j = resp.json()
            print(
                f"  inserted: {j.get('inserted', 0)}  errors: {len(j.get('errors', []))}",
                file=sys.stderr,
            )
            for e in j.get("errors", [])[:5]:
                print(f"    ! {e}", file=sys.stderr)
        else:
            print(f"  ERROR {resp.status_code}: {resp.text[:200]}", file=sys.stderr)


if __name__ == "__main__":
    main()
