"""Slice 2: Emma's BFS + embedding retrieval against Loka.

Given a start node, assemble a relevance-ranked sequence of label-rendered
triples to feed the base model:

  1. BFS from N: every triple on N (out + in), then expand to adjacent
     nodes for `--hops` more levels (graph adjacency).
  2. Embedding expansion: VECTOR_SIMILAR on nodeEmb (nodes near N) and on
     tripleEmb (triples near N's triples) — pulls in related material the
     pure graph walk misses.
  3. Score every gathered triple: relatedness = w_graph * (1/(1+depth))
     + w_vec * sim. Order LEAST -> MOST related (most-related last: the
     base model weights the tail of the prompt most).
  4. Render each triple with rdfs:label so the model sees text, not QIDs.

Reusable: `retrieve(endpoint, node, ...) -> list[(s_lab,p_lab,o_lab)]`.
"""
from __future__ import annotations
import argparse, sys
from pathlib import Path
import requests

ROOT = Path(__file__).resolve().parent.parent
NODE_EMB = "http://loka.dev/retrieval/nodeEmb"
TRIPLE_EMB = "http://loka.dev/retrieval/tripleEmb"
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"
_ENC = None
# Prebuilt {iri: label} from build_retrieval_data.py — used for rendering so
# we never do an N+1 SPARQL round-trip per IRI (that timed out at 200 s).
import json as _json  # noqa: E402
_LABELS = {}
_lp = ROOT / "training" / "finetune" / "data" / "retrieval" / "labels.json"
if _lp.exists():
    _LABELS = _json.loads(_lp.read_text(encoding="utf-8"))


def _encoder():
    global _ENC
    if _ENC is None:
        from sentence_transformers import SentenceTransformer
        _ENC = SentenceTransformer("sentence-transformers/all-MiniLM-L6-v2",
                                    device="cpu")
    return _ENC


import sys as _sys, time as _time  # noqa: E402
_NQ = [0]


def _sparql(ep, q, timeout=20):
    _NQ[0] += 1
    t = _time.time()
    r = requests.post(f"{ep}/sparql", data=q,
                      headers={"Content-Type": "application/sparql-query",
                               "Accept": "application/sparql-results+json"},
                      timeout=timeout)
    dt = _time.time() - t
    if dt > 2.0:
        print(f"  [slow {dt:.1f}s q#{_NQ[0]}] {q[:90]}", file=_sys.stderr,
              flush=True)
    r.raise_for_status()
    return r.json().get("results", {}).get("bindings", [])


def _label(ep, iri, cache=None):
    """Render an IRI as text via the prebuilt label map — no SPARQL (the
    per-IRI query was the N+1 that timed out). Literals pass through."""
    if not iri.startswith("http"):
        return iri
    return _LABELS.get(iri) or iri.rsplit("/", 1)[-1].rsplit("#", 1)[-1]


def retrieve(endpoint, node, hops=2, vec_k=8, budget=60):
    ep = endpoint.rstrip("/")
    _t0 = _time.time()

    def _mark(phase):
        print(f"  [retrieve] {phase} @ {_time.time()-_t0:.1f}s "
              f"({_NQ[0]} queries)", file=_sys.stderr, flush=True)
    _mark("start")
    cache = {}
    # gathered[(s,p,o)] = best relatedness score
    gathered = {}

    def add(s, p, o, score):
        k = (s, p, o)
        if k not in gathered or score > gathered[k]:
            gathered[k] = score

    # 1. graph BFS (depth 0..hops); closer depth = more related
    frontier, seen = [node], {node}
    for depth in range(hops + 1):
        nxt = []
        gw = 1.0 / (1.0 + depth)
        for nd in frontier:
            for b in _sparql(ep, f'SELECT ?p ?o WHERE {{ <{nd}> ?p ?o }} LIMIT 60'):
                p, o = b["p"]["value"], b["o"]["value"]
                if p == RDFS_LABEL:
                    continue
                add(nd, p, o, 0.5 + 0.5 * gw)
                if b["o"]["type"] == "uri" and o not in seen and depth < hops:
                    seen.add(o); nxt.append(o)
            for b in _sparql(ep, f'SELECT ?s ?p WHERE {{ ?s ?p <{nd}> }} LIMIT 40'):
                s, p = b["s"]["value"], b["p"]["value"]
                if p == RDFS_LABEL:
                    continue
                add(s, p, nd, 0.5 + 0.5 * gw)
                if s not in seen and depth < hops:
                    seen.add(s); nxt.append(s)
        # Hard-bound the next BFS level: a hub like Q42 has hundreds of
        # neighbours and expanding all of them was the O(N) round-trip
        # blow-up that timed out. 12/level keeps it to a few-second walk.
        frontier = nxt[:12]
    _mark(f"BFS done ({len(gathered)} triples)")

    # 2. embedding expansion from N's label
    qv = _encoder().encode([_label(ep, node, cache)],
                           normalize_embeddings=True)[0]
    _mark("encoder loaded+encoded")
    vstr = " ".join(f"{x:.6f}" for x in qv)
    # 2a. nodes similar to N -> pull their direct triples. VECTOR_SIMILAR
    # can bind the same ?n many times (multiple subjects share a vector
    # object); dedupe + cap or we re-query the same node 3x at ~2s each
    # (that was the timeout).
    sim_nodes = []
    for b in _sparql(ep, f'SELECT ?n WHERE {{ VECTOR_SIMILAR(?n <{NODE_EMB}> '
                          f'"{vstr}"^^<http://loka.dev/f32vec>, 0.4, k:={vec_k}) }}'):
        v = b["n"]["value"]
        if v not in sim_nodes:
            sim_nodes.append(v)
    for sim_n in sim_nodes[:5]:
        for r in _sparql(ep, f'SELECT ?p ?o WHERE {{ <{sim_n}> ?p ?o }} LIMIT 12'):
            if r["p"]["value"] == RDFS_LABEL:
                continue
            add(sim_n, r["p"]["value"], r["o"]["value"], 0.6)
    # 2b. triples similar to N's neighbourhood (idx-triple)
    for b in _sparql(ep, f'SELECT ?t WHERE {{ VECTOR_SIMILAR(?t <{TRIPLE_EMB}> '
                          f'"{vstr}"^^<http://loka.dev/f32vec>, 0.4, k:={vec_k}) }}'):
        t = b["t"]["value"]
        if t.startswith("<<") and t.endswith(">>"):
            inner = t[2:-2].strip()
            parts = inner.split(None, 2)
            if len(parts) == 3:
                s = parts[0].strip("<>"); p = parts[1].strip("<>")
                o = parts[2].strip().strip("<>").strip('"')
                add(s, p, o, 0.7)

    _mark(f"vec done ({len(gathered)} triples total)")
    ranked = sorted(gathered.items(), key=lambda kv: kv[1])[-budget:]
    out = []
    for (s, p, o), _ in ranked:  # least -> most related
        out.append((_label(ep, s, cache), _label(ep, p, cache),
                    _label(ep, o, cache) if o.startswith("http") else o))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--endpoint", default="http://localhost:3031")
    ap.add_argument("--node", required=True, help="start node IRI")
    ap.add_argument("--hops", type=int, default=2)
    ap.add_argument("--budget", type=int, default=60)
    args = ap.parse_args()
    seq = retrieve(args.endpoint, args.node, hops=args.hops, budget=args.budget)
    for s, p, o in seq:
        print(f"{s} | {p} | {o}")
    print(f"\n[{len(seq)} triples, least->most related]", file=sys.stderr)


if __name__ == "__main__":
    main()
