"""Build web-studio/testdata.nt from the cleaned retrieval graph.

The Studio "Load test data" button needs a small slice of the *normalized*
corpus — the same plain-English, all-literal form the world model is trained
on and the sidecar emits (see tools/retrieval_generate.py): triples are

    "Douglas Adams" "instance of" "human" .

No Wikidata Q/P identifiers, no rdfs:label rows. This script takes a BFS
slice of training/finetune/data/retrieval/graph.nt (which is still in raw
Q-ID form), resolves every term through the project's own label maps
(labels.json for entities + property_label_cache.json for predicates), and
emits the normalized all-literal triples. Re-run after the graph changes:

    python tools/build_testdata.py
"""
from __future__ import annotations
import json
import re
from collections import deque
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RET = ROOT / "training" / "finetune" / "data" / "retrieval"
OUT = ROOT / "web-studio" / "testdata.nt"

SEED = "http://www.wikidata.org/entity/Q42"  # Douglas Adams — validated seed
MAX_TRIPLES = 55

TRIPLE_RE = re.compile(r"^<([^>]+)>\s+<([^>]+)>\s+<([^>]+)>\s*\.\s*$")
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"


def load_labels() -> dict[str, str]:
    labels: dict[str, str] = {}
    for name in ("property_label_cache.json",):  # predicates first
        p = ROOT / "training" / name
        if p.exists():
            labels.update(json.loads(p.read_text(encoding="utf-8")))
    # entity labels (and any predicate labels) override / extend
    lj = RET / "labels.json"
    if lj.exists():
        labels.update(json.loads(lj.read_text(encoding="utf-8")))
    return labels


def escape(v: str) -> str:
    return v.replace("\\", "\\\\").replace('"', '\\"')


def main() -> None:
    labels = load_labels()

    # Parse the raw graph into an adjacency list (skip label rows).
    adj: dict[str, list[tuple[str, str]]] = {}
    for line in (RET / "graph.nt").read_text(encoding="utf-8").splitlines():
        m = TRIPLE_RE.match(line)
        if not m:
            continue
        s, p, o = m.groups()
        if p == RDFS_LABEL:
            continue
        adj.setdefault(s, []).append((p, o))

    # BFS from the seed so the slice is one connected component.
    out: list[str] = []
    seen_triples: set[tuple[str, str, str]] = set()
    visited: set[str] = set()
    q: deque[str] = deque([SEED])
    while q and len(out) < MAX_TRIPLES:
        s = q.popleft()
        if s in visited:
            continue
        visited.add(s)
        for p, o in adj.get(s, []):
            if len(out) >= MAX_TRIPLES:
                break
            sl, pl, ol = labels.get(s), labels.get(p), labels.get(o)
            if not (sl and pl and ol):
                continue  # only emit fully-resolved (truly cleaned) triples
            key = (sl, pl, ol)
            if key in seen_triples:
                continue
            seen_triples.add(key)
            out.append(f'"{escape(sl)}" "{escape(pl)}" "{escape(ol)}" .')
            if o not in visited:
                q.append(o)

    header = (
        "# Loka Studio demo graph — a slice of the NORMALIZED corpus.\n"
        "# Plain-English, all-literal triples (\"subject\" \"predicate\" \"object\"),\n"
        "# the same form the world model trains on and the sidecar emits — no\n"
        "# Wikidata Q/P identifiers, no rdfs:label rows. Built by\n"
        "# tools/build_testdata.py from the cleaned retrieval graph.\n"
    )
    OUT.write_text(header + "\n".join(out) + "\n", encoding="utf-8")
    print(f"wrote {len(out)} normalized triples to {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
