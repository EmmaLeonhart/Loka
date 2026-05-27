# Loka — Claude Code Context

## What This Is

Loka is a lean, high-performance RDF-star triplestore written in Rust with native HNSW vector indexing and a hybrid SPARQL extension. It is a single-purpose database: store triples, answer queries, at any scale.

It is **not** a combination of existing databases. It replaces both a vector database (e.g. Qdrant) and a SPARQL triplestore (e.g. Apache Jena Fuseki) with a single unified system where vectors are just triples.

Full architecture: see `docs/architecture.md`.

---

## Workflow Rules

- **v14 is donor-only.** v11–v13 are tractable on Emma's laptop (4070 Laptop, 8 GB VRAM); v14 (4M-triple corpus, ~40 h sustained GPU) is not. Donor flow: `tools/contribute_v14_training.py`. Full instructions: `pages/contribute/index.html` (live at <https://loka.emmaleonhart.com/contribute/>) and README's "Contributing GPU time" section. **Do NOT add a self-donation/cloud-GPU step to any cron** — v14 stays donor-only until that changes.
- **Training-box hardware — laptop, thermally constrained.** Full specs and 2026-05-13 instability verdict in `planning/system-instability-verdict-2026-05-13.md`. Binding constraints:
    - **RTX 4070 Laptop, ~8 GB VRAM, 35–115 W TGP** (NOT the 200 W desktop 4070). Power-cap-bound during sustained training (~74 W against an 80 W cap observed v13 epoch 4).
    - **Thermally marginal for sustained N-hour GPU runs.** 10 firmware-level unexpected-shutdown events Mar 27 → 2026-05-13 (no BSOD, no minidump — OS doesn't see them coming).
    - **Workload serialisation is mandatory:** `loka serve` + ingest + training must not run concurrently. The v9/v10 auto-cron pattern succeeded only because the corpus was small (94k triples); at 50M scale it thermally overloads the box.
    - **TDR registry:** raise `TdrDelay` to 10 s before any long training run (default 2 s is too short for sustained CUDA kernels).
    - **Halve desktop-tuned batch sizes** (e.g. `--batch-size 32` not 64) when running locally.
    - **Prefer cloud GPU rental for long runs** (Lambda / RunPod / Vast.ai, ~$5 / 3.5 h pass). Corpus + tokenizer already on Hugging Face. Treat the laptop as dev box, cloud as training box.
    - **Reported perplexity numbers are laptop-config.** A contributor on a 24 GB card at `--batch-size 64` is a different data point, not just faster.
    - **New loops/crons:** budget for thermal envelope, not theoretical capacity. Add temperature gates, sleeps between heavy phases, consecutive-failure stops.

  **Status (2026-05-13):** `loka-v11-kickoff` Disabled; `training_cron.py` + `post_eval_cron.py` on hold until at least one v11 cycle completes by hand under the verdict's mitigation list.
- **Quiet windows.** Sometimes the user explicitly requests a no-commit / no-push window — for example, "do not commit and push anything until 8 hours from now" while a downstream review pipeline catches up. Respect the window exactly. The default "commit early and often" rule below is suspended during quiet windows. The user will declare the window verbally; record the declared end-time in DEVLOG.md so a future session can see it. The 2026-05-11 declaration was: "no commits/pushes for 8 h after 22:35 UTC; then a post-eval cron fires every 6 h starting +12 h, for up to 48 h total." See `tools/post_eval_cron.py`.
- **Commit early and often.** Every meaningful change gets a commit with a clear message explaining *why*, not just what.
- **Plan into `queue.md` FIRST, then execute.** When entering planning mode (or doing any multi-step think-before-do), the FIRST action is to write the plan into `queue.md` as concrete items. Only then begin executing. Chat context dies on session interrupt; the queue survives.
- **Update `queue.md` in the same commit as the work.** Delete completed items in the same commit — do not leave checkmarks or status markers behind. A stale queue.md is worse than no queue.md.
- **Mirror `queue.md` into the task tool.** `TaskCreate` items as you add them to queue.md; mark `in_progress` when starting; `completed` when done. The two views must not drift.
- **Do not enter planning-only modes.** All thinking must produce files and commits. If scope is unclear, create a `planning/` directory and write `.md` files there instead of using an internal planning mode.
- **Implement every discussed idea BEFORE starting a run; discuss before you escalate a run.** Recurring past failure (e.g. the 2026-05-18 fine-tune session): a design is discussed — optimizer checkpointing, a watchdog, per-epoch HF push, the fine-tune model itself — and then a training run is launched *without those pieces actually built*, or a model is "approved" and never implemented. Do not start (or restart) a training run while any part of the agreed plan is still only discussed. Build it, commit it, then launch. **And** any spec/config change that makes training more demanding or longer — bigger corpus slice, more epochs, larger base model, larger batch or sequence length, higher LoRA rank, anything that increases wall-clock or VRAM — MUST be raised with the user and agreed *before* the run is (re)launched. Never silently scale a run up; a surprise on training duration is not acceptable.
- **Keep this file up to date.** As the project takes shape, record architectural decisions, conventions, and anything needed to work effectively in this repo.
- **Update README.md regularly.** It should always reflect the current state of the project for human readers.
- **Every release MUST have informative release notes.** When tagging a release, always write a proper description covering what changed and why — features, fixes, breaking changes. Never leave auto-generated "What's Changed" boilerplate as the release description. Use `gh release edit` to fix descriptions retroactively if needed. Uninformative release notes make the software look abandoned.

## Queue and longer-horizon work

(Clarity model adopted from the `cleanvibe` scaffold — the bar for "clear project docs.")

- **`queue.md`** — what is being worked on *right now*: concrete, executable steps. Items are deleted in the same commit that completes them — no checkmarks, no "done" markers, no status narration, no progress snapshots. If a line is still in `queue.md`, it is not done. If a task is not in `queue.md`, it is not in scope for the current session.
- **`todo.md`** — the long-term horizon: abstract, multi-session goals. A *destination, not a step*. `todo.md` is the *basis for* `queue.md`; parked / deferred / reference material lives here, never in `queue.md`.
- **Forward flow only:** `todo.md` (abstract) → `queue.md` (concrete steps) → task tool (in-flight) → `git log` (history). Items only move forward; done work is deleted, not annotated. A stale `queue.md` is worse than no `queue.md`.

---

## Training & Ship Workflow

Each successive model version (v6 → v7 → v8 → ...) goes through a fixed pipeline. The full cycle is automated by `tools/training_cron.py` for v9+; before that the steps were run by hand. **When a new version finishes training, all of these must happen — half-shipping a checkpoint is worse than not shipping.** This is the canonical checklist.

### 0. Preconditions

- `huggingface-cli login` has been run once with a write-scoped token (HF push uses it).
- `cargo build --release -p loka-cli` has produced `target/release/loka.exe` (or `.so`/`.dylib`) — the cron's HF-import step needs the Loka binary to stand up a per-cycle data dir.
- The BPE tokenizer (`training/data/tokenizer_bpe.json`) and BPE vocab (`training/data/vocab_bpe.json`) are present. They have been stable since v6 and are pinned via `MODEL.json`.
- `MODEL.json` is committed and points at the most-recent shipped checkpoint. If it points at vN-1 when you're shipping vN, that's normal — the workflow below updates it.

### 1. Training corpus

If `--no-data-refresh` is not set, the cron pulls a fresh HF slice into a per-cycle Loka instance, then preprocesses to `training/data/triples_vN.txt` with the v7 datatype filter applied. The `wikidata_hf_import_state.json` is stashed-and-restored per cycle so the importer doesn't think prior runs already filled the new Loka.

Without `--no-data-refresh` (manual run from a populated Loka):

```bash
python training/preprocess.py --endpoint http://localhost:3030 --output training/data/triples_vN.txt
```

### 2. Train

```bash
python training/train.py \
    --data training/data/triples_vN.txt \
    --vocab training/data/vocab_bpe.json \
    --bpe-tokenizer training/data/tokenizer_bpe.json \
    --checkpoint training/checkpoints/wikidata_vN.pt \
    --epochs 20 --batch-size 64 --tokens-per-role 8 \
    --d-model 512 --nhead 8 --layers 6
```

Always log to `training/logs/vN_train.log`. The log is committed (part of the version record).

### 3. Evaluate — propgen test

```bash
# Compute PageRank for the seed (cached when fresh)
python tools/compute_pagerank.py --nt-file training/data/seed_Q42.nt --output training/data/pagerank_Q42.json

# Run the propgen test, rank-biased
python training/test_autoregressive_propgen.py \
    --seed-file training/data/seed_Q42.nt \
    --output training/data/test_propgen_Q42_vN.nt \
    --checkpoint training/checkpoints/wikidata_vN.pt \
    --bpe-tokenizer training/data/tokenizer_bpe.json \
    --rank-file training/data/pagerank_Q42.json \
    --max-source-triples 30 --children-per-source 10 \
    --confidence 0.25 --model-version loka-wikidata-vN
```

Inspect the output. Compare to the previous version's `_meta.json`: catalog vs semantic predicate ratio, emission volume, citation-pool growth, asymmetric-drop count. These are the headline numbers for the DEVLOG entry.

### 4. DEVLOG entry

Append a new top-of-file section to `DEVLOG.md`, mirroring the v7/v8 entry structure:

- Headline one-liner (what changed, what improved/regressed)
- Per-epoch perplexity table from the training log
- Generation-comparison table (vN vs vN-1 vs older comparisons that still matter)
- Selected outputs with raw model emissions (don't hide BPE artifacts — they're real)
- Status block: where the checkpoint lives, HF tag, MODEL.json state, next planned step

### 5. paper/paper.md

Add `§5.X` for vN under "Experiments", with the same table structure as the DEVLOG entry. Update the abstract to past-tense with the new headline number. Add the new tag to the snapshot list at the top of the paper.

### 6. Push checkpoint to Hugging Face

```bash
python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name vN --no-loka-data
```

This auto-discovers all `wikidata_v*.pt` in `training/checkpoints/`, uploads the new ones, tags `vN`, and **regenerates the dataset README** so the description on https://huggingface.co/datasets/EmmaLeonhart/loka reflects the current pin. The README template lives in `tools/hf_snapshot.py` (`README_TEMPLATE` + `render_readme()`); when fields change, update the template, not just MODEL.json.

### 7. Pin the new version

Edit `MODEL.json`: bump `name`, `version`, `released`, `revision`, and `files.checkpoint.{in_repo,local}`. Rewrite `notes` to describe what's distinctive about this version (architecture changes, corpus changes, headline result). The notes are visible in the HF README via the `{{LATEST_*}}` placeholders.

### 8. Commit + push

One commit covers DEVLOG + paper + MODEL.json + training log. Push triggers `.github/workflows/papers-ci.yml` which submits the paper to clawRxiv for AI peer review. A second push (after the paper-CI cron polishes §5.X for the reviewer) is fine — that's what the remote crons set up via the `schedule` skill are for.

### What goes wrong + recovery

- **`wikidata_hf_import.py` short-circuits at startup** if `wikidata_hf_import_state.json` shows cumulative inserts ≥ `--max-triples`. The training cron stashes that file per-cycle.
- **MODEL.json pinned to a non-existent HF revision** (the v6/v6-bpe tag confusion). Always confirm the tag exists on HF before bumping.
- Other historical bugs (HF push using a hardcoded file list, stale dataset README, sled per-triple-transaction stalls) are fixed in code; see git log for `tools/hf_snapshot.py`, `upload_readme`, `PersistentStore::insert_batch`.

### What `tools/training_cron.py` does for you

Automates steps 1–8 for v9+ on a 12-hour interval: fresh per-cycle Loka instances (`loka-data-cron-cN`), per-cycle HF import state stashing, free-disk gating, PageRank-biased propgen tests. Does *not* polish paper prose — that's what the `schedule`-skill remote crons are for.

---

## Core Philosophy — Read This First

These are non-negotiable. Do not add features that violate them.

1. **Store first, reason second.** The database stores what you put in. OWL constraints are validated **client-side by SDKs**, not by the database itself. The database will never reject a triple for OWL violations — it accepts everything. SDKs throw exceptions on constraint violations (OWL enabled by default in SDKs). RDFS inference is out of scope.

2. **Vectors are triples.** A vector embedding is an attribute of a node or edge, stored via a predicate typed `loka:f32vec`. It is indexed by HNSW, but it is not a separate system — it is just another index alongside SPO/POS/OSP.

3. **Full traversal in a single query.** Any traversal of any depth across the entire database must be expressible in one SPARQL query. This is the whole point of a graph database.

4. **Lean by default.** Every feature must justify itself. Complexity is the enemy of performance. When in doubt, push it to the application layer.

5. **Serverless by default, server when needed.** Like SQLite, Loka can be embedded directly — just open a `.sdb` file. No daemon, no config. Server mode (HTTP/SPARQL endpoint) is opt-in via `loka serve`. Same `.sdb` storage format either way.

6. **Agent-first, GUI-optional.** Loka should be fully operable by an AI agent without ever touching a GUI. The CLI is the primary interface. Loka Studio (GUI) exists only for visual intuitions agents can't provide: HNSW health diagnostics, graph visualization, manual emergency editing. The agent can install, configure, launch, and manage everything — including opening the GUI for the user when they ask.

7. **SQLite defaults, production opt-ins.** Start with zero config. Features that add complexity (auth, TLS, backups, rate limiting) must be explicitly enabled. Three deployment tiers:
   - **Embedded** — zero config, no auth, local `.sdb` file, agent-friendly
   - **Served** — adds optional auth (simple passcode), rate limiting, query timeouts, HTTP API
   - **Production/Premium** — RBAC, encryption at rest, audit logging, replication, clustering

---

## Crate Structure

```
loka-core/      # Triple storage engine, LSM indexes, IRI interning, RDF-star IDs
loka-hnsw/      # HNSW index, vector literal type, predicate index registry
loka-sparql/    # SPARQL 1.1 parser, query planner, executor, hybrid extension
loka-proto/     # SPARQL HTTP protocol, Graph Store Protocol, REST API
loka-cli/       # CLI tools: serve, query, import, export, health, mcp
loka-ffi/       # C-compatible FFI shared library for Loka Studio and other non-Rust consumers
```

**Dependency rules:**
- `loka-hnsw` has **zero** dependency on `loka-sparql`. It is a pure data structure crate.
- `loka-sparql` depends on both `loka-core` and `loka-hnsw`.
- `loka-proto` depends on `loka-sparql`.
- `loka-cli` depends on `loka-proto` and `loka-sparql`.
- `loka-ffi` depends on `loka-core`, `loka-hnsw`, and `loka-sparql`. Produces `.dll`/`.so`/`.dylib`.

## Loka Studio & FFI

**Single-process architecture:** Studio, the MCP server, and the database engine all run in one process. Flutter loads `loka_ffi.dll`/`.so`/`.dylib` via `dart:ffi`, which contains the full database engine. The MCP server runs on a background thread in the same process, sharing the same database handle. The GUI is optional — `loka mcp` runs the same engine headless.

**Two entry points:**
- `loka mcp` → MCP + database, no GUI
- Loka Studio → GUI + database + optional MCP server, all one process

**FFI functions:** `loka_db_open`, `loka_db_close`, `loka_query`, `loka_insert_ntriples`, `loka_health_report`, `loka_export_ntriples`, `loka_verify_consistency`, `loka_repair`, `loka_db_info`, `loka_intern`, `loka_resolve`, `loka_version`. All use opaque pointers and null-terminated C strings. Thread-safe via `Arc<Mutex<...>>`.

Studio can also fall back to HTTP mode for connecting to remote instances.

The MCP server has `download_studio` and `launch_studio` tools so agents can install and open Studio without user intervention. Auto-update keeps Studio in sync with the CLI version.

---

## Data Model

### RDF-star
All data is RDF-star triples. Any position (subject, predicate, object) can be a quoted triple. This gives embeddings and metadata on edges natively.

```turtle
# Embedding on a node
:paper_42 :hasEmbedding "0.23 -0.11 0.87 ..."^^loka:f32vec .

# Embedding on an edge (RDF-star)
<< :paper_42 :discusses :TransformerArchitecture >> :hasEmbedding "0.23 -0.11 ..."^^loka:f32vec .
<< :paper_42 :discusses :TransformerArchitecture >> :confidence 0.91 .
```

### Vector Literals
- Type: `loka:f32vec` — a fixed-dimension array of f32
- Dimensionality is declared per predicate at schema time and enforced on insert
- Mismatched dimensions = hard error
- The database is model-agnostic: raw floats only, no embedding model metadata

### Schema Declaration
```turtle
loka:declareVectorPredicate :hasEmbedding ;
    loka:dimensions 1536 ;
    loka:hnswM 16 ;
    loka:hnswEfConstruction 200 .
```

---

## Storage Engine

### Indexes
Four index types over integer-interned IRI IDs:

| Index | Purpose |
|---|---|
| SPO | Subject → Predicate → Object (primary store, star-shaped queries via prefix scan) |
| POS | Predicate → Object → Subject (type lookups, vector reverse resolution) |
| OSP | Object → Subject → Predicate (reverse traversal) |
| VECTOR(p) | One HNSW index per vector predicate (ANN search, keyed by vector object ID) |

No separate SP or PO indexes needed — they are prefix scans on SPO and POS respectively.

### Implementation Notes
- Underlying storage: LSM-tree (RocksDB or sled TBD — see open questions)
- IRIs and blank nodes interned to u64 at write time
- Quoted triples get a content-addressed u64 ID: hash(S, P, O). **A persisted `quoted_triple_id → (s,p,o)` reverse index** (`loka-core`, written inside the batch transaction, rehydrated by `load_terms_into` on reopen) makes that one-way hash reversible — required for faithful `<< s p o >>` rendering across query/CSV/Turtle/N-Triples and for dereferencing `propositionInferredFrom` sources. **Always mint quoted ids via `TermDictionary::register_quoted` (or `PersistentStore::register_quoted`), never bare `quoted_triple_id`, on any ingest path** — otherwise the id is unreversible and renders as `_:idN`.
- All index entries operate on u64 IDs, never strings
- **Cascade-retraction** (`loka-core::retract_set`): remove a node + every generated inference that transitively cited it, bounded to `http://loka.dev/provenance/`, following only `propositionInferredFrom` (real→real is not a dependency), cycle-safe. Surfaces: `POST /retract/preview` (read-only), `POST /retract` (commit-gated), `retract_node` MCP tool, Loka Studio action. Destructive path is opt-in everywhere (dry-run default). Design: `planning/cascade-retraction.md`.

---

## HNSW Index

### Parameters
- `M`: max connections per node per layer (default 16, range 8–64)
- `ef_construction`: beam width during build (default 200)
- `ef_search`: beam width during query, tunable per-query
- `dimensions`: fixed at predicate declaration, enforced on insert

### Design
- Keyed by vector object's TermId (the vector literal is a graph primitive)
- Insert: vector literal interned, triple created, HNSW entry added under object's TermId
- Delete: **tombstoned** (flagged inactive, still traversable for graph connectivity — never removed until full rebuild)
- Virtual triples: HNSW neighbor edges exposed as `loka:hnswNeighbor` triples, generated on-the-fly, not stored in SPO/POS/OSP
- Persistence: HNSW is ephemeral — rebuilt from stored vector triples on startup. Optional snapshot for faster cold start.
- Concurrency: search is `&self` (per-call visited list, Qdrant pattern); concurrent reads don't block

### Node layout (per HNSW node)
```rust
struct HnswNode {
    vector: Vec<f32>,          // 4 * dimensions bytes
    layer: u8,
    neighbors: Vec<Vec<u32>>,  // neighbor lists per layer, bounded by M
    triple_id: u64,            // back-reference into triple store
    deleted: bool,
}
```

---

## SPARQL+ Extension

Loka's query language is **SPARQL+** — a superset of SPARQL 1.1. Extensions include VECTOR_SIMILAR, VECTOR_SCORE, and predicate-based exit conditions (UNTIL) on property path traversal.

### VECTOR_SIMILAR operator
```sparql
SELECT ?doc ?entity WHERE {
  ?entity rdf:type :Person .
  ?doc :mentions ?entity .
  VECTOR_SIMILAR(?doc :hasEmbedding "..."^^loka:f32vec, 0.85)
}

# With explicit ef_search hint
VECTOR_SIMILAR(?doc :hasEmbedding "..."^^loka:f32vec, 0.85, ef:=200)

# Score in ORDER BY
ORDER BY DESC(VECTOR_SCORE(?doc :hasEmbedding "..."^^loka:f32vec))
```

### Query planner heuristic (v0.1)
- Subject **bound** before VECTOR_SIMILAR: execute graph first, filter by vector
- Subject **unbound**: execute vector search first (top-k), then evaluate graph patterns over candidates
- Adaptive execution (runtime reordering) is future work

---

## Query Language Policy

**Supported:** **SPARQL+** — SPARQL 1.1 superset with VECTOR_SIMILAR, VECTOR_SCORE, and predicate-based exit conditions (UNTIL)
**Planned:** Cypher and GQL (ISO graph query language) as translation layers/wrappers over SPARQL. These are graph query languages that map naturally to the RDF data model.
**Never:** SQL, MongoDB Query Language, GraphQL.

SQL and MQL are deliberately excluded — not because they can't be mapped to SPARQL, but because offering them would mislead AI agents and users into choosing a relational/document query pattern over the graph pattern that Loka is designed for. An agent seeing SQL support might default to `SELECT * FROM table` thinking when the correct approach is SPARQL graph traversal. SPARQL is the right query language for a graph database. Offering SQL as an alternative would be a disservice to users by implying that relational thinking applies here.

---

## Deployment & Feature Tiers

Features are organized into tiers following the SQLite-defaults principle:

### Open Source (free)
- Serverless mode (`.sdb` file, zero config)
- Server mode (`loka serve`)
- Simple passcode authentication (server mode only, opt-in)
- Query timeouts (configurable)
- Rate limiting (server mode, opt-in)
- Periodic backups in server mode (configurable: hourly/daily, stored in separate directory)
- Agent-first installer with config-as-markdown
- Loka Studio GUI (desktop/web/mobile)
- All SDKs with client-side OWL validation
- MCP server (future — standardized agent↔database interface)

### Premium (future — for customers who need it)
Everything the creator doesn't fully understand yet. Drawing the line here avoids overcommitting. Premium features will be shaped by customer feedback.

- RBAC (role-based access control, per-user permissions)
- Encryption at rest
- TLS / encryption in transit (cert management)
- Audit logging (who did what, when)
- Replication (multi-node high availability)
- Clustering / sharding (horizontal scale)
- Multi-tenancy (isolated databases in one instance)
- Connection pooling

### Explicitly Out of Scope (never)

Do not implement these without explicit instruction:

- RDFS inference
- Built-in graph algorithms (PageRank, community detection, etc.)
- SQL or MongoDB query interfaces (offering them would mislead agents into relational/document thinking)
- Distributed execution / sharding (open-source tier)
- Embedding model metadata enforcement
- Multi-embedding-space / cross-modal queries
- GraphQL interface

---

## Reference Architectures: Oxigraph + Qdrant

Loka draws from two Rust databases:

- **[Oxigraph](https://github.com/oxigraph/oxigraph)** — Rust RDF triplestore. Reference for storage (RocksDB), indexing (SPO/POS/OSP), SPARQL pipeline (parser → optimizer → evaluator), snapshot-based transaction isolation.
- **[Qdrant](https://github.com/qdrant/qdrant)** — Rust vector database. Reference for HNSW implementation (immutable GraphLayers for search, thread-local visited pools, per-node RwLock during construction), vector preprocessing (normalize-at-insert for cosine).

Loka's differentiator: unifying both into one system where vectors are triples and the query planner treats HNSW as a 4th index type alongside SPO/POS/OSP.

---

## Open Questions (Unresolved)

- ~~**RDF-star vs. RDF 1.2**~~ **Resolved: RDF-star.** Direct edge annotation (`<< s p o >> :hasEmbedding ...`) is the natural pattern for vector work.
- **LSM-tree**: build from scratch vs. wrap RocksDB/sled? Oxigraph chose RocksDB.
- **IRI encoding**: Sequential interning (current) vs. hash-based (Oxigraph's SipHash approach)?
- **HNSW compaction**: what threshold triggers a background pass to clean deleted nodes?
- **SPARQL property paths** (`+`, `*`, `?`): traversal strategy for cycles on large graphs?
- ~~**License**: Apache 2.0 (patent grant) vs MIT (simplicity)?~~ **Resolved: Apache 2.0.**

---

## Agent-First Installer

Loka includes a CLI installer designed for AI agents (`loka install-agent` or similar):

- Exposes all configuration options as structured markdown prompts
- Agent reasons through each option and makes a decision
- Agent outputs a `<dbname>_loka_notes.md` file explaining what it chose and why
- Serverless: notes file stored alongside the `.sdb` file
- Server: notes file in the server data directory, viewable via CLI or Loka Studio
- Agent can also install optional tools (Protege, Loka Studio) and launch them for the user

The goal: a user says "set up a database for my project" and the agent handles everything.

---

## OWL Validation Strategy

OWL is stored in the database as regular triples. The database **does not enforce** OWL constraints.

Validation happens **client-side** in the SDKs:
- SDKs load the OWL ontology from the database
- OWL validation is **enabled by default** in all SDKs
- On constraint violation, the SDK throws an exception *before* the triple hits the database
- The database itself always accepts the triple — lean store, smart clients
- Users can disable OWL validation per-SDK if they want raw inserts

Loka Studio shows the ontology (Protege-like browser) and can highlight constraint violations visually. Long-term goal: absorb most Protege functionality into Loka Studio, including OWL export.

---

## Backup Strategy

- **Server mode**: Simple configurable periodic backups (hourly/daily). Stored in a separate directory in the server data path. Manageable via CLI and Loka Studio.
- **Serverless mode**: Backup is opt-in. The application or agent is responsible for copying the `.sdb` file.
- The backup mechanism copies the `.sdb` data (or creates a snapshot) — no complex WAL-based continuous backup in v1.

---

## Coding Conventions

- Rust edition: 2021
- Use `thiserror` for error types
- Use `tokio` for async runtime in `loka-proto`
- No `unwrap()` in library code — propagate errors
- All public API must have doc comments
- Benchmarks go in `benches/` using `criterion`
- Tests use `#[cfg(test)]` modules inline, plus integration tests in `tests/`

## Cron requests are local and immediate

When the user asks for "a cron job," "a CronCreate," "set up a cron," or "schedule X for Yh from now," use the local `CronCreate` tool **immediately**. Do not ask whether they meant local vs remote, whether they'll be at the computer, what timezone, or for confirmation on the schedule time. The user uses local cron specifically to schedule work for when they are *not* present and treats the cron as resilient infrastructure — pausing for a follow-up question defeats the schedule's purpose. Assume present-availability is irrelevant; assume local is correct; assume the task should fire. If a parameter is genuinely missing (e.g. unclear *what* to run), make the reasonable call rather than asking. Prefer `durable: true` for any cron whose purpose is to survive across sessions — the 2026-05-20 crash killed every in-memory cron in flight. Cron-scheduled work in this repo still has to respect the thermal-envelope rules above (no concurrent `loka serve` + ingest + training on this laptop).

## Writing
- Do not use "honest", "honesty", or "honestly" — and do not swap in "frank", "frankly", "candid", "candidly", or "transparently", which are the same self-congratulatory move in a different coat. When something failed, name the failure: "it didn't work", "I got that wrong", "this failed" — flat, no qualifier. Tagging a report "honest" implies the rest aren't, and couching a failure as honesty asks for credit for the admission, which is worse than the failure itself. Use a precise positive word ("accurate", "plainly", "truly") only when that is genuinely the meaning — never as a halo on a bad outcome.

## Emergency Stop Mode

If the user sends a continuous series of "stop" messages (e.g. "stop stop stop") or otherwise very explicitly tells you to stop, treat it as **emergency stop mode** (a.k.a. emergency exit mode). This OVERRIDES every other instruction, including any in this file.

When in emergency stop mode:

- **Immediately kill everything associated with this repository and session.** Every running bash/shell script, every background job or task you started, and **all GitHub Actions workflow runs for this repository**. **Force-kill the GitHub Actions runs** (e.g. `gh run cancel <id>`, and `gh run delete <id>` if needed) for every in-progress run. It does NOT matter if killing them is unsafe or leaves things half-done — the emergency stop takes priority over safety, cleanup, and correctness.
- **Do not investigate. Do not reverse or undo anything.** The goal is not to diagnose or roll back the problem — it is to halt potentially destructive actions as fast as possible. Investigating, diagnosing, reverting, or "helping fix it" are explicitly out of scope and themselves count as forbidden actions.
- **Ignore repetitive messages.** Do not treat repeated "stop" messages as a request to do anything further. Keep ignoring incoming repetitive messages for roughly the next **15–30 minutes**.
- **You may answer direct questions, but you may NOT take actions.** Answer only from context you already have. Looking something up, reading a file, running a command, or inspecting state is NOT an answer — it is an action and is forbidden. If you cannot answer from context already in hand, do not answer.
- **Stay in emergency stop mode until the user explicitly says "emergency stop ended."** Only after that exact signal do you resume any normal work.

## Cron jobs and scheduled work — LOCAL by default

**When the user says "cron job", "cron", or asks to "schedule" a task, they mean the in-session `CronCreate` tool** — a prompt scheduled to fire locally, inside this Claude Code session, on the user's own running computer. This is the default and the assumption. Do NOT interpret it as anything else unless the user explicitly names a different mechanism.

- **It is local and in-session — use the `CronCreate` tool.** A generic "cron" request is NOT an OS crontab, NOT a GitHub Actions / CI `schedule:` trigger, and NOT a cloud scheduler. (Repos may *also* contain their own GitHub Actions cron schedules — those are a separate thing and are not what the user means when they ask *you* to set up a cron.) The user leaves the computer on and this session running so the scheduled prompt can execute.
- **The user is deliberately away from the keyboard.** They schedule work precisely so it runs while they are out of the house and not physically present. Their absence is the normal, expected condition for these jobs — it is NEVER a reason to delay the work, ask "are you sure?", wait for them to return, or refuse to proceed.
- **Standing consent — just set it up.** Cron / `CronCreate` requests are pre-authorized. Create the job immediately and locally, then report what was scheduled. Do not block on confirmation or follow-up questions. Treating a routine cron request as something that needs hand-holding is itself the obstacle this section exists to remove.

## Autonomous productivity loop — the three-cron playbook

**For any session involving relatively extensive work — above all, any large-scale population of `queue.md` with created tasks — this is the default way of working.** It is three local `CronCreate` jobs that turn "barrel through `queue.md`, and when it's empty atomise the next `todo.md` item into it" into a self-sustaining hourly cadence with a commit/push backstop and a heartbeat. The crons are **session-local** (`durable: false` — they die when the session ends), so they are recreated at the start of every session.

Stagger the minutes so the three ticks don't collide:

1. **Work-loop cron — `3 * * * *` (hourly at :03).** The engine. Each tick does, in order:
   - **(a) SYNC** — `git fetch origin`; fast-forward or rebase the working branch (never force-push, never `reset --hard`, never discard a sibling machine's work).
   - **(b) WORK** — take the top actionable item from `queue.md` and do it. If nothing in `queue.md` is actionable (all blocked / needs user / a product decision), promote the next *genuinely-unblocked, bounded, verifiable* `todo.md` item — **plan it into `queue.md` first**, mirror to the task tool, then execute.
   - **(c) HARD RAILS** — never fake; never weaken / skip / delete a test to make it pass; never claim "works" / "verified" / "passes" without having actually RUN it and measured. A real defect → strict `xfail` or a precise documented blocker, never a loosened assertion. Don't implement what you don't 100% understand — write the spec / queue item instead. Name unbuilt or hard things plainly; don't paper over difficulty. Verify CI green, not just local — local-green does not imply CI-green.
   - **(d) COMMIT** — commit early/often with *why*; update `queue.md` in the same commit (delete completed items); append the dated entry to `devlog.md`; mark task-tool items done; push.
   - **(e) REPORT** — one line: the commit shas advanced, or `nothing actionable; <reason>`.

2. **Auto-flush cron — `15 * * * *` (hourly at :15).** The backstop. Commit + push all pending work so nothing sits uncommitted between manual pushes; report shas or "nothing pending". Only commit / push when something is actually pending — no empty commits.

3. **Status-report cron — `42 * * * *` (hourly at :42).** The heartbeat — **reporting only, no code changes.** Covers: what advanced since the last report (shas + one-line each); current `queue.md` state; how the work held the hard rails (and any place it brushed one); blockers / items deliberately not done autonomously and why; test-suite health.

**Why this exists:** the most common autonomous-agent failure is doing a large amount of work and silently losing the thread of what it is doing. The work-loop forces steady, verifiable, committed progress; the auto-flush guarantees nothing is lost between ticks; the status-report keeps the thread legible.

**Lifecycle around a large-scale queue fill:**

- **(a) START all three crons at the beginning of any extensive work session.** A fresh session has none of them running, so the opening move — the first queue item — is to *create them*.
- **(b) On a mid-session large-scale queue RE-FILL** (a planning burst that repopulates the queue), the FIRST item of that fill **kills the running crons**, then the work items follow top to bottom, and the pinned tail restarts them.
- **(c) Entering planning mode DISABLES the crons.** Their restart therefore lives at the **end** of the queue, not the beginning of the next burst.
- **(d) The LAST TWO queue items, always kept pinned at the tail, are:**
  1. **Ensure the three crons are running** — start them if this session never did, restart them if a planning burst / queue re-fill killed them.
  2. **Run the status-report action once more, independently** — an end-of-session summary of everything that happened this session.

In short: a fresh session **starts** the crons up front and the tail **ensures they are still running** + summarizes; a mid-session re-fill **kills** them up front and the tail **restarts** them + summarizes. Either way the queue both opens and closes on the cron set.

## Check cleanvibe for skill updates (weekly)

This `CLAUDE.md` carries cleanvibe-shaped sections (writing rules, emergency stop, cron policy, the productivity loop). cleanvibe ships new sections / skills over time — when one lands, this file should pick it up.

**The check is weekly, not per-session.** At the top of any session, look at the *last cleanvibe update check* date below. If it has been more than 7 days, do this:

1. **Fetch the current skill index** — `WebFetch https://cleanvibe.emmaleonhart.com/updates.md`. This is the canonical, hand-maintained page describing every section / skill / convention cleanvibe templates currently ship, keyed by the cleanvibe version that introduced it.
2. **Compare against the version below.** If `updates.md` lists sections introduced in later versions, fold those sections into THIS `CLAUDE.md`. Match the wording from `updates.md`; don't paraphrase. Repo-specific carve-outs already in this file stay — only the generic sections are kept in sync.
3. **Update the version + date below** to reflect the check. Commit the changes with a message describing which sections were folded in.

If the fetch fails (offline, DNS, page not yet up), leave the date alone and try next session — the check is opportunistic, not mandatory.

- **Last synced cleanvibe version:** `1.11.0`
- **Last cleanvibe update check:** `2026-05-26`
- **Updates source:** <https://cleanvibe.emmaleonhart.com/updates.md>
