"""Auto-regressive proposition generation test.

For every triple in a seed graph, generate K child triples auto-regressively:
the i-th child sees and cites everything in the context pool at emit time,
which grows monotonically as siblings are added.

Protocol is documented in `planning/autoregressive-propgen-test.md`. This is a
post-training behavioural test, not a production generation pipeline. The model
is single-triple (S, P, masked O -> O); "auto-regressive" here applies to the
*context pool* and *citation set*, not to model conditioning.

Usage (assumes a fresh Loka instance loaded with the seed file is running):
    loka serve --data-dir ./test-data --port 3031
    python training/test_autoregressive_propgen.py --endpoint http://localhost:3031

Or, against a seed .nt file directly (no Loka required, all in-memory):
    python training/test_autoregressive_propgen.py \\
        --seed-file training/data/seed_Q42.nt \\
        --output training/data/test_propgen_Q42.nt
"""
from __future__ import annotations

import argparse
import io
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

import torch
import torch.nn.functional as F

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

sys.path.insert(0, str(Path(__file__).resolve().parent))
from model import TripleTransformer  # noqa: E402
from tokenizer import (  # noqa: E402
    PAD_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID, MASK_ID, UNK_ID,
    encode as encode_word,
)
from preprocess import (  # noqa: E402
    parse_literal,
    is_reserved_predicate,
    fetch_wikidata_property_labels,
    RDFS_LABEL,
    LOKA_GENERATED,
    LOKA_GENERATED_BY,
    LOKA_CONFIDENCE,
    LOKA_INFERRED_FROM,
    WIKIDATA_PROP_RE,
)
from infer_with_citations import (  # noqa: E402
    escape_literal,
    quoted,
    fmt_term,
    o_key,
    build_inference_inputs,
    XSD_BOOLEAN,
    XSD_DECIMAL,
)


# ─── N-Triples loader ───────────────────────────────────────────────────────

# A pragmatic regex parser. Loka's exporter and `wikidata_random_seed.py` both
# emit clean one-line N-Triples; we don't need a full RDF-star parser here.
NT_LINE_RE = re.compile(
    r'^\s*'
    r'(<[^>]+>|_:\S+)\s+'                               # subject
    r'(<[^>]+>)\s+'                                     # predicate
    r'(<[^>]+>|_:\S+|"(?:[^"\\]|\\.)*"(?:@\S+|\^\^<[^>]+>)?)\s*'  # object
    r'\.\s*$'
)


def parse_nt_object(tok: str) -> dict:
    """Render an N-Triples object token as the SPARQL-JSON-ish dict shape."""
    if tok.startswith("<") and tok.endswith(">"):
        return {"type": "uri", "value": tok[1:-1]}
    if tok.startswith("_:"):
        return {"type": "bnode", "value": tok[2:]}
    if tok.startswith('"'):
        # Strip optional language / datatype suffixes.
        if tok.endswith(">") and "^^<" in tok:
            value, _, dt = tok.rpartition("^^<")
            return {
                "type": "literal",
                "value": value[1:-1],
                "datatype": dt[:-1],
            }
        if "@" in tok and tok.rfind('"') < tok.rfind("@"):
            value, _, lang = tok.rpartition("@")
            return {"type": "literal", "value": value[1:-1], "xml:lang": lang}
        return {"type": "literal", "value": tok[1:-1]}
    raise ValueError(f"Unparseable N-Triples object token: {tok!r}")


def parse_nt_subject(tok: str) -> dict:
    if tok.startswith("<"):
        return {"type": "uri", "value": tok[1:-1]}
    if tok.startswith("_:"):
        return {"type": "bnode", "value": tok[2:]}
    raise ValueError(f"Unparseable N-Triples subject token: {tok!r}")


def load_nt_file(path: Path) -> list[dict]:
    """Parse a one-line-per-triple N-Triples file."""
    triples: list[dict] = []
    with path.open(encoding="utf-8") as f:
        for raw in f:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            m = NT_LINE_RE.match(line)
            if not m:
                continue
            s_tok, p_tok, o_tok = m.group(1), m.group(2), m.group(3)
            triples.append({
                "s": parse_nt_subject(s_tok),
                "p": {"type": "uri", "value": p_tok[1:-1]},
                "o": parse_nt_object(o_tok),
            })
    return triples


