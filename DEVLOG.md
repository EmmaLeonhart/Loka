# Loka — Development Log

The single canonical record of how this project evolved. Newest entries at the top.

This started as **SutraDB**, a lean RDF-star triplestore with native vector indexing and a hybrid SPARQL extension. Over time the *purpose* shifted: it became one half of a neuro-symbolic world-model engine — explicit memory (the store, exact answers) plus implicit memory (a transformer trained on the same triples, plausible answers with cited inference chains). The model/data distribution side is being rebranded **Loka** on Hugging Face; the GitHub repo will follow.

The "why" matters more than the "what." Per-commit detail lives in `git log`. This document is for narrative continuity — so a cold pickup understands the *trajectory* of the project, not just its current state. (For the current state, see `status.md`.)

---

## 2026-05 — The neuro-symbolic world-model pivot

### What changed in framing

The earlier framing was "lean RDF-star triplestore that handles vector queries natively." That's still true mechanically, but the *purpose* moved: the engine is now one half of a two-system composition.

- **The store** = explicit memory. Stores what is known. Returns exact answers.
- **A small transformer trained from scratch on the same triples** = implicit memory. Predicts what is plausible. Returns inferred answers with cited inference chains.

Both expose the same SPARQL+ interface. The caller doesn't pick which system answered — federation is implicit, except through provenance edges on the result. Canonical vision: `planning/world-model-thesis.md`.

Product framing: **what Ollama is to LLMs, Loka is to world models.** Pull or train a world model locally; pluggable; agent-first; honest provenance. The "agent-first" stance was already baked into SutraDB; the world-model layer is what makes the whole project a thing you'd want to install rather than just a database.

The thesis explicitly *rejected* fine-tuning a general LLM on RDF (§6.6) for provenance, closed-world, and hallucination reasons. That rejection was revisited mid-period (see §10.5 of the thesis) and admitted as a parallel near-term track, for empirical pragmatism: small from-scratch models on small corpora produce word salad, while a fine-tuned 1–7B base could plausibly produce coherent triples in days. Both tracks share the same `propositionInferredFrom` output schema. See `planning/fine-tuning-track.md`.

### RDF-star is THE citation mechanism

RDF-star moved from "one feature among many" to **load-bearing**. It's how every kind of citation in the system is expressed:

| Verb | Used for | Emitted by |
|---|---|---|
| `propositionInferredFrom` | model-generated triple → context that informed it | `infer_with_citations.py` |
| Wikidata `wdt:P854` / `wdt:P248` / `wdt:P813` / ... | external curated references | importers |

All use the identical `<<S P O>> verb <<source>>` shape. Wikidata's API distinguishes "qualifiers" from "references" but Loka collapses both into the same RDF-star annotation form because they're semantically the same thing.

**Reserved namespace.** Every predicate under `http://sutra.dev/provenance/` is system-internal. The world model **never** sees, proposes, or emits one. Three layers of enforcement:

1. Corpus stripping (`preprocess.py`) drops every row whose predicate matches the prefix.
2. SPARQL-star `FILTER NOT EXISTS << ?s ?p ?o >> propositionGenerated ?_g` excludes inner generated triples at query time.
3. Inference (`infer_with_citations.py`) refuses to consider reserved-namespace predicates as candidates and refuses to emit one even if a downstream bug allowed it.

Names are deliberately verbose (`propositionInferredFrom`, not `inferredFrom`) so a human scanning raw triples spots them at a glance. The discipline matters: if the model ever learned the provenance predicates exist, it could hallucinate fake citation edges, undermining the auditability that is the whole point of the system.

Hallucinated *content* in citations is **not** a blocker. A fabricated citation is still an RDF-star row pointing at concrete context — auditable, filterable, often informative about what the model thinks the reasoning is. Don't add elaborate guards.

### The data layer rebuild

The corpus underwent a complete rebuild over this period.

