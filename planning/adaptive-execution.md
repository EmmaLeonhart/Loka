# Adaptive execution: reorder joins from observed sizes (spec)

`planning/large-features.md` Phase 4 left this out on purpose: "Adaptive execution (reordering
mid-query) is a separate, later step; it gets a spec, not a guess." This is the spec. Nothing
here is built yet.

## The gap it closes

The planner (`loka-sparql/src/planner.rs`, `optimize_with_vectors`) orders patterns once, before
execution, from unconditional estimates:
- `store.estimate_cardinality` for triple patterns;
- `min(k, indexed vectors)` for unbound vector searches.

After the first pattern, those estimates are blind to the data in hand. `?x :p ?y` is costed at
`count(p)` whether 3 or 30,000 rows bind `?x` going in, and correlations between patterns are
invisible. `/health/queries` can only score the first pattern for exactly this reason (see
`planning/query-metrics.md`). Adaptive execution uses what *is* known mid-query, the actual
row count and bindings so far, to choose the next pattern.

## Scope: what may be reordered

Only patterns whose order doesn't change the result:
- **Reorderable:** `Pattern::Triple` (including paths), `PathUntil`, `VectorSimilar` and
  `MetricSearch`. These are inner joins, so they commute. Scores travel with rows by source
  index (`7d09efa`), so reordering doesn't mix them up.
- **Barriers (stay in place, and split the query into segments):** `Optional`, `Union`,
  `Bind`, `Values`, `Subquery`, the temporal scopes, and `Filter`. A filter may move only to
  just after the last pattern binding its variables, as `pushdown_filters` already does. It is
  re-placed after each reorder, never evaluated before its variables are bound.
- Reordering happens **within a segment** between barriers. The planner's static order is the
  starting point; adaptive execution only changes which reorderable pattern in the current
  segment runs next.

## The decision at each step

With `n` current rows and a set of bound variables, for each remaining reorderable pattern `q`
in the segment, estimate the rows it would produce:

```
rows(q) ≈ n × fanout(q | bound positions)
```

- `fanout` is the average matches per input row given which positions are bound. For a triple
  pattern with predicate `p`:
  - subject bound: `count(p) / distinct_subjects(p)`;
  - object bound: `count(p) / distinct_objects(p)`;
  - both bound: `≤ 1`;
  - neither: `count(p)` (a cross product: only chosen if nothing else is possible).
- **New statistic needed:** per-predicate distinct subject and object counts. `TripleStore`
  doesn't keep them. Maintain them incrementally in `insert`/`remove`, next to the
  per-predicate generations added in Phase 6, or compute them on demand from the POS/SPO
  ranges and cache them by generation. Decide by measuring both on the 2M-triple store before
  building.
- **Sampling** (alternative or tie-breaker): probe the first `s` current rows (e.g. 32) against
  `q` and extrapolate. It's exact for the rows probed and costs `s` lookups. Use it when the
  statistic is missing or when `n` is small enough that probing everything is cheap.
- Choose the `q` with the smallest estimated rows; ties go to the planner's order.
- **When to bother:** only when the choice differs from the planner's next pattern *and* the
  estimated saving is large: next-in-order estimate ≥ 4× the best, and at least 1,000 rows.
  Otherwise keep the static order. Small queries pay nothing, and plans stay stable.

## Interactions

- **LIMIT pushdown** (`pushable_limit`) is applied only to the last pattern of a segment
  today. With reordering, it stays tied to whichever pattern runs last.
- **Fused pseudo-table runs** (`find_fuseable_pattern_run`) are found on the static order. A
  fused run is treated as one reorderable unit, so adaptive execution must not split one.
- **Metrics:** record each adaptive choice (which pattern, estimated against actual rows) in
  `/health/queries`. That gives a q-error for every *later* pattern, which the current scoring
  can't produce, and it measures whether adaptivity helps.
- **Deadline:** each step already checks the deadline; probing adds lookups, so sampling stops
  at the deadline as well.

## Tests (before it can be called done)

1. **Same results.** For a set of multi-pattern queries (stars, chains, mixed with filters,
   OPTIONAL, VALUES), adaptive and static execution return the same multiset of rows and the
   same scores, on several datasets including skewed ones.
2. **Barriers hold.** An OPTIONAL and a BIND between patterns are never crossed; a FILTER never
   runs before its variables are bound.
3. **It picks the better plan where the static plan is wrong.** A correlated dataset where
   `count(p)` misleads. For example, `?x :p ?y . ?y :q ?z` with `count(p)` small but every `y`
   having thousands of `q`, while a third pattern on `?x` is far more selective once `?x` is
   bound. Adaptive execution chooses the selective pattern; the bench (`sparql_query.rs`)
   shows the wall-time difference, measured, not asserted in tests.
4. **No regression where the static plan is right.** The existing benches don't slow down by
   more than noise (the 4× / 1,000-row gate exists for this).

## Measured: is it worth building? (2026-10-08)

Yes. Test 3's dataset, now the `adaptive_gap` bench in `loka-sparql/benches/sparql_query.rs`:
- 2000 `?x :p ?y` over 20 hubs;
- 500 `:q` per hub;
- `:s` on only 2 of the x's, but on 10,000 other subjects, so `count(s)` misleads.

The static planner runs `p, q, s` through a 1M-row intermediate. Both runs give the same
1,000 rows:

| order | release probe (median of 5) | criterion |
|---|---|---|
| planner (`p, q, s`) | 636 ms | 923 ms |
| best (`p, s, q`) | 4.4 ms | 2.9 ms |

Either fanout method picks `s` second here. Sampling bound rows finds almost no `:s`. Distinct
counts give `s` about 1 per x, against 500 per y for `q`. So this case doesn't decide between
them. The insert-overhead measurement below still does.

## Open questions (to settle before building)

- Maintained or on-demand distinct counts: measure the insert overhead on the 2M store.
- Whether sampling alone is enough, so no new statistic is needed. That would be simpler. It
  should be tried first on the bench in test 3.
