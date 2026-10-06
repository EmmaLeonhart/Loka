"""Re-encode a generated N-Triples-star file three ways and count the cost.

Input: the output of `training/infer_with_citations.py` (RDF-star annotation
blocks). For every generated triple G with metadata rows and k cited source
statements, it writes:

- rdfstar: the input as is: G plus one annotation row per metadata item and
  per citation, each with the quoted G as subject.
- reification: G asserted, plus an rdf:Statement node for G (4 rows) carrying
  the metadata and one prov:wasDerivedFrom per citation pointing at an
  rdf:Statement node for the source (4 rows per distinct source, shared).
- namedgraph (N-Quads): G inside its own named graph g, the metadata and
  prov:wasDerivedFrom on g; cited sources live in the default graph and can
  only be pointed at through a reification node, as above.

Prints rows (triples or quads) and bytes per encoding, and writes the files
so the triple-based ones can be loaded and queried.

Usage:
    python tools/provenance_encodings.py generated.nt OUTDIR
"""
from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

PROV = "http://loka.dev/provenance/"
INFERRED = f"<{PROV}propositionInferredFrom>"
RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
W3PROV = "http://www.w3.org/ns/prov#"
QUOTED = re.compile(r"<< (\S+) (\S+) (.+?) >>")


def node(prefix: str, s: str, p: str, o: str) -> str:
    h = hashlib.sha1(f"{s} {p} {o}".encode("utf-8")).hexdigest()[:16]
    return f"<http://loka.dev/{prefix}/{h}>"


def reify(stmt_node: str, s: str, p: str, o: str) -> list[str]:
    return [
        f"{stmt_node} <{RDF}type> <{RDF}Statement> .",
        f"{stmt_node} <{RDF}subject> {s} .",
        f"{stmt_node} <{RDF}predicate> {p} .",
        f"{stmt_node} <{RDF}object> {o} .",
    ]


def main() -> None:
    src, outdir = sys.argv[1], Path(sys.argv[2])
    outdir.mkdir(parents=True, exist_ok=True)
    asserted, meta, cites = [], {}, {}
    for line in open(src, encoding="utf-8"):
        line = line.rstrip("\n")
        if not line:
            continue
        if not line.startswith("<<"):
            asserted.append(line)
            continue
        q = QUOTED.findall(line)
        g = q[0]
        if INFERRED in line:
            cites.setdefault(g, []).append(q[1])
        else:
            rest = line[line.index(">>") + 2:].strip()
            meta.setdefault(g, []).append(rest[:-1].strip())  # "<pred> obj"

    gens = list(meta)
    rdfstar = [ln for ln in open(src, encoding="utf-8").read().splitlines() if ln]

    reif, sources = list(asserted), set()
    for g in gens:
        gn = node("stmt", *g)
        reif += reify(gn, *g)
        reif += [f"{gn} {m} ." for m in meta[g]]
        for c in cites.get(g, []):
            cn = node("stmt", *c)
            reif.append(f"{gn} <{W3PROV}wasDerivedFrom> {cn} .")
            if c not in sources:
                sources.add(c)
                reif += reify(cn, *c)

    quads, qsources = [], set()
    for g in gens:
        gg = node("graph", *g)
        quads.append(f"{g[0]} {g[1]} {g[2]} {gg} .")
        quads += [f"{gg} {m} ." for m in meta[g]]
        for c in cites.get(g, []):
            cn = node("stmt", *c)
            quads.append(f"{gg} <{W3PROV}wasDerivedFrom> {cn} .")
            if c not in qsources:
                qsources.add(c)
                quads += reify(cn, *c)

    out = {}
    for name, rows, ext in (("rdfstar", rdfstar, "nt"), ("reification", reif, "nt"),
                            ("namedgraph", quads, "nq")):
        text = "\n".join(rows) + "\n"
        (outdir / f"{name}.{ext}").write_text(text, encoding="utf-8")
        out[name] = {"rows": len(rows), "bytes": len(text.encode("utf-8"))}
    out["generated_triples"] = len(gens)
    out["citations"] = sum(len(v) for v in cites.values())
    out["distinct_cited_sources"] = len(sources)
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
