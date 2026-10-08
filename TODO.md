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

## ✅ 2026-07-30: `--version` now carries a build stamp, so staleness is visible

`loka --version` printed `CARGO_PKG_VERSION` alone, so a May build and today's build both said
`loka 0.4.1`. That single fact is why the trap below stayed invisible for two months, and it has now
cost three separate investigations:

1. `loka serve` binding 0.0.0.0 — Emma's overnight firewall prompts — after the 127.0.0.1 fix had
   been in source for weeks.
2. Three "engine bugs" investigated and filed, none of which reproduce on current source.
3. **2026-07-30:** a Pramana verification run failed with `parse error … expected prefixed name` on
   exactly the queries the parser had just been fixed to accept — because the release binary had been
   rebuilt an hour *before* that fix landed. Same trap, one hour of drift instead of two months.

Now: `loka 0.4.1 (306a148 2026-07-30T05:09:58Z)`, with `-dirty` when the tree has uncommitted
changes, and the same string in the serve banner (`Loka 0.4.1 (…) listening on …`) so a *running*
server declares its build. `loka-cli/build.rs` stamps sha + UTC time, re-running when `.git/HEAD` or
the index moves so an incremental build cannot keep a stale stamp. Best-effort: no git, no crash —
the sha becomes `unknown`.

`GET /health` deliberately still returns exactly `ok`; Pramana's client only checks the status code,
but changing a health body to carry diagnostics is how a health check stops being a health check.

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

## Pramana dogfooding bugs (2026-07-20): not reproducible on main as of 2026-10-08

The two bugs logged here (prefixed predicate + literal object matching nothing; an object-variable
join into a literal-bound pattern returning 0 rows) don't reproduce on current main. Checked:
in-memory executor; HTTP server with N-Triples ingest and the planner; a persistent store before
and after a restart; full and prefixed IRIs; both join orders. Regression tests:
`loka-sparql/tests/pramana_bugs.rs`, `loka-proto` `pramana_label_and_uuid_join_shapes_match`.
If Pramana hits either again, reopen with that store's data: the original report said the
behaviour varied between stores.

The 2026-07-20 addendum is closed too:
- **"Nondeterministic across processes":** ten fresh `loka serve` processes on one
  persistent 48k-triple store returned identical rows for 12 join queries. The planner's
  order is deterministic (Vec order, first minimum), guarded by
  `the_planner_picks_the_same_order_every_time`.
- **"~2 s per lookup":** found and fixed: the planner materialised matches to count them
  (DEVLOG, 2026-10-08).

# DO THE STUFF IN THE QUEUE.MD

This is very important! Please actually do the stuff in that file! Do all of it. Base it off of the actual Loka repo's usage of it and its description of it in the CLAUDE.md. The actual Loka repo for the programming language has one that's relatively well done, although it is a bit messy at the same time.

Also, please, I don't know why it is that this TODO.md is so cluttered, and you will really need to actually work on the clutter here. I'd say, for this particular Q file that we have, it has the stuff that I consider to be kind of most important to do immediately. Add every single thing here, except for the rename into the queue.md, so that we can work on this stuff. 

---

## Paper / arXiv — ON HOLD (Emma, 2026-10-08)

"The arXiv thing is on hold now with whatever we have right now." The state it holds at:

- `paper/paper.md`: latest clawRxiv review v16 (post 2915) **Accept**
  (<https://clawrxiv.io/abs/2915>). The remaining cons are model-bound (weak model, label
  output, exact-match resolution; checked 2026-10-07) or beyond the laptop (billion-triple scale).
  Strong Accept would need a better model, i.e. training, which Emma declined.
- Submission package: `paper/arxiv/METADATA.md` (fields, abstract, steps). The source tarball
  and PDF are built by `paper-pdf.yml`, and a copy is published at
  <https://loka.emmaleonhart.com/arxiv/>.
- To resume: Emma submits on arxiv.org (her account, her licence choice), then the arXiv
  identifier goes on the `/arxiv/` page.

## Windows installer — multi-model support

The Inno Setup installer (`installer/loka.iss`) currently offers a single
optional inference model (Qwen 2.5 1.5B Instruct, declared in
`installer/models.toml`). Extend so the user can choose between several
models at install time. Loose plan:

