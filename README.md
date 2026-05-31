# Loka

**Website · [loka.emmaleonhart.com](https://loka.emmaleonhart.com)**

A lean, high-performance RDF-star triplestore written in Rust with native HNSW vector indexing, ontochronological temporal queries, and SPARQL+ query language.

[![CI](https://github.com/EmmaLeonhart/Loka/actions/workflows/ci.yml/badge.svg)](https://github.com/EmmaLeonhart/Loka/actions/workflows/ci.yml)

**[loka.emmaleonhart.com](https://loka.emmaleonhart.com)** — Documentation, theory, and interactive visualizations.

> **Status: Developer Preview.** Core engine, SPARQL+, vector indexing, HTTP server, ACID compliance, self-update, and MCP server are fully functional. APIs may evolve before 1.0. See the [Roadmap](https://loka.emmaleonhart.com/roadmap/) for what's done and what's next.

> **AI Agents:** If you are an AI agent setting up a database, see **[Agent Setup Guide](docs/AGENT_SETUP.md)** for complete CLI reference, API endpoints, and step-by-step instructions. Loka is designed to be fully operable by AI agents without ever touching a GUI. **Start with serverless mode** (just open a `.sdb` file, no server needed) — only use server mode for multi-client or remote access scenarios.

## What is this?

Loka is a single-purpose database: store triples, answer queries, at any scale. It replaces both a vector database (e.g. Qdrant) and a SPARQL triplestore (e.g. Apache Jena Fuseki) with a single unified system where **vectors are just triples**.

The vector indexing architecture is heavily influenced by [Qdrant](https://github.com/qdrant/qdrant), reimplemented from first principles and unified with a triple store. The RDF/SPARQL semantics draw from Apache Jena's TDB2, but without the JVM overhead.

### Core principles

1. **Store first, reason second.** The database stores what you put in. OWL validation happens client-side in SDKs, not in the database.
2. **Vectors are triples.** A vector embedding is an attribute of a node or edge, stored via a typed predicate and indexed by HNSW — not a separate system.
3. **Full traversal in a single query.** Any traversal of any depth must be expressible in one SPARQL query.
4. **Lean by default.** Every feature must justify itself. Complexity is the enemy of performance.
5. **Serverless by default, server when needed.** Like SQLite, Loka can be embedded directly — just open a `.sdb` file. No daemon, no config. Server mode (`loka serve`) is opt-in for when you need HTTP access, concurrent clients, or remote connections.
6. **Agent-first, GUI-optional.** The CLI is the primary interface. Loka Studio (GUI) exists for visual diagnostics.

## Quick Start

### Serverless (recommended — like SQLite)

No server process needed. Just point at a `.sdb` directory:

```bash
# Build
cargo build --release -p loka-cli

# Import data directly into a .sdb file
loka import data.nt -d ./my-database

# Query directly — no server needed
loka query -d ./my-database "SELECT * WHERE { ?s ?p ?o } LIMIT 10"

# Check database health
loka health -d ./my-database

# Use with AI agents via MCP (serverless mode)
loka mcp --data_dir ./my-database
```

### Server mode (for multi-client or remote access)

Only use server mode when you need HTTP access, concurrent clients, or remote connections:

```bash
# Start server (persistent storage)
loka serve

# Insert some data
curl -X POST http://localhost:3030/triples \
  -d '<http://example.org/Alice> <http://example.org/knows> <http://example.org/Bob> .'

# Query
curl -X POST http://localhost:3030/sparql \
  -d 'SELECT * WHERE { ?s ?p ?o } LIMIT 10'
```

## World Model (Loka) — two HF datasets, one model series

The Loka project ships **two parallel artifacts on Hugging Face**:

| Repo | What it is | Latest |
|---|---|---|
| **[`EmmaLeonhart/loka`](https://huggingface.co/datasets/EmmaLeonhart/loka)** | Trained model checkpoints (44.5 M-param BPE transformer, v3 → v14) + the training corpora used for each | `v14` (epoch-4 ppl **202.01** — series best, 2026-05-15) |
| **[`EmmaLeonhart/normalized-wikidata`](https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata)** | Clean text-form Wikidata triples for world-model training — published as a standalone artifact for anyone | `v14-1M` (4 021 409 triples, 2026-05-15) |

The world model is a small role-aware transformer that predicts missing slot values in `<<S P O>>` triples. Its predictions land back in the Loka triplestore as RDF-star annotations with `propositionInferredFrom` provenance edges. That provenance graph is **actionable**: [cascade-retraction](#whats-new--rdf-star-hardening--cascade-retraction-2026-05-16) removes any node together with every generated inference that transitively cited it. The training corpus and the model live in separate HF repos starting from v11 — the corpus is independently useful even if you don't use Loka.

> **Practical inference path (2026-05-19).** Alongside the from-scratch transformer, the shipped inference path that drives the browse.html double-click is a resident `:8092` sidecar (`tools/infer_server.py`) wired to **base Qwen 2.5 1.5B + BFS+embedding retrieval over Loka** — no fine-tune. A QLoRA fine-tune attempt was retired 2026-05-18 after the masked-SFT lobotomised the base model; the adapter remains on HF only as a negative-result reference. Three vector indexes (node-by-id, node-by-name, and the **triple itself**) feed the retrieval. See `planning/base-retrieval.md`.

### Multi-rung pipeline (v11 → v14, all shipped)

After v10 (ppl 55.52, 94 k triples), the pipeline was rebuilt: the preprocessor (`tools/preprocess_from_hf.py`) streams `philippesaade/wikidata` directly from HF — no Loka in the data path — and emits a clean text-form corpus published as `EmmaLeonhart/normalized-wikidata`. Each scale tier trains a corresponding Loka model. **The full series is shipped; the headline is a corpus-scale result** — architecture, tokenizer, batch size and optimizer held fixed, only the cleaned corpus scaled:

| Corpus tag | Entity rows | Output triples | Model | Best ppl |
|---|---|---|---|---|
| `v11-50k` | 50 000 | 350 428 | `v11` | 279.12 (3 epochs; CUDA OOM at batch 32 on the 8 GB laptop GPU) |
| `v12-100k` | 100 000 | 671 817 | `v12` | 250.82 (epoch-6 snapshot; shared-GPU contention; epoch-4 best 226.86 lost) |
| `v13-500k` | 500 000 | 2 511 771 | `v13` | 242.75 (epoch-2 canonical; loss plateaued by epoch 2) |
| `v14-1M` | 1 000 000 | 4 021 409 | `v14` | **202.01** (epoch-4 canonical — series best, still descending where v13 plateaued) |

An 11× increase in clean training data drove a **28 % perplexity reduction** (279.12 → 202.01). Each epoch is **snapshotted and tagged separately** on HF (`v12.*`, `v13.*`, `v14.*`) via `tools/epoch_snapshot_pusher.py`, so a divergent late epoch can't take down the earlier ones — this preserved every epoch across three separate training disruptions.

### Pulling a model in 2 lines

A fresh `git clone` is a few MB. The pinned model and tokenizer download lazily from HF on first inference:

```bash
# Inspect the current pin
python training/loader.py
# -> Pinned model: loka-wikidata-v14 (v14, released 2026-05-15)
#    repo: EmmaLeonhart/loka@v14

# Run inference — checkpoint + BPE tokenizer + vocab download automatically (~180 MB once, then cached)
python training/infer_with_citations.py \
    --bpe-tokenizer training/data/tokenizer_bpe.json \
    --max-subjects 20
```

### Pulling the normalized-wikidata corpus standalone

If you just want clean Wikidata triples without Loka:

```python
from huggingface_hub import hf_hub_download
path = hf_hub_download(
    repo_id="EmmaLeonhart/normalized-wikidata",
    repo_type="dataset",
    filename="triples_normalized.txt",
    revision="v14-1M",  # top tier; also v11-50k / v12-100k / v13-500k
)
```

The corpus is one tab-separated `subject\tpredicate\tobject\n` line per claim, English labels in every position, noise datatypes (`external-id`, `url`, `commonsMedia`, etc. — ~82 % of Wikidata's property types) excluded, time/quantity values normalised.

Full training pipeline + paper live under `training/`, `tools/`, `paper/`, `scripts/`. See `DEVLOG.md` for the v3 → v14 history including the v7 catalog-noise discovery (76 % of v6 corpus was external identifiers), the v9/v10 cron-loop automation, and the v11–v14 normalized-wikidata corpus-scale series. The paper (`paper/paper.md`) covers the full series in §5.9–§5.12.

### 🤝 Contributing GPU time (v14)

**v11–v14 are all shipped — but v14 floored at its epoch-4 checkpoint (ppl 202.01, series best).** [EmmaLeonhart](https://github.com/EmmaLeonhart)'s training box is a laptop (RTX 4070 *Laptop*, 8 GB VRAM); v11–v14 all trained in that envelope at `--batch-size 16`, but v14 only got a partial 5-epoch run, and a bounded continuation confirmed ~202 is the practical floor for a fresh optimizer resumed mid-line on this hardware. A **full clean 10-epoch run with a single optimizer** (4 M-triple corpus × 10 epochs ≈ 40 h sustained exclusive GPU) does not fit the laptop — that's the run expected to push meaningfully below 202.

**If you have a GPU and time to donate**, the entire run is one command:

```bash
huggingface-cli login   # paste YOUR HF token (not Emma's)
python tools/contribute_v14_training.py --hf-user YOUR_HF_USERNAME
```

The script pulls the v14-1M corpus from `EmmaLeonhart/normalized-wikidata` and the BPE tokenizer from `EmmaLeonhart/loka@v14` (byte-identical across all tags — stable since v6), trains 10 epochs of the standard 44.5 M-parameter architecture at batch 16, and pushes every epoch to **your** HF account as `<your-user>/loka-v14-contribution` tagged `v14.1` through `v14.10`. Even if a late epoch dies, every earlier epoch is preserved on HF.

**Please open a GitHub issue at <https://github.com/EmmaLeonhart/Loka/issues> before you start** so the work doesn't get duplicated, and comment on it with your HF link when done — Emma can then mirror your result to `EmmaLeonhart/loka@v14` and credit you.

Wall-time estimate at batch 16: ~4 h/epoch on an RTX 4090, ~8 h/epoch on an RTX 4070 Laptop. Full instructions and the rationale for the contributor path live at <https://loka.emmaleonhart.com/contribute/>.

## What's New — RDF-star hardening + cascade-retraction (2026-05-16)

- **Cascade-retraction.** Remove a node — real data or model-generated — and every generated inference that transitively cited it disappears with it. Propagation follows **only** `propositionInferredFrom` provenance back-edges, bounded to the reserved `http://loka.dev/provenance/` namespace: an ordinary data edge is never mistaken for a derivation (real→real is not a dependency) and the traversal is cycle-safe. Ships end-to-end — a pure engine function (`retract_set`), a read-only `POST /retract/preview`, a commit-gated `POST /retract`, a `retract_node` MCP tool (the **13th**), and a Loka Studio "Retract (cascade)" confirm action. **Destructive path is opt-in: dry-run preview is the default at every surface.**
- **RDF-star is now solid across every path.** Engine bug #2 (literal values in the predicate slot) is fully closed: a query-layer no-literal-predicate invariant, *plus* the ingest-side root cause — content-addressed `quoted_triple_id` had no persisted reverse map (a one-way hash, so a quoted subject couldn't render and the bulk path stored a `<<QUOTED_TRIPLE>>` sentinel). A persisted `quoted_triple_id → (s,p,o)` reverse index (rehydrated on reopen) fixes faithful `<< s p o >>` rendering across query / CSV / Turtle / N-Triples export, and two ingest paths (`loka-ffi`, the serverless MCP tool) that dropped inner triples were switched to the RDF-star parser. RDF-star round-trips losslessly across ingest, persistence, query, and export.

## What's New in v0.3

- **Loka Studio pre-built binaries** — Windows, Linux, and macOS desktop apps ship with every release. No Flutter SDK required.
- **MCP Studio tools** — AI agents can download and launch Loka Studio directly via `download_studio` and `launch_studio` MCP tools.
- **FFI layer (planned)** — `loka-ffi` crate will produce a C shared library (`.dll`/`.so`/`.dylib`) so Studio and other apps can open `.sdb` files directly via `dart:ffi` without needing an HTTP server.
- **Studio auto-update** — when the CLI auto-updates, Studio is also updated to match.
- **12 MCP tools** — added `download_studio`, `launch_studio` alongside existing database tools.

### Previous releases

**v0.2:**
- ACID compliance — atomic sled transactions, startup consistency verification, durable flushes
- Self-update — `loka update`, `loka --version`, startup version check
- MCP server for AI agents — dual-mode (serverless + server), maintenance tools
- HNSW rebuild HTTP endpoint — `POST /vectors/rebuild`
- COSINE_SEARCH, EUCLID_SEARCH, DOTPRODUCT_SEARCH — explicit distance metric operators

## Data Model

All data is **RDF-star** triples. Vectors are stored as `loka:f32vec` literals:

```turtle
# Embedding on a node
:paper_42 :hasEmbedding "0.23 -0.11 0.87 ..."^^loka:f32vec .

# Embedding on an edge (RDF-star)
<< :paper_42 :discusses :TransformerArchitecture >> :hasEmbedding "0.23 -0.11 ..."^^loka:f32vec .
```

## SPARQL+

Loka's query language is **SPARQL+** — a superset of SPARQL 1.1 with vector search, temporal scope operators, and predicate-based exit conditions:

```sparql
# Vector search: find semantically similar documents
SELECT ?doc ?entity WHERE {
  ?entity rdf:type :Person .
  ?doc :mentions ?entity .
  VECTOR_SIMILAR(?doc :hasEmbedding "..."^^loka:f32vec, 0.85)
}

# Temporal: query the world state at a specific time
SELECT ?person ?location WHERE {
  AT_TIME("1810-06-15"^^xsd:dateTime) {
    ?person :locatedIn ?location .
  }
}

# Temporal diff: what changed between two points in time
SELECT ?change_type ?s ?p ?o WHERE {
  TEMPORAL_DIFF("2024-01-01"^^xsd:dateTime, "2024-06-01"^^xsd:dateTime) {
    ?s ?p ?o .
  }
}
```

### Supported SPARQL Features

SELECT, ASK, CONSTRUCT, DESCRIBE | INSERT DATA, DELETE DATA | FILTER (=, !=, <, >, <=, >=, &&, ||, !) | FILTER NOT EXISTS / EXISTS | OPTIONAL, UNION | BIND, VALUES | GROUP BY + COUNT/SUM/AVG/MIN/MAX | ORDER BY, LIMIT, OFFSET, DISTINCT | VECTOR_SIMILAR, VECTOR_SCORE | AT_TIME, DURING, WORLD_STATE, TEMPORAL_DIFF | String functions (CONTAINS, STRSTARTS, STRENDS, REGEX) | LANG(), LANGMATCHES(), isIRI(), isLiteral() | PREFIX declarations

## Architecture

| Crate | Purpose | Status |
|---|---|---|
| `loka-core` | Triple storage engine, IRI interning, RDF-star IDs, sled persistence | Implemented |
| `loka-hnsw` | HNSW vector index with SIMD (AVX2/SSE), multiple distance metrics | Implemented |
| `loka-sparql` | SPARQL 1.1 parser, query planner, executor, hybrid extension | Implemented |
| `loka-proto` | HTTP server, SPARQL protocol, Graph Store Protocol | Implemented |
| `loka-cli` | CLI: serve, query, import, export, health, MCP server | Implemented |
| `loka-ffi` | C FFI shared library for embedding in non-Rust apps | Planned |

## CLI

```bash
# Serverless operations (no server needed)
loka query -d ./mydb "SELECT ..." # Run SPARQL query directly
loka import -d ./mydb data.nt     # Import N-Triples
loka export -d ./mydb -o dump.nt  # Export all triples
loka info -d ./mydb               # Show database stats
loka health -d ./mydb             # Database health diagnostics
loka mcp --data_dir ./mydb        # MCP server (serverless mode)

# Server mode (when you need HTTP/multi-client access)
loka serve                        # Start HTTP server (port 3030)
loka serve --memory-only          # In-memory only
loka mcp --url http://host:3030   # MCP server (server mode)

# Maintenance
loka health --rebuild_hnsw        # Rebuild HNSW indexes
loka update                       # Check for updates and self-update
loka install-agent mydb           # Agent-first database setup
```

See **[CLI Reference](docs/cli-reference.md)** for the full list of commands, flags, and options.

## SDKs

| Language | Package | Install |
|----------|---------|---------|
| Python | [`loka`](https://pypi.org/project/loka/) | `pip install loka` |
| TypeScript | [`loka`](https://www.npmjs.com/package/loka) | `npm install loka` |
| Go | [`loka`](sdks/go/) | `go get github.com/EmmaLeonhart/Loka/sdks/go` |
| Rust | [`loka`](sdks/rust/) | `cargo add loka` |
| Java | [`loka-java`](sdks/java/) | Maven dependency |
| .NET | [`Loka.Client`](sdks/dotnet/) | `dotnet add package Loka.Client` |

## Loka Studio

Flutter desktop/web GUI for visual database management — graph visualization, HNSW health diagnostics, SPARQL query editor, and ontology browsing.

**Pre-built binaries** ship with every release (Windows, Linux, macOS). No Flutter SDK needed.

**Via MCP (AI agents):**
```
# The agent calls these MCP tools:
download_studio    # Downloads Studio for your platform
launch_studio      # Opens Studio connected to your database
```

**Via CLI:**
```bash
loka install-agent --launch-studio   # Download + launch during setup
```

**From source:**
```bash
cd loka-studio/electron && npm install && npm run studio:js
```

Studio connects to Loka via HTTP (server mode) or will connect directly via FFI (serverless mode, planned). The FFI layer (`loka-ffi`) will allow Studio to open `.sdb` files without any server process — like how SQLite browsers open database files directly.

## MCP Server

Native Model Context Protocol server for AI agents. Runs over JSON-RPC 2.0 on stdin/stdout.

```bash
# Serverless mode (recommended — opens .sdb file directly, no server needed)
loka mcp --data-dir ./my-database

# Server mode (connects to running instance — only if you need multi-client access)
loka mcp --url http://localhost:3030
```

12 tools: `health_report`, `rebuild_hnsw`, `verify_consistency`, `database_info`, `sparql_query`, `insert_triples`, `backup`, `vector_search`, `download_studio`, `launch_studio`, `check_update`, `decline_update`.

Auto-updates the CLI binary (and Studio if installed) from GitHub releases on startup, with a 2-minute decline window.

## Documentation

| Document | Description |
|---|---|
| **[Architecture](docs/architecture.md)** | Full technical architecture: data model, storage engine, indexes, SPARQL+, crate structure |
| **[Query Examples](docs/query-examples.md)** | 70+ SPARQL+ query examples from basic patterns to hybrid vector+temporal+graph queries |
| **[Temporal Queries](docs/temporal-queries.md)** | Practical guide to AT_TIME, DURING, WORLD_STATE, and TEMPORAL_DIFF operators |
| **[Ontochronology](docs/ontochronology.md)** | Theory and design of Loka's temporal (ontochronological) data model |
| **[Vector SPARQL](docs/vectorSPARQL.md)** | VECTOR_SIMILAR, VECTOR_SCORE, edge embeddings, query planner heuristics |
| **[CLI Reference](docs/cli-reference.md)** | All CLI commands, flags, HTTP API endpoints, and MCP tools |
| **[Agent Setup](docs/AGENT_SETUP.md)** | Quick start guide for AI agents |

## Test Suite

256 tests across 5 crates:

```bash
cargo test --workspace
```

## Building

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

## Docker

```bash
docker build -t loka .
docker run -p 3030:3030 -v loka-data:/data loka
```

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later). See [`LICENSE`](LICENSE).
