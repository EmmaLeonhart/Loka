# Loka Studio — JS/HTML build (`web-studio/`)

A plain HTML/JavaScript replica of the Flutter Loka Studio, **real
DOM** (no Flutter/CanvasKit — so vis-network and other JS compose
freely). Built because Flutter-web wraps the whole UI in one
`<canvas>`, which blocks the browser-native knowledge-graph
interop Emma wants.

**Tabs** (Emma's spec — *"replication of the flutter app in JS, but
all of the existing html things are tabs in it; JS knowledge graph
is best"*):

| Tab | What it is |
|---|---|
| **Knowledge Graph** | `<iframe>` → the engine's `/browse` (the vis-network viewer — the JS graph) |
| **SPARQL** | JS editor + examples + type-coloured results (`LokaClient.query`) |
| **Triples** | Paged `SELECT ?s ?p ?o` table |
| **Health** | Reachability, triple count, type distribution, HNSW `/vectors/health` |
| **Ontology** | Full graph as Turtle / N-Triples (`GET /graph`) + download |
| **Playground** | `<iframe>` → the engine's `/` (the existing SPARQL IDE) |

`app.js` carries `LokaClient` — a 1:1 JS port of
`loka-studio/lib/services/loka_client.dart`. The Flutter app
(`loka-studio/lib`, `build/web`, `electron/`) is **frozen** as the
spec + a fallback; nothing here modifies it.

## Run

Needs a Loka endpoint (default `http://localhost:3030` — e.g.
`!playground.bat` for the Shinto demo, or `!serve.bat`). The
endpoint is editable in the top bar and persisted.

**Browser:**
```
STUDIO_WEB_ROOT="$PWD/web-studio" STUDIO_WEB_PORT=8091 \
  node loka-studio/electron/server.js
# → http://localhost:8091
```

**Electron:** `!studio.bat`, or:
```
cd loka-studio/electron && npm run studio:js
```
`studio:js` sets `STUDIO_WEB_ROOT`/`STUDIO_WEB_PORT` so the shared
`server.js`/`main.js` serve this instead of the Flutter build;
plain `npm start` still loads the frozen Flutter build (independent
paths).
