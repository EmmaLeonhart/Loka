# Base-model BFS + embedding retrieval (Emma's algorithm)

**Decided 2026-05-19** after the masked-SFT fine-tune was shown to lobotomise
the model (`tools/_ft_probe3.py`: base Qwen2.5-1.5B continues RDF triples
well; the epoch-4 adapter emits bare fragments). Pivot: **no fine-tune** —
base Qwen + retrieval + a continuation prompt. Emma chose the *full*
BFS+embedding scope (not BFS-only).

## The algorithm (Emma's spec, verbatim intent)

1. Take node **N**.
2. Fetch every triple directly on N (graph frontier, depth 0).
3. Expand the frontier two ways:
   - **graph adjacency** — the nodes those triples connect to;
   - **vector similarity** — nearest nodes to N's embedding (Loka HNSW
     `VECTOR_SIMILAR`).
4. BFS outward to a depth/budget: for each reached node, fetch its triples
   and also pull triples *similar to the ones it sits in* (embedding).
5. Rank every gathered triple **least → most related** (combined score:
   BFS depth + vector similarity). Most-related goes **last** in the
   prompt — LLMs weight the tail of the context most.
6. Feed the ordered sequence to **base Qwen2.5-1.5B-Instruct** (no adapter)
   with a "continue this sequence of RDF triples" prompt; parse the emitted
   `s | p | o` lines.
7. Write generated triples back to the store with the existing
   `http://loka.dev/provenance/` RDF-star annotations (reuse
   `infer_with_citations`'s emitter helpers — no parallel impl).

## Embedding-index architecture (Emma, 2026-05-19) — THREE indexes

This is a change to *what the database indexes*, not just app code:

- **idx-node-id** — vector keyed to the node entity (existing Loka shape:
  `<node> <emb> "v"^^loka:f32vec`, HNSW on the object TermId).
- **idx-node-name** — vector of the node's proper *label/name text*, a
  second declared vector predicate (so name-similarity is first-class).
- **idx-triple** — vector of the **triple itself**, attached to the
  RDF-star quoted triple: `<< s p o >> <tripleEmb> "v"^^loka:f32vec`.
  Enables step 4 ("triples similar to the one it sits in") for real,
  not approximated from node similarity.

idx-node-id / idx-node-name are just two declared vector predicates —
existing mechanism. idx-triple is the open one: it needs
quoted-triple-as-vector-subject + `VECTOR_SIMILAR` + faithful
reverse-resolution of a hit back to `<< s p o >>` — the historically
fragile engine area (engine-bug-#2 / quoted-triple reverse map). May be
expressible with current primitives; may be a real engine change.

## Feasibility gates (validate BEFORE building — the new CLAUDE.md rule)

- **G1a — multiple declared vector predicates** (idx-node-id +
  idx-node-name): declare two `loka:f32vec` predicates, insert, confirm
  each answers `VECTOR_SIMILAR` independently.
- **G1b — triple-level vector (idx-triple): the decisive gate.** Store a
  vector on a quoted triple `<< s p o >>`, build HNSW, run
  `VECTOR_SIMILAR`, and confirm the hit resolves+renders back to the
  correct `<< s p o >>`. If this fails → it IS an engine change; scope
  and surface it before proceeding, do not fake triple-sim from nodes.
- **G2 — an embedding model is available locally** (sentence-transformers
  / Ollama / other). Leanest offline option; CPU fine (training is dead).

If any gate fails, surface it and adjust — do not build on an unproven
base, and do not silently substitute node-similarity for triple-similarity.

### VERDICT 2026-05-19 (recon + runtime smoke)

- **G2 ✓** — sentence-transformers 5.4.1 + Ollama `all-minilm` (offline, CPU).
- **G1a ✓** — code + runtime: `VECTOR_SIMILAR` on the live
  playground_server returned the 4 nearest shrines correctly. Multiple
  independent vector predicates supported (VectorRegistry keyed by
  predicate TermId; POST /vectors/declare + POST /vectors + parser/
  executor all real). **idx-node-id + idx-node-name = green, existing
  mechanism.**
- **G1b — bounded ENGINE CHANGE required.** Read path works (HNSW keys
  the vector-object TermId; executor binds `?subject` via
  find_by_predicate_object; quoted triples render faithfully via the
  reverse map). Gap = insert path only: `POST /vectors`
  (`loka-proto/src/server.rs:1360-1434`) interns the subject as a plain
  IRI string and never handles a `<< s p o >>` subject; INSERT DATA
  registers quoted triples but does not auto-index a vector on one.
  Fix (localized): in the vector-insert path, detect a quoted-triple
  subject → `register_quoted(s,p,o)` → intern vec object → store
  `<<s p o>> tripleEmb vecobj` → `vectors.insert(tripleEmb, vec,
  vecobj_id)`. Then rebuild the engine. This is a real engine
  modification (Emma anticipated it) — checkpoint before doing it.

## Build slices (commit per slice; queue.md mirrors)

1. **Data + embeddings.** Small real-Wikidata seed via
   `tools/wikidata_random_seed.py` → load into a fresh Loka
   (`loka import` or POST /triples). Embed each node's label, insert
   `<node> <emb-pred> "..."^^loka:f32vec`, declare the vector predicate so
   HNSW indexes it. Verify `VECTOR_SIMILAR` returns sane neighbours.
2. **Retrieval module** (`tools/graph_retrieval.py`): `node → ranked
   triple sequence` via SPARQL prefix scans (BFS) ∪ `VECTOR_SIMILAR`
   (embedding), with the combined least→most ranking + a budget cap.
3. **Generation:** base Qwen continuation over the sequence; parse +
   provenance-tag emitted triples (reuse the emitter helpers).
4. **Wire into the double-click path:** point `tools/infer_server.py` at
   base-Qwen + this retrieval (replacing the lobotomised adapter path);
   end-to-end test from the Studio graph; honest eval.

## Non-negotiables carried over

- No training. Base model only. (A format-only light adapter is a
  *maybe-much-later*, never knowledge transfer — see `_ft_probe3` result.)
- Reuse `infer_with_citations` emitter/provenance + `candidate_predicates`
  where applicable; do not fork.
- Provenance RDF-star is engine-storage detail; the model only ever sees/
  produces label text (the hashing fear was settled with corpus data).
