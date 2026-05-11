# PageRank-driven bootstrap prioritisation

A design note for using PageRank over the entity graph to focus corpus
expansion and propgen-test source selection on the *most-connected* entities,
rather than treating all Wikidata entities as equally interesting.

## The intuition

The current v7-v8 corpus is built by BFS expansion from a starting QID, with
no signal about which entities are structurally important. Pulling more data
without a prioritisation signal means we pay ingest cost evenly across (a)
the kinds of entities that anchor the graph (countries, languages, common
relations, well-known people) and (b) the long tail of rarely-referenced
nodes (specific local administrative regions, niche professional roles, etc).
For a model that has to fit a fixed parameter budget, learning the anchors
well matters more — they appear in subject *or object* slot of many more
training triples than tail nodes do.

PageRank captures this directly: high-rank entities are the ones cited by
many other entities, weighted by the importance of the citers. Using
PageRank to bias bootstrap means:

- **At ingest time:** prefer pulling neighborhoods of high-rank entities.
- **At propgen test time:** prefer sourcing generation from high-rank subjects
  so the test exercises the part of the graph the model should know well.

## Where this plugs into the pipeline

```
┌─────────────────────┐
│  Wikidata HF stream │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  tools/wikidata_hf_import.py        │  ← optional --rank-file flag:
│  imports into Loka, with PageRank-  │    fetch the qrank dump from
│  weighted prioritisation if a       │    qrank.toolforge.org once,
│  rank file is supplied              │    pass as a CSV of QID,rank
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  tools/compute_pagerank.py          │  ← post-import: compute
│  computes PageRank over the         │    PageRank over what's
│  current Loka triples, writes       │    actually in our store,
│  ranks.json (entity_iri -> score)   │    independent of qrank
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  training/preprocess.py             │  (no change — operates on all
│  builds triples_vN.txt              │   triples; ranks live alongside)
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  training/test_autoregressive_      │  ← optional --rank-file flag:
│   propgen.py                        │    bias source-triple selection
│  uses ranks.json to pick sources    │    by entity rank when present
└─────────────────────────────────────┘
```

## Two complementary signals

| Source | Provenance | Cost | Coverage |
|---|---|---|---|
| **qrank** (external) | `qrank.toolforge.org` — pre-computed PageRank-like score over all of Wikidata, weekly refresh. | One ~20 MB CSV download. | Full Wikidata (~100M entities). Used *before* ingest to prioritise pulling. |
| **Local PageRank** (internal) | Computed by `tools/compute_pagerank.py` over whatever's currently in Loka. | NetworkX iterative PageRank; runs in seconds on ≤1M-triple corpora, in minutes on 10M+. | Just our corpus. Used *after* ingest to bias evaluation and downstream tooling. |

Use both: qrank as a pre-filter at ingest, local PageRank as a post-ingest
signal that reflects the *actual* corpus structure we built. They will
differ — qrank covers the long tail we *didn't* pull, while local PageRank
reflects the entities our particular BFS happened to anchor on.

## What `compute_pagerank.py` does

```
python tools/compute_pagerank.py \
    --endpoint http://localhost:3030 \
    --output training/data/pagerank.json \
    --top-k 1000 \
    --damping 0.85
```

- Pull all `?s ?p ?o` triples from a Loka instance (or read an `.nt` file).
- Build a directed graph: `s --p--> o` for every triple where both `s` and
  `o` are URIs. Predicate is an edge attribute but doesn't influence the
  PageRank computation in v1.
- Compute PageRank with NetworkX (default damping 0.85, max iter 100, tol 1e-6).
- Write `{ "computed_at": ..., "n_nodes": ..., "n_edges": ...,
  "scores": [ [iri, score], ... ] }` sorted descending by score.
- Optionally also write back to Loka as RDF-star annotations:
  `<<E>> loka:pagerank "0.0042"^^xsd:decimal .` — auditable like any other
  generated triple. (Gated on `--write-back`.)

## What `test_autoregressive_propgen.py` does with it

When `--rank-file ranks.json` is passed:

1. Load the score map.
2. When picking source triples, weight each candidate triple by the geometric
   mean of its subject and object PageRank (literals contribute baseline 1).
3. Optionally also use the rank as a tiebreaker in `candidate_pairs` (so the
   model is asked first about higher-rank subjects).

Without the flag, behaviour is unchanged from current uniform-random
selection. The flag is purely additive.

## What's intentionally out of scope for v1

- **Predicate-weighted PageRank.** A weighted variant where edges via
  `instance of` count more than edges via, say, `commons category` would
  better capture "structural" importance. Worth doing later; the baseline
  unweighted version is already much better than uniform sampling.
- **Personalised PageRank.** With a seed of "important to the user" nodes,
  the resulting rank biases toward their neighborhoods. Useful for
  domain-specific corpora; not needed for the generic Wikidata bootstrap.
- **Bidirectional PageRank** (running over the reverse graph and combining).
  Forward-only is the right default for "what does the rest of the graph
  point at."
- **Live PageRank in Loka.** Computing PageRank inside the engine would be a
  proper SPARQL extension (`pagerank()` builtin). Out of scope until the
  Python tool's value is demonstrated.

## How this interacts with the asymmetric filter

The asymmetric-cardinality filter in `test_autoregressive_propgen.py` already
drops triples whose `(p, o)` cardinality is high (the "5,000 shrines worship
Amaterasu" pattern). PageRank is orthogonal: a high-rank object (e.g. France,
Q142) may have many subjects pointing at it, *and* be a structurally
important node. The asymmetric filter trims it from per-source *context*; the
PageRank bias still uses it for *source selection* because we want generation
test results on the high-rank parts of the graph.
