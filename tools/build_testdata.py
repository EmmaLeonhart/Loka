"""Build web-studio/testdata.nt from a slice of the actual training corpus.

The Studio "Load test data" button needs a small, clean, multi-entity graph to
populate the demo. Earlier versions of this script BFS-sliced the Q42 retrieval
graph and capped at one entity's edges, so the result was a single Douglas Adams
star — boring, no traversal, not representative of the data the model trains on.

Instead, slice straight from the *actual* normalized training corpus
(``training/finetune/data/_corpus_v14/triples_normalized.txt``): the same clean,
plain-English, tab-separated ``subject<TAB>predicate<TAB>object`` lines the world
model is trained on. We take a balanced slice — many entities, a per-subject
out-degree cap so no single hub (Belgium, Portugal, …) turns the graph view into
a hairball — and emit it as angle-bracket-IRI N-Triples.

Why IRIs and not literals: Loka's N-Triples parser (loka-core/src/ntriples.rs)
requires the subject to be an IRI/blank/quoted-triple and the predicate to be an
IRI — a literal in either position makes the whole line parse to None and get
silently skipped (that bug shipped an empty demo once). We encode the cleaned
English text AS the IRI (``<Douglas Adams> <occupation> <novelist> .``); the
parser reads up to '>', so spaces are fine. Keeping objects as IRIs too means
every object is a traversable node — the whole point of a graph demo.

Re-run after the corpus changes:

    python tools/build_testdata.py

The corpus file is git-ignored (training/finetune/data/), so on a fresh checkout
pull it first: dataset ``EmmaLeonhart/normalized-wikidata`` tag ``v14-1M``.
"""
from __future__ import annotations

import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "training" / "finetune" / "data" / "_corpus_v14" / "triples_normalized.txt"
OUT = ROOT / "web-studio" / "testdata.nt"

# Slice shape. Tunable — these defaults give a varied, connected, instantly
# loadable graph that renders cleanly in the Studio graph view.
MAX_TRIPLES = 400        # total triples in the demo graph
MAX_PER_SUBJECT = 12     # cap any one hub's out-degree so the view stays legible


def iri(v: str) -> str:
    """Encode cleaned English text as the body of an angle-bracket IRI.

    The parser reads everything up to '>', so spaces are fine; we only neutralise
    characters that would terminate the IRI early or break the N-Triples line.
    """
    return (
        v.replace("<", "(")
        .replace(">", ")")
        .replace("\t", " ")
        .replace("\r", " ")
        .replace("\n", " ")
        .strip()
    )


def main() -> None:
    if not CORPUS.exists():
        sys.exit(
            f"corpus not found: {CORPUS}\n"
            "Pull it from Hugging Face first: dataset "
            "EmmaLeonhart/normalized-wikidata, tag v14-1M."
        )

    per_subject: dict[str, int] = defaultdict(int)
    out: list[str] = []
    seen: set[tuple[str, str, str]] = set()

    with CORPUS.open(encoding="utf-8") as fh:
        for line in fh:
            if len(out) >= MAX_TRIPLES:
                break
            parts = line.rstrip("\r\n").split("\t")
            if len(parts) != 3:
                continue
            s, p, o = (x.strip() for x in parts)
            if not (s and p and o):
                continue
            if per_subject[s] >= MAX_PER_SUBJECT:
                continue
            key = (s, p, o)
            if key in seen:
                continue
            seen.add(key)
            per_subject[s] += 1
            out.append(f"<{iri(s)}> <{iri(p)}> <{iri(o)}> .")

    n_subjects = len(per_subject)
    header = (
        "# Loka Studio demo graph — a slice of the actual v14 training corpus.\n"
        "# Clean, plain-English, multi-entity: the same normalized data the world\n"
        f"# model trains on ({n_subjects} entities, no Wikidata Q/P IDs, no label\n"
        "# rows). The English text IS the identifier. Objects are IRIs too, so\n"
        "# every node is traversable. Built by tools/build_testdata.py from\n"
        "# training/finetune/data/_corpus_v14/triples_normalized.txt — re-run to\n"
        "# regenerate.\n"
    )
    OUT.write_text(header + "\n".join(out) + "\n", encoding="utf-8")
    print(
        f"wrote {len(out)} triples across {n_subjects} entities "
        f"to {OUT.relative_to(ROOT)}"
    )


if __name__ == "__main__":
    main()
