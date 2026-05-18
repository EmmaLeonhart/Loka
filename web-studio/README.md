# Loka Studio — JS/HTML build (`web-studio/`)

A plain HTML/JavaScript replica of the Flutter Loka Studio, **real
DOM** (no Flutter/CanvasKit — so vis-network and other JS compose
freely). Built because Flutter-web wraps the whole UI in one
`<canvas>`, which blocks the browser-native knowledge-graph
interop Emma wants.

**Tabs** (Emma's spec — *"replication of the flutter app in JS, but
all of the existing html things are tabs in it; JS knowledge graph
is best"*) — a **left side rail**, not a top nav:

| Tab | What it is |
|---|---|
| **Knowledge Graph** | `<iframe>` → the engine's `/browse` (the vis-network viewer). Click a node = expand real neighbours; **double-click = the world model generates triples for it** (see below) |
| **SPARQL** | JS editor + examples + type-coloured results (`LokaClient.query`) |
| **Triples** | Paged `SELECT ?s ?p ?o` table |
| **Health** | Reachability, triple count, type distribution, HNSW `/vectors/health` |
| **Ontology** | Full graph as Turtle / N-Triples (`GET /graph`) + download |

There is **no Playground tab**: the engine's old `:3030/` SPARQL IDE
was redundant with the SPARQL tab, so it was dropped and the
`playground_server` example no longer serves an HTML page at `/`
(real `loka serve` never did). This studio is the single integrated
UI; point it at any Loka endpoint.

`app.js` carries `LokaClient` — a 1:1 JS port of
`loka-studio/lib/services/loka_client.dart`. The Flutter app
(`loka-studio/lib`, `build/web`, `electron/`) is **frozen** as the
spec + a fallback; nothing here modifies it.

## Double-click → world model generates triples

In the **Knowledge Graph** tab, double-clicking an IRI node asks the
trained world model to generate new triples for that node. The flow:

`browse.html` → POST `/generate` to the **inference sidecar**
(`tools/infer_server.py`, default `:8092`) → the sidecar runs the
pinned checkpoint (it reuses `infer_with_citations.py`'s
`load_model` / `build_inference_state` / `generate_for_subject` — no
forked inference logic), tags the output with `loka.dev/provenance/`
RDF-star, POSTs it to the Loka endpoint (`/triples`), and returns it.
The new triples appear in the graph as **amber dashed edges** (a
`✨ generated` legend item + edge filter), and persist in Loka — so
they're queryable and retractable via cascade-retraction.

The engine deliberately exposes **no** model inference itself (it
stays lean); the sidecar is the bridge. Start it with `!infer.bat`:

```
python tools/infer_server.py --port 8092
```

First start downloads the pinned checkpoint+vocab+tokenizer (~180 MB)
from the public `EmmaLeonhart/loka` HF dataset (no login needed). The
model endpoint is shown/editable in the `/browse` header (with a
status dot) and persisted. If the sidecar is down, double-click shows
a message instead of failing silently.

**Note:** the pinned model is trained on normalized Wikidata; the
Shinto demo (`example.org`) is out-of-distribution, so emissions are
rough/hallucinated by design — auditable via the provenance tags and
filterable. That's expected (see `planning/world-model-doubleclick.md`).

## Run

Needs a Loka endpoint (default `http://localhost:3030` — e.g.
`!playground.bat` for the Shinto demo, or `!serve.bat`). The
endpoint is editable in the top bar and persisted. For the
double-click→generate feature, also run `!infer.bat` (the world-model
sidecar, `:8092`) — see "Double-click → world model" below.

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
