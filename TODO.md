# Loka — TODO

**Status: 228 of 249 items complete (92%)**

## 🔒 FIX (2026-07-20): `loka serve` now binds 127.0.0.1 (was 0.0.0.0)

Root cause of Emma's "Loka requesting to go past the firewall all night": every `loka serve` launch
listened on ALL interfaces (0.0.0.0), so Windows Firewall raised its inbound-allow prompt for each
server process during the Pramana store sessions — dozens overnight, nobody present to dismiss.
No outbound traffic was involved (the only outbound in the codebase is the GitHub releases
update-check on the `loka mcp` path, which never ran). Fix: both bind sites now default to
127.0.0.1 (matches the serverless-by-default principle). Remote access needs an explicit `--host`
flag (not yet built — add when Remote Studio lands). NOTE: source-only fix; rebuild the binary
before the next serve (`cargo build --release -p loka-cli`).

## 🐛 OBS (2026-07-20): /graph export lags recent INSERT DATA writes by more than the sled flush interval

`GET /graph?format=nt` served a snapshot missing triples written seconds earlier via POST /sparql
INSERT DATA (SELECT saw them immediately). Observed lag sometimes >3s (2s flush config). Pramana
works around it with a 6s settle before rebuilding its read index. Export should probably read
through the same view SELECT uses, or flush first.

**DOES NOT REPRODUCE on current source (tested 2026-07-28).** Added
`graph_export_sees_writes_immediately` (loka-proto/src/server.rs): INSERT DATA over the router,
then `GET /graph?format=nt` with no sleep and no flush. The triple is present, and SELECT agrees.
368 workspace tests green.

The code path explains the non-reproduction: `execute_insert_data` takes the write lock on
`state.store` and inserts there before returning, and `export_graph` iterates that *same*
in-memory `TripleStore`. The two read paths cannot diverge, and the sled flush interval governs
durability, not read visibility — so there is no flush for the export to be late behind. The
suggested fixes ("read through the same view SELECT uses", "flush first") are therefore both
already true / not the mechanism.

**Not marked fixed.** Same discipline as the three bugs above: the original was intermittent, and
one passing test is evidence, not proof. Most likely the same root cause as those three — the
stale installed binary (see the DO-NOT-RUN-FROM-INSTALLER note above), which was a May build.
Pramana's 6s settle rests on the same premise and can be revisited.

## ⚠ DO NOT RUN LOKA FROM THE INSTALLER (Emma, 2026-07-22)

**The installer dependency is the bug.** `C:\Program Files\Loka\loka.exe` is a **2026-05-27 build**; current source builds to 2026-07-22. Both report `loka 0.4.1`, so two months of drift was invisible. That one fact explains two separately-investigated incidents:

- **The overnight firewall prompts** Emma called critical — the May build binds `0.0.0.0`; source binds `127.0.0.1`. The fix was in source for weeks and never reached the binary being run.
- **The three “engine bugs” below**, none of which reproduce on a current build — after they had already cost Pramana an in-memory workaround for a problem that wasn't there.

**Rule: always run the repo-local build**, `external/Loka/target/release/loka.exe`, built with `cargo build --release -p loka-cli`. Never invoke bare `loka` (it resolves via PATH to the installer copy). Replacing the installed binary was considered and rejected — it treats the symptom and leaves the same trap for the next stale install.

## ✅ RE-TESTED 2026-07-22 — ALL THREE DOGFOODING BUGS BELOW FAIL TO REPRODUCE

Re-ran every repro against a **fresh `cargo build --release` of current source**, on the same
162,761-triple `.sdb` the originals were filed from:

| Filed | Reported | Measured 2026-07-22 |
|---|---|---|
| PERF: single-pattern lookup | ~2s | **~1.6 ms** (5 runs: 2.0/1.7/1.6/1.5/2.3 ms) |
| BUG 2: object-var ⋈ literal-bound join | 0 rows | **8 rows, correct** |
| BUG 2 addendum: nondeterministic per process | varies by process | **deterministic** — 8 rows in 3 separate fresh `loka serve` processes |
| BUG 1: prefixed predicate + literal object | 0 rows | **1 row**, same as the full-URI form |

**Likely explanation: the originals were measured against the STALE INSTALLED BINARY.**
`C:\Program Files\Loka\loka` was never rebuilt after the source fixes — it was still binding
0.0.0.0 on 2026-07-22 when the 127.0.0.1 change had long been in source. Same staleness would
explain these. Not proven, but it fits every symptom including the "inconsistent across sessions"
note.