- [x] `installer/models.toml` is a list: Qwen 2.5 1.5B (default) and 0.5B (2026-10-08)
- [x] `installer/gen_models.py` generates the `[Types]`/`[Components]` and a `SelectedModel`
      Pascal function; `release.yml` runs it before ISCC; CI tests it (`installer-models` job)
- [x] Model components are `exclusive` children of one "model" checkbox (pick one, or none)
- [x] `install-selection.toml` records the chosen model's id and repo
- [ ] **Unverified until a release build:** `loka.iss` itself only compiles under ISCC on a
      `v*` tag (no Inno Setup here). Check the first rc/tag build.
- [ ] Nothing reads `install-selection.toml` yet: `loka.exe` has no first-run model fetch.
      That's the step that makes the model choice do anything.

Future candidates: a smaller-footprint Qwen / Phi / Llama option for users
without 3 GB to spare, and a "bring your own GGUF" file picker.

---

## Next Release (v0.3.1) — Gradle Migration, MCP Agentic UX

Merge the Gradle migration (local) and MCP agentic UX work (claude.ai remote session) then cut v0.3.1.

### Release Checklist: obsolete (checked 2026-10-08)
- [x] Merge claude.ai remote branch (MCP agentic UX work) into main
- [x] Merge Gradle migration setup: done (`sdks/java/build.gradle.kts`, no `pom.xml`)
- [x] ~~Bump version to 0.3.1 in all SDK configs~~: not needed. `publish-sdks.yml` sets every
      SDK's version from the git tag at publish time.