- **BFS importer learned RDF-star qualifiers + references.** Each Wikidata claim now emits the main triple plus an RDF-star annotation per qualifier *and* per reference snak, all sharing the `<<S P O>>` quoted-triple subject. Wikidata's `pq:` and `pr:` namespaces collapse into the same `wdt:` predicate URI on the annotation row — the qualifier-vs-reference distinction is structural (subject is a quoted triple), not lexical.

- **BFS → Hugging Face parquet stream.** Wikidata's API rate-limit (1.5s per request) made BFS the bottleneck — 5M triples needed days at that rate. Switched to streaming `philippesaade/wikidata` (CC0, ~30M entities, JSON-shaped per-entity rows in parquet) via the HuggingFace `datasets` library. Local-bandwidth-bound instead of API-bound. End state: 5,055,385 triples / 1,695,402 RDF-star annotations / 27,780 entities / 770 MB on-disk store.

- **`propositionImportedFrom` dropped.** Initially every imported triple got `<<S P O>> sutra:propositionImportedFrom <wikidata.org/wiki/Q...>`. For a database where every row came from Wikidata, that's redundant noise — 22,593 rows (~46% of all annotations). The actual provenance is already in Wikidata's own reference predicates.

- **Multilingual labels — every language Wikidata has.** Previously hardcoded en/ja/de/fr/zh; now iterates every language in `entity.labels` and `entity.descriptions`. The training preprocessor still filters to English, but the database keeps the multilingual richness.

- **Embeddings: gone.** The original BFS importer called Ollama (mxbai-embed-large) per entity. The world-model loop tokenizes English labels — vectors don't enter the training corpus. Stripped from the importer. The HNSW index in `sutra-core` stays — that's an engine feature, not specific to import.

### Two engine bugs surfaced

- **SPARQL `?s ?p ?o` occasionally returns literal values in the predicate slot.** RDF disallows literal predicates, so this is invalid output from the executor — almost certainly RDF-star annotation rows with positions getting confused. Filtered at preprocess (drops ~1% of rows on a 5M corpus). Real engine bug; fix later.

- **`POST /triples` wedges after roughly every 5–6× growth in stored triples.** Hit at ~174k and again at ~1M during the HF ingest. `/health` keeps responding, but `/triples` and SPARQL hang indefinitely until the server is restarted. On restart, all data is intact on disk. Symptoms point at LSM compaction or persistent-index rebuild holding the write lock. Workaround: an automated stop/restart loop. Real engine bug.

A separate proto-layer bug was found and **fixed** mid-period: `POST /triples` was returning HTTP 400 for the entire batch the moment any RDF-star annotation's inner triple already existed in persistent storage. The in-memory branch already discarded `DuplicateTriple`; only the persistent branch propagated. Fixed at `server.rs:935` and `:962` so both branches handle duplicates the same way.

### Training pipeline

Versioning: v0/v1/v2 were the early smoke-test checkpoints on a 6,300-triple shrine-only corpus. v3 onward use the 5M-triple HF-derived corpus.

| Model | Architecture | Corpus | Final ppl | Notes |
|---|---|---|---|---|
| v3 | d_model 256, 4 layers, 16M params | 779k label-substituted triples | 53.4 | Pre-cleanup; misleadingly low ppl from memorising `xmlschema decimal` URI fragments |
| v4 | same | 757k cleaned triples | 92.5 | Higher ppl, *better* output. Numerical regression masks real-quality improvement |
| v5 | d_model 512, 6 layers, **44M params** | same 757k | _in flight as of writing_ | Bigger-model experiment; 3× capacity |

**Two corpus quality fixes between v3 and v4:**