**Consequence for Pramana:** its in-memory read index (`src/graph_index.py`) was built explicitly
because "Loka answers even single-pattern SPARQL in ~2s". That premise no longer holds. The index
is still defensible on round-trip grounds (a page render is hundreds of lookups, and HTTP per
lookup is worse than one dump), so this is NOT a call to remove it — but the stated reason should
be corrected, and BUG-2 workarounds (constructing entity URIs to avoid joins) can be revisited.

**Left below unchanged, as filed.** A non-reproducing bug is not a fixed bug: the addendum itself
says the failure was intermittent, so three clean processes is evidence, not proof.

## 🐛 BUG 2 addendum + PERF (2026-07-20): join failures are NONDETERMINISTIC per process; ~2s/query at 157k triples

Two additions from continued dogfooding:
1. **The BUG-2 join failure is nondeterministic across server processes**: the same multi-pattern
   query on the same `.sdb` returns the correct rows in one `loka serve` process and `[]` in another.
   Suspect hash-seeded planner join-order (Rust `HashMap` RandomState) selecting the broken
   object-var⋈literal join path only sometimes. This masked/confused diagnosis badly (looked like
   whitespace/state effects). A deterministic planner order (or fixing the join path) would make it
   reproducible.
2. **Query latency ~2s for even single-pattern lookups at 157k triples** (e.g.
   `SELECT ?t WHERE { ?t <...EntityLabel> "X" }`). Pramana's page renders need dozens-to-hundreds of
   such lookups → unusable. POS/SPO prefix scans should make these ~ms; something is scanning.

## 🐛 BUG 2 (found 2026-07-20, same dogfooding): object-variable ⋈ literal-bound join returns 0 rows

A join where a variable appears as the OBJECT of one pattern and the SUBJECT of a literal-bound
pattern returns 0 rows, though each leg matches alone:

```sparql
# 0 rows (both legs individually match):
SELECT ?p WHERE { ?p <.../subject> ?s . ?s <.../uuid> "3946bf48-..." }
# workaround — bind the object URI directly (works, 5 rows):
SELECT ?p WHERE { ?p <.../subject> <http://pramana.org/entity/3946bf48-...> }
```

Subject-side joins on the same store work (`?e <uuid> "..." . ?e <label> ?l` matches). Suspect the
object→subject join path (OSP/POS usage) when the driving pattern is a literal-bound lookup.
Distinct from BUG 1 (this one uses full URIs throughout). Behaviour was inconsistent across stores/
sessions (the same query shape returned rows on an older store) — possibly planner join-order
dependent. Pramana works around it by constructing entity URIs directly (WD namespace + uuid).

## 🐛 BUG (found 2026-07-20 dogfooding Pramana-on-Loka): prefixed predicate + literal object matches nothing

A SPARQL pattern using a PREFIXED predicate with a LITERAL object returns 0 rows, while the identical
query with the full predicate URI returns the correct match. Repro (data present in both cases):

```sparql
# MATCHES (1 row):
SELECT ?e WHERE { ?e <http://pramana.org/prop/direct/EntityLabel> "GAP2-timing-probe" }
# MATCHES NOTHING (0 rows) — identical semantics:
PREFIX wdt: <http://pramana.org/prop/direct/>
SELECT ?e WHERE { ?e wdt:EntityLabel "GAP2-timing-probe" }
```

Prefixed predicates with VARIABLE objects work fine (`?e wdt:EntityLabel ?l` matches). So the bug is
specifically prefixed-name expansion in patterns with a constant literal object — likely in the query
parser/planner path that special-cases bound-object lookups. Found via Pramana's `_find_entity_by_label`
silently never matching (caused duplicate entity creation). Pramana works around it with full URIs.

# DO THE STUFF IN THE QUEUE.MD

This is very important! Please actually do the stuff in that file! Do all of it. Base it off of the actual Loka repo's usage of it and its description of it in the CLAUDE.md. The actual Loka repo for the programming language has one that's relatively well done, although it is a bit messy at the same time.

Also, please, I don't know why it is that this TODO.md is so cluttered, and you will really need to actually work on the clutter here. I'd say, for this particular Q file that we have, it has the stuff that I consider to be kind of most important to do immediately. Add every single thing here, except for the rename into the queue.md, so that we can work on this stuff. 

---

## Windows installer — multi-model support

The Inno Setup installer (`installer/loka.iss`) currently offers a single
optional inference model (Qwen 2.5 1.5B Instruct, declared in
`installer/models.toml`). Extend so the user can choose between several
models at install time. Loose plan:

