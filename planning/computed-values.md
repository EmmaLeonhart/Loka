# Computed values in query results — design

**Status:** stages 1–2 built, stage 3 partly built (SPARQL-results JSON only). Written 2026-07-29
to settle the id-range question on paper before touching code, and kept as the record of *why* it is
shaped this way. Per-stage status at the bottom.

## The problem, concretely

`BIND(REPLACE(STR(?type), "^.*/", "") AS ?typeLocal)` parses (since `1dd4f4a`) and returns an
explicit "not supported yet" execution error. Numeric expressions bind fine —
`BIND(STRLEN(?label) + 1 AS ?n)` works, because an inline integer *is* its own id. A **string**
result has nowhere to live: a binding is a `TermId`, every `TermId` is either an inline literal or
a dictionary pointer, and `execute` holds `&TermDictionary`.

The same wall blocks four other things that are ordinary SPARQL:

| shape | why it is blocked |
|---|---|
| `BIND(CONCAT(?first, " ", ?last) AS ?name)` | computed string |
| `SELECT (LCASE(?label) AS ?key) WHERE …` | computed string in projection |
| `ORDER BY LCASE(?label)` | needs a comparable value per row |
| `GROUP BY (REPLACE(STR(?type), "^.*/", "") AS ?t)` | Pramana's type-count query, currently grouped on the full IRI and folded client-side |

Filters are unaffected — `compare_filter_terms` evaluates values directly and never needs an id.
This is specifically about values that must **survive into a result row**.

## Rejected: `&mut TermDictionary` in the executor

The obvious fix, and wrong for four reasons:

1. **Public API break in five crates.** `execute(&Query, &TripleStore, &TermDictionary)` is called
   from `loka-proto`, `loka-cli`, `loka-ffi` and the MCP server. All of them hold the dictionary
   behind a read lock during queries.
2. **Every query becomes a writer.** The concurrency story is "search is `&self`, concurrent reads
   don't block" (CLAUDE.md, HNSW section). A query that needs the dictionary's write lock
   serialises reads against each other for no user-visible reason.
3. **The dictionary is persisted.** `intern_synced` exists precisely so `INSERT DATA` writes reach
   both the in-memory and on-disk dictionaries. A query interning `"Xater"` because someone wrote
   `REPLACE(?label, "^W", "X")` would grow the stored dictionary with values that are not in the
   graph — silent database growth from read-only traffic.
4. **It is not what the value IS.** A computed value is not a term in the graph. Interning it says
   it is.

## Proposed: per-query value table, addressed by a new inline type

`TermId`'s layout already has room (`loka-core/src/id.rs`):

```
bit 63     = 1 → inline value, 0 → dictionary pointer
bits 62-56 = type tag (7 bits — 128 types, 3 used: Integer 0x01, Boolean 0x02, Temporal 0x03)
bits 55-0  = payload
```

Add `InlineType::Computed = 0x7F`, payload = **index into a per-query value table**. The top of the
tag space, not the next free number, so it reads as "not a normal literal type" and leaves 0x04+
for Float and friends.

```rust
/// Values computed during one query's evaluation. Lives as long as the QueryResult.
pub struct QueryValues {
    values: Vec<String>,
    by_value: HashMap<String, TermId>,   // interning WITHIN the query
}

impl QueryValues {
    pub fn intern(&mut self, s: &str) -> Option<TermId>;   // None if the table exceeds 2^56
    pub fn get(&self, id: TermId) -> Option<&str>;
}
```

### Why this shape

- **No collision is possible.** Dictionary pointers have bit 63 clear; every computed id has it
  set. Nothing needs a reserved *range* within the dictionary's space, which was the open question
  in `queue.md` — the answer is that the inline tag already gives us a disjoint space, so no range
  reservation and no "don't let the dictionary grow past N" invariant.
- **Existing code degrades correctly.** `decode_inline_integer`/`_boolean`/`_temporal` all check the
  tag and return `None` for an unknown one; `dict.resolve` returns `None` for an inline id. So today's
  consumers see "no value", which is the current behaviour — not a wrong value.
- **Interning within the query makes equality work.** Two rows computing `"Entity"` get the same id,
  so `DISTINCT`, `GROUP BY`, and joins on a computed variable behave. Without by-value interning
  they would be distinct ids for equal strings, which is the subtle bug this design has to avoid.
- **Ordering** goes through the same value lookup as rendering: computed ids compare by their
  string, never by id. (Insertion order is not sort order — the same trap as the negative-integer
  ordering bug fixed 2026-07-29.)

### The real work: every render path

