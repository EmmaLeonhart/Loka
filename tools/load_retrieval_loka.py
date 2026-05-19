"""Slice 1 loader: push the retrieval dataset into a running Loka.

Loads training/finetune/data/retrieval/ into the Loka endpoint:
  - graph.nt           -> POST /triples
  - declare 3 vector predicates (nodeEmb / nameEmb / tripleEmb), 384-d cosine
  - vectors_node/name/triple.jsonl -> POST /vectors  (triple vectors use the
    quoted-triple subject the engine now supports)
Then sanity-checks each index with a VECTOR_SIMILAR query.

Run a fresh server first, e.g.:
  target/release/loka.exe serve --data-dir loka-retrieval --port 3031
then:
  python tools/load_retrieval_loka.py --endpoint http://localhost:3031
"""
from __future__ import annotations
import argparse, json, sys
from pathlib import Path
import requests

ROOT = Path(__file__).resolve().parent.parent
RET = ROOT / "training" / "finetune" / "data" / "retrieval"
NODE_EMB = "http://loka.dev/retrieval/nodeEmb"
NAME_EMB = "http://loka.dev/retrieval/nameEmb"
TRIPLE_EMB = "http://loka.dev/retrieval/tripleEmb"
DIM = 384


def declare(ep, pred):
    r = requests.post(f"{ep}/vectors/declare", json={
        "predicate": pred, "dimensions": DIM, "m": 16,
        "ef_construction": 200, "metric": "cosine"}, timeout=30)
    print(f"  declare {pred.rsplit('/',1)[-1]}: {r.status_code} {r.text[:120]}",
          file=sys.stderr)


def post_vectors(ep, pred, jsonl, label):
    n, errs = 0, 0
    with (RET / jsonl).open(encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            r = requests.post(f"{ep}/vectors", json={
                "predicate": pred, "subject": row["subject"],
                "vector": row["vector"]}, timeout=30)
            n += 1
            if r.status_code != 200:
                errs += 1
                if errs <= 3:
                    print(f"  ! {label} {r.status_code}: {r.text[:160]}",
                          file=sys.stderr)
            if n % 500 == 0:
                print(f"  {label}: {n} posted ({errs} err)", file=sys.stderr,
                      flush=True)
    print(f"  {label}: {n} posted, {errs} errors", file=sys.stderr)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--endpoint", default="http://localhost:3031")
    args = ap.parse_args()
    ep = args.endpoint.rstrip("/")

    requests.get(f"{ep}/health", timeout=10).raise_for_status()

    nt = (RET / "graph.nt").read_text(encoding="utf-8")
    r = requests.post(f"{ep}/triples", data=nt.encode("utf-8"),
                      headers={"Content-Type": "text/plain; charset=utf-8"},
                      timeout=300)
    j = r.json() if r.status_code == 200 else {}
    print(f"graph.nt -> inserted {j.get('inserted','?')} "
          f"errors {len(j.get('errors',[]))}", file=sys.stderr)

    for p in (NODE_EMB, NAME_EMB, TRIPLE_EMB):
        declare(ep, p)
    post_vectors(ep, NODE_EMB, "vectors_node.jsonl", "nodeEmb")
    post_vectors(ep, NAME_EMB, "vectors_name.jsonl", "nameEmb")
    post_vectors(ep, TRIPLE_EMB, "vectors_triple.jsonl", "tripleEmb")

    # sanity: each index returns something for a query vector taken from
    # its own first row (a vector is always most-similar to itself).
    for pred, jsonl, lbl in ((NODE_EMB, "vectors_node.jsonl", "nodeEmb"),
                             (TRIPLE_EMB, "vectors_triple.jsonl", "tripleEmb")):
        first = json.loads((RET / jsonl).open(encoding="utf-8").readline())
        vec = " ".join(f"{x:.6f}" for x in first["vector"])
        q = (f'SELECT ?x WHERE {{ VECTOR_SIMILAR(?x <{pred}> '
             f'"{vec}"^^<http://loka.dev/f32vec>, 0.8, k:=3) }}')
        rr = requests.post(f"{ep}/sparql", data=q,
                           headers={"Content-Type": "application/sparql-query",
                                    "Accept": "application/sparql-results+json"},
                           timeout=60)
        b = rr.json().get("results", {}).get("bindings", []) if rr.ok else []
        print(f"sanity {lbl}: {len(b)} hits; top={b[0]['x']['value'][:80] if b else 'NONE'}",
              file=sys.stderr)


if __name__ == "__main__":
    main()