- [ ] Generalise `installer/models.toml` to a list (schema already shaped for it).
- [ ] Pre-process `models.toml` in CI into one `[Components]` entry per model
      (Inno Setup can't read TOML at runtime; CI emits a generated `.iss`
      include from a Python or PowerShell pre-step).
- [ ] Make the components mutually exclusive (`Flags: exclusive`) so the user
      picks at most one model, or "no model" via component deselect.
- [ ] Update `install-selection.toml` to record the chosen model id so
      `loka.exe` knows what to fetch on first run.

Future candidates: a smaller-footprint Qwen / Phi / Llama option for users
without 3 GB to spare, and a "bring your own GGUF" file picker.

---

## Next Release (v0.3.1) — Gradle Migration, MCP Agentic UX

Merge the Gradle migration (local) and MCP agentic UX work (claude.ai remote session) then cut v0.3.1.

### Release Checklist
- [x] Merge claude.ai remote branch (MCP agentic UX work) into main
- [ ] Merge Gradle migration setup (local commits)
- [ ] Bump version to 0.3.1 in `sdks/java/build.gradle.kts` and all other SDK configs
- [ ] Tag `v0.3.1` and push to trigger publish workflow

### Java/Kotlin SDK — Locally Complete
The SDK is functionally complete (3 classes, ~400 LOC). Build migrated from Maven to Gradle (Kotlin DSL).

- [x] JUnit 5 test suite: 24 unit tests with HTTP mocking for all LokaClient methods
- [x] Add `rebuildHnsw()` method (calls `POST /vectors/rebuild`)
- [x] Add `healthReport()` method (calls `GET /health` + `GET /vectors/health`)
- [x] Bump version to 0.3.0 (match main project)
- [x] Migrate from Maven (pom.xml) to Gradle (Kotlin DSL)
- [x] Switch to Gradle `maven-publish`
- [x] In-memory GPG signing (no GPG binary needed in CI)
- [x] GroupId: `io.github.emmaleonhart`, artifact: `loka`
- [ ] Integration test: start Loka, insert triples, query, verify round-trip

---

## GPU-gated follow-ups & watched blockers (relocated from queue.md 2026-06-01)

These are not actionable on the thermally-constrained training laptop without a
sustained GPU run or a large risky ingest; they wait for cloud GPU or a donor.

- **Donor clean-Adam 10-epoch v14** via `tools/contribute_v14_training.py` —
  explicit successor experiment per paper §5.12. GPU-gated.
- **Clean v12 retrain** — epoch-4 best 226.86 lost to shared-GPU contention.
  GPU-gated.
- **Propgen test (Q42 seed) on v11–v14** — deferred since v11 due to GPU
  fragility during shared use. GPU-gated.
- **Engine bug #1 — sustained-ingest verification (open, watched).** Probable
  fix shipped in `c36760b` (explicit `sled::Config`: 256 MB cache, 2 s flush,
  `Mode::HighThroughput`). Reopen-in-place verified 2026-05-13 (WAL replay
  recovered 32,877,248 triples). Residual question: does the tuning hold against
  *fresh* sustained ingest past 32.88 M triples? If a re-test ingest panics at
  the next plateau, escalate to RocksDB migration (sled 0.34 unmaintained since
  2021). Not blocking under the current base+retrieval pivot.

---

## Future Versions

### AI Agent Installer (remaining)
- [ ] End-to-end test: fresh install → insert → query → verify

### HNSW Traversal via SPARQL Property Paths
- [ ] Greedy descent + beam search semantics from graph structure and property path evaluation
- [ ] Test: `loka:hnswNeighbor+` produces correct ANN results

