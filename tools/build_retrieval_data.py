"""Slice 1 data build for the base-model BFS+embedding retrieval.

Input : training/finetune/data/seed_Q42.nt  (real Wikidata, QID/PID-based)
Output: training/finetune/data/retrieval/
  - graph.nt            seed triples + ALL resolved rdfs:label triples
  - vectors_node.jsonl  {subject:<entityIRI>, vector:[...]}   (idx-node-id)
  - vectors_name.jsonl  {subject:<entityIRI>, vector:[...]}   (idx-node-name)
  - vectors_triple.jsonl{subject:"<< <s> <p> <o> >>", vector:[...]} (idx-triple)
  - labels.json         {iri: label} (traceability)

Label resolution reuses preprocess.fetch_wikidata_property_labels for PIDs
and the same Wikidata SPARQL endpoint pattern for entity QIDs (no forked
resolver). Embeddings: sentence-transformers all-MiniLM-L6-v2 (cached,
CPU). The graph keeps real IRIs; labels live as rdfs:label data; the
model only ever sees label text at the retrieval boundary.
"""
from __future__ import annotations
import json, re, sys, time
from pathlib import Path

import requests

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "training"))
from preprocess import (  # noqa: E402
    fetch_wikidata_property_labels, WIKIDATA_SPARQL,
)

SEED = ROOT / "training" / "finetune" / "data" / "seed_Q42.nt"
OUT = ROOT / "training" / "finetune" / "data" / "retrieval"
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"
NT = re.compile(r'^<([^>]+)>\s+<([^>]+)>\s+(?:<([^>]+)>|"((?:[^"\\]|\\.)*)"(?:@\w+)?(?:\^\^<[^>]+>)?)\s*\.\s*$')
QID = re.compile(r'^http://www\.wikidata\.org/entity/Q\d+$')


def parse_seed():
    triples, infile_labels = [], {}
    for line in SEED.read_text(encoding="utf-8").splitlines():
        m = NT.match(line.strip())
        if not m:
            continue
        s, p, o_iri, o_lit = m.groups()
        if p == RDFS_LABEL and o_lit is not None:
            infile_labels[s] = o_lit
            continue
        o = ("uri", o_iri) if o_iri is not None else ("lit", o_lit)
        triples.append((s, p, o))
    return triples, infile_labels


def fetch_entity_labels(qids, batch=120, pause=0.4):
    """Batch-resolve entity QIDs -> English label via the SAME Wikidata
    SPARQL endpoint preprocess uses for properties (consistent, not forked)."""
    out, qids = {}, sorted(qids)
    for i in range(0, len(qids), batch):
        chunk = qids[i:i + batch]
        values = " ".join(f"wd:{q.rsplit('/',1)[-1]}" for q in chunk)
        q = (f"SELECT ?e ?l WHERE {{ VALUES ?e {{ {values} }} "
             f"?e rdfs:label ?l . FILTER(LANG(?l)='en') }}")
        try:
            r = requests.get(WIKIDATA_SPARQL,
                             params={"query": q, "format": "json"},
                             headers={"User-Agent": "Loka-Retrieval/0.1"},
                             timeout=90)
            r.raise_for_status()
            for b in r.json()["results"]["bindings"]:
                out[b["e"]["value"]] = b["l"]["value"]
        except Exception as e:  # noqa: BLE001
            print(f"  entity batch {i//batch} failed: {e}", file=sys.stderr)
        print(f"  entities {min(i+batch,len(qids))}/{len(qids)} "
              f"({len(out)} resolved)", file=sys.stderr, flush=True)
        time.sleep(pause)
    return out


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    triples, labels = parse_seed()
    print(f"{len(triples)} non-label triples, {len(labels)} in-file labels",
          file=sys.stderr)

    ents = {t[0] for t in triples} | {o[1] for _, _, o in
                                      ((s, p, o) for s, p, o in triples)
                                      if o[0] == "uri"}
    preds = {p for _, p, _ in triples}
    need_ent = sorted(e for e in ents if QID.match(e) and e not in labels)
    print(f"resolving {len(need_ent)} entity labels via Wikidata…",
          file=sys.stderr)
    labels.update(fetch_entity_labels(need_ent))

    print(f"resolving {len(preds)} predicate labels…", file=sys.stderr)
    plabels = fetch_wikidata_property_labels(
        {p for p in preds if p not in labels},
        ROOT / "training" / "property_label_cache.json")
    labels.update(plabels)

    def lab(iri):
        return labels.get(iri) or iri.rsplit("/", 1)[-1]

    # graph.nt = seed triples + every resolved rdfs:label (real IRIs kept)
    g = []
    for s, p, o in triples:
        if o[0] == "uri":
            g.append(f"<{s}> <{p}> <{o[1]}> .")
        else:
            g.append(f'<{s}> <{p}> "{o[1]}" .')
    for iri, l in labels.items():
        esc = l.replace("\\", "\\\\").replace('"', '\\"')
        g.append(f'<{iri}> <{RDFS_LABEL}> "{esc}" .')
    (OUT / "graph.nt").write_text("\n".join(g) + "\n", encoding="utf-8")

    # embeddings
    from sentence_transformers import SentenceTransformer
    enc = SentenceTransformer("sentence-transformers/all-MiniLM-L6-v2",
                              device="cpu")
    nodes = sorted({s for s, _, _ in triples} |
                   {o[1] for _, _, o in triples if o[0] == "uri"})
    node_vecs = enc.encode([lab(n) for n in nodes], batch_size=64,
                            normalize_embeddings=True)
    with (OUT / "vectors_node.jsonl").open("w", encoding="utf-8") as fn, \
         (OUT / "vectors_name.jsonl").open("w", encoding="utf-8") as fm:
        for n, v in zip(nodes, node_vecs):
            row = json.dumps({"subject": n, "vector": [round(float(x), 6) for x in v]})
            fn.write(row + "\n")
            fm.write(row + "\n")  # same encoder; separate predicate/index

    with (OUT / "vectors_triple.jsonl").open("w", encoding="utf-8") as ft:
        texts, keys = [], []
        for s, p, o in triples:
            ov = o[1] if o[0] == "uri" else o[1]
            keys.append((s, p, o))
            texts.append(f"{lab(s)} {lab(p)} {lab(ov) if o[0]=='uri' else ov}")
        tv = enc.encode(texts, batch_size=64, normalize_embeddings=True)
        for (s, p, o), v in zip(keys, tv):
            obj = f"<{o[1]}>" if o[0] == "uri" else f'"{o[1]}"'
            subj = f"<< <{s}> <{p}> {obj} >>"
            ft.write(json.dumps({"subject": subj,
                                 "vector": [round(float(x), 6) for x in v]}) + "\n")

    (OUT / "labels.json").write_text(json.dumps(labels), encoding="utf-8")
    print(f"DONE: {len(nodes)} node vecs, {len(triples)} triple vecs, "
          f"{len(labels)} labels, graph.nt {len(g)} lines", file=sys.stderr)


if __name__ == "__main__":
    main()
