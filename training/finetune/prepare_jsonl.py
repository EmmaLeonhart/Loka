"""Build SFT JSONL for the fine-tuning track from a Loka triplestore.

For each triple (S, P, O) found via SPARQL, this emits one masked-triple
training example with the format described in `planning/fine-tuning-track.md`:

    {
      "context": [["...","...","..."], ...],   # up to K co-subject triples
      "target":  ["Tokyo", "population", "?"], # the row with one slot masked
      "answer":  "13929286",                    # the masked value
      "slot":    "object"                       # which slot was masked
    }

Context comes from same-subject triples (S = the masked triple's subject),
because masking the object of "Tokyo | population | ?" is most usefully
conditioned on other facts about Tokyo, not unrelated facts.

This script does NOT do training — it produces the dataset that `finetune.py`
consumes. It is safe to re-run; the JSONL is overwritten each invocation.

Usage:

    python training/finetune/prepare_jsonl.py \
        --endpoint http://localhost:3030 \
        --output training/finetune/data/sft_v1.jsonl \
        --max-rows 200000 \
        --context-k 5 \
        --mask object

Provenance: model-generated rows (predicate IRI under
`http://loka.dev/provenance/`) are excluded — same policy as
`training/preprocess.py`.
"""
from __future__ import annotations

import argparse
import json
import random
import sys
from collections import defaultdict
from pathlib import Path

import requests

USER_AGENT = "Loka-Finetune/0.1 (https://github.com/EmmaLeonhart/Loka)"
RESERVED_PROVENANCE_PREFIX = "http://loka.dev/provenance/"

# Use the same training-corpus SPARQL as from-scratch: exclude generated rows.
# We pull subject/predicate/object as plain bindings; label substitution is
# expected to have already happened upstream via training/preprocess.py
# (which writes triples to the Loka instance pointed at by --endpoint).
# This script assumes the endpoint contains labelled triples.
SPARQL_QUERY = """
SELECT ?s ?p ?o WHERE {
  ?s ?p ?o .
  FILTER(!STRSTARTS(STR(?p), "http://loka.dev/provenance/"))
}
"""


def fetch_triples(endpoint: str, page_size: int = 100_000) -> list[tuple[str, str, str]]:
    """Page through the Loka SPARQL endpoint, returning (S, P, O) triples.

    Pagination mirrors training/preprocess.py: 100k-row pages with a generous
    per-page timeout. sled iteration is byte-order stable so unordered pages
    are safe.
    """
    triples: list[tuple[str, str, str]] = []
    offset = 0
    while True:
        paged = f"{SPARQL_QUERY} LIMIT {page_size} OFFSET {offset}"
        resp = requests.post(
            f"{endpoint}/sparql",
            data=paged,
            headers={
                "Content-Type": "application/sparql-query",
                "Accept": "application/sparql-results+json",
                "User-Agent": USER_AGENT,
            },
            timeout=900,
        )
        resp.raise_for_status()
        page = resp.json()["results"]["bindings"]
        if not page:
            break
        for row in page:
            s = row["s"]["value"]
            p = row["p"]["value"]
            o = row["o"]["value"]
            triples.append((s, p, o))
        print(
            f"  page {offset // page_size + 1}: {len(page):,} rows "
            f"(total {len(triples):,})",
            file=sys.stderr,
            flush=True,
        )
        if len(page) < page_size:
            break
        offset += page_size
    return triples


def build_sft_examples(
    triples: list[tuple[str, str, str]],
    context_k: int,
    mask: str,
    rng: random.Random,
) -> list[dict]:
    """Group by subject, then emit one masked-triple example per row.

    `mask` is one of "object" / "subject" / "predicate" / "random".
    `context_k` controls how many same-subject rows are sampled as context
    for each masked row (the masked row itself is excluded from its own
    context).
    """
    by_subject: dict[str, list[tuple[str, str, str]]] = defaultdict(list)
    for t in triples:
        by_subject[t[0]].append(t)

    examples: list[dict] = []
    for subject, rows in by_subject.items():
        for masked_idx, (s, p, o) in enumerate(rows):
            slot = mask if mask != "random" else rng.choice(
                ["object", "subject", "predicate"]
            )
            if slot == "object":
                target = [s, p, "?"]
                answer = o
            elif slot == "subject":
                target = ["?", p, o]
                answer = s
            else:  # predicate
                target = [s, "?", o]
                answer = p

            pool = [rows[i] for i in range(len(rows)) if i != masked_idx]
            if len(pool) > context_k:
                context = rng.sample(pool, context_k)
            else:
                context = pool
            context = [list(c) for c in context]

            examples.append(
                {
                    "context": context,
                    "target": target,
                    "answer": answer,
                    "slot": slot,
                }
            )
    return examples


def write_jsonl(examples: list[dict], output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", encoding="utf-8") as f:
        for ex in examples:
            f.write(json.dumps(ex, ensure_ascii=False))
            f.write("\n")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--endpoint",
        help="Loka SPARQL endpoint, e.g. http://localhost:3030 (legacy Loka-source path). "
        "Not required when --fixture is given — v11+ uses the normalized-wikidata TSV corpus.",
    )
    ap.add_argument("--output", required=True, type=Path, help="JSONL output path")
    ap.add_argument("--max-rows", type=int, default=0, help="Cap total examples (0 = no cap)")
    ap.add_argument("--context-k", type=int, default=5, help="Number of same-subject context rows per example")
    ap.add_argument("--mask", choices=["object", "subject", "predicate", "random"], default="object")
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument(
        "--fixture",
        type=Path,
        help="Read tab-separated subject\\tpredicate\\tobject triples from this file "
        "instead of hitting --endpoint. This is the current-pipeline path: point it at "
        "a normalized-wikidata corpus (e.g. training/data/triples_v14.txt).",
    )
    args = ap.parse_args()

    if not args.endpoint and not args.fixture:
        ap.error("one of --fixture (normalized-wikidata TSV) or --endpoint (legacy Loka) is required")

    rng = random.Random(args.seed)

    if args.fixture:
        triples: list[tuple[str, str, str]] = []
        for line in args.fixture.read_text(encoding="utf-8").splitlines():
            parts = line.split("\t")
            if len(parts) == 3:
                triples.append((parts[0], parts[1], parts[2]))
        print(f"loaded {len(triples):,} triples from fixture {args.fixture}", file=sys.stderr)
    else:
        print(f"fetching from {args.endpoint} ...", file=sys.stderr)
        triples = fetch_triples(args.endpoint)
        print(f"fetched {len(triples):,} triples", file=sys.stderr)

    examples = build_sft_examples(triples, args.context_k, args.mask, rng)
    if args.max_rows and len(examples) > args.max_rows:
        rng.shuffle(examples)
        examples = examples[: args.max_rows]
    print(f"emitting {len(examples):,} SFT examples", file=sys.stderr)

    write_jsonl(examples, args.output)
    print(f"wrote {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
