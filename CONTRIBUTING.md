# Contributing to Loka

Loka is an RDF-star triplestore in Rust with native HNSW vector indexing, temporal
(ontochronological) queries, and SPARQL+. It is at **Developer Preview** — the core engine,
SPARQL+, vector indexing, HTTP server, ACID compliance, self-update and MCP server work, but
APIs may still change before 1.0.

## The design principles a patch has to respect

These are from the README and they decide most review outcomes:

1. **Store first, reason second.** The database stores what you put in. OWL validation happens
   **client-side in the SDKs**, not in the engine. Don't add reasoning or schema enforcement to
   the store.
2. **Vectors are triples.** An embedding is an attribute of a node or edge, stored via a typed
   predicate and indexed by HNSW — not a parallel subsystem with its own storage.
3. **Full traversal in a single query.** Any traversal of any depth must be expressible in one
   SPARQL query.
4. **Lean by default.** Every feature justifies itself. A patch that adds a dependency or a
   configuration knob needs to argue for it.
5. **Serverless by default, server when needed.** Loka embeds like SQLite — open a `.sdb` and
   go. `loka serve` is opt-in.
6. **Agent-first, GUI-optional.** The CLI is the primary interface; Loka must be fully operable
   without a GUI. Loka Studio exists for visual diagnostics.

## Build and test

Rust stable. The workspace is `loka-core`, `loka-hnsw`, `loka-sparql`, `loka-proto`,
`loka-cli`, `loka-ffi` (`sdks/rust` is excluded from the workspace).

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings   # clippy warnings fail CI
cargo fmt --all
```

CI runs all four. Formatting is auto-committed by the `fmt` job, so don't hand-fix style —
but running `cargo fmt --all` locally keeps the diff clean.

### Always run the repo-local build

```bash
cargo build --release -p loka-cli
./target/release/loka ...
```

**Never invoke a bare `loka`.** It resolves through `PATH` to an installed copy, and a stale
install has already cost this project real debugging time: an installed binary two months
behind source reported the same `0.4.1` version string, which hid the fact that it still bound
`0.0.0.0` after the source had moved to `127.0.0.1`, and produced three "engine bugs" that did
not reproduce on a current build. Version strings do not distinguish builds. Use the path.

### SDK tests

```bash
cd sdks/python && python -m pytest tests/ -q
```

SDKs live under `sdks/` for Python, TypeScript, Rust, Go, Java and .NET. Client-side OWL
validation is an SDK responsibility (principle 1), so validation changes usually belong here
rather than in the engine.

## Networking and defaults

`loka serve` binds **`127.0.0.1`** by default, matching serverless-by-default. Binding all
interfaces raises an OS firewall prompt per launch, which is disruptive when a tool starts
servers programmatically. Remote access is meant to arrive as an explicit `--host` flag rather
than by widening the default; don't change the default bind address in a patch.

The only outbound network call in the codebase is a GitHub releases update check on the
`loka mcp` path. Adding outbound traffic needs a strong reason and should be documented.

## Reporting a bug

Include:

- The **build** you ran — `cargo build --release` from which commit — not just the version
  string. See above for why.
- The `.sdb` size / triple count, since several behaviours are scale-dependent.
- The exact SPARQL, and whether each pattern matches on its own.
- Whether it reproduces across **separate processes**. At least one past report varied per
  process, which pointed at hash-seeded planner join ordering and made diagnosis much harder
  than it needed to be. Say so if you see that.

A bug that stops reproducing is not automatically fixed — if the original was intermittent,
note that in the issue rather than closing it.

## Workflow conventions

- **`TODO.md`** — long-horizon backlog, with a completed-items tally.
- **`queue.md`** — what is being worked on right now. Items migrate `TODO.md` → `queue.md` →
  deleted on completion, in the same commit as the work.
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/`** — specs (`world-model-thesis.md` is the canonical vision).
- **`benchmarks/`** — results are committed by CI; `BENCHMARKS.md` is the summary.

Update `queue.md` in the same commit as the work it describes.

## Pull requests

- Branch from `main`; CI gates on `ci.yml` (check, test, clippy, fmt, plus per-SDK jobs).
- State measured results for anything performance-related. Latency claims should say the
  triple count and the build.
- Don't weaken or delete a failing test to get green.

License: see `LICENSE`.
