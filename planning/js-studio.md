# JS/HTML Loka Studio — port Dart → JavaScript (real DOM)

**Decision (Emma, 2026-05-17):** Flutter-web-in-Electron works but
carries CanvasKit interop limits (whole UI is one `<canvas>`; can't
compose vis-network/JS). Direction: build a **plain HTML/JS** Studio
(real DOM), **converting the Flutter Dart code to JavaScript**, with
the **vis-network `/browse` graph as THE graph surface** (no Flutter
canvas graph). Flutter Studio (`loka-studio/lib`, `build/web`,
`electron/`) is **frozen as-is** — keep, don't delete; it's the spec
+ a working fallback. Build **incrementally, commit per slice**.

## Why this is tractable

The Dart app is small and API-driven. Every screen is just SPARQL /
REST calls against the Loka endpoint (default `http://localhost:3030`,
CORS `*`). No business logic to speak of — it's a thin client. The
graph screen is *not ported* (replaced by the existing vis-network
viewer the engine serves at `/browse`).

## Source = spec (port these Dart files to JS)

`loka-studio/lib/`:
- `services/connection_provider.dart`, `models/connection_config.dart`
  — endpoint state, default `http://localhost:3030`, persisted.
- `screens/sparql_screen.dart` — query editor + results table. The
  existing `pages/playground.html` already implements ~all of this in
  JS; **reuse/adapt it** rather than re-derive.
- `screens/triples_screen.dart` — paged triple browser (SPARQL
  `SELECT ?s ?p ?o … LIMIT/OFFSET`).
- `screens/health_screen.dart` — `GET /vectors/health` + `/health`.
- `screens/ontology_screen.dart` — `GET /graph` (Turtle) viewer.
- `screens/auth_screen.dart` — endpoint field + optional passcode.
- `screens/graph_screen.dart` — **DO NOT PORT.** Use the engine's
  `/browse` (vis-network) instead, embedded.
- `theme/loka_theme.dart` — port the color tokens (they already
  match `/identity.css` palette — reuse `pages/style.css` /
  `/identity.css` tokens instead of re-deriving).

Read each Dart screen for the exact SPARQL/endpoints it calls; the
JS port issues the same requests with `fetch`.

## Target layout

New dir `web-studio/` (sibling of `loka-studio/`), plain static:
```
web-studio/
  index.html        # app shell: top bar (brand, endpoint field,
                     # theme toggle), left nav, screen container
  app.js            # router + connection state (localStorage)
  screens/
    graph.js        # embeds the engine's /browse in an <iframe>
                     # (real DOM, full vis-network interop)
    sparql.js       # adapted from pages/playground.html
    triples.js      # paged SELECT table
    health.js       # /vectors/health + /health cards
    ontology.js     # /graph Turtle view
  style.css         # link /identity.css tokens; Loka chrome
```
Reuse the shared identity: link `/identity.css` (served by the
engine) or vendor it; match the site kit (`pages/style.css`).

Run it: the existing `loka-studio/electron/server.js` already serves
a static dir with correct wasm MIME + SPA fallback + EADDRINUSE
tolerance. **Repoint its `ROOT`** from `../build/web` to
`../../web-studio` (or add a `STUDIO_WEB_ROOT` env override), then
`loka-studio/electron/main.js` loads it in Electron unchanged. Same
server → browser tab + Electron both work.

## Slices (commit each)

1. **Shell + graph.** `index.html` + `app.js` (nav, endpoint state,
   theme via identity.css) + `screens/graph.js` embedding `/browse`
   in an iframe. Repoint `electron/server.js` ROOT (env override so
   Flutter build still works). Open in browser + Electron. This alone
   delivers the core vision: browser-native vis-network graph in a
   real-DOM app.
2. **SPARQL screen.** Adapt `pages/playground.html`'s editor +
   results renderer into `screens/sparql.js`.
3. **Triples screen.** Paged `SELECT ?s ?p ?o` table.
4. **Health screen.** `/vectors/health` + `/health` cards (port
   `health_screen.dart`).
5. **Ontology screen.** `/graph` Turtle viewer (port
   `ontology_screen.dart`).
6. Polish: theme toggle persistence, endpoint auth field, error
   states. Update README/site to mention the JS Studio.

## Environment / running state (context dies — read this)

As of 2026-05-17 these were running locally (may need restart in a
fresh session — restart commands in `queue.md`):
- `target/debug/examples/playground_server.exe` → Shinto demo, 73
  triples, `http://localhost:3030` (`/`, `/sparql`, `/browse`,
  `/graph`, `/vectors/health`; CORS `*`). Rebuild/run:
  `cargo build --example playground_server -p loka-proto` then run
  the exe (kill the old one first — it locks the exe).
- `target/release/loka.exe` exists (`!serve.bat` works; empty store).
- `node loka-studio/electron/server.js` → static server :8090.
- Electron (`loka-studio/electron/`, `node_modules` installed,
  electron v33) — `./node_modules/.bin/electron .`.
- Flutter at `/c/Users/Immanuelle/flutter/bin` (not on PATH).
- Engine `/browse` route = `loka-proto/src/server.rs` `serve_browse`
  (`tools/browse.html`); playground "Graph Browser" button → /browse.

## Guardrails

- Do **not** delete/break `loka-studio/` Flutter — it's the spec +
  fallback.
- Do **not** re-run Flutter web build or `npm install` unless needed
  (already done; thermal-sensitive laptop — see CLAUDE.md).
- Commit per slice with a why-focused message; keep `queue.md` in
  sync in the same commit.
- The graph is **always** the JS `/browse` viewer, never a ported
  canvas.
