# Loka

A lean, high-performance RDF-star triplestore written in Rust with native HNSW vector indexing, ontochronological temporal queries, and SPARQL+ query language.

[![CI](https://github.com/EmmaLeonhart/Loka/actions/workflows/ci.yml/badge.svg)](https://github.com/EmmaLeonhart/Loka/actions/workflows/ci.yml)

**[sutradb.org](https://sutradb.org)** — Documentation, theory, and interactive visualizations.

> **Status: Developer Preview.** Core engine, SPARQL+, vector indexing, HTTP server, ACID compliance, self-update, and MCP server are fully functional. APIs may evolve before 1.0. See the [Roadmap](https://sutradb.org/roadmap/) for what's done and what's next.

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

## World Model (Loka)

A small role-aware transformer trained on the RDF-star Wikidata corpus lives in this repo as the **Loka world model**. It predicts missing slot values in `<<S P O>>` triples and writes its predictions back as RDF-star annotations with `propositionInferredFrom` provenance.

**The model itself is on Hugging Face, not in this clone.** A fresh `git clone` is small (a few MB). The current pinned model is recorded in [`MODEL.json`](MODEL.json) and pulled lazily on first run.

```bash
# What's pinned right now?
python training/loader.py
# -> Pinned model: loka-wikidata-v6 (v6, released 2026-05-10)
#    repo: EmmaLeonhart/loka@v6-bpe
#    checkpoint    -> training/checkpoints/wikidata_v6.pt   [missing (will download)]
#    vocab         -> training/data/vocab_bpe.json          [missing (will download)]
#    tokenizer_bpe -> training/data/tokenizer_bpe.json      [missing (will download)]

# Run inference — checkpoint, vocab, and BPE tokenizer download automatically
# on first call (~180 MB once, then cached). v6 was trained with the BPE
# tokenizer, so pass it through:
python training/infer_with_citations.py \
    --bpe-tokenizer training/data/tokenizer_bpe.json \
    --max-subjects 20
```

To bump the current model: upload via `python tools/hf_snapshot.py --snapshot-name vN`, then edit `MODEL.json` (`version`, `revision`, `files.checkpoint.in_repo`, `files.vocab.in_repo`) and commit. The pin is the single source of truth for "what does fresh-clone-and-run get you."

Full training pipeline, BPE tokenizer, generative-citation loop, and clawRxiv submission stack live under `training/`, `tools/`, `paper/`, `scripts/` — see `DEVLOG.md` for the v3 → v4 → v5 → v6 → v7 history. The v7 round (2026-05-10, see DEVLOG and `planning/wikidata-datatype-processing.md`) found that **76% of the v6 corpus was Wikidata external-identifier predicates** — Freebase, ISNI, GND, LCCN, Dewey, etc. — and rebuilt the corpus with a per-datatype keep/drop policy that excludes 10,525 of Wikidata's 12,756 properties and normalises time/quantity literal formats. v7 perplexity matches v6 on the cleaner, 4×-smaller corpus, but the catalog-format hallucinations (`ISNI -> "00000000"`, `instance of -> "+ Ġof - 00 - 03 T 00"`) that dominated v6 generations are gone.

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
cd loka-studio && flutter run -d chrome
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

Apache 2.0
