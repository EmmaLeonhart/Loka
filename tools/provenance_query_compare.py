"""Time the same provenance question against two encodings of the same data.

Question, for every entity X in the seed: "which generated triples cite a
stored statement whose object is X?" Asked of
- an RDF-star store (annotation blocks as written by infer_with_citations.py):
    SELECT ?s ?p ?o WHERE {
      << ?s ?p ?o >> prov:propositionInferredFrom << ?cs ?cp X >> }
- a reification + PROV-O store (tools/provenance_encodings.py output):
    SELECT ?s ?p ?o WHERE {
      ?g rdf:subject ?s ; rdf:predicate ?p ; rdf:object ?o ;
         prov:wasDerivedFrom ?c . ?c rdf:object X }

Both servers must hold the same curated seed. Checks that both return the
same set of generated triples for every X, and reports per-query latency
(HTTP included) for each encoding.

Usage:
    python tools/provenance_query_compare.py --star http://127.0.0.1:3035 \\
        --reif http://127.0.0.1:3036 --seed seed_Q42.nt --output out.json
"""
from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path

import requests

INFERRED = "<http://loka.dev/provenance/propositionInferredFrom>"
RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
WDF = "<http://www.w3.org/ns/prov#wasDerivedFrom>"


def run(session, endpoint, query):
    t0 = time.perf_counter()
    r = session.post(f"{endpoint}/sparql", data=query.encode("utf-8"),
                     headers={"Content-Type": "application/sparql-query",
                              "Accept": "application/sparql-results+json"}, timeout=120)
    ms = (time.perf_counter() - t0) * 1000
    r.raise_for_status()
    rows = {(b["s"]["value"], b["p"]["value"], b["o"]["value"])
            for b in r.json()["results"]["bindings"]}
    return ms, rows


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--star", required=True)
    ap.add_argument("--reif", required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--output", required=True)
    args = ap.parse_args()

    roots = sorted({line.split(" ", 1)[0] for line in open(args.seed, encoding="utf-8")
                    if line.startswith("<http")})
    s = requests.Session()
    star_ms, reif_ms, disagree, nonempty = [], [], [], 0
    for x in roots:
        q_star = f"SELECT ?s ?p ?o WHERE {{ << ?s ?p ?o >> {INFERRED} << ?cs ?cp {x} >> }}"
        q_reif = (f"SELECT ?s ?p ?o WHERE {{ ?g <{RDF}subject> ?s . ?g <{RDF}predicate> ?p . "
                  f"?g <{RDF}object> ?o . ?g {WDF} ?c . ?c <{RDF}object> {x} }}")
        a_ms, a = run(s, args.star, q_star)
        b_ms, b = run(s, args.reif, q_reif)
        star_ms.append(a_ms)
        reif_ms.append(b_ms)
        nonempty += bool(a)
        if a != b:
            disagree.append({"x": x, "star": len(a), "reif": len(b)})

    def dist(v):
        v = sorted(v)
        return {"median": round(statistics.median(v), 2),
                "p95": round(v[int(0.95 * (len(v) - 1))], 2), "max": round(v[-1], 2)}

    result = {"roots": len(roots), "roots_with_results": nonempty,
              "disagreements": len(disagree), "disagreement_examples": disagree[:5],
              "rdfstar_ms": dist(star_ms), "reification_ms": dist(reif_ms)}
    Path(args.output).write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
