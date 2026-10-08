# Pseudo-tables: exact columns, invalidation, and serving queries (Phase 6)

Phase 6 of `planning/large-features.md`. Written before the code.

## What exists, and what's wrong with it

- `loka-core/src/pseudotable.rs` discovers pseudo-tables (characteristic sets) and stores
  them as columnar segments with zonemaps.
- The executor has `try_pseudo_table_scan` and a fused multi-pattern version
  (`try_fused_pseudo_table_scan`), which together recognise multi-pattern queries over one
  subject.
- **Nothing that runs queries passes it a registry.** The server, CLI, MCP and FFI all set
  `pseudo_tables: None`; only `loka health --refresh` discovers tables, for the report. So
  the columnar path has never served a query.
- **It would give wrong answers if it did.** It answers `?s :p ?o` from one table:
  1. **members only:** nodes with `:p` that aren't rows of that table are left out;
  2. **one value per cell:** a node with two `:p` values contributes at most one.
  Nothing checks either case, and nothing notices when the data changes after discovery.

## What to build

1. **Exact columns.** At discovery, a subject-position column for predicate `p` is *exact*
   when its non-null cells account for every triple with predicate `p` in the store. Each
   non-null cell is one existing `(node, p, value)` triple, so it suffices that the
   non-null count equals `store.find_by_predicate(p).len()`. Only an exact column may answer
   a pattern on `p`. That covers both problems: no non-member has `p`, and no node has a
   second value. Recorded per column in `PseudoTable::exact`.
2. **Invalidation.** `TripleStore` keeps a per-predicate generation counter, bumped by every
   `insert` and `remove` that changes the store; those are its only mutators, so every write
   path is covered. A table records the generation of each exact column's predicate when it
   is built. A column serves only while its predicate's generation is unchanged. This is
   column-level, not row-level: a column's contents depend only on triples with its predicate,
   so a stale column is never served. Tracking at row level would only change *which* columns
   get rebuilt, and the rebuild below is whole-registry anyway.
3. **Executor.** `try_pseudo_table_scan` / the fused scan use a column only if it is exact and
   current; otherwise they fall through to the triple indexes. A fused scan needs every
   column it touches to qualify.
4. **Wiring.**
   - `AppState` gets `pseudo_tables: RwLock<Option<PseudoTableRegistry>>`, and the SPARQL
     handlers pass it to the executor (`execute_with_pseudo_tables`).
   - Discovery runs in the Phase 5 maintenance cycle while the server is idle: when there is
     no registry, or when any table has an exact column that has gone stale.
   - Discovery is expensive at scale, so it stays inside the opt-in `--maintenance-idle-secs`
     cycle.
5. **Only if it's faster.** A bench compares a pseudo-table-served star query with the
   triple-index path on the same data. If the columnar path isn't faster, it doesn't get
   wired into serving, and that result goes in DEVLOG.

## Tests

- **Identical results:** for star queries over a discovered table (single pattern and fused
  multi-pattern), results with the registry equal results without it.
- **Members only:** a non-member node with the column predicate makes the column inexact,
  so the query still returns it (it is answered from the triple indexes).
- **Multi-valued:** a member with two values makes the column inexact, so both values are
  returned.
- **Stale is never served:** insert or remove a triple with the column's predicate after
  discovery, and the query reflects the change. Writes to other predicates don't invalidate
  the column.
- **Served when valid:** a counter on the context shows the columnar path answered when it
  should, so the identical-results test isn't passing only because of fallback.
