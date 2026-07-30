//! Loka CLI: server, query, import, export.

mod mcp;

use std::io::{BufRead, Write};
use std::sync::{Arc, RwLock};

use clap::{Parser, Subcommand};

/// Version string including the build stamp from `build.rs`.
///
/// `CARGO_PKG_VERSION` alone could not distinguish a May binary from today's —
/// both said `0.4.1` — which is how the installed copy stayed two months stale
/// while looking current, and cost three separate investigations (see build.rs).
const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("LOKA_BUILD_SHA"),
    " ",
    env!("LOKA_BUILD_TIME"),
    ")"
);

#[derive(Parser)]
#[command(
    name = "loka",
    about = "Loka — RDF-star triplestore with HNSW vector indexing",
    version = VERSION,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the SPARQL HTTP server.
    Serve {
        /// Port to listen on.
        #[arg(short, long, default_value = "3030")]
        port: u16,

        /// Data directory for persistent storage (.sdb).
        #[arg(short, long, default_value = "./loka-data")]
        data_dir: String,

        /// Run in-memory only (no persistence).
        #[arg(long)]
        memory_only: bool,

        /// Simple passcode authentication. When set, all requests
        /// (except /health) require `Authorization: Bearer <passcode>`.
        #[arg(long)]
        passcode: Option<String>,

        /// Enable periodic backups (interval in minutes, 0 = disabled).
        #[arg(long, default_value = "0")]
        backup_interval: u64,
    },
    /// Execute a SPARQL query from the command line.
    Query {
        /// The SPARQL query string.
        query: String,

        /// Data directory.
        #[arg(short, long, default_value = "./loka-data")]
        data_dir: String,
    },
    /// Import N-Triples (or N-Triples-star) data from a file into the database.
    Import {
        /// Path to the N-Triples file (use - for stdin).
        file: String,

        /// Data directory.
        #[arg(short, long, default_value = "./loka-data")]
        data_dir: String,

        /// Print each line that fails to parse or insert, with the reason.
        #[arg(long)]
        show_errors: bool,
    },
    /// Export all triples as N-Triples.
    Export {
        /// Data directory.
        #[arg(short, long, default_value = "./loka-data")]
        data_dir: String,

        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<String>,

        /// Export format: nt (N-Triples) or ttl (Turtle).
        #[arg(short, long, default_value = "nt")]
        format: String,
    },
    /// Show database statistics.
    Info {
        /// Data directory.
        #[arg(short, long, default_value = "./loka-data")]
        data_dir: String,
    },
    /// Database health diagnostics for AI agents and humans.
    ///
    /// Outputs a structured health report covering HNSW vector indexes,
    /// pseudo-tables, and storage. Every metric includes context explaining
    /// what healthy vs unhealthy looks like.
    ///
    /// Use --rebuild-hnsw to compact and rebuild all HNSW indexes.
    /// Use --refresh to rediscover pseudo-tables from current data.
    Health {
        /// Data directory.
        #[arg(short, long, default_value = "./loka-data")]
        data_dir: String,

        /// Rebuild all HNSW indexes (removes tombstones, restores connectivity).
        /// This is the recommended fix when the health report shows WARNING or
        /// CRITICAL for tombstone ratio or connectivity.
        #[arg(long)]
        rebuild_hnsw: bool,

        /// Rediscover pseudo-tables from current graph data.
        /// Scans all nodes for shared predicate patterns and materializes
        /// columnar indexes for groups that qualify.
        #[arg(long)]
        refresh: bool,

        /// Emit the health report as JSON instead of AI-readable text.
        /// Intended for programmatic agent consumption — the same metrics,
        /// machine-parseable. Status fields are HEALTHY/WARNING/CRITICAL.
        #[arg(long)]
        json: bool,
    },
    /// Check for updates and self-update the binary from GitHub releases.
    Update {
        /// Just check for updates without installing.
        #[arg(long)]
        check: bool,
    },
    /// Agent-first installer: outputs structured config for AI agents.
    /// Generates a markdown notes file documenting the database setup.
    #[command(name = "install-agent")]
    InstallAgent {
        /// Database name (used for directory and notes file).
        #[arg(default_value = "loka-db")]
        name: String,

        /// Port for the server.
        #[arg(long, default_value = "3030")]
        port: u16,

        /// Enable passcode authentication.
        #[arg(long)]
        passcode: Option<String>,

        /// Vector dimensions (for default embedding predicate).
        #[arg(long, default_value = "1024")]
        dimensions: usize,

        /// Distance metric: cosine, euclidean, dot.
        #[arg(long, default_value = "cosine")]
        metric: String,

        /// Skip server startup.
        #[arg(long)]
        no_serve: bool,

        /// Launch Loka Studio after setup.
        #[arg(long)]
        launch_studio: bool,

        /// Emit the setup result as JSON for programmatic agent consumption.
        /// Still creates the database and writes the notes file, but prints a
        /// single JSON object describing the setup and does NOT start the
        /// blocking server or launch Studio (run `loka serve` yourself).
        #[arg(long)]
        json: bool,
    },
    /// Start the MCP (Model Context Protocol) server for AI agents.
    ///
    /// Runs a JSON-RPC server over stdin/stdout that exposes database
    /// maintenance and query tools to AI agents (Claude, GPT, etc.).
    /// Supports both server mode (HTTP) and serverless mode (direct .sdb).
    Mcp {
        /// Loka HTTP endpoint (server mode).
        #[arg(long, default_value = "http://localhost:3030")]
        url: String,

        /// Data directory for serverless mode (direct .sdb access).
        /// When set, ignores --url and operates directly on disk.
        #[arg(long)]
        data_dir: Option<String>,

        /// Passcode for authenticated server connections.
        #[arg(long)]
        passcode: Option<String>,

        /// Disable auto-update on startup. By default, the MCP server checks
        /// for updates and auto-installs after a 2-minute window unless the
        /// agent declines via the decline_update tool.
        #[arg(long)]
        no_auto_update: bool,

        /// Also launch Loka Studio GUI alongside the MCP server.
        /// Studio connects to the same database for visual diagnostics.
        #[arg(long)]
        studio: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Update { check } => {
            return handle_update(check).await;
        }
        Commands::Mcp {
            url,
            data_dir,
            passcode,
            no_auto_update,
            studio,
        } => {
            if studio {
                // Launch Loka Studio as a detached process
                let studio_dir = std::path::Path::new("loka-studio");
                let endpoint = if data_dir.is_some() {
                    // Serverless — Studio will connect via FFI or default localhost
                    "http://localhost:3030".to_string()
                } else {
                    url.clone()
                };

                // Try pre-built binary first, fall back to flutter run
                let exe_dir = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("loka-studio")));
                let studio_exe = exe_dir.as_ref().map(|d| {
                    if cfg!(target_os = "windows") {
                        d.join("loka_studio.exe")
                    } else {
                        d.join("loka_studio")
                    }
                });

                if let Some(ref exe) = studio_exe {
                    if exe.exists() {
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("cmd")
                            .args(["/c", "start", "", &exe.to_string_lossy()])
                            .env("LOKA_ENDPOINT", &endpoint)
                            .spawn();
                        #[cfg(not(target_os = "windows"))]
                        let _ = std::process::Command::new(exe)
                            .env("LOKA_ENDPOINT", &endpoint)
                            .spawn();
                        eprintln!("[OK] Loka Studio launched");
                    } else if studio_dir.exists() {
                        eprintln!("Launching Loka Studio from source...");
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("cmd")
                            .args(["/c", "start", "", "flutter", "run", "-d", "windows"])
                            .current_dir(studio_dir)
                            .env("LOKA_ENDPOINT", &endpoint)
                            .spawn();
                        #[cfg(not(target_os = "windows"))]
                        let _ = std::process::Command::new("flutter")
                            .args(["run", "-d", "linux"])
                            .current_dir(studio_dir)
                            .env("LOKA_ENDPOINT", &endpoint)
                            .spawn();
                    } else {
                        eprintln!("[WARN] Loka Studio not found. Use the MCP download_studio tool to install it.");
                    }
                }
            }
            return mcp::run_mcp_server(url, data_dir, passcode, !no_auto_update).await;
        }
        Commands::Serve {
            port,
            data_dir,
            memory_only,
            passcode,
            backup_interval,
        } => {
            // Background version check (non-blocking, best-effort)
            tokio::spawn(async {
                if let Some(latest) = check_latest_version().await {
                    let current = env!("CARGO_PKG_VERSION");
                    if latest != current {
                        tracing::info!(
                            "Update available: v{} → v{} (run `loka update` to install)",
                            current,
                            latest
                        );
                    }
                }
            });
            let state = if memory_only {
                tracing::info!("Running in-memory only (no persistence)");
                Arc::new(loka_proto::AppState {
                    store: RwLock::new(loka_core::TripleStore::new()),
                    dict: RwLock::new(loka_core::TermDictionary::new()),
                    vectors: RwLock::new(loka_hnsw::VectorRegistry::new()),
                    persistent: None,
                    passcode: passcode.clone(),
                    rate_limit_per_min: 0,
                    rate_counter: std::sync::atomic::AtomicU64::new(0),
                })
            } else {
                tracing::info!("Opening persistent store at {}", data_dir);
                let ps = loka_core::PersistentStore::open(&data_dir)?;

                // Verify index consistency on startup and repair if needed
                if !ps.verify_consistency() {
                    tracing::warn!(
                        "Index inconsistency detected (possible prior crash). Repairing..."
                    );
                    match ps.repair() {
                        Ok(count) => {
                            tracing::info!(
                                "Repair complete: rebuilt {} triples in secondary indexes",
                                count
                            );
                            ps.flush()?;
                        }
                        Err(e) => {
                            tracing::error!("Repair failed: {}. Database may be corrupt.", e);
                            return Err(e.into());
                        }
                    }
                }

                let mut dict = loka_core::TermDictionary::new();
                let mut store = loka_core::TripleStore::new();

                let term_count = ps.load_terms_into(&mut dict);
                tracing::info!("Loaded {} terms from disk", term_count);

                let mut triple_count = 0usize;
                for triple in ps.iter() {
                    let _ = store.insert(triple);
                    triple_count += 1;
                }
                tracing::info!("Loaded {} triples from disk", triple_count);

                // Rebuild HNSW indexes from stored vector triples. The
                // helper skips malformed rows (predicate is a literal or
                // inline value) — the corruption mode observed on the
                // 2026-05-20 sled rehydrate, where /vectors/health
                // reported predicate slots resolving to f32vec literals
                // instead of IRIs.
                let (vectors, vec_count) = loka_hnsw::rebuild_from_store(
                    &store,
                    &dict,
                    &loka_hnsw::RebuildDefaults::default(),
                );
                if vec_count > 0 {
                    tracing::info!("Rebuilt {} vectors in HNSW indexes from disk", vec_count);
                }

                Arc::new(loka_proto::AppState {
                    store: RwLock::new(store),
                    dict: RwLock::new(dict),
                    vectors: RwLock::new(vectors),
                    persistent: Some(RwLock::new(ps)),
                    passcode: passcode.clone(),
                    rate_limit_per_min: 0,
                    rate_counter: std::sync::atomic::AtomicU64::new(0),
                })
            };

            // Start periodic backup task if configured
            if backup_interval > 0 && !memory_only {
                let backup_dir = format!("{}/backups", data_dir);
                let _ = std::fs::create_dir_all(&backup_dir);
                let data_dir_clone = data_dir.clone();
                let interval = std::time::Duration::from_secs(backup_interval * 60);
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(interval).await;
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let backup_path =
                            format!("{}/backups/backup_{}", data_dir_clone, timestamp);
                        tracing::info!("Creating backup at {}", backup_path);
                        // Copy the sled directory
                        if let Err(e) = copy_dir_recursive(
                            std::path::Path::new(&data_dir_clone),
                            std::path::Path::new(&backup_path),
                        ) {
                            tracing::error!("Backup failed: {}", e);
                        } else {
                            tracing::info!("Backup complete: {}", backup_path);
                        }
                    }
                });
                tracing::info!(
                    "Periodic backups enabled: every {} minutes to {}/backups/",
                    backup_interval,
                    data_dir
                );
            }

            let app = loka_proto::router(state);
            let addr = format!("127.0.0.1:{}", port);
            // Announce the BUILD, not just the version. A server whose log says
            // only "0.4.1" cannot tell you it predates the fix you are testing —
            // which is exactly how a stale binary wasted three investigations.
            tracing::info!("Loka {} listening on {}", VERSION, addr);

            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await?;
        }

        Commands::Query { query, data_dir } => {
            let ps = loka_core::PersistentStore::open(&data_dir)?;

            let mut dict = loka_core::TermDictionary::new();
            let mut store = loka_core::TripleStore::new();

            ps.load_terms_into(&mut dict);
            for triple in ps.iter() {
                let _ = store.insert(triple);
            }

            let vectors = loka_hnsw::VectorRegistry::new();

            let mut parsed = loka_sparql::parse(&query)?;
            loka_sparql::optimize(&mut parsed);
            let result = loka_sparql::execute_with_vectors(&parsed, &store, &dict, &vectors)?;

            // Print results as a simple table
            if result.columns.len() == 1 && result.columns[0] == "result" {
                // ASK query
                if let Some(row) = result.rows.first() {
                    if let Some(&id) = row.get("result") {
                        if loka_core::decode_inline_boolean(id) == Some(true) {
                            println!("true");
                        } else {
                            println!("false");
                        }
                    }
                }
            } else {
                // SELECT query
                println!("{}", result.columns.join("\t"));
                println!("{}", "-".repeat(result.columns.len() * 20));
                for row in &result.rows {
                    let vals: Vec<String> = result
                        .columns
                        .iter()
                        .map(|col| {
                            row.get(col)
                                .map(|&id| resolve_result_id(id, &dict, &result.values))
                                .unwrap_or_default()
                        })
                        .collect();
                    println!("{}", vals.join("\t"));
                }
                println!("\n{} rows", result.rows.len());
            }
        }

        Commands::Import {
            file,
            data_dir,
            show_errors,
        } => {
            let ps = loka_core::PersistentStore::open(&data_dir)?;

            let reader: Box<dyn BufRead> = if file == "-" {
                Box::new(std::io::stdin().lock())
            } else {
                let f = std::fs::File::open(&file)?;
                Box::new(std::io::BufReader::new(f))
            };

            let mut inserted = 0usize;
            let mut errors = 0usize;
            let mut line_no = 0usize;

            for line in reader.lines() {
                let line = line?;
                line_no += 1;

                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }

                let parsed = match loka_core::parse_ntriples_star_line(&line) {
                    Some(t) => t,
                    None => {
                        errors += 1;
                        if show_errors {
                            eprintln!("  line {}: parse error: {}", line_no, trimmed);
                        }
                        continue;
                    }
                };

                // If the subject is a quoted triple, intern the inner triple
                // and compute a content-addressed ID for it.
                let s_id = if let Some((inner_s, inner_p, inner_o)) = &parsed.inner_subject {
                    let is_id = ps.intern(inner_s)?;
                    let ip_id = ps.intern(inner_p)?;
                    let io_id = ps.intern(inner_o)?;
                    // Store the inner triple itself + the reverse mapping so
                    // the content-hash id can be reversed (faithful rendering
                    // + provenance cascade).
                    let _ = ps.insert(loka_core::Triple::new(is_id, ip_id, io_id));
                    ps.register_quoted(is_id, ip_id, io_id)?
                } else {
                    ps.intern(&parsed.subject)?
                };

                let p_id = ps.intern(&parsed.predicate)?;

                // If the object is a quoted triple, intern it too
                let o_id = if let Some((inner_s, inner_p, inner_o)) = &parsed.inner_object {
                    let is_id = ps.intern(inner_s)?;
                    let ip_id = ps.intern(inner_p)?;
                    let io_id = ps.intern(inner_o)?;
                    let _ = ps.insert(loka_core::Triple::new(is_id, ip_id, io_id));
                    ps.register_quoted(is_id, ip_id, io_id)?
                } else {
                    ps.intern(&parsed.object)?
                };

                match ps.insert(loka_core::Triple::new(s_id, p_id, o_id)) {
                    Ok(()) => inserted += 1,
                    Err(e) => {
                        errors += 1;
                        if show_errors {
                            eprintln!("  line {}: insert error: {}: {}", line_no, e, trimmed);
                        }
                    }
                }

                #[allow(clippy::manual_is_multiple_of)]
                if inserted > 0 && inserted % 10000 == 0 {
                    eprintln!("  {} triples imported (line {})", inserted, line_no);
                }
            }

            ps.flush()?;
            println!(
                "Imported {} triples ({} errors) from {}",
                inserted, errors, file
            );
        }

        Commands::Export {
            data_dir,
            output,
            format,
        } => {
            let ps = loka_core::PersistentStore::open(&data_dir)?;

            let mut writer: Box<dyn Write> = if let Some(path) = &output {
                Box::new(std::fs::File::create(path)?)
            } else {
                Box::new(std::io::stdout().lock())
            };

            let mut count = 0usize;
            for triple in ps.iter() {
                let s = ps
                    .resolve(triple.subject)?
                    .unwrap_or_else(|| format!("_:id{}", triple.subject));
                let p = ps
                    .resolve(triple.predicate)?
                    .unwrap_or_else(|| format!("_:id{}", triple.predicate));
                let o = ps
                    .resolve(triple.object)?
                    .unwrap_or_else(|| resolve_object_persistent(triple.object, &ps));

                if format == "ttl" || format == "turtle" {
                    // Simplified Turtle (no prefix compression for CLI)
                    if s.starts_with("_:") {
                        write!(writer, "{}", s)?;
                    } else {
                        write!(writer, "<{}>", s)?;
                    }
                    write!(writer, " <{}> ", p)?;
                    if o.starts_with('"') || o.starts_with("_:") {
                        writeln!(writer, "{} .", o)?;
                    } else {
                        writeln!(writer, "<{}> .", o)?;
                    }
                } else {
                    // N-Triples
                    if s.starts_with("_:") {
                        write!(writer, "{}", s)?;
                    } else {
                        write!(writer, "<{}>", s)?;
                    }
                    write!(writer, " <{}> ", p)?;
                    if o.starts_with('"') || o.starts_with("_:") {
                        writeln!(writer, "{} .", o)?;
                    } else {
                        writeln!(writer, "<{}> .", o)?;
                    }
                }
                count += 1;
            }

            if output.is_some() {
                eprintln!("Exported {} triples", count);
            }
        }

        Commands::Info { data_dir } => {
            let ps = loka_core::PersistentStore::open(&data_dir)?;
            let triple_count = ps.len();

            // Count terms
            let mut dict = loka_core::TermDictionary::new();
            let term_count = ps.load_terms_into(&mut dict);

            println!("Loka — {}", data_dir);
            println!("  Triples: {}", triple_count);
            println!("  Terms:   {}", term_count);
        }

        Commands::Health {
            data_dir,
            rebuild_hnsw,
            refresh,
            json,
        } => {
            // Load the persistent store and hydrate in-memory structures.
            let ps = loka_core::PersistentStore::open(&data_dir)?;
            let mut store = loka_core::TripleStore::new();
            let mut dict = loka_core::TermDictionary::new();
            ps.load_terms_into(&mut dict);
            for triple in ps.iter() {
                let _ = store.insert(triple);
            }

            // Hydrate vector registry (empty for now — vector declarations
            // are rebuilt from stored triples at server startup, not here).
            let vectors = Arc::new(RwLock::new(loka_hnsw::VectorRegistry::new()));

            // Rebuild HNSW indexes if requested.
            if rebuild_hnsw {
                let mut vecs = vectors.write().unwrap();
                let mut total_removed = 0usize;
                for pred_id in vecs.predicates() {
                    if let Some(index) = vecs.get_mut(pred_id) {
                        let pred_name = dict.resolve(pred_id).unwrap_or("unknown");
                        let before = index.len();
                        let removed = index.compact();
                        total_removed += removed;
                        if !json {
                            println!(
                                "Rebuilt HNSW index '{}': removed {} tombstones ({} → {} nodes)",
                                pred_name,
                                removed,
                                before,
                                index.len()
                            );
                        }
                    }
                }
                if !json {
                    if total_removed == 0 {
                        println!("No tombstones found — all HNSW indexes are clean.");
                    }
                    println!();
                }
            }

            // Discover pseudo-tables if requested.
            let pseudo_tables = if refresh {
                if !json {
                    println!("Discovering pseudo-tables from graph structure...");
                }
                let node_props = loka_core::extract_node_properties(&store);
                let registry = loka_core::discover_pseudo_tables(&node_props, &store);
                if !json {
                    println!(
                        "Discovered {} pseudo-table(s) covering {} nodes.",
                        registry.len(),
                        registry.total_coverage()
                    );
                    println!();
                }
                Some(registry)
            } else {
                None
            };

            // Generate and display the health report.
            let vecs = vectors.read().unwrap();
            let report =
                loka_sparql::generate_health_report(&store, &dict, &vecs, pseudo_tables.as_ref());
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", report.to_ai_text());
            }
        }

        Commands::InstallAgent {
            name,
            port,
            passcode,
            dimensions,
            metric,
            no_serve,
            launch_studio,
            json,
        } => {
            let data_dir = format!("./{}", name);
            let notes_file = format!("{}_loka_notes.md", name);

            if !json {
                println!("# Loka Agent Installer");
                println!();
                println!("Setting up database: {}", name);
                println!("  Data directory: {}", data_dir);
                println!("  Port: {}", port);
                println!(
                    "  Authentication: {}",
                    if passcode.is_some() {
                        "enabled"
                    } else {
                        "none"
                    }
                );
                println!("  Default vector dimensions: {}", dimensions);
                println!("  Distance metric: {}", metric);
                println!();
            }

            // Create the persistent store
            let ps = loka_core::PersistentStore::open(&data_dir)?;
            ps.flush()?;
            if !json {
                println!("[OK] Database created at {}", data_dir);
            }

            // Generate notes file
            let auth_note = match &passcode {
                Some(p) => format!(
                    "Authentication is enabled. Use header: `Authorization: Bearer {}`",
                    p
                ),
                None => "No authentication configured. All requests are accepted.".to_string(),
            };
            let serve_flag = match &passcode {
                Some(p) => format!(" --passcode {}", p),
                None => String::new(),
            };
            let auth_header = match &passcode {
                Some(p) => format!(" \\\n  -H \"Authorization: Bearer {}\"", p),
                None => " \\".to_string(),
            };
            let auth_header_query = match &passcode {
                Some(p) => format!(" \\\n  -H \"Authorization: Bearer {}\"", p),
                None => String::new(),
            };

            let notes = format!(
                r#"# Loka Setup Notes — {}

## Configuration
- **Database:** {}
- **Data directory:** `{}`
- **Port:** {}
- **Authentication:** {}
- **Default vector dimensions:** {}
- **Distance metric:** {}

## Quick Start
```bash
# Start the server
loka serve --port {} --data-dir {}{}

# Check health
curl http://localhost:{}/health

# Insert triples
curl -X POST http://localhost:{}/triples \
  -H "Content-Type: text/plain"{}  -d '<http://example.org/s> <http://example.org/p> <http://example.org/o> .'

# Query
curl -X POST http://localhost:{}/sparql{} \
  -d 'SELECT * WHERE {{ ?s ?p ?o }} LIMIT 10'
```

## Endpoints
| Endpoint | Method | Description |
|----------|--------|-------------|
| `/sparql` | GET/POST | SPARQL queries |
| `/triples` | POST | Insert N-Triples |
| `/vectors/declare` | POST | Declare vector predicate |
| `/vectors` | POST | Insert vector |
| `/graph` | GET | Export as Turtle |
| `/health` | GET | Health check |
| `/vectors/health` | GET | HNSW diagnostics |

## Generated by
Loka Agent Installer v0.1.0
"#,
                name,
                name,
                data_dir,
                port,
                auth_note,
                dimensions,
                metric,
                port,
                data_dir,
                serve_flag,
                port,
                port,
                auth_header,
                port,
                auth_header_query,
            );

            std::fs::write(&notes_file, &notes)?;

            // Programmatic-setup mode: emit a single JSON object describing the
            // setup and stop here (no blocking server, no Studio launch).
            if json {
                let serve_command = format!(
                    "loka serve --port {} --data-dir {}{}",
                    port, data_dir, serve_flag
                );
                let result = serde_json::json!({
                    "name": name,
                    "data_dir": data_dir,
                    "notes_file": notes_file,
                    "port": port,
                    "auth": if passcode.is_some() { "enabled" } else { "none" },
                    "dimensions": dimensions,
                    "metric": metric,
                    "served": false,
                    "studio_launched": false,
                    "serve_command": serve_command,
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }

            println!("[OK] Notes written to {}", notes_file);

            if launch_studio {
                // Check if loka-studio directory exists relative to the binary
                let studio_dir = std::path::Path::new("loka-studio");
                if !studio_dir.exists() {
                    eprintln!("[WARN] loka-studio/ directory not found in current directory.");
                    eprintln!("       Run install-agent from the Loka repository root,");
                    eprintln!(
                        "       or launch Loka Studio manually: cd loka-studio && flutter run"
                    );
                } else {
                    println!("Launching Loka Studio...");
                    #[cfg(target_os = "windows")]
                    let result = std::process::Command::new("cmd")
                        .args(["/c", "start", "", "flutter", "run", "-d", "windows"])
                        .current_dir(studio_dir)
                        .spawn();
                    #[cfg(not(target_os = "windows"))]
                    let result = std::process::Command::new("flutter")
                        .args(["run", "-d", "linux"])
                        .current_dir(studio_dir)
                        .spawn();
                    if let Err(e) = result {
                        eprintln!("[WARN] Could not launch Loka Studio: {}", e);
                        eprintln!("       Ensure Flutter is installed: https://flutter.dev/docs/get-started/install");
                    }
                }
            }

            if !no_serve {
                println!();
                println!("Starting Loka server...");
                println!("  loka serve --port {} --data-dir {}", port, data_dir);
                println!();

                // Actually start the server
                let ps2 = loka_core::PersistentStore::open(&data_dir)?;

                // Verify index consistency on startup
                if !ps2.verify_consistency() {
                    println!("[WARN] Index inconsistency detected. Repairing...");
                    let count = ps2.repair()?;
                    ps2.flush()?;
                    println!("[OK] Repair complete: rebuilt {} triples", count);
                }

                let mut dict = loka_core::TermDictionary::new();
                let mut store = loka_core::TripleStore::new();

                let term_count = ps2.load_terms_into(&mut dict);
                let mut triple_count = 0usize;
                for triple in ps2.iter() {
                    let _ = store.insert(triple);
                    triple_count += 1;
                }
                if triple_count > 0 {
                    println!(
                        "[OK] Loaded {} terms, {} triples from disk",
                        term_count, triple_count
                    );
                }

                // Rebuild HNSW indexes from stored vector triples
                let mut vectors = loka_hnsw::VectorRegistry::new();
                let mut vec_count = 0usize;
                let f32vec_suffix = "^^<http://loka.dev/f32vec>";
                for triple in store.iter() {
                    if let Some(obj_str) = dict.resolve(triple.object) {
                        if obj_str.contains(f32vec_suffix) {
                            if let Some(start) = obj_str.find('"') {
                                let end = obj_str[start + 1..].find('"').map(|p| p + start + 1);
                                if let Some(end) = end {
                                    let vec_str = &obj_str[start + 1..end];
                                    let floats: Vec<f32> = vec_str
                                        .split_whitespace()
                                        .filter_map(|s| s.parse::<f32>().ok())
                                        .collect();
                                    if !floats.is_empty() {
                                        let dims = floats.len();
                                        if !vectors.has_index(triple.predicate) {
                                            let config = loka_hnsw::VectorPredicateConfig {
                                                predicate_id: triple.predicate,
                                                dimensions: dims,
                                                m: 16,
                                                ef_construction: 200,
                                                metric: loka_hnsw::DistanceMetric::Cosine,
                                            };
                                            let _ = vectors.declare(config);
                                        }
                                        let _ =
                                            vectors.insert(triple.predicate, floats, triple.object);
                                        vec_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                if vec_count > 0 {
                    println!("[OK] Rebuilt {} vectors in HNSW indexes", vec_count);
                }

                let state = Arc::new(loka_proto::AppState {
                    store: RwLock::new(store),
                    dict: RwLock::new(dict),
                    vectors: RwLock::new(vectors),
                    persistent: Some(RwLock::new(ps2)),
                    passcode,
                    rate_limit_per_min: 0,
                    rate_counter: std::sync::atomic::AtomicU64::new(0),
                });

                let app = loka_proto::router(state);
                let addr = format!("127.0.0.1:{}", port);
                println!("[OK] Loka {} listening on http://{}", VERSION, addr);
                let listener = tokio::net::TcpListener::bind(&addr).await?;
                axum::serve(listener, app).await?;
            }
        }
    }

    Ok(())
}

/// Render a QUERY RESULT id: the per-query computed-value table first, then the
/// dictionary.
///
/// Separate from `resolve_id` because that one also renders ids read straight out
/// of the store, where no computed value can occur. Threading an always-empty
/// table through those call sites would suggest they can see one.
fn resolve_result_id(
    id: loka_core::TermId,
    dict: &loka_core::TermDictionary,
    values: &loka_core::QueryValues,
) -> String {
    if let Some(value) = values.get(id) {
        return value.to_string();
    }
    resolve_id(id, dict)
}

fn resolve_id(id: loka_core::TermId, dict: &loka_core::TermDictionary) -> String {
    if let Some(n) = loka_core::decode_inline_integer(id) {
        return n.to_string();
    }
    if let Some(b) = loka_core::decode_inline_boolean(id) {
        return b.to_string();
    }
    dict.resolve(id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("_:id{}", id))
}

fn resolve_object_persistent(id: loka_core::TermId, ps: &loka_core::PersistentStore) -> String {
    if let Some(n) = loka_core::decode_inline_integer(id) {
        return format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#integer>", n);
    }
    if let Some(b) = loka_core::decode_inline_boolean(id) {
        return format!("\"{}\"^^<http://www.w3.org/2001/XMLSchema#boolean>", b);
    }
    ps.resolve(id)
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("_:id{}", id))
}

// ─── Self-Update ─────────────────────────────────────────────────────────────

const GITHUB_REPO: &str = "EmmaLeonhart/Loka";

/// Check the latest release version from GitHub.
async fn check_latest_version() -> Option<String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", format!("loka/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let tag = json["tag_name"].as_str()?;
    // Strip leading 'v' from tag name
    Some(tag.strip_prefix('v').unwrap_or(tag).to_string())
}

/// Get the download URL for the current platform from a GitHub release.
fn get_asset_url(assets: &[serde_json::Value]) -> Option<String> {
    let target = if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-arm64"
        } else {
            "macos-x64"
        }
    } else {
        "linux-x64"
    };

    for asset in assets {
        if let Some(name) = asset["name"].as_str() {
            // Match the CLI asset (e.g. "loka-windows-x64.zip") but not
            // the Studio asset (e.g. "loka-studio-windows-x64.zip")
            if name.contains(target) && !name.contains("studio") {
                return asset["browser_download_url"]
                    .as_str()
                    .map(|s| s.to_string());
            }
        }
    }
    None
}

/// Handle the `loka update` command.
async fn handle_update(check_only: bool) -> anyhow::Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!("Loka v{}", current);
    println!("Checking for updates...");

    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", format!("loka/{}", current))
        .send()
        .await?;

    if !resp.status().is_success() {
        println!("Could not check for updates (HTTP {})", resp.status());
        return Ok(());
    }

    let json: serde_json::Value = resp.json().await?;
    let tag = json["tag_name"].as_str().unwrap_or("unknown");
    let latest = tag.strip_prefix('v').unwrap_or(tag);

    if latest == current {
        println!("Already up to date (v{}).", current);
        return Ok(());
    }

    println!("New version available: v{} → v{}", current, latest);

    if check_only {
        if let Some(body) = json["body"].as_str() {
            let summary: String = body.lines().take(10).collect::<Vec<_>>().join("\n");
            println!("\nRelease notes:\n{}", summary);
        }
        println!("\nRun `loka update` to install.");
        return Ok(());
    }

    // Find the right binary for this platform
    let assets = json["assets"].as_array().cloned().unwrap_or_default();
    let asset_url = match get_asset_url(&assets) {
        Some(url) => url,
        None => {
            println!("No pre-built binary found for this platform.");
            println!(
                "Update manually: cargo install --git https://github.com/{} loka-cli",
                GITHUB_REPO
            );
            return Ok(());
        }
    };

    println!("Downloading {}...", asset_url);
    let archive_resp = client
        .get(&asset_url)
        .header("User-Agent", format!("loka/{}", current))
        .send()
        .await?;

    if !archive_resp.status().is_success() {
        anyhow::bail!("Download failed (HTTP {})", archive_resp.status());
    }

    let bytes = archive_resp.bytes().await?;

    // Get current executable path
    let current_exe = std::env::current_exe()?;
    let backup_path = current_exe.with_extension("old");

    // Extract binary from archive
    let binary_name = if cfg!(target_os = "windows") {
        "loka.exe"
    } else {
        "loka"
    };

    let extracted = if asset_url.ends_with(".zip") {
        extract_from_zip(&bytes, binary_name)?
    } else {
        extract_from_tar_gz(&bytes, binary_name)?
    };

    // Replace current binary: rename current → .old, write new
    println!("Installing to {}...", current_exe.display());
    if backup_path.exists() {
        std::fs::remove_file(&backup_path).ok();
    }
    std::fs::rename(&current_exe, &backup_path)?;
    std::fs::write(&current_exe, &extracted)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))?;
    }

    println!(
        "Updated to v{}. Old binary saved as {}",
        latest,
        backup_path.display()
    );
    Ok(())
}

