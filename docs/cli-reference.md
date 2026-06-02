# Loka — CLI Reference

> Complete reference for `loka` command-line interface.

> **Serverless by default.** Most commands operate directly on a `.sdb` directory via the `-d` flag — no server needed. Only use `loka serve` when you need HTTP access, concurrent clients, or remote connections.

---

## Commands

### `loka query`

Execute a SPARQL query directly on a `.sdb` directory. No server needed.

```bash
loka query -d ./my-database "SELECT * WHERE { ?s ?p ?o } LIMIT 10"
loka query -d /data/mydb "SELECT ?name WHERE { ?s :name ?name }"
```

| Argument/Flag | Default | Description |
|---|---|---|
| `query` (positional) | required | The SPARQL query string |
| `-d, --data-dir` | `./loka-data` | Data directory |

---

### `loka import`

Import N-Triples data from a file into the database. No server needed.

```bash
loka import -d ./my-database data.nt     # import from file
loka import -d ./my-database -            # import from stdin
```

| Argument/Flag | Default | Description |
|---|---|---|
| `file` (positional) | required | Path to N-Triples file (use `-` for stdin) |
| `-d, --data-dir` | `./loka-data` | Data directory |

---

### `loka export`

Export all triples from the database. No server needed.

```bash
loka export -d ./my-database              # export to stdout as N-Triples
loka export -d ./my-database -o backup.nt # export to file
loka export -d ./my-database -f ttl       # export as Turtle
```

| Flag | Default | Description |
|---|---|---|
| `-d, --data-dir` | `./loka-data` | Data directory |
| `-o, --output` | stdout | Output file path |
| `-f, --format` | `nt` | Export format: `nt` (N-Triples) or `ttl` (Turtle) |

---

### `loka info`

Show database statistics (triple count, term count, vector indexes, etc.). No server needed.

```bash
loka info -d ./my-database
```

| Flag | Default | Description |
|---|---|---|
| `-d, --data-dir` | `./loka-data` | Data directory |

---

### `loka health`

Database health diagnostics. No server needed.

```bash
loka health -d ./my-database              # full health report
loka health -d ./my-database --rebuild-hnsw # rebuild HNSW indexes
loka health -d ./my-database --refresh    # rediscover pseudo-tables
loka health -d ./my-database --json       # machine-readable JSON report
```

| Flag | Default | Description |
|---|---|---|
| `-d, --data-dir` | `./loka-data` | Data directory |
| `--rebuild-hnsw` | off | Rebuild all HNSW indexes |
| `--refresh` | off | Rediscover pseudo-tables from current graph data |
| `--json` | off | Emit the report as JSON for programmatic agents (status fields HEALTHY/WARNING/CRITICAL) instead of AI-readable text |

---

### `loka serve`

Start the SPARQL HTTP server. **Only needed for multi-client access, remote connections, or HTTP API consumers.** For single-process use, prefer the serverless commands above.

```bash
loka serve                                # defaults: port 3030, data in ./loka-data
loka serve -p 8080 -d /data/mydb          # custom port and data directory
loka serve --memory-only                   # in-memory only, no persistence
loka serve --passcode mysecret             # require Bearer token on all requests
loka serve --backup-interval 60            # auto-backup every 60 minutes
```

| Flag | Default | Description |
|---|---|---|
| `-p, --port` | `3030` | Port to listen on |
| `-d, --data-dir` | `./loka-data` | Data directory for persistent `.sdb` storage |
| `--memory-only` | off | Run in-memory only (no persistence) |
| `--passcode` | none | Simple passcode auth; all requests except `/health` require `Authorization: Bearer <passcode>` |
| `--backup-interval` | `0` (disabled) | Periodic backup interval in minutes |

---

### `loka update`

Check for updates and self-update the binary from GitHub releases.

```bash
loka update                                # check and install update
loka update --check                        # just check, don't install
```

| Flag | Default | Description |
|---|---|---|
| `--check` | off | Just check for updates without installing |

---

### `loka install-agent`