1. *Strip `^^<datatype>` suffixes from typed literals.* SutraDB's SPARQL serialisation embeds the datatype in the value string. Without stripping, literal values like `+1966-02-18T00:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime>` reached the tokenizer as if the URI fragments were entity content. The model dutifully memorised them and emitted predictions like `Abbas Mirza | has works in collection | 1 http www w3 org 2001 xmlschema decimal`. After stripping: `Abbas Mirza | has works in collection | metropolitan museum of museum`. The Met genuinely holds Abbas Mirza pieces — that's real cross-entity inference, not memorised junk.

2. *Drop rows with non-URI predicates.* The SutraDB SPARQL quirk above produced literal values in the predicate slot, ~1% of 5M.

**Inference quality lever: cumulative repetition penalty in `infer_with_citations.py`.** The masked-prediction objective doesn't penalise the model for emitting the same token over and over, so even when the model "knows" the answer is `university of <something>`, decoding produces `university of of of of of of of`. The penalty divides each repeated token's logit by `repetition_penalty ** count` (default 3.0, cumulative — 3rd repeat divides by 27, usually drops below per-token floor and breaks the loop). Same v4 checkpoint, smarter decoder. Loops collapse to clean shorter outputs.

### Inference loop closes end-to-end

`training/infer_with_citations.py` is the generative-citation entry point. For a candidate subject:

1. Find predicates used by graph-neighbors (subjects sharing at least one (p, o) statement) but missing from this subject.
2. Mask the object slot, run the trained transformer.
3. If mean per-token confidence ≥ threshold, emit the new triple plus four kinds of RDF-star annotations:

```
<S> <P> "predicted-label" .
<<S P "predicted-label">>  sutra-prov:propositionGenerated     "true"^^xsd:boolean .
<<S P "predicted-label">>  sutra-prov:propositionGeneratedBy   "wikidata_v4" .
<<S P "predicted-label">>  sutra-prov:propositionConfidence    "0.43"^^xsd:decimal .
<<S P "predicted-label">>  sutra-prov:propositionInferredFrom  <<S existing_p existing_o>> .
   ...one row per cited context triple (default 10)
```

`--post` writes the result back into SutraDB. The reserved-namespace machinery ensures the model never sees its own provenance edges in subsequent training runs.

**Quality sample at v4 (50 subjects):** 32/250 candidate predictions met confidence threshold 0.4. Of those, ~⅓ are recognisably correct in shape and content, ~⅔ have the right semantic *category* but degenerate decoding ("university of of of"), a handful are wrong/garbage. The cumulative repetition penalty collapses the looping cases without losing the right-category signal.

The loop is genuinely closed: model produces predictions → predictions land in SutraDB tagged `propositionGenerated true` with `propositionInferredFrom` edges → preprocessor's SPARQL-star filter excludes them on the next training pass. Self-citing inference per `world-model-thesis.md` §5.5, at v0 fidelity.

### Loka — the rebrand

"SutraDB" is a name for the engine. The project that's emerging (engine + corpus + trained world model + inference layer) needs its own identity. **Loka** is the name on Hugging Face; the GitHub repo will be renamed to match later.

`tools/hf_snapshot.py` pushes corpus + checkpoints to a single dataset repo `<user>/loka` with each upload tagged as a snapshot revision (`v3`, `v4`, etc.). Each upload is a commit; tagged snapshots are pullable via `revision="v4"`. LFS is handled transparently by `huggingface_hub`. First push of v4 got 7 of 8 things up before the file-lock on the live `sutra-data/db` blocked the folder upload — added `--sutra-data-path` so future pushes can point at a frozen backup directory instead of the live store.

---

## 2026-04 — ManuForge integration testing → v0.3.7

Brief period of production-readiness fixes after testing SutraDB against the ManuForge SDK consumer. Surfaced a small set of real issues:

- **RDF-star query support fixes** — quoted-triple wildcards weren't matching correctly.
- **HTTP star import** — N-Triples-star payloads via `POST /triples` had edge cases on parsing.
- **CRLF line endings** — Windows clients sending CRLF-terminated N-Triples weren't being recognised.
- **Self-update asset bug** — release-pipeline self-update was downloading the wrong artifact name.
- **Import error reporting** — the response now lists per-line errors rather than failing the whole batch.