def load_endpoint_triples(endpoint: str) -> list[dict]:
    """Pull every (s, p, o) from a Loka /sparql endpoint."""
    import requests
    resp = requests.post(
        f"{endpoint}/sparql",
        data="SELECT ?s ?p ?o WHERE { ?s ?p ?o }",
        headers={
            "Content-Type": "application/sparql-query",
            "Accept": "application/sparql-results+json",
        },
        timeout=300,
    )
    resp.raise_for_status()
    return resp.json()["results"]["bindings"]


# ─── Indices over the seed graph ───────────────────────────────────────────

def triple_id(t: dict) -> tuple:
    """Hashable identity for a triple — used for dedupe and pool membership."""
    return (
        (t["s"]["type"], t["s"]["value"]),
        (t["p"]["type"], t["p"]["value"]),
        (t["o"]["type"], t["o"]["value"], t["o"].get("xml:lang", ""), t["o"].get("datatype", "")),
    )


def build_indices(triples: list[dict]):
    """Build the indices the BFS-context routines need.

    Returns:
        subj_facts:  s_iri -> [triple, ...]   (only s.type == 'uri')
        obj_back:    o_key -> [triple, ...]   (anything pointing AT this object)
        pred_obj:    (p_iri, o_key) -> count  (for the asymmetric filter)
        subj_pred:   (s_iri, p_iri) -> count
        labels:      iri -> english label
    """
    subj_facts: dict[str, list[dict]] = defaultdict(list)
    obj_back: dict[str, list[dict]] = defaultdict(list)
    pred_obj_count: Counter[tuple[str, str]] = Counter()
    subj_pred_count: Counter[tuple[str, str]] = Counter()
    labels: dict[str, str] = {}
    for t in triples:
        if t["p"]["value"] == RDFS_LABEL and t["o"]["type"] == "literal":
            value, lang = parse_literal(t["o"])
            if (lang or "en") == "en" and t["s"]["type"] == "uri":
                labels[t["s"]["value"]] = value
            continue
        if is_reserved_predicate(t["p"]["value"]):
            continue
        if t["s"]["type"] == "uri":
            subj_facts[t["s"]["value"]].append(t)
            subj_pred_count[(t["s"]["value"], t["p"]["value"])] += 1
        ok = o_key(t["o"])
        obj_back[ok].append(t)
        pred_obj_count[(t["p"]["value"], ok)] += 1
    return subj_facts, obj_back, pred_obj_count, subj_pred_count, labels


# ─── Per-source-triple context construction ────────────────────────────────

def adjacent_context(
    src: dict,
    subj_facts: dict[str, list[dict]],
    obj_back: dict[str, list[dict]],
    pred_obj_count: Counter,
    subj_pred_count: Counter,
    n_pred_obj_max: int,
    n_subj_pred_max: int,
    cap: int,
    hops: int = 2,
) -> tuple[list[dict], list[dict]]:
    """BFS-collect adjacent triples (s-shared, o-shared, object-traversal),
    out to `hops` levels, then run the asymmetry filter and cap.

    1-hop is rarely enough on small seed graphs: a triple <S, P, literal>
    only finds more S-rooted triples that way, all of whose predicates are
    already on S. 2-hop walks into S's URI neighbors and harvests their
    predicate-set, which is what makes "propose a new predicate for S" work.
    """
    seen: set[tuple] = {triple_id(src)}
    raw: list[dict] = []

    frontier_subjects: set[str] = set()
    if src["s"]["type"] == "uri":
        frontier_subjects.add(src["s"]["value"])
    if src["o"]["type"] == "uri":
        frontier_subjects.add(src["o"]["value"])
    src_o_key = o_key(src["o"])
    # Reverse-traversal: subjects whose object-key matches the source's object.
    for t in obj_back.get(src_o_key, []):
        if t["s"]["type"] == "uri":
            frontier_subjects.add(t["s"]["value"])
        tid = triple_id(t)
        if tid not in seen:
            raw.append(t)
            seen.add(tid)

    visited_subjects: set[str] = set()
    for _ in range(max(hops, 1)):
        next_frontier: set[str] = set()
        for s in frontier_subjects:
            if s in visited_subjects:
                continue
            visited_subjects.add(s)
            for t in subj_facts.get(s, []):
                tid = triple_id(t)
                if tid in seen:
                    continue
                seen.add(tid)
                raw.append(t)
                if t["o"]["type"] == "uri":
                    next_frontier.add(t["o"]["value"])
        frontier_subjects = next_frontier

    kept: list[dict] = []
    dropped: list[dict] = []
    for t in raw:
        ok = o_key(t["o"])
        if pred_obj_count[(t["p"]["value"], ok)] > n_pred_obj_max:
            dropped.append(t)
            continue
        if t["s"]["type"] == "uri" and subj_pred_count[(t["s"]["value"], t["p"]["value"])] > n_subj_pred_max:
            dropped.append(t)
            continue
        kept.append(t)
        if len(kept) >= cap:
            break
    return kept, dropped