- [x] Tag `v0.3.1`: exists, and releases have since reached `v0.4.6`

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
- [x] Integration test: start Loka, insert triples, query, verify round-trip (`LokaIntegrationTest`, run in CI's sdk-java job against a live server, 2026-10-07)

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
- [x] End-to-end test: fresh install → insert → query → verify (`loka-cli/tests/install_agent_e2e.rs`, adds a restart-and-query persistence check; 2026-10-07)

### HNSW Traversal via SPARQL Property Paths
- [x] `+`/`*` paths walk the virtual HNSW edges (`loka-sparql/tests/hnsw_paths.rs`; 2026-10-07)
- [x] Greedy descent: `?s loka:hnswNeighbor+ ?n GREEDY(vector)` (`loka-sparql/tests/path_until.rs`; 2026-10-07)
- [x] Beam search as a path mode: `?entry loka:hnswNeighbor+ ?n BEAM(vector, k)` (2026-10-07)

### Predicate-Based Exit Conditions (UNTIL)
- [x] All of it, 2026-10-07: syntax in `planning/until-syntax.md`; per-step evaluation,
  per-branch exit, BFS-then-value order, GREEDY local-optimality exit; tests in
  `loka-sparql/tests/path_until.rs`.

### Cost-Based Query Planning (remaining)
- [x] HNSW as access path: planner chooses "HNSW index scan" vs "SPO triple scan" based on cost (`planning/cost-based-hnsw.md`; 2026-10-07)
- [x] Adaptive execution v1 (sampling only): reorder commuting joins mid-query from sampled row counts (`planning/adaptive-execution.md`, 2026-10-08)
- [x] Adaptive execution v2: reorders cross EXISTS-free FILTERs when the moved pattern binds none of the filter's variables (2026-10-08)
- [ ] Maintained distinct counts for adaptive execution, only if sampling proves too noisy on real data

### Background Maintenance Cycle
- [x] Low-usage detection, background HNSW rebuild off the lock, atomic swap: `loka serve --maintenance-idle-secs` (`planning/background-maintenance.md`; 2026-10-07)
- [x] Background pseudo-table rediscovery: the idle maintenance cycle rediscovers once the store has changed (2026-10-07)
- [x] `INSERT DATA` / `DELETE DATA` accept `f32vec` literals: inserted in canonical form and indexed; deleted by value match, tombstoning the HNSW node (2026-10-07)

### Pseudo-Tables (remaining)
- [x] Invalidation tracking (column-level, per-predicate store generations) and serving from exact columns, rediscovered in the idle maintenance cycle (`planning/pseudo-table-serving.md`; 2026-10-07)
- [x] Multi-pattern star queries over one subject are fused into one columnar scan (existing fused scan, now only over exact, current columns)
- [ ] Deep (multi-hop) pseudo-tables never serve queries: their columns are paths, not predicates; serving them needs path-level exactness. Spec: `planning/deep-pseudo-table-serving.md` (2026-10-08). Measured worth building (35 → ~4 ms on a 20k-root chain); first step is storing each column's full path

### Database Health Dashboard (remaining)
- [x] Query performance metrics: per-pattern latency percentiles, planner estimate accuracy (`GET /health/queries`, `planning/query-metrics.md`; 2026-10-07)
- [x] Studio page reading `/health/queries`: Query performance section of the web-studio Health tab (2026-10-08)
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

**Ordering (`<`, `>`, `<=`, `>=`) — FIXED 2026-10-07.** It compared raw `TermId`s, meaningful
only for inline integers and temporal ids, so it was kept narrow (string ordering matched
nothing). It now compares values: strings with strings, IRIs with IRIs, numbers numerically,
temporal ids by id; mixed kinds are a type error (no match). The same id-vs-value bug was in
ORDER BY and is fixed there too. Tests: `ordering_on_strings_and_iris_compares_values` and
`string_ordering_is_by_value_not_insertion_order` in `loka-sparql/tests/filter_equality.rs`.

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

**Both remaining items CLOSED 2026-07-29 — see the sections below**, along with arithmetic in
operand position and a numeric-ordering defect that fell out of it. What is left of a full
SPARQL 1.1 `Expression` grammar is **operator precedence inside arithmetic** (`?a + 2 * 3`
associates left instead of binding `*` tighter) — pinned by a test, not silently wrong-shaped.

The Cypher transpiler's two workarounds — pushing NOT to the leaves and splitting
top-level ANDs into separate FILTER clauses — are still correct and still tested, but are
no longer *necessary*. It can emit grouped filters directly and drop its
`(a AND b) OR c` rejection whenever someone wants to simplify it.

### ✅ FIXED 2026-07-29: arithmetic in FILTER operand position was parsed and thrown away

`parse_comparison_expr` recognised `?var (+|-|*|/) term <cmp> term` and then built the
comparison from the LEFT VARIABLE ALONE — `let _arith_right = self.parse_term()?;`, dropped —
so `FILTER(?age + 5 > 30)` evaluated as `FILTER(?age > 30)`. Wrong predicate, no error. The
branch's own comment ("the executor will need to handle this — for now return a structural
match") shows it shipped known-incomplete.

Fixed with a `Term::Arith { left, op, right }` node (+ `ArithOp`) produced by a new
`parse_arith_operand`, which is used for **both** sides of a comparison — so `24 < ?age + 5`
works too, having previously been a parse error. Evaluation is `numeric_operand` in the
executor, recursive over nested arithmetic.

Decisions worth knowing:

- **`f64`, not `i64`.** Division has to mean division: truncating integer division would make
  `?a / 3 = 2` true for `?a = 7`. Values come from a 56-bit integer encoding, well inside
  f64's exact-integer range, so nothing is lost on the way in.
- **Division by zero yields no value**, so the enclosing comparison is false. SPARQL raises a
  type error there, which has the same effect on the filter; the alternative is an infinity
  that can satisfy a comparison.
- **Non-numeric operands match nothing** rather than erroring, matching how unresolvable terms
  already behave throughout filter evaluation.
- **Arithmetic in pattern position is `Ok(None)`/`None`** in `resolve_term` and the planner's
  `term_to_constant_id` — it is a filter-operand-only term with a value but no interned id.

**Operator precedence — FIXED 2026-10-07.** `*` and `/` bind tighter than `+` and `-`; unary
minus and parenthesised arithmetic parse. Pinned by `arithmetic_respects_operator_precedence` and
`unary_minus_negates_an_operand` in `loka-sparql/tests/filter_numeric_ordering.rs`.

### ✅ FIXED 2026-07-29: ordering comparisons were wrong whenever a negative integer was involved

Found while writing the arithmetic tests, and worse than the bug being fixed. Ordering compared
raw `TermId`s. An inline integer's payload is two's-complement in the low 56 bits, so a negative
value sets the payload's high bit and the **unsigned** id sorts above every positive one.

- `FILTER(?t > -5)` dropped rows it should have kept.
- Worse: in a store containing any negative value, `FILTER(?t > 4)` was ALSO wrong — the
  negatives outranked the bound and came back as matches. So the blast radius was not "queries
  with negative literals", it was "any ordering query over a column that has negatives".

`compare_filter_terms` now decodes both operands to numbers and compares values, falling back to
raw-id comparison only for the non-numeric residue (temporal literals, whose ids are
chronological by construction; the deliberately-narrow string behaviour is unchanged).

9 tests in `loka-sparql/tests/filter_numeric_ordering.rs`; all 9 failed before the fix, and two
of them fail on the *positive*-bound cases, which is the part that would have been easy to miss.
448 workspace tests green.

### 🚨 FIXED 2026-07-29: five of the nine query shapes our own consumer sends did not parse

The find that matters most today, and it came from asking a question the test suite could not:
**what SPARQL does Pramana actually send us?** Pramana is the ERP-for-agents store that runs on
Loka (118k triples, `src/sparql_connector.py` → `GET /sparql`). Extracting the distinct query
shapes from its Python and running them through `parse()`:

| shape | before |
|---|---|
| `FILTER(LCASE(STR(?label)) = LCASE("Water"))` — entity resolver | **parse error** |
| `FILTER(CONTAINS(LCASE(?label), LCASE("wat")))` — search box | **parse error** |
| `FILTER(STRSTARTS(REPLACE(STR(?uuid), "-", ""), "a5bc"))` — uuid lookup | **parse error** |
| `FILTER(STRSTARTS(STR(?property), "http://…/direct/"))` — property filter | **parse error** |
| `BIND(REPLACE(STR(?type), "^.*/", "") AS ?typeLocal)` — entity page | **parse error** |
| the other four (NOT EXISTS, isLiteral, `!=`) | ok |

Two causes: `LCASE`/`UCASE`/`REPLACE`/`STRLEN`/`CONCAT` did not exist at all, and every string
function's *arguments* were parsed with `parse_term`, so even `STRSTARTS(STR(?p), "…")` — using
only functions that did exist — was rejected because one function call cannot contain another.

**Why nobody noticed:** Pramana's client returns `None` on a non-200 and its callers treat that
as "no results", so its entity page, search box and entity resolver rendered *empty* rather than
erroring. On the Loka side every test passed, because the tests only ever used the shapes Loka's
author thought to write. A hand-written test suite tests the author's imagination; the consumer
tests reality.

Fixed by a `Term::Func { func, args }` node and a `parse_value_expr` that arguments recurse
through, so nesting works everywhere a value is expected — filter operands, string-function
arguments, and BIND. Functions: `STR`, `LCASE`, `UCASE`, `STRLEN` (numeric), `REPLACE`, `CONCAT`,
with arity checked at parse time.

Consequences worth knowing:

- **`REGEX` was a substring match**, with a comment admitting it ("full regex would need a regex
  crate"), so `REGEX(?s, "^zzz")` matched anything *containing* the text `^zzz` and no anchor or
  character class worked. `REPLACE` needs the same machinery, and `regex` was **already in the
  lockfile** transitively, so both now use a real regex, compiled once per pattern and cached
  (filters run per row).
- **The bespoke `STR(?v) = x` branch is gone**, along with `FilterExpr::StrEquals`. It accepted
  only a variable argument and only `=`, so `STR(?a) != "x"` and `LCASE(?a) = "x"` were errors
  while `STR(?a) = "x"` worked. One value path now, not two — two paths with different notions of
  what a comparison means is how the string-equality defect survived.
- **Mixed numeric/string comparison is now a type error (no match)** instead of a lexicographic
  comparison of "Water" against "4". Without that rule `FILTER(STR(?label) > 4)` matched
  every row.
- **`BIND` takes an expression.** Numeric results bind for real (an inline integer needs no
  dictionary write). **String results return an explicit "not supported yet" error**, because
  binding one means interning a new literal and `execute` holds `&TermDictionary`. The available
  alternatives were an error or a variable that binds only when the string already happens to be
  in the dictionary; a column that silently vanishes is the exact failure shape this week has
  been spent removing. Implementing it properly needs either `&mut` in the executor's public API
  or a per-query value overlay — a real design decision, not a patch.

`loka-sparql/tests/value_functions.rs` keeps the consumer's real shapes as a permanent regression
test, so a future change that breaks what Pramana sends fails here rather than in a blank page.
459 workspace tests green.

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