Bumped to **v0.3.7** at the end of this round. The ManuForge integration also produced `docs/AGENT_SETUP.md` for AI-agent consumers and a "limitations found in production" note that fed into the next round.

---

## 2026-03 (late) — Ontochronology + Sutra Studio + FFI

After v0.2.0, two large pieces landed in parallel:

### Sutra Studio (Flutter desktop/web/mobile client)

`sutra-ffi` crate wraps the engine in a C-compatible shared library so non-Rust consumers (Flutter, in particular) can embed the database in-process. Studio uses `dart:ffi` to load `sutra_ffi.dll`/`.so`/`.dylib` and runs the engine on a background thread sharing the same handle as the optional MCP server. Two entry points:
- `sutra mcp` → MCP + database, no GUI
- Sutra Studio → GUI + database + optional MCP server, all one process

Studio also auto-starts the server in serverless mode when launched, so the user never has to run `sutra serve` manually. Auto-update keeps Studio in sync with the CLI version. Includes graph view (D3 then vis-network), HNSW health diagnostics, OWL/Turtle export, dark/light theme, persistent connection settings. Launch via `sutra mcp --studio` or via the agent-installer's `download_studio` and `launch_studio` MCP tools.

### Ontochronology

A non-trivial extension: every triple is conceptually contained in a temporal interval, and queries can ask "what was true at time T" or "what changed between T1 and T2" without reifying every statement individually. Implementation phases:

- **Phase 1–3** — temporal literal type, predicates (`sutra:assertedAt`, `sutra:validFrom`, `sutra:validTo`), TSPO index.
- **Phase 4a** — `AT_TIME` and `DURING` query operators.
- **Phase 4b** — `WORLD_STATE` and `TEMPORAL_DIFF`.
- Temporal-aware property path traversal.

Containment semantics use three-valued query logic (true / false / unknown). Design lives in `docs/ontochronology.md`.

### Other March-late items

- **Pseudo-tables to deep subgraph columnar indexes** — generalised the columnar shortcut so multi-hop subgraph queries can run vectorised SIMD scans where the structure repeats.
- **Cost-based query planning** — predicate pushdown, HNSW edge labelling, join-strategy selection, hash join optimization for large intermediate result sets.
- **Vector SPARQL operators** — `COSINE_SEARCH`, `EUCLID_SEARCH`, `DOTPRODUCT_SEARCH`.
- **ACID compliance** — atomic transactions, durability, isolation. `PersistentStore.clear()` and GSP DELETE durability fixes.
- **Self-update + version check + HNSW rebuild endpoint.**
- **Theory pages on sutradb.org** — 18+ explainer pages: HNSW-in-RDF, four-index architecture, RDF-star edges, SPARQL exit conditions, hybrid databases, traversal indexing, cost-based planning, etc.
- **Code of Ethics page** — Buddhist/Shinto-techno-animist framing, deadpan style.

---

## 2026-03 (mid) — v0.2.0 Developer Preview

A consolidating release. Headlines: query planner, agent installer, Java SDK, Sutra Studio first cut. All four SDKs (Go, Rust, Java, .NET) had endpoint mismatches caught and fixed during this period. SDK publish workflow + integration test CI added.

Pseudo-tables landed in this window too: columnar indexes with zonemap pruning and vectorized scans, on top of the standard SPO/POS/OSP indexes. Designed to make multi-hop subgraph queries (the kind RDF databases are typically slow at) competitive with property-graph databases.

Released as **v0.2.0** Developer Preview on 2026-03-18.

---

## 2026-03-15 — The SPARQL completeness sweep

A single very productive day. Brought SPARQL coverage from "minimum viable" to roughly feature-complete for SPARQL 1.1 over RDF-star.