def predicate_set(
    s_iri: str, subj_facts: dict[str, list[dict]]
) -> tuple[str, ...]:
    return tuple(sorted({t["p"]["value"] for t in subj_facts.get(s_iri, [])}))


def parallel_subgraph_extension(
    context: list[dict],
    subj_facts: dict[str, list[dict]],
    cap_siblings: int = 10,
    min_group_size: int = 3,
) -> list[dict]:
    """Python approximation of pseudotable sibling lookup.

    Group all subjects by their predicate-set; if a context subject is in a
    group with ≥ min_group_size members, sample up to cap_siblings of the
    siblings and return their triples.
    """
    by_pset: dict[tuple, list[str]] = defaultdict(list)
    for s in subj_facts:
        by_pset[predicate_set(s, subj_facts)].append(s)

    extra: list[dict] = []
    seen_subj: set[str] = set()
    for ct in context:
        if ct["s"]["type"] != "uri":
            continue
        s_iri = ct["s"]["value"]
        pset = predicate_set(s_iri, subj_facts)
        group = by_pset.get(pset, [])
        if len(group) < min_group_size:
            continue
        for sib in group:
            if sib == s_iri or sib in seen_subj:
                continue
            seen_subj.add(sib)
            extra.extend(subj_facts[sib])
            if len(seen_subj) >= cap_siblings:
                break
        if len(seen_subj) >= cap_siblings:
            break
    return extra


# ─── Model inference helpers (single-triple, mirrors infer_with_citations) ─

def predict_object(
    model, s_label, p_label, vocab, inv_vocab, tokens_per_role, device,
    per_token_floor=0.05, repetition_penalty=3.0, encode_fn=None,
):
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
    tokens, roles, attention = tokens.to(device), roles.to(device), attention.to(device)
    with torch.no_grad():
        logits = model(tokens, roles, attention)
        probs = F.softmax(logits, dim=-1)
    skip_ids = {PAD_ID, MASK_ID, UNK_ID, CLS_ID, SEP_S_ID, SEP_P_ID, SEP_O_ID}
    out_tokens, out_probs = [], []
    emit_counts: dict[int, int] = {}
    for mp in masked_positions:
        p = probs[0, mp].clone()
        for sid in skip_ids:
            p[sid] = 0
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
    return " ".join(out_tokens), sum(out_probs) / len(out_probs)


def candidate_predicates(
    src_subj_iri: str,
    context: list[dict],
    labels: dict[str, str],
    already_emitted_preds: set[str],
    src_subj_existing_preds: set[str],
) -> list[tuple[str, int]]:
    """Rank predicates by frequency in context, excluding already-on-subject ones."""
    counts: Counter[str] = Counter()
    for t in context:
        p = t["p"]["value"]
        if p == RDFS_LABEL:
            continue
        if is_reserved_predicate(p):
            continue
        if p in src_subj_existing_preds:
            continue
        if p in already_emitted_preds:
            continue
        if p not in labels:
            continue
        counts[p] += 1
    return counts.most_common()


