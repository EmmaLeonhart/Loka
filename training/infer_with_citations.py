"""Generative-citation inference: predict new triples and write them back to Loka.

For each candidate subject, find predicates used by graph-neighbors but missing
from this subject. Mask the object slot, run the trained transformer, decode
top-1 tokens. If the prediction is high-confidence, emit:

    <S>  <P>  "predicted-label" .
    << <S> <P> "predicted-label" >>  loka:generated     "true"^^xsd:boolean .
    << <S> <P> "predicted-label" >>  loka:generatedBy   "wikidata_v2" .
    << <S> <P> "predicted-label" >>  loka:confidence    "0.87"^^xsd:decimal .
    << <S> <P> "predicted-label" >>  loka:inferredFrom  << <S> <p_existing> <o_existing> >> .
    ...

Generated triples are tagged so they can be (a) hidden from default queries and
(b) excluded from future training corpora. preprocess.py honours that filter.

Object is emitted as a plain literal: the model output is text. URI resolution
for predicted objects is the HNSW-decoder milestone (world-model thesis §5.7),
not implemented yet.

Usage:
    python training/infer_with_citations.py --post     # write to Loka
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
    encode as encode_word,
)
from preprocess import (  # noqa: E402
    fetch_all_triples,
    build_qid_label_map,
    collect_unlabeled_predicates,
    fetch_wikidata_property_labels,
    parse_literal,
    is_reserved_predicate,
    RDFS_LABEL,
    RESERVED_PROVENANCE_PREFIX,
    LOKA_GENERATED,
    LOKA_GENERATED_BY,
    LOKA_CONFIDENCE,
    LOKA_INFERRED_FROM,
)

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
    model,
    s_label,
    p_label,
    vocab,
    inv_vocab,
    tokens_per_role,
    device,
    per_token_floor=0.05,
    repetition_penalty: float = 3.0,
    encode_fn=None,
):
    """Return (predicted_label, mean_confidence) or None if no usable prediction.

    Decoding uses greedy top-1 with a repetition penalty: every time we emit a
    token, its probability mass at later masked positions is divided by
    `repetition_penalty`. This prevents the model from looping on filler tokens
    like "of"/"and" while still letting it reuse a token if the unpenalised
    distribution strongly demands it.

    `encode_fn`, if provided, is a callable that maps a label string to a list
    of token IDs (e.g. a BPE tokenizer's encoder). If None, falls back to the
    word-level regex encoder against `vocab`.
    """
    if encode_fn is not None:
        s_ids = encode_fn(s_label)
        p_ids = encode_fn(p_label)
    else:
        s_ids = encode_word(s_label, vocab)
        p_ids = encode_word(p_label, vocab)
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
    emit_counts: dict[int, int] = {}
    for mp in masked_positions:
        p = probs[0, mp].clone()
        for sid in skip_ids:
            p[sid] = 0
        # Cumulative repetition penalty: divide by `repetition_penalty ** count`
        # so each repeat compounds. Three emissions of "of" with penalty 3.0
        # multiply its divisor by 27, which usually drops it below
        # per_token_floor and breaks the loop. A genuinely-needed repeat
        # (e.g. "of [thing] of [thing]") can still win the second time
        # through, just not a third or fourth.
        if repetition_penalty != 1.0:
            for prev_id, count in emit_counts.items():
                p[prev_id] = p[prev_id] / (repetition_penalty ** count)
        idx = int(p.argmax().item())
        prob = float(p[idx].item())
        if prob < per_token_floor:
            break
        out_tokens.append(inv_vocab[idx])
        out_probs.append(prob)
        emit_counts[idx] = emit_counts.get(idx, 0) + 1

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


def load_model(checkpoint, vocab_path, device, bpe_tokenizer=None, model_version=None):
    """Load the pinned/file checkpoint once and return a reusable bundle.

    Shared by ``main()`` and the resident inference sidecar
    (``tools/infer_server.py``) so model loading lives in exactly one place.
    ``checkpoint``/``vocab_path`` of ``None`` fall back to the MODEL.json pin
    (auto-downloaded from Hugging Face on first run).
    """
    if checkpoint is None or vocab_path is None:
        from loader import resolve_checkpoint, resolve_vocab, info as pin_info
        if checkpoint is None:
            checkpoint = str(resolve_checkpoint())
        if vocab_path is None:
            vocab_path = str(resolve_vocab())
        if model_version is None:
            model_version = pin_info().get("name", Path(checkpoint).stem)
    if model_version is None:
        model_version = Path(checkpoint).stem

    vocab: dict[str, int] = json.loads(Path(vocab_path).read_text(encoding="utf-8"))
    inv_vocab: list[str] = [""] * len(vocab)
    for tok, i in vocab.items():
        inv_vocab[i] = tok

    ckpt = torch.load(checkpoint, map_location=device, weights_only=False)
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
        f"Loaded {checkpoint} ({n_params:,} params, {ckpt['vocab_size']:,} vocab)",
        file=sys.stderr,
    )

    encode_fn = None
    if bpe_tokenizer:
        from tokenizers import Tokenizer  # type: ignore
        bpe_tok = Tokenizer.from_file(bpe_tokenizer)

        def encode_fn(text: str) -> list[int]:  # noqa: E306
            return bpe_tok.encode(text, add_special_tokens=False).ids

        print(f"Using BPE tokenizer: {bpe_tokenizer}", file=sys.stderr)

    return {
        "model": model,
        "vocab": vocab,
        "inv_vocab": inv_vocab,
        "tokens_per_role": tokens_per_role,
        "encode_fn": encode_fn,
        "model_version": model_version,
        "checkpoint": checkpoint,
        "vocab_size": ckpt["vocab_size"],
    }


def build_inference_state(triples, property_cache="training/property_label_cache.json"):
    """Build ``(labels, subj_facts, pred_usage, n_reserved_skipped)`` from raw
    SPARQL-JSON triples.

    ``rdfs:label`` and reserved-namespace (``http://loka.dev/provenance/``)
    rows are kept out of inference state — defense in depth on top of the
    SPARQL-star corpus filter in ``fetch_all_triples``.
    """
    labels = build_qid_label_map(triples)
    missing_props = collect_unlabeled_predicates(triples, labels)
    prop_labels = fetch_wikidata_property_labels(missing_props, Path(property_cache))
    labels.update(prop_labels)

    # subject_uri -> [(predicate_uri, object_term)]
    subj_facts: dict[str, list[tuple[str, dict]]] = defaultdict(list)
    # predicate_uri -> [(subject_uri, object_term)]
    pred_usage: dict[str, list[tuple[str, dict]]] = defaultdict(list)

    n_reserved_skipped = 0
    for t in triples:
        if t["s"]["type"] != "uri":
            continue
        p_iri = t["p"]["value"]
        if p_iri == RDFS_LABEL:
            continue
        if is_reserved_predicate(p_iri):
            n_reserved_skipped += 1
            continue
        s_uri = t["s"]["value"]
        subj_facts[s_uri].append((p_iri, t["o"]))
        pred_usage[p_iri].append((s_uri, t["o"]))
    return labels, subj_facts, pred_usage, n_reserved_skipped


def generate_for_subject(
    model,
    s_uri,
    *,
    labels,
    subj_facts,
    pred_usage,
    vocab,
    inv_vocab,
    tokens_per_role,
    device,
    model_version,
    confidence=0.4,
    repetition_penalty=3.0,
    max_candidates_per_subject=5,
    max_citations=10,
    encode_fn=None,
    fallback_candidates=False,
):
    """Generate provenance-tagged N-Triples-star for ONE subject.

    Candidate predicates are those used by graph-neighbours (subjects sharing a
    ``(p, o-key)`` with S) but missing from S. Returns ``(out_lines, log)``:
    ``out_lines`` is N-Triples-star text, ``log`` is human-readable emission
    lines (each accepted triple starts with ``"  + "``). Reserved-namespace
    predicates are refused at every gate.

    ``fallback_candidates`` (default off — the batch pipeline keeps its exact
    behaviour) tops the candidate list up from global predicate frequency when
    the neighbour heuristic underfills it. Small/symmetric graphs (e.g. an
    interactive double-click on the 73-triple Shinto demo) otherwise yield no
    candidates and emit nothing; this makes the gesture actually do something.
    """
    out_lines: list[str] = []
    log: list[str] = []
    if s_uri not in labels or s_uri not in subj_facts:
        return out_lines, log

    s_label = labels[s_uri]
    s_existing_preds = {p for p, _ in subj_facts[s_uri]}

    # Graph-neighbours: subjects that share at least one (p, o-key) with S.
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
    candidate_preds = [
        p for p, _ in candidate_preds if not is_reserved_predicate(p)
    ][:max_candidates_per_subject]

    if fallback_candidates and len(candidate_preds) < max_candidates_per_subject:
        have = set(candidate_preds) | s_existing_preds
        for p, _users in sorted(pred_usage.items(), key=lambda kv: -len(kv[1])):
            if len(candidate_preds) >= max_candidates_per_subject:
                break
            if p in have or p not in labels or is_reserved_predicate(p):
                continue
            candidate_preds.append(p)
            have.add(p)

    for p_uri in candidate_preds:
        if is_reserved_predicate(p_uri):
            continue
        p_label = labels[p_uri]
        res = predict_object(
            model, s_label, p_label, vocab, inv_vocab, tokens_per_role, device,
            repetition_penalty=repetition_penalty,
            encode_fn=encode_fn,
        )
        if res is None:
            continue
        o_label, conf = res
        if conf < confidence:
            continue
        if len(o_label) < 2:
            continue
        # Skip if (S, P) already has an object whose label matches the
        # prediction (the model is just memorising).
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
        if is_reserved_predicate(p_uri):
            log.append(f"  ! REFUSED to emit reserved-namespace predicate {p_uri!r}")
            continue

        s_term = f"<{s_uri}>"
        p_term = f"<{p_uri}>"
        o_term = f'"{escape_literal(o_label)}"'

        out_lines.append(f"{s_term} {p_term} {o_term} .")
        qt = quoted(s_term, p_term, o_term)
        out_lines.append(f'{qt} <{LOKA_GENERATED}> "true"^^<{XSD_BOOLEAN}> .')
        out_lines.append(f'{qt} <{LOKA_GENERATED_BY}> "{escape_literal(model_version)}" .')
        out_lines.append(f'{qt} <{LOKA_CONFIDENCE}> "{conf:.4f}"^^<{XSD_DECIMAL}> .')
        for cp_uri, co_term in subj_facts[s_uri][:max_citations]:
            cited = quoted(s_term, f"<{cp_uri}>", fmt_term(co_term))
            out_lines.append(f"{qt} <{LOKA_INFERRED_FROM}> {cited} .")
        log.append(f"  + {s_label!s} | {p_label!s} | {o_label!s}  (conf={conf:.3f})")

    return out_lines, log


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--checkpoint",
        default=None,
        help="Path to a .pt checkpoint. If omitted, uses the model pinned in "
             "MODEL.json and auto-downloads from Hugging Face on first run.",
    )
    parser.add_argument(
        "--vocab",
        default=None,
        help="Path to vocab.json. If omitted, uses the vocab pinned in "
             "MODEL.json and auto-downloads from Hugging Face on first run.",
    )
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
    parser.add_argument(
        "--repetition-penalty",
        type=float,
        default=3.0,
        help="Divide already-emitted token probabilities by this factor at later "
             "positions. 1.0 disables. Higher = harder for the model to loop.",
    )
    parser.add_argument("--output", default="training/data/generated.nt")
    parser.add_argument(
        "--post",
        action="store_true",
        help="POST results to Loka /triples (in addition to writing the file)",
    )
    parser.add_argument(
        "--model-version",
        default=None,
        help="Tag to record on every generated triple (default: checkpoint stem)",
    )
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--device", default=None)
    parser.add_argument(
        "--bpe-tokenizer",
        default=None,
        help="Optional path to a tokenizer_bpe.json (HuggingFace tokenizers format). "
             "If provided, encode S/P labels with BPE instead of the word-level vocab. "
             "Use the same tokenizer that the checkpoint was trained with.",
    )
    args = parser.parse_args()

    random.seed(args.seed)
    torch.manual_seed(args.seed)

    if args.device is None:
        args.device = "cuda" if torch.cuda.is_available() else "cpu"
    device = torch.device(args.device)

    bundle = load_model(
        args.checkpoint, args.vocab, device,
        bpe_tokenizer=args.bpe_tokenizer, model_version=args.model_version,
    )
    model = bundle["model"]
    vocab = bundle["vocab"]
    inv_vocab = bundle["inv_vocab"]
    tokens_per_role = bundle["tokens_per_role"]
    encode_fn = bundle["encode_fn"]
    args.model_version = bundle["model_version"]

    print(f"Fetching triples from {args.endpoint}...", file=sys.stderr)
    triples = fetch_all_triples(args.endpoint)
    print(f"  got {len(triples):,} triples", file=sys.stderr)

    print("Building label maps...", file=sys.stderr)
    labels, subj_facts, pred_usage, n_reserved_skipped = build_inference_state(
        triples, args.property_cache
    )
    print(f"  resolved {len(labels):,} URI -> label mappings", file=sys.stderr)
    if n_reserved_skipped:
        print(
            f"  dropped {n_reserved_skipped} reserved-namespace rows from inference state",
            file=sys.stderr,
        )

    candidate_subjects = [
        s for s, facts in subj_facts.items() if len(facts) >= 3 and s in labels
    ]
    random.shuffle(candidate_subjects)
    candidate_subjects = candidate_subjects[: args.max_subjects]
    print(f"Inferring on {len(candidate_subjects)} subjects...", file=sys.stderr)

    out_lines: list[str] = []
    n_emitted = 0

    for s_uri in candidate_subjects:
        lines, log = generate_for_subject(
            model, s_uri,
            labels=labels, subj_facts=subj_facts, pred_usage=pred_usage,
            vocab=vocab, inv_vocab=inv_vocab, tokens_per_role=tokens_per_role,
            device=device, model_version=args.model_version,
            confidence=args.confidence,
            repetition_penalty=args.repetition_penalty,
            max_candidates_per_subject=args.max_candidates_per_subject,
            max_citations=args.max_citations,
            encode_fn=encode_fn,
        )
        out_lines.extend(lines)
        for m in log:
            print(m, file=sys.stderr)
            if m.lstrip().startswith("+"):
                n_emitted += 1

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(out_lines) + "\n", encoding="utf-8")
    print(
        f"\n{n_emitted} predictions met threshold "
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