The id is the easy half. A `TermId` becomes text in more places than one would like:

- `loka-proto`: SPARQL-results JSON, CSV/TSV, Turtle, N-Triples
- `loka-cli`: `query` output, `mcp` tool results
- `loka-ffi`: `loka_query`, `loka_resolve`
- `loka-sparql`: `health.rs` reporting, aggregate rendering

Each currently resolves through the dictionary. They need to consult the value table first, which
means `QueryResult` carries it:

```rust
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Bindings>,
    pub scores: Vec<HashMap<String, f32>>,
    pub values: QueryValues,   // new; empty for every query that computes nothing
}
```

Additive: a struct-literal construction of `QueryResult` outside the crate would break, but adding
`..Default::default()`-style construction internally keeps the change contained. **A render path
that forgets the table shows an empty cell**, which is why the staging below puts a
round-trip test in every one of them rather than trusting the audit.

### The one hard invariant: a computed id must never be stored

If a computed id reached a triple in the store, its payload would be an index into a table that no
longer exists — the id would resolve to whatever value happened to occupy that slot in a later
query. That is worse than any bug fixed this week: it is silent data corruption rather than a
silently wrong answer.

Today the blast radius is nil: SPARQL update supports only `INSERT DATA` / `DELETE DATA` over
literal triples, so no query-derived value can reach a write. `INSERT … WHERE` would change that,
and is exactly the feature that would introduce the hazard.

So the invariant is enforced at the boundary, not by discipline: `TripleStore::insert` and
`PersistentStore::insert*` reject any id whose inline tag is `Computed`, with a test asserting the
rejection. Cheap now, and it means `INSERT … WHERE` can be built later without having to remember
this document.

## Staging

Each stage is independently committable and testable.

1. **`InlineType::Computed` + `QueryValues` in loka-core**, with the insert-path rejection and its
   test. No behaviour change to anything else.
2. **BIND over a string expression** — `ExecutionContext` gains `&mut QueryValues`, `QueryResult`
   carries it, `bind_computed_value` interns instead of erroring. Test: Pramana's original
   `BIND(REPLACE(STR(?type), "^.*/", "") AS ?typeLocal)` returns `Entity`.
3. **Render paths**, one commit per crate, each with a round-trip test that a computed binding
   appears in that output format.
4. **Projection and ORDER BY** — `SELECT (expr AS ?v)`, `ORDER BY expr`. This is where the parser
   also needs `(expr AS ?var)` in the select clause.
5. **`GROUP BY` on a computed value**, which needs the group key to be the value not the id — free
   if by-value interning is in place from stage 1.

Stage 2 alone unblocks Pramana's entity page. Stages 4–5 are what make it a general facility rather
than a BIND special case.

## Status per stage (2026-07-30)

1. **DONE** — `InlineType::Computed`, `QueryValues`, storage rejection (9 tests in
   `loka-core/tests/computed_values.rs`).
2. **DONE** — `ExecutionContext` owns a `QueryValues`, `QueryResult` carries it,
   `bind_computed_value` interns instead of erroring. `BIND(REPLACE(STR(?type), "^.*/", "") AS
   ?typeLocal)` returns `Entity`, and two rows computing the same string share an id.
3. **DONE** — every result-rendering path consults the table: SPARQL-results JSON, CSV, TSV, XML
   (`loka-proto`), the `loka query` table, the MCP query tool, and the FFI boundary. Tests:
   HTTP round-trips for JSON and for CSV/TSV/XML asserting both that the value appears and that
   `_:id` does NOT; a real `loka_query` round trip across the FFI; and renderer unit tests for the
   CLI and MCP paths (neither file had a test module before).
   **Turtle and N-Triples are not applicable, and that is a conclusion rather than an omission:**
   `resolve_term_for_turtle` serves only `export_graph`, which iterates the *store*, and a computed
   id can never be stored (rejected at the boundary in stage 1). The design listed them because it
   was reasoning from format names; the code says otherwise.
4. **NOT STARTED** — `SELECT (expr AS ?v)`, `ORDER BY expr` (parser work too).
5. **NOT STARTED** — `GROUP BY` on a computed value.

## What this does not do

- No datatype or language tag on computed values: they are plain strings (`xsd:string`), which is
  what SPARQL says `CONCAT`/`LCASE`/`REPLACE` return. `STRLEN` and arithmetic stay inline integers.
- No non-integral numbers. `BIND(?a / 3 AS ?x)` still yields no binding, because there is no inline
  float. That is a separate item (`InlineType::Float = 0x04`, already anticipated in the id
  comment) and should not be smuggled in here.
