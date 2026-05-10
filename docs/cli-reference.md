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
| `-d, --data_dir` | `./loka-data` | Data directory |

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
| `-d, --data_dir` | `./loka-data` | Data directory |

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
| `-d, --data_dir` | `./loka-data` | Data directory |
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
| `-d, --data_dir` | `./loka-data` | Data directory |

---

### `loka health`

Database health diagnostics. No server needed.

```bash
loka health -d ./my-database              # full health report
loka health -d ./my-database --rebuild_hnsw # rebuild HNSW indexes
loka health -d ./my-database --refresh    # rediscover pseudo-tables
```

| Flag | Default | Description |
|---|---|---|
| `-d, --data_dir` | `./loka-data` | Data directory |
| `--rebuild_hnsw` | off | Rebuild all HNSW indexes |
| `--refresh` | off | Rediscover pseudo-tables from current graph data |

---

### `loka serve`

Start the SPARQL HTTP server. **Only needed for multi-client access, remote connections, or HTTP API consumers.** For single-process use, prefer the serverless commands above.

```bash
loka serve                                # defaults: port 3030, data in ./loka-data
loka serve -p 8080 -d /data/mydb          # custom port and data directory
loka serve --memory_only                   # in-memory only, no persistence
loka serve --passcode mysecret             # require Bearer token on all requests
loka serve --backup_interval 60            # auto-backup every 60 minutes
```

| Flag | Default | Description |
|---|---|---|
| `-p, --port` | `3030` | Port to listen on |
| `-d, --data_dir` | `./loka-data` | Data directory for persistent `.sdb` storage |
| `--memory_only` | off | Run in-memory only (no persistence) |
| `--passcode` | none | Simple passcode auth; all requests except `/health` require `Authorization: Bearer <passcode>` |
| `--backup_interval` | `0` (disabled) | Periodic backup interval in minutes |

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
loka install-agent mydb --no_serve --launch_studio
```

| Argument/Flag | Default | Description |
|---|---|---|
| `name` (positional) | `loka-db` | Database name (used for directory and notes file) |
| `--port` | `3030` | Port for the server |
| `--passcode` | none | Enable passcode authentication |
| `--dimensions` | `1024` | Vector dimensions for default embedding predicate |
| `--metric` | `cosine` | Distance metric: `cosine`, `euclidean`, `dot` |
| `--no_serve` | off | Skip server startup |
| `--launch_studio` | off | Launch Loka Studio after setup |

---

### `loka mcp`

Start the MCP (Model Context Protocol) server for AI agents. Runs a JSON-RPC server over stdin/stdout.

```bash
loka mcp --data_dir ./mydb.sdb             # serverless mode (recommended — direct .sdb access)
loka mcp                                   # server mode: connect to http://localhost:3030
loka mcp --url http://remote:3030 --passcode secret
loka mcp --studio                          # also launch Loka Studio GUI
loka mcp --no_auto_update                  # disable auto-update check
```

| Flag | Default | Description |
|---|---|---|
| `--url` | `http://localhost:3030` | Loka HTTP endpoint (server mode) |
| `--data_dir` | none | Data directory for serverless mode; when set, ignores `--url` |
| `--passcode` | none | Passcode for authenticated server connections |
| `--no_auto_update` | off | Disable auto-update on startup |
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
