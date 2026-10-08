# Deep (multi-hop) pseudo-tables serving queries (spec)

Follows `planning/pseudo-table-serving.md` (depth-1 tables, built and serving since
2026-10-07). Nothing here is built yet.

## What exists

`discover_deep_pseudo_tables` (`loka-core/src/pseudotable.rs`) mines repeated rooted subgraph
shapes and materialises them with `materialize_subgraph_table`. Each column is a
`SubgraphPath`: a sequence of `PathStep { predicate, direction }` hops from the root. The cell
for a row is `resolve_path(root, path, store)`: one leaf `TermId`, or null. Deep tables
currently get `column_generations = None` on every column, so they never answer a query.

## When a path column may answer a chain of patterns

A path column for `root -p1-> m1 -p2-> … -pk-> leaf` corresponds to the chain query

```sparql
?r :p1 ?m1 . ?m1 :p2 ?m2 . … ?m(k-1) :pk ?leaf      # Reverse steps swap subject/object
```

**Exactness (decided at discovery, like depth-1):** the column's non-null cell count equals the
number of solutions of that chain over the whole store, **as a bag** (with duplicates). Each
cell is one real path in the store, so equal counts mean the solutions are exactly the cells,
one each. That rules out three things:
- a non-member root with the path (one more solution than cells);
- a root with two leaves (`resolve_path` keeps one);
- the same leaf reached through two different middles (two solutions, one cell).

The count is computed once at discovery (a k-pattern join over the store), only for columns
that pass the coverage threshold.

**Freshness:** record the store generation of **every** predicate on the path. The column
serves only while all of them are unchanged. (Depth-1 needed one predicate's generation; a path
needs each hop's.)

## Recognising a chain in a query

The executor gets a new fused case: a run of triple patterns that forms a chain from one
variable root, as above, matching a path column. One extra condition depth-1 didn't need: **the
intermediate variables `?m1 … ?m(k-1)` must not appear anywhere else in the query**. That
covers the projection, other patterns, filters, ORDER BY and GROUP BY. The table doesn't store
intermediates, so a query that reads one can't be answered from it. A fixed leaf
(`?m :p2 :X`) becomes an equality filter on the column, as for depth-1.

## Is it worth building?

Measure first, as for depth-1 and adaptive execution. Benchmark a two-hop chain
(`?paper :author ?a . ?a :name ?n`) over a tree-like dataset where discovery yields a deep
table, comparing the triple path (two joins) with a column scan. Build only if the column scan
is clearly faster. The depth-1 star gain was 6× after the join fix; a chain's second hop is a
point lookup per row, so the gain may be smaller.

## Tests (before it can be called done)

1. Same rows with and without the registry, for chain queries of depth 2 and 3, with hit counts
   so a pass can't come from fallback.
2. Non-member root with the path → column not served, row still returned.
3. A root with two leaves, and a leaf reached through two middles → not served, all solutions
   returned.
4. A write to any hop's predicate → not served until rediscovery.
5. A query that also reads an intermediate variable → not served, same rows.
6. Mutation check: ignoring exactness fails 2–5.
