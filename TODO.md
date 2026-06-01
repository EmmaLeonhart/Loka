# Loka — TODO

**Status: 228 of 249 items complete (92%)**

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
- [ ] OWL validation (match Python SDK: domain/range/subclass/disjoint/equivalent)
- [ ] Connection retry logic with configurable timeouts

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
- [ ] Serverless mode testing (no `--serve`, just create the `.sdb`)
- [ ] Agent-consumable structured output (JSON mode for programmatic setup)

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
- [ ] `loka health --json` mode for programmatic agent consumption
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
- [ ] Cypher → SPARQL transpiler: MATCH/WHERE/RETURN mapped to SPARQL patterns
- [ ] GQL (ISO 39075) → SPARQL transpiler: ISO standard graph query language mapped to SPARQL
### Query validation: reject constructs that can't map to the RDF data model

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

## Website

- [ ] **Benchmarks page chart is visually weird (non-critical).** On
  `pages/benchmarks/` the release-milestone markers (red dashed lines
  + `v0` / `v0.3.5` / `.3.4` / `v0.3.7` / `v0.4.0` pills) overlap and
  crowd the x-axis, and the early milestones bunch together
  illegibly. Emma flagged it 2026-05-16 — important to fix but not
  blocking the cross-site visual-identity work. Fix: dedupe/space the
  milestone labels (stagger or collapse adjacent ones), lighten the
  marker styling so it doesn't fight the series lines, and make sure
  it reads at mobile width. Keep the data correct — only the
  presentation is the problem.

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