# ─── Main test loop ────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    src = parser.add_mutually_exclusive_group(required=True)
    src.add_argument("--endpoint", help="Loka SPARQL endpoint to read seed from")
    src.add_argument("--seed-file", help="Path to a local N-Triples seed file")
    parser.add_argument("--checkpoint", default=None)
    parser.add_argument("--vocab", default=None)
    parser.add_argument("--bpe-tokenizer", default=None)
    parser.add_argument("--property-cache", default="training/property_label_cache.json")
    parser.add_argument("--output", required=True, help="Path for generated N-Triples")
    parser.add_argument("--max-source-triples", type=int, default=20,
                        help="How many seed triples to generate from")
    parser.add_argument("--children-per-source", type=int, default=10)
    parser.add_argument("--context-cap", type=int, default=10,
                        help="Max adjacent triples kept after asymmetry filter")
    parser.add_argument("--hops", type=int, default=2,
                        help="BFS hops from the source triple when building context")
    parser.add_argument("--prefer-uri-objects", action="store_true", default=True,
                        help="Bias source-triple selection toward URI-object triples (default)")
    parser.add_argument("--n-pred-obj-max", type=int, default=10,
                        help="Drop adjacent triples whose (p, o) appears more often than this")
    parser.add_argument("--n-subj-pred-max", type=int, default=20,
                        help="Drop adjacent triples whose (s, p) appears more often than this")
    parser.add_argument("--max-citations", type=int, default=20)
    parser.add_argument("--confidence", type=float, default=0.4)
    parser.add_argument("--repetition-penalty", type=float, default=3.0)
    parser.add_argument("--simd-cap", type=int, default=10)
    parser.add_argument("--no-simd", action="store_true",
                        help="Disable parallel-subgraph context extension")
    parser.add_argument("--verbose", action="store_true",
                        help="Print per-candidate prediction outcomes")
    parser.add_argument("--device", default=None)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--model-version", default=None)
    args = parser.parse_args()

    import random as _random
    _random.seed(args.seed)
    torch.manual_seed(args.seed)

    if args.device is None:
        args.device = "cuda" if torch.cuda.is_available() else "cpu"
    device = torch.device(args.device)

    # Resolve checkpoint + vocab via MODEL.json if not given.
    if args.checkpoint is None or args.vocab is None:
        from loader import resolve_checkpoint, resolve_vocab, info as pin_info
        if args.checkpoint is None:
            args.checkpoint = str(resolve_checkpoint())
        if args.vocab is None:
            args.vocab = str(resolve_vocab())
        if args.model_version is None:
            args.model_version = pin_info().get("name", Path(args.checkpoint).stem)
    if args.model_version is None:
        args.model_version = Path(args.checkpoint).stem

    vocab = json.loads(Path(args.vocab).read_text(encoding="utf-8"))
    inv_vocab = [""] * len(vocab)
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
    print(
        f"Loaded {args.checkpoint} "
        f"({sum(p.numel() for p in model.parameters()):,} params, "
        f"{ckpt['vocab_size']:,} vocab)",
        file=sys.stderr,
    )

    encode_fn = None
    if args.bpe_tokenizer:
        from tokenizers import Tokenizer  # type: ignore
        bpe_tok = Tokenizer.from_file(args.bpe_tokenizer)
        def encode_fn(text: str) -> list[int]:
            return bpe_tok.encode(text, add_special_tokens=False).ids
        print(f"Using BPE tokenizer: {args.bpe_tokenizer}", file=sys.stderr)

    # ── Load seed graph
    if args.seed_file:
        triples = load_nt_file(Path(args.seed_file))
        print(f"Loaded {len(triples):,} triples from {args.seed_file}", file=sys.stderr)
    else:
        triples = load_endpoint_triples(args.endpoint)
        print(f"Pulled {len(triples):,} triples from {args.endpoint}", file=sys.stderr)

    subj_facts, obj_back, pred_obj_count, subj_pred_count, labels = build_indices(triples)
    print(f"  indexed {len(subj_facts):,} subjects, {len(labels):,} labels", file=sys.stderr)

    # Resolve missing Wikidata property labels via public Wikidata endpoint.
    missing_props = {
        t["p"]["value"]
        for t in triples
        if t["p"]["type"] == "uri"
        and t["p"]["value"] not in labels
        and WIKIDATA_PROP_RE.match(t["p"]["value"])
    }
    if missing_props:
        prop_labels = fetch_wikidata_property_labels(missing_props, Path(args.property_cache))
        labels.update(prop_labels)
        print(f"  resolved {len(prop_labels)} extra property labels", file=sys.stderr)

    # ── Pick source triples
    candidate_sources = [
        t for t in triples
        if t["s"]["type"] == "uri"
        and t["s"]["value"] in labels
        and t["p"]["value"] != RDFS_LABEL
        and not is_reserved_predicate(t["p"]["value"])
    ]
    if args.prefer_uri_objects:
        uri_obj = [t for t in candidate_sources if t["o"]["type"] == "uri"]
        lit_obj = [t for t in candidate_sources if t["o"]["type"] != "uri"]
        _random.shuffle(uri_obj)
        _random.shuffle(lit_obj)
        # 80% URI-object sources, 20% literal-object so we still see how the
        # context routine behaves on both kinds.
        n_uri = int(args.max_source_triples * 0.8)
        sources = uri_obj[:n_uri] + lit_obj[: args.max_source_triples - n_uri]
        _random.shuffle(sources)
    else:
        _random.shuffle(candidate_sources)
        sources = candidate_sources[: args.max_source_triples]
    print(f"\nGenerating from {len(sources)} source triples\n", file=sys.stderr)

    out_lines: list[str] = []
    summary_per_source = []
    total_emitted = 0

    for src_idx, src in enumerate(sources, start=1):
        s_iri = src["s"]["value"]
        s_label = labels.get(s_iri, "")
        p_iri = src["p"]["value"]
        p_label = labels.get(p_iri, p_iri)
        print(
            f"[{src_idx}/{len(sources)}] SRC  {s_label!s} | {p_label!s}",
            file=sys.stderr,
        )

        adj, dropped = adjacent_context(
            src, subj_facts, obj_back, pred_obj_count, subj_pred_count,
            args.n_pred_obj_max, args.n_subj_pred_max, args.context_cap,
            hops=args.hops,
        )
        if args.no_simd:
            simd_extra: list[dict] = []
        else:
            simd_extra = parallel_subgraph_extension(
                adj, subj_facts, cap_siblings=args.simd_cap,
            )
        context_pool: list[dict] = list(adj) + list(simd_extra)
        print(
            f"      ctx: {len(adj)} adj + {len(simd_extra)} simd "
            f"(dropped {len(dropped)} asymmetric)",
            file=sys.stderr,
        )

        existing_preds = {t["p"]["value"] for t in subj_facts.get(s_iri, [])}
        emitted_preds: set[str] = set()
        produced_for_src = 0

        for child_idx in range(args.children_per_source):
            ranked = candidate_predicates(
                s_iri, context_pool, labels, emitted_preds, existing_preds
            )
            if not ranked:
                break

            chosen = None
            for cand_p, _ in ranked:
                p_lbl = labels.get(cand_p, "")
                if not p_lbl:
                    if args.verbose:
                        print(f"        skip (no label): {cand_p}", file=sys.stderr)
                    continue
                res = predict_object(
                    model, s_label, p_lbl, vocab, inv_vocab,
                    tokens_per_role, device,
                    repetition_penalty=args.repetition_penalty,
                    encode_fn=encode_fn,
                )
                if res is None:
                    if args.verbose:
                        print(f"        skip (no prediction): {p_lbl}", file=sys.stderr)
                    continue
                o_label, conf = res
                if conf < args.confidence:
                    if args.verbose:
                        print(f"        skip (conf {conf:.3f}<{args.confidence}): {p_lbl} -> {o_label!r}", file=sys.stderr)
                    continue
                if len(o_label) < 2:
                    if args.verbose:
                        print(f"        skip (too short): {p_lbl} -> {o_label!r}", file=sys.stderr)
                    continue
                # Don't re-emit a label the model already produced for this S/P
                # in the seed.
                existing_objs = {
                    (parse_literal(t["o"])[0] if t["o"]["type"] == "literal"
                     else labels.get(t["o"]["value"], "")).lower()
                    for t in subj_facts.get(s_iri, []) if t["p"]["value"] == cand_p
                }
                if o_label.lower() in existing_objs:
                    if args.verbose:
                        print(f"        skip (dup): {p_lbl} -> {o_label!r}", file=sys.stderr)
                    continue
                chosen = (cand_p, p_lbl, o_label, conf)
                break
            if chosen is None:
                if args.verbose:
                    print(f"      no candidate accepted, stopping for this source", file=sys.stderr)
                break
            cand_p, p_lbl, o_label, conf = chosen
            emitted_preds.add(cand_p)

            s_term = f"<{s_iri}>"
            p_term = f"<{cand_p}>"
            o_term = f'"{escape_literal(o_label)}"'
            new_triple = {
                "s": {"type": "uri", "value": s_iri},
                "p": {"type": "uri", "value": cand_p},
                "o": {"type": "literal", "value": o_label},
            }
            qt = quoted(s_term, p_term, o_term)
            out_lines.append(f"{s_term} {p_term} {o_term} .")
            out_lines.append(f'{qt} <{LOKA_GENERATED}> "true"^^<{XSD_BOOLEAN}> .')
            out_lines.append(f'{qt} <{LOKA_GENERATED_BY}> "{escape_literal(args.model_version)}" .')
            out_lines.append(f'{qt} <{LOKA_CONFIDENCE}> "{conf:.4f}"^^<{XSD_DECIMAL}> .')
            # Citations: every triple in the pool at time of emission, capped.
            citations = context_pool[: args.max_citations]
            for ct in citations:
                cited = quoted(
                    f"<{ct['s']['value']}>" if ct["s"]["type"] == "uri" else f"_:{ct['s']['value']}",
                    f"<{ct['p']['value']}>",
                    fmt_term(ct["o"]),
                )
                out_lines.append(f"{qt} <{LOKA_INFERRED_FROM}> {cited} .")

            print(
                f"      gen#{child_idx + 1}: {p_lbl!s} -> {o_label!s} "
                f"(conf={conf:.3f}, cites={len(citations)})",
                file=sys.stderr,
            )
            produced_for_src += 1
            total_emitted += 1
            # Auto-regressive: extend the pool with the just-emitted triple,
            # so triple #{i+1} sees and may cite triple #{i}.
            context_pool.append(new_triple)

        summary_per_source.append({
            "source_subject": s_iri,
            "source_subject_label": s_label,
            "source_predicate": p_iri,
            "source_predicate_label": p_label,
            "context_adj": len(adj),
            "context_simd": len(simd_extra),
            "context_dropped_asymmetric": len(dropped),
            "children_emitted": produced_for_src,
        })

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(out_lines) + "\n", encoding="utf-8")

    summary = {
        "model_version": args.model_version,
        "checkpoint": args.checkpoint,
        "source": args.endpoint or args.seed_file,
        "source_triples_attempted": len(sources),
        "children_emitted_total": total_emitted,
        "config": {
            "children_per_source": args.children_per_source,
            "context_cap": args.context_cap,
            "n_pred_obj_max": args.n_pred_obj_max,
            "n_subj_pred_max": args.n_subj_pred_max,
            "max_citations": args.max_citations,
            "confidence_threshold": args.confidence,
            "simd_enabled": not args.no_simd,
            "simd_cap": args.simd_cap,
        },
        "per_source": summary_per_source,
    }
    summary_path = out_path.with_suffix(".meta.json")
    summary_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
    print(
        f"\nWrote {total_emitted} generated triples "
        f"({len(out_lines)} N-Triples lines incl. citations) -> {out_path}",
        file=sys.stderr,
    )
    print(f"Summary -> {summary_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