### Predicate-Based Exit Conditions (UNTIL)
- [ ] Design UNTIL syntax for exit conditions on property path traversal
- [ ] Per-step predicate evaluation during traversal (not post-filter)
- [ ] Backtracking interaction (exit on one branch doesn't kill others)
- [ ] Ordered traversal (exit conditions require defined traversal order)
- [ ] HNSW-specific exit: "no closer neighbor found" (local optimality termination)
- [ ] Test: ordered traversal with UNTIL produces correct early termination

### Cost-Based Query Planning (remaining)
- [ ] HNSW as access path: planner chooses "HNSW index scan" vs "SPO triple scan" based on cost
- [ ] Adaptive execution: observe intermediate result sizes at runtime, reorder mid-query

### Background Maintenance Cycle
- [ ] Low-usage detection heuristic (query rate below threshold for N seconds)
- [ ] Background HNSW rebuild: fresh graph from current vectors, old graph serves queries until swap
- [ ] Atomic swap: replace old HNSW with rebuilt one
- [ ] Background pseudo-table rediscovery and rebuild

### Pseudo-Tables (remaining)
- [ ] Invalidation tracking: flag stale rows when interior nodes change, rebuild during maintenance cycle
- [ ] Update query planner to recognize multi-pattern SPARQL queries that match a subgraph pseudo-table

### Database Health Dashboard (remaining)
- [ ] Query performance metrics: per-pattern latency percentiles, planner decision accuracy
- [ ] Iterate CLI health output format based on real agent usage
- [ ] Loka Studio health dashboard as Flutter landing page: overall status, per-index cards, action buttons

### SDK Publishing — EMMA-GATED (audit complete 2026-05-31, verdict in `planning/sdk-publish-readiness.md`)

Audit done; licenses aligned to `AGPL-3.0-or-later`; local dry-runs clean. First
publish is the irreversible step and needs Emma's explicit go + these setups:

- **npm:** create the npm account + add `NPM_TOKEN` GitHub secret. The name `loka`
  is **taken on npm** (unrelated v1.0.1) → Emma picks a new name/scope
  (e.g. `@emmaleonhart/loka`) for the TS SDK.
- **PyPI:** register the *pending trusted publisher* (project `loka`, owner
  `EmmaLeonhart`, repo `Loka`, workflow `publish-sdks.yml`, no environment). No
  token secret — it uses OIDC. `loka` is **available on PyPI**.
- Publish fires on a `v*` git tag.

- [ ] Python SDK → PyPI (name available; needs trusted-publisher registration)
- [ ] TypeScript SDK → npm (needs account + `NPM_TOKEN` + a non-`loka` name)
- [ ] Rust SDK → crates.io
- [ ] C# SDK → NuGet
- [ ] Go SDK → tag for Go modules

### Loka Studio
- [x] Pre-built binaries in release pipeline (Windows, Linux, macOS)
- [x] MCP download_studio + launch_studio tools
- [x] LOKA_ENDPOINT env var for launch-time connection
- [x] `loka mcp --studio` flag to launch MCP + Studio together
- [ ] Remote Studio access: connect Studio to a remote Loka over the network
- [ ] Dart FFI bindings: replace HTTP client with direct loka_ffi.dll calls
- [ ] Studio-embedded MCP server: start MCP on background thread from within Studio
- [ ] Flutter graph view: remaining browse.html parity
- [ ] Long-term: absorb core Protege functionality

### Query Language Wrappers
- [ ] GQL (ISO 39075) → SPARQL transpiler: ISO standard graph query language mapped to SPARQL.
      The Cypher transpiler (`loka-sparql/src/cypher.rs`) is the template — same
      text-in/SPARQL-text-out shape, same rejection discipline. Reuse its tokenizer.

### ✅ FIXED 2026-07-29: string / IRI equality in FILTER now matches

`filter_term_value` resolved only variables and integer literals and returned `None` for
everything else, so `FILTER(?n = "Ada")` compared `Some(id)` against `None` — always false.
Equality now goes through `filter_term_id`, which delegates to `resolve_term`, the same
resolver the triple-pattern path uses. A term therefore means the same thing in a FILTER as
in a pattern, which is the invariant that was broken.

Fixes string literals, IRIs, prefixed names and typed literals in `=` / `!=` alike — all
four were silently matching nothing. 8 tests in `loka-sparql/tests/filter_equality.rs`,
including one asserting the filter and pattern paths agree. 425 workspace tests green.

**Ordering (`<`, `>`, `<=`, `>=`) was deliberately left narrow.** It compares raw `TermId`s,
which is meaningful for inline-encoded integers and meaningless for interned strings, where
the id is insertion order. Widening it would turn `FILTER(?name < "M")` from "matches
nothing" into "matches an arbitrary subset" — silently wrong rather than silently empty,
which is worse. A test pins the current behaviour so a future widening has to confront the
choice. Real string collation needs the executor to compare resolved *values*, not ids.

<details><summary>Original finding, for context</summary>

`FILTER(?n = "Ada")` returns **0 rows** against a store that contains the matching triple.
The same literal in *pattern* position matches fine, so the two paths disagree:

```rust
let name = dict.intern("http://loka.dev/name");
let ada  = dict.intern("http://loka.dev/ada");
store.insert(Triple::new(ada, name, dict.intern("\"Ada\"")));
```

| query | rows |
|---|---|
| `?a loka:name "Ada" .` (pattern position) | **1** ✅ |
| `?a loka:name ?n . FILTER(?n = "Ada")` | **0** ❌ |
| `?a loka:name ?n . FILTER(?n = "\"Ada\"")` | **0** ❌ |

Quoting the literal both ways fails, so it is not simply the stored-with-quotes convention.
Numeric FILTER comparisons are unaffected — `FILTER(?age = 36)` and `FILTER(?age > 30)` work,
which is why this went unnoticed: the parser produces `Equals(Variable, Literal("Ada"))`
correctly (verified by dumping the AST), so the defect is in how the executor resolves a
`Literal` to a `TermId` for comparison, not in parsing.

Not a regression from the grouping work below — pattern position and numeric filters both
predate it and still behave. Found while writing an end-to-end test that used a string
conjunct; the branch was silently dead and the test passed for the wrong reason until the
row counts were checked. Any query filtering on a string literal is currently returning
nothing rather than erroring, which is the bad shape of failure.

</details>

### ✅ FIXED 2026-07-28: the FILTER grammar now has parenthesised grouping

`FILTER((?a = 1) && (?b = 2))`, `FILTER(?a = 1 && (?b = 2 || ?c = 3))`,
`FILTER((?a = 1 && ?b = 2) || ?c = 3)` and `FILTER(!(?a = 1))` all parse and evaluate.
Added a `(`-group branch to `parse_filter_inner` **and** to `parse_filter` (which reaches
`parse_comparison_expr` directly, so a *leading* group needed its own branch), plus
`parse_bool_expr` — a `&&`/`||` chain that does not consume a closing paren, since the
existing chain logic in `parse_filter` eats FILTER's own `)` and cannot be reused.

Additive by construction: a `(` in expression position previously fell through to
`parse_term` and errored, so no query that parsed before reaches the new code. 7 tests in
`loka-sparql/tests/filter_grouping.rs`, including a guard that the flat chain is unchanged
and that redundant parens don't alter results. 415 workspace tests green.

**Also fixed 2026-07-29: filters of three or more terms.** `parse_filter` inlined a
one-shot chain — one comparison, at most ONE `&&`/`||` continuation, then `)`. So
`FILTER(?a = 1 && ?b = 2 && ?c = 3)` was a parse error, which is an ordinary SPARQL filter.
It now delegates to `parse_bool_expr`, which loops.

Removing the early `bound` / `!bound` / `!` branches from `parse_filter` at the same time
fixed a positional asymmetry: each consumed FILTER's own closing paren before returning, so
they worked as an entire filter but not as the left operand of a chain
(`FILTER(bound(?a) && ?b = 1)` failed while `FILTER(?b = 1 && bound(?a))` worked).
`parse_filter_inner` already handled all three without eating the outer paren.

**Both remaining items CLOSED 2026-07-29 — see the two sections below.** What is still
missing from a full SPARQL 1.1 `Expression` grammar is **arithmetic in operand position**,
which `parse_comparison_expr` only half-handles: it parses `?a + 1 > 5`, then *discards the
right-hand side of the arithmetic* and compares `?a` against `5`. That is a wrong-answer
path, not a parse gap, and it is the next real gap here (§ below).

The Cypher transpiler's two workarounds — pushing NOT to the leaves and splitting
top-level ANDs into separate FILTER clauses — are still correct and still tested, but are
no longer *necessary*. It can emit grouped filters directly and drop its
`(a AND b) OR c` rejection whenever someone wants to simplify it.

### 🐛 OPEN: arithmetic in FILTER operand position is parsed and then thrown away

`parse_comparison_expr` recognises `?var (+|-|*|/) term <cmp> term`, and then builds the
comparison from the LEFT VARIABLE ALONE:

```rust
let _arith_right = self.parse_term()?;   // parsed, dropped
...
"=" => Ok(FilterExpr::Equals(left, cmp_val)),
```

So `FILTER(?age + 5 > 30)` evaluates as `FILTER(?age > 30)` — silently the wrong predicate,
with no error. `FilterExpr` has no arithmetic node to hold the operation, so fixing it means
adding one (`Arith(Term, Op, Term)` as a comparison operand, or a general expression node)
plus executor evaluation for inline integers, and deciding what non-numeric operands do.
Filed rather than fixed: it needs an AST change, and the existing comment in that branch
("the executor will need to handle this — for now return a structural match") shows it was
known-incomplete when written.

### ✅ FIXED 2026-07-29: `&&` now binds tighter than `||`

`parse_bool_expr` was a single left-associative loop over both connectives, so
`a || b && c` parsed as `(a || b) && c` where SPARQL 1.1 means `a || (b && c)` — different
predicates, so any mixed-connective filter returned wrong rows. Split into two levels
(`parse_bool_expr` = `||` loop over `parse_and_expr` = `&&` loop), which is the
`ConditionalOrExpression`/`ConditionalAndExpression` shape from the spec.

Unlike the earlier grouping work this is **not** additive: it deliberately re-associates
existing mixed queries, which is why the previous session left it and pinned the old
behaviour in a test instead. Those queries were being evaluated as something their author
did not write, so the re-association is the fix. `precedence_binds_and_tighter_than_or`
(`loka-sparql/tests/filter_grouping.rs`) replaces the pin and asserts row counts on cases
where the two readings actually differ — including one that returns 2 rows under SPARQL
precedence and 1 under the old association.

### ✅ FIXED 2026-07-29: every FILTER leaf form composes in any position

`LANGMATCHES`, `LANG(?v) =`, `COALESCE`, `IF`, `DATATYPE(?v) =`, `STR(?v) =` and the
parenthesised `EXISTS` / `NOT EXISTS` were the last forms still parsed in `parse_filter`,
each consuming FILTER's own closing paren, so each worked as an entire filter and errored as
an operand (`FILTER(STR(?a) = "x" && ?b = 1)`). All moved to `parse_filter_inner` without the
extra paren; `parse_filter` now closes FILTER exactly once after the chain. The
*unparenthesised* `FILTER NOT EXISTS { ... }` form stays in `parse_filter`, matched before
FILTER's `(` — it has no outer paren to leave alone.

Two things came out of the move:

- **`peek_function`** — `peek_keyword` is word-bounded but `:` is not a word character, so
  `peek_keyword("STR")` matches the prefixed name `str:label`. Harmless while the branch only
  ran in leading position; once reachable as an operand it would demand a `(` and reject a
  valid query. The new helper requires a following `(`, and a test covers `str:` / `lang:` /
  `if:` / `datatype:` / `coalesce:` prefixes in operand position.
- **`COALESCE()` with no arguments** indexed `vars[0]` and panicked. It is a parse error now.

8 tests in `loka-sparql/tests/filter_leaf_position.rs` asserting row counts, not parse
success (a dead branch parses fine — that is how the string-equality defect hid). 439
workspace tests green.

<details><summary>Original finding, for context</summary>

Surfaced while building the Cypher transpiler. `parser.rs::parse_filter_inner` parses a
comparison, then optionally `&&` / `||` followed by a recursive call — a flat right-nested
chain. `parse_comparison_expr` expects a *term* in operand position, so a parenthesised
sub-expression is a parse error:

```sparql
FILTER((?a = 1) && (?b = 2))     -- parse error: expected term
FILTER(?a = 1 && ?b = 2)         -- ok
```

Two consequences: `!` only parses in leading position (`FILTER(!bound(?x))`), never nested;
and a disjunction with a conjunctive branch — `(a && b) || c` — cannot be expressed at all,
because the flat chain always associates to the right.

The transpiler works around both: it pushes `NOT` down to the leaves (De Morgan + operator
inversion) and splits top-level `AND`s into separate `FILTER` clauses, which SPARQL conjoins.
It rejects `(a AND b) OR c` with a message telling the user to rewrite in DNF.

Worth fixing in the parser proper — a real SPARQL 1.1 `Expression` grammar with grouping and
precedence — at which point the transpiler's workarounds can be simplified. Not urgent; the
workaround is correct, just narrower than SPARQL allows.

</details>

---


## Reference Architectures

| System | Why |
|--------|-----|
| [Qdrant](https://github.com/qdrant/qdrant) | HNSW impl, visited pools, normalize-at-insert |
| [Oxigraph](https://github.com/oxigraph/oxigraph) | RDF storage, SPO/POS/OSP, SPARQL pipeline |
| [DataFusion](https://github.com/apache/datafusion) | Cost-based planning, join ordering, vectorized execution |
| [DuckDB](https://github.com/duckdb/duckdb) | Columnar analytics, zonemap pruning, join ordering |
| [GlueSQL](https://github.com/gluesql/gluesql) | Small readable query engine |
| [Limbo](https://github.com/tursodatabase/limbo) | Rust SQLite reimpl, storage ideas |
| [Materialize](https://github.com/MaterializeInc/materialize) | Streaming SQL on Differential Dataflow |

---

## Completed (185 items)

<details>
<summary>Click to expand</summary>

### Query Engine Optimization
- [x] Cost-based query planning: cardinality estimation integrated into join ordering
- [x] Predicate pushdown: FILTERs repositioned after the pattern that binds their last variable
- [x] HNSW edge labeling: distinct predicates for vertical descent vs horizontal neighbor edges
- [x] HNSW typed edge filtering in executor (hnswHorizontalNeighbor, hnswLayerDescend)
- [x] Join strategy selection: cost-based hash join on subject, hash join on object, nested-loop
- [x] Object hash join: reverse-traversal optimization using POS/OSP indexes
- [x] Hash join threshold lowered from 100 to 50 for earlier amortization
- [x] Directional HNSW edge encoding for SPARQL property path traversal
- [x] Make virtual HNSW edge triples queryable in SPARQL patterns
- [x] Label vertical vs horizontal HNSW edges with distinct predicates
- [x] Encode directionality for property path descent/fan-out

### Database Health Dashboard
- [x] `loka health` CLI command with AI-readable structured output
- [x] HNSW health: tombstone ratio, layer distribution, avg/min/max connectivity, entry point diversity
- [x] Pseudo-table health: coverage ratio, cliff steepness, segment count, avg tail properties
- [x] Storage health: triple count, term dictionary size, unique predicate count
- [x] HNSW rebuild via `loka health --rebuild-hnsw`

### Pseudo-Tables & Vectorized Execution
- [x] Property model: predicate + position (Subject/Object) pairs per node
- [x] Property extraction: full graph scan to build PropertySet for every node
- [x] Group discovery: Jaccard-similarity merging of characteristic sets (≥80% overlap)
- [x] Pseudo-table materialization: columnar storage with ≥33% threshold columns, null support
- [x] Tail property tracking: per-row count of properties not in the pseudo-table schema
- [x] Cliff steepness metric: core/tail coverage ratio for schema health assessment
- [x] Per-column statistics: min/max/null_count/distinct_count (DataFusion Precision<T> pattern)
- [x] Segment-level storage: ~2048 rows per segment for zonemap granularity
- [x] Zonemap pruning: per-segment min/max skips entire segments
- [x] Row sorting by most selective column for tighter zonemaps
- [x] Vectorized column scans: scan_column_eq, scan_column_range, scan_column_not_null
- [x] SIMD-accelerated TermId comparison: packed columns (dense u64 + sentinel nulls), AVX2 (4 u64/cycle), SSE2 (2 u64/cycle)
- [x] Batch scan intersection: sorted merge for multi-column predicate evaluation
- [x] Query planner integration: recognize pseudo-table-matching SPARQL patterns
- [x] Expose health metrics via health endpoint / Loka Studio

### Core Engine
- [x] Database configuration model, HNSW edges as virtual RDF triples
- [x] VECTOR_SIMILAR + VECTOR_SCORE, planner integration, ef/k hints
- [x] VectorRegistry, ORDER BY, UNION, N-Triples/N-Quads/Turtle/RDF-XML/JSON-LD parsers
- [x] POST /triples, /vectors/declare, /vectors, /graph, /graph-store endpoints
- [x] PersistentStore (sled), persistent term dictionary, HNSW rebuilt on startup
- [x] SIMD distance functions (AVX2/FMA + SSE), HashSet visited list
- [x] Multiple HNSW entry points, HNSW compaction, parallel bulk_insert (rayon)
- [x] Hash joins, cardinality estimation, materialized adjacency lists
- [x] Named graph support (Triple::quad), crash recovery (verify + repair)
- [x] Query timeout enforcement, LIMIT push-down

### SPARQL Completeness
- [x] SELECT, ASK, CONSTRUCT, DESCRIBE, INSERT DATA, DELETE DATA
- [x] BIND/VALUES, GROUP BY/HAVING, aggregates (COUNT/SUM/AVG/MIN/MAX)
- [x] Property paths (+, *, ?, /), Subqueries, RDF-star quoted triples
- [x] FILTER: =, !=, <, >, <=, >=, &&, ||, !, NOT EXISTS, EXISTS
- [x] String functions: CONTAINS, STRSTARTS, STRENDS, REGEX
- [x] LANG, LANGMATCHES, DATATYPE, STR, COALESCE, IF, isIRI, isLiteral
- [x] Arithmetic expression parsing, OPTIONAL, UNION, DISTINCT, PREFIX

### HTTP & Server
- [x] Content negotiation (Accept → JSON/XML/CSV/TSV)
- [x] Passcode authentication, rate limiting, query timeouts
- [x] HNSW health endpoint, service description, Graph Store Protocol
- [x] Periodic backups (--backup-interval), schema declaration via SPARQL
- [x] GET /graph (Turtle/N-Triples export)

### SDKs & Ecosystem
- [x] 6 SDKs (Python, TypeScript, Go, Rust, Java, .NET) + endpoint fixes
- [x] Python OWL validation (domain/range/subclass/disjoint/equivalent/sameAs/inverse)
- [x] Verification query generation, integration test CI, publish workflow
- [x] LangChain VectorStore, Jupyter %%sparql magic, MCP server (6 tools)
- [x] Agent installer CLI (--launch-studio), Protege plugin, Dockerfile

### Loka Studio (Flutter)
- [x] Desktop/web scaffold, Dart client, force-directed graph
- [x] View modes, triple editor, SPARQL editor, ontology viewer
- [x] HNSW health diagnostics, heatmap, backup management panel
- [x] IRI shortening, click-to-expand, predicate filtering, triple list panel
- [x] Japanese labels, HNSW virtual edges, dark/light theme, persistent settings
- [x] OWL export, graph export hint, Windows desktop platform

### Data & Benchmarks
- [x] 82K triples + 79K vectors (embedding-mapping), 500K+1M stress test
- [x] 439 Wikidata BFS import (16K triples), 435 Japanese embeddings
- [x] Benchmark suite: <1ms queries, 20K inserts/sec, 40ms full export
- [x] Storage benchmark baseline (sled), IRI encoding evaluation

### ACID Compliance
- [x] Atomicity: sled multi-tree transactions for SPO/POS/OSP insert and remove
- [x] Consistency: startup verification (verify_consistency + repair) on persistent store open
- [x] Isolation: PersistentStore wrapped in RwLock; vector inserts hold store+vectors locks together
- [x] Durability: explicit flush() after all server mutation endpoints before returning success
- [x] Error propagation: all persistent write errors reported to caller (no silent `let _ =`)
- [x] GSP DELETE clears persistent store and flushes

### Native MCP Server
- [x] `loka mcp` command: native Rust MCP server built into the binary (no Python needed)
- [x] Dual-mode: `--url` for server mode, `--data-dir` for serverless mode
- [x] 12 tools: health_report, rebuild_hnsw, verify_consistency, database_info, sparql_query, insert_triples, backup, vector_search, download_studio, launch_studio, check_update, decline_update
- [x] Auto-update on MCP startup with 2-minute decline window (`--no-auto-update` to disable)
- [x] Direct library calls in serverless mode (no PATH dependency on `loka` binary)
- [x] MCP resources: loka://connection, loka://version, loka://schema
- [x] MCP prompts: explore_graph, find_similar, count_by_type query templates
- [x] MCP notifications: notifications/message for update progress, HNSW rebuild progress
- [x] Async stdin loop with tokio::select! for concurrent notification delivery
- [x] Backup works in server mode (exports via /graph endpoint)

### Documentation
- [x] Agent setup guide, SDK publishing/accounts guides, session notes
- [x] README, Open Graph meta tags, AI agent website callout

</details>

## Benchmark Results

Benchmark results are tracked automatically by CI. See:
- **[benchmarks/LATEST.md](benchmarks/LATEST.md)** — most recent Criterion results
- **[benchmarks/HISTORY.md](benchmarks/HISTORY.md)** — full history over time

### Baseline (manual, 16K triples, 435 vectors)

| Query | Latency |
|-------|---------|
| Health check | 0.6ms |
| SELECT LIMIT 10 | 0.7ms |
| SELECT LIMIT 1000 | 5.2ms |
| 2-pattern join | 0.6ms |
| GROUP BY aggregate | 0.6ms |
| FILTER CONTAINS | 0.4ms |
| OPTIONAL | 0.7ms |
| INSERT/DELETE DATA | <1ms |
| Full Turtle export (16K) | 41ms |
| N-Triples export | 35ms |
| Bulk insert (2000) | 76ms (20K/sec) |
| Point lookup p50 | 0.61ms |
| Point lookup p99 | 1.25ms |

## Electron Loka Studio — desktop installers (added 2026-05-30)

The Flutter Studio was deleted 2026-05-30; Loka Studio is now `web-studio/` (JS) shelled
by `loka-studio/electron/`. The release pipeline (`.github/workflows/release.yml`) no
longer ships a built desktop Studio — its Flutter `build-studio` job was removed.

Replace it with a job that packages the Electron Studio into per-platform desktop
installers (electron-builder or electron-forge): bundle `loka-studio/electron/` +
`web-studio/`, produce Windows (NSIS `.exe`), Linux (AppImage/`.tar.gz`), macOS
(`.dmg`/`.app`). Re-add the resulting assets to the `release` job's `files:` list.
**Must be verified on a throwaway `v*-rc` tag before trusting it** — release.yml is
tag-triggered, so it cannot be validated by a normal push. Until then releases are
engine-only. Pairs with the website's "forthcoming .exe installer" line.
