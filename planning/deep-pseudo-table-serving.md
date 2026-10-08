# Deep (multi-hop) pseudo-tables serving queries (spec)

Follows `planning/pseudo-table-serving.md` (depth-1 tables, built and serving since
2026-10-07). Nothing here is built yet.

## What exists

`discover_deep_pseudo_tables` (`loka-core/src/pseudotable.rs`) mines repeated rooted subgraph
shapes and materialises them with `materialize_subgraph_table`. Each column is a
`SubgraphPath`: a sequence of `PathStep { predicate, direction }` hops from the root. The cell
for a row is `resolve_path(root, path, store)`: one leaf `TermId`, or null. Deep tables
currently get `column_generations = None` on every column, so they never answer a query.

## Precondition found while measuring: columns lose their path

`materialize_subgraph_table` labels each column with the **first** step of its path, as a
`Property`, but the cell holds the path's **last** node. `country -hasCapital-> capital
-hasMayor-> mayor` is labelled `(hasCapital, Subject)` and holds mayors. Two paths with the
same first step get identical labels. This is harmless today: deep tables never enter the
serving registry, and every column has `column_generations = None`, guarded by
`deep_tables_are_never_servable`. But serving has to start by storing the full `SubgraphPath`
per column (e.g. `PseudoTable::column_paths`), and must match queries on paths, never on
these labels.

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

## Measured: worth building (2026-10-08)

20,000 countries, each `country -hasCapital-> capital -hasMayor-> mayor`, plus names
(120k triples). Discovery found one deep table with a country→mayor column (20,000 rows), and
took 0.86 s: fine for the idle cycle, not per query. For the chain query
`?c :hasCapital ?k . ?k :hasMayor ?m`, same 20,000 answers, release build, medians of 15:

| | time |
|---|---|
| triple path (two joins) | 35 ms |
| column scan, building result rows (realistic serving floor) | 4.0 ms |
| bare column scan | 0.1 ms |

That's about 8.8×, so building it is justified, behind the precondition above.

## Built (2026-10-08)

- `PseudoTable::column_paths` keeps each deep column's full `SubgraphPath`. Deep columns
  are only ever matched on it (`servable_column` stays false for them; the guard test
  `deep_tables_are_never_servable` still holds).
- **Exactness:** `exact_path_generations` compares a column's non-null cells with
  `chain_solution_count`, a one-pass dynamic-programming count of the chain's bag of
  solutions (a multiplicity per node, one pass per hop, no join materialised). If they're
  equal, it records `(predicate, generation)` for every hop. `servable_path_column` requires
  all of them unchanged.
- **Executor:** `try_deep_chain` runs before the depth-1 fused scan when exactly one row is in
  hand. `match_chain` checks the patterns are exactly the path: predicates, directions, root
  variable, linked variables, and a variable or known-constant leaf. Intermediates must be
  fresh, unbound, and not appear in the rest of the query (checked conservatively against its
  Debug text). Not used under `SELECT *` (empty projection) or a temporal scope. Recorded in
  `/health/queries` as `deep_chain(k)`.
- `loka serve`'s idle maintenance discovers deep tables into the registry too
  (`PseudoTableRegistry::deep`).
- Tests (7): same rows and served; constant leaf; intermediate read (projection, FILTER,
  `SELECT *`) → not served; non-member root, second leaf, and two middles → not served, with
  all solutions returned; writes to either hop stop serving, unrelated writes don't.
  Mutations: dropping exactness fails the three inexactness tests; dropping the intermediate
  rule fails that test.

## Tests (before it can be called done)

1. Same rows with and without the registry, for chain queries of depth 2 and 3, with hit counts
   so a pass can't come from fallback.
2. Non-member root with the path → column not served, row still returned.
3. A root with two leaves, and a leaf reached through two middles → not served, all solutions
   returned.
4. A write to any hop's predicate → not served until rediscovery.
5. A query that also reads an intermediate variable → not served, same rows.
6. Mutation check: ignoring exactness fails 2–5.