### Core engine
- **First-query cold-start fix** — replaced dense `Vec<bool>` visited list with `HashSet`, cut ~2s page-fault overhead at 200K+ HNSW nodes.
- **HNSW cross-cluster search** — multiple entry points (up to 8), score all and start from best. Fixed a long-standing bias toward the first-inserted cluster.
- **Persistence** — `PersistentStore` (sled-backed) wired to the HTTP server with write-through. In-memory stores hydrate on startup. Data survives restart.
- **Blank node support** in the N-Triples parser.
- **Query timeout** — `execute_with_timeout()` with per-pattern deadline checks and `SparqlError::Timeout`.
- **SIMD-accelerated distance functions** — AVX2/FMA + SSE fallback for `dot_product`, `squared_euclidean`, `l2_norm`.
- **HNSW rebuild from stored vector triples on startup** — vectors persist; the index is reconstructed lazily.
- **HNSW compaction** — background pass to clean tombstoned nodes, plus `/vectors/health` endpoint for diagnostics.
- **Hash join optimization** for large intermediate result sets.
- **Cardinality estimation** for cost-based planning.
- **Crash recovery** — `verify_consistency()` and `repair()` for index integrity.
- **Adjacency lists** materialized for Neo4j-speed traversal.
- **Parallel HNSW construction** via rayon.

### SPARQL completeness
- `FILTER NOT EXISTS` / `EXISTS` with sub-pattern evaluation and `LIMIT 1` push-down.
- `ASK` queries.
- `GROUP BY` + aggregates (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, with `DISTINCT`).
- `BIND(term AS ?var)` and `VALUES ?var { ... }`.
- Boolean operators (`&&`, `||`, `!`) in `FILTER`.
- String functions (`CONTAINS`, `STRSTARTS`, `STRENDS`, `REGEX`).
- Comparison operators (`>=`, `<=`).
- Type checks (`isIRI()`, `isLiteral()`).
- `LANG()` / `LANGMATCHES()`.
- `INSERT DATA` / `DELETE DATA` (SPARQL Update).
- `CONSTRUCT` and `DESCRIBE`.
- `HAVING` clause for `GROUP BY` filtering.
- Property paths (`+`, `*`, `?`, `/`).
- Subqueries (nested `SELECT`).
- `DATATYPE()`, `STR()`, `COALESCE()`, `IF()`.
- Arithmetic in `FILTER` (`+`, `-`, `*`, `/`).
- **RDF-star quoted triple patterns** in SPARQL (`<< ?s ?p ?o >>`).

### CLI, distribution, and protocols
- `sutra import` (streaming line-by-line N-Triples to sled).
- `sutra export` (Turtle/N-Triples).
- `sutra info` (triple/term counts).
- `sutra install-agent` — agent-first installer that reasons through configuration and writes `<dbname>_sutra_notes.md` with its decisions.
- Install scripts (`install.bat`, `install.sh`).
- Dockerfile (multi-stage, exposes 3030, `/data` volume).
- `GET /graph` (Turtle/N-Triples export — Protégé integration point).
- `/sparql.csv` and `/sparql.tsv` formats.
- `/sparql.xml` (SPARQL Results XML).
- Content negotiation via `Accept` header.
- Service description at `/service-description`.
- Graph Store Protocol (`GET`/`PUT`/`DELETE /graph-store`).
- Simple passcode auth (server mode, opt-in).
- Rate limiting (server mode, opt-in).
- Periodic backups (server mode, configurable hourly/daily).