Agent-first installer: generates structured config and a markdown notes file documenting the database setup. Designed for AI agents to call programmatically.

```bash
loka install-agent mydb
loka install-agent mydb --port 8080 --passcode secret --dimensions 768
loka install-agent mydb --no-serve --launch-studio
loka install-agent mydb --json            # emit setup result as JSON (no server start)
```

| Argument/Flag | Default | Description |
|---|---|---|
| `name` (positional) | `loka-db` | Database name (used for directory and notes file) |
| `--port` | `3030` | Port for the server |
| `--passcode` | none | Enable passcode authentication |
| `--dimensions` | `1024` | Vector dimensions for default embedding predicate |
| `--metric` | `cosine` | Distance metric: `cosine`, `euclidean`, `dot` |
| `--no-serve` | off | Skip server startup |
| `--launch-studio` | off | Launch Loka Studio after setup |
| `--json` | off | Emit the setup result as JSON for programmatic agents; still creates the DB + notes file but does NOT start the blocking server or launch Studio |

---

### `loka mcp`

Start the MCP (Model Context Protocol) server for AI agents. Runs a JSON-RPC server over stdin/stdout.

```bash
loka mcp --data-dir ./mydb.sdb             # serverless mode (recommended — direct .sdb access)
loka mcp                                   # server mode: connect to http://localhost:3030
loka mcp --url http://remote:3030 --passcode secret
loka mcp --studio                          # also launch Loka Studio GUI
loka mcp --no-auto-update                  # disable auto-update check
```

| Flag | Default | Description |
|---|---|---|
| `--url` | `http://localhost:3030` | Loka HTTP endpoint (server mode) |
| `--data-dir` | none | Data directory for serverless mode; when set, ignores `--url` |
| `--passcode` | none | Passcode for authenticated server connections |
| `--no-auto-update` | off | Disable auto-update on startup |
| `--studio` | off | Also launch Loka Studio GUI alongside MCP |

#### MCP Tools

The MCP server exposes 12 tools via JSON-RPC:

| Tool | Description |
|---|---|
| `health_report` | Full database diagnostics (HNSW, storage, consistency) |
| `rebuild_hnsw` | Compact and rebuild all HNSW vector indexes |
| `verify_consistency` | Check SPO/POS/OSP index consistency, auto-repair |
| `database_info` | Triple count, term count, vector index count |
| `sparql_query` | Execute SPARQL+ queries |
| `insert_triples` | Insert N-Triples data |
| `backup` | Create database snapshot |
| `vector_search` | ANN search via VECTOR_SIMILAR |
| `download_studio` | Download and install Loka Studio |
| `launch_studio` | Open Loka Studio (downloads first if needed) |
| `check_update` | Check for new Loka releases |
| `decline_update` | Cancel pending auto-update |

---

## HTTP API Endpoints

When running in server mode (`loka serve`), the following endpoints are available:

| Endpoint | Method | Description |
|---|---|---|
| `/sparql` | GET/POST | SPARQL query endpoint (standard SPARQL protocol) |
| `/triples` | POST | Insert N-Triples data |
| `/triples` | GET | Export triples (supports content negotiation) |
| `/health` | GET | Health check (always accessible, even with passcode) |
| `/info` | GET | Database statistics |
| `/vectors/search` | POST | Vector similarity search |
| `/graph` | GET/PUT/DELETE | SPARQL Graph Store Protocol |
| `/backup` | POST | Create backup snapshot |

### Content Negotiation

The `/sparql` and `/triples` endpoints support content negotiation via the `Accept` header:

| Accept Header | Format |
|---|---|
| `application/sparql-results+json` | SPARQL JSON results |
| `application/sparql-results+xml` | SPARQL XML results |
| `text/csv` | CSV |
| `text/tab-separated-values` | TSV |
| `application/n-triples` | N-Triples |
| `text/turtle` | Turtle |

### Authentication

When `--passcode` is set, all endpoints except `/health` require:

```
Authorization: Bearer <passcode>
```