/// Extract a file from a zip archive in memory.
fn extract_from_zip(data: &[u8], file_name: &str) -> anyhow::Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name().ends_with(file_name) {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut buf)?;
            return Ok(buf);
        }
    }
    anyhow::bail!("Binary '{}' not found in archive", file_name)
}

/// Extract a file from a tar.gz archive in memory.
fn extract_from_tar_gz(data: &[u8], file_name: &str) -> anyhow::Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(data);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if path.file_name().and_then(|n| n.to_str()) == Some(file_name) {
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf)?;
            return Ok(buf);
        }
    }
    anyhow::bail!("Binary '{}' not found in archive", file_name)
}

/// Recursively copy a directory (for backups).
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            // Skip the backups subdirectory to avoid recursive backup
            if entry.file_name() == "backups" {
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CLI's result renderer must consult the per-query computed-value table
    /// before the dictionary. Without that, a `BIND` column prints `_:idN` — the
    /// fallback for an unresolvable id — which reads as a blank node rather than
    /// as the string the query computed.
    #[test]
    fn result_rendering_prefers_computed_values() {
        let mut dict = loka_core::TermDictionary::new();
        let mut values = loka_core::QueryValues::new();
        let computed = values.intern("Entity").unwrap();
        let iri = dict.intern("http://example.org/a");

        assert_eq!(resolve_result_id(computed, &dict, &values), "Entity");
        // Without the table it would fall through to the `_:idN` fallback.
        assert!(resolve_id(computed, &dict).starts_with("_:id"));

        // Ordinary ids are unaffected, including when a table is present.
        assert_eq!(
            resolve_result_id(iri, &dict, &values),
            "http://example.org/a"
        );
        assert_eq!(
            resolve_result_id(loka_core::inline_integer(42).unwrap(), &dict, &values),
            "42"
        );
    }
}
