# Cost-based choice between HNSW and graph-first (Phase 4)

Phase 4 of `planning/large-features.md`. Written before the code.

## What happens now

- **Planner.** `VECTOR_SIMILAR` with an unbound subject has weight 1 and cardinality 1, so it
  runs first unless a pattern is fully bound. That holds whatever the data looks like:
  `?s a ex:Rare . VECTOR_SIMILAR(?s ...)` with 5 `ex:Rare` subjects still starts with an HNSW
  beam search (default k = ef = 500). The planner never sees the vector index: the server calls
  `optimize_full(query, store, dict)`.
- **Planner, prefixed names.** `term_to_constant_id` returns `None` for every prefixed name,
  so `?s a ex:Rare` is estimated as `?s a ?o`, which is all `rdf:type` triples. The cardinality
  signal is missing for the most common way queries are written.
- **Executor, bound subject.** With the subject bound, `evaluate_vector_similar` still runs the
  HNSW search (k = 500) and keeps a row only if one of the subject's vectors is in that result
  list. Two problems:
  1. **Recall.** A bound subject whose similarity is above the threshold but which isn't
     among the ANN top-500 is dropped. That is a wrong answer to "is this subject's vector
     at least 0.85 similar", and the query asked exactly that.
  2. **Cost.** Five candidates cost a full beam search instead of five dot products.

## What to build

1. **`HnswIndex::vector_of(triple_id) -> Option<&[f32]>`** (via the existing `triple_to_node`
   map; `None` for deleted nodes).
2. **Executor: two access paths for a bound subject, chosen by cost at run time.**
   - *Exact*: for each bound subject, score each of its vectors directly against the query.
     Cost: C = the number of candidate vectors.
   - *Index*: the existing HNSW search plus membership check. Cost estimate:
     `min(N, ef · M · (⌈log2 N⌉ + 1))` distance computations, where N is the number of
     active nodes. HNSW visits each node at most once, so N bounds it.
   - Choose exact when C ≤ the index estimate. If `k:=` is given explicitly, always use the
     index path, since "in the top k" is then part of what the query asks.
   - Exact scoring uses the same metric the index path would: the override metric if one is
     given, otherwise the index's metric. The query is preprocessed with that metric, exactly
     as `search_with_metric` does.
   - Semantics: for threshold queries the exact path returns a superset of the index path's
     rows: the same rows, plus bound subjects ANN missed. That is the fix for problem 1.
3. **Planner: the vector index as an access path with a real cost.**
   - Add `optimize_with_vectors(query, store, dict, vectors)`. `optimize_full` keeps its
     signature and passes no vectors, so plan-only callers behave as before.
   - Unbound `VECTOR_SIMILAR` / `*_SEARCH` cost = `min(k or 500, active vectors)`: the rows
     it can produce. A graph pattern whose estimated cardinality (× weight) is lower goes
     first, and the vector pattern then runs as a bound-subject filter, which (2) makes cheap
     and exact.
   - Expand prefixed names with `query.prefixes` in cardinality estimation.
   - The server uses `optimize_with_vectors`.
4. **Adaptive execution** (reordering mid-query from observed sizes) is not in this phase. It
   gets a spec when it is picked up; the decision in (2) is made at run time per pattern
   already, which covers the case that matters most.

## Tests

- **Recall:** 600 vectors closer to the query than the target, the target above the
  threshold, and the subject bound to the target. The old path drops it (not in the top 500);
  the exact path returns it. Also: a bound subject below the threshold is still excluded.
- **Explicit `k:=`** keeps the index path (the same 600-vector set, `k:=10`, so the target
  is excluded).
- **Planner order,** with store and vectors: `ex:Rare` (5 subjects) goes before the vector
  pattern; `ex:Common` (more subjects than k) goes after it. Without vectors, the existing
  tests are unchanged.
- **Prefixed-name cardinality:** `?s a ex:Rare` is estimated at 5, not at all `rdf:type`.
- **Measured:** a bench (`loka-sparql/benches/sparql_query.rs`) times the vector-first plan
  against the graph-first plan the cost model now picks, on a few thousand vectors with a
  rare type. The numbers go in DEVLOG. Timing isn't asserted in tests, because that would be
  flaky in CI.