### Ecosystem
- **Protégé plugin** — Java OSGi bundle: Connect/Start, Load from SutraDB, Save to SutraDB, OWL Validate.
- **MCP server** for AI-agent ↔ SutraDB integration. `sutra mcp` runs the engine + MCP in one process.
- **Client-side OWL validation in Python SDK.**
- **owl:equivalentClass / owl:sameAs / owl:inverseOf / rdfs:subPropertyOf** support added to SDKs.
- **OWL verification query generation** — turn ontology constraints into SPARQL ASK queries.
- **Schema declaration via SPARQL `INSERT DATA`** — vector predicate dimensions, etc.
- **N-Quads parser** with named graph support.
- **Turtle parser** for bulk import.
- **RDF/XML parser** for OWL ontology imports.
- **JSON-LD parser.**
- **LangChain VectorStore integration** for SutraDB.
- **Jupyter `%%sparql` cell magic.**
- **Japanese label embedding script.**

### First Wikidata BFS import
On 2026-03-15: **439 entities / 16,084 triples / 439 vectors** (1024-dim mxbai-embed-large) from the Engishiki Jinmyōchō (Q11064932) BFS, 0 errors, 7,316 entities remaining in queue. Later abandoned as the corpus base in favour of the HF parquet stream (see 2026-05) — the BFS rate limit made it impractical to scale.

---

## 2026-03 (early) — Foundation + scale stress test

Project began on **2026-03-13** with the cleanvibe scaffold. Within 24 hours: architecture docs, normalised `sutra-*` workspace structure, `sutra-core` and `sutra-hnsw` foundations. Apache 2.0 license. CI workflow. Borrowed patterns explicitly from Qdrant (HNSW: immutable `GraphLayers` for search, thread-local visited pools, per-node `RwLock` during construction) and Jena TDB2 (storage, IRI interning, sled triplestore baseline).

Subsequent days landed:
- **Sled-backed persistent triple store.**
- **SPARQL parser, query planner, executor.**
- **HTTP server + CLI** with SPARQL endpoint.
- **Vector SPARQL integration** — connecting HNSW to the query engine. Architectural decisions documented (`docs/vectorSPARQL.md`): subject-bound-before-`VECTOR_SIMILAR` runs graph first, subject-unbound runs vector search first.
- **REST endpoints** for triple insertion + N-Triples parser.
- **Serverless-by-default philosophy** + `.sdb` file extension. Single-binary, embed-or-serve.
- **Vector architecture fix** — vectors are graph objects, not standalone. Every vector insertion now creates a corresponding triple, and the graph browser doesn't try to expand vector literal nodes.
- **GitHub Pages landing page** + Open Graph meta tags + 18+ theory pages.
- **Client SDKs in six languages** — Python, Go, Rust, Java, .NET, TypeScript.
- **Browser graph debug tool** (D3 force, later vis-network).
- **1M embedding stress test** — first hard scale check. Uncovered three performance issues that all got fixed in the same window. Final stress test passed all 14 queries with zero failures.
- **HNSW edges as RDF triples** — query the index structure itself via SPARQL.
- **Mutex → RwLock** for concurrent reads.
- **Documented architectural decisions:** Oxigraph as the reference for storage/indexing patterns; RDF-star as the reification model (vs RDF 1.2); SPARQL+ as the query language with extensions for vector and exit conditions; SQL/MongoDB query interfaces *permanently rejected* (offering them would mislead AI agents into relational/document thinking).

By the end of the first 48 hours the project had: a working engine, a working SPARQL surface, a working vector layer, persistence, six SDKs, CI, a website, and a stress test passing at 1M scale. That set the pace for everything that came after.

---

## Reference: how to read this document going forward

- **Newest entries at the top.** Drop a new dated section above existing ones when something meaningful lands.
- **Narrative, not flat lists.** Per-commit detail belongs in `git log`. Devlog entries explain *why*.
- **Headlines first.** A reader skimming for "what changed in the last month" should be able to get it from the first paragraph of each section.
- **Rebrand reminder.** "SutraDB" still appears in code and on the website; "Loka" is the model/data distribution name, currently only on Hugging Face. The repo rename is pending.

Status of in-flight work always lives in `status.md`, not here. This document is the record; `status.md` is the dashboard.
