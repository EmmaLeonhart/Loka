# SutraDB World-Model Thesis

**Status:** Canonical vision spec — supersedes ad-hoc framings in chats.
**Date:** 2026-05-07
**Source material:** `chats/world-models.md`, `chats/ai-bubble.md`.

This document captures the architectural commitments and rejections that define what SutraDB is becoming. It is the contract between the engine work and the model work, and the lens through which feature proposals should be evaluated.

---

## 1. Premise

**SutraDB is one half of a two-system composition. The other half is a transformer trained from scratch on RDF triples. Both expose the same SPARQL+ interface.**

- SutraDB = explicit memory. Stores what is known. Returns exact answers.
- The world-model transformer = implicit memory. Predicts what is plausible. Returns inferred answers with cited inference chains.
- Same query language. The caller (human or agent) doesn't know which system answered, except via the provenance edges on the result.

The database grows itself: inferred triples are written back into SutraDB with their inference chain as first-class provenance, available for future queries and for re-training.

RDF is the universal representation. Not because of Semantic Web aesthetics, but because **open-world semantics is load-bearing for masked prediction over knowledge graphs.** A closed-world relational schema would force the model to assume completeness; RDF lets it stay agnostic about gaps.

---

## 2. The architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  data sources       ┌─────────────────────────┐                 │
│  ──────────►  ETL ►─┤   SutraDB (.sdb)        ├─►  SPARQL+ Q    │
│  (Wikidata,         │                         │                 │
│   DBpedia,          │   RDF-star storage      │                 │
│   enrolled local)   │   SPO/POS/OSP/HNSW      │                 │
│                     └─────────┬───────────────┘                 │
│                               │                                 │
│                               │  triples (training corpus)      │
│                               ▼                                 │
│                     ┌─────────────────────────┐                 │
│                     │   World-model           │                 │
│                     │   transformer           │                 │
│                     │   (trained from scratch)│                 │
│                     └─────────┬───────────────┘                 │
│                               │                                 │
│                               │  inferred triples + cited       │
│                               │  inference chains               │
│                               ▼                                 │
│                     ┌─────────────────────────┐                 │
│                     │   SutraDB (write back)  │                 │
│                     └─────────────────────────┘                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

The arrows compose into a loop. Inferred triples become training data on the next pass.

---

## 3. Training corpus

Primary sources, in priority order:

1. **Wikidata** — RDF dumps. Largest single source. Multilingual, broad-domain.
2. **DBpedia** — Wikipedia-derived RDF. Complementary coverage to Wikidata.
3. **Wikifunctions** — function definitions and their behavior.
4. **OpenAlex** — academic publications and citation graph.
5. **MusicBrainz / GeoNames / etc.** — domain-specific RDF dumps as needed.

**Secondary (deferred):** enrolled local sources via `planning/enrollment-v1.md`. Wikidata + DBpedia alone are enough to start; enrollment is for ingesting non-RDF sources after the model is producing useful inferences.

**Key claim: a small model is sufficient.** RDF data is structured and high-quality. The model doesn't need to learn syntax, ambiguity, or conversational coherence. It needs to learn entity relationships and structural patterns. Quality > scale for this regime. Concrete size estimate is deferred until first scale tests, but the working assumption is that the model is decisively smaller than a general-purpose LLM at the same task.

### 3.1 Working assumptions for early training runs

These are starting positions for the first experiments, **not architectural commitments**. They will evolve as we learn what actually works.

**Wikidata first, but not Wikidata only.** Wikidata is the obvious starting corpus: the dump is large, the structure is consistent, and the entities have stable identifiers. But Wikidata-only training is not a commitment — multiple datasets (DBpedia, Wikifunctions, OpenAlex, domain-specific dumps) will likely be needed at scale. Do not lock the pipeline to a single source.

**Use human-readable labels, not opaque IDs.** Wikidata's QID/PID architecture (`Q42`, `P31`) is not human-readable. The training-corpus preprocessing substitutes labels — names of items and properties — for the IDs. The model sees `Douglas Adams instance-of human`, not `Q42 P31 Q5`. The user's framing: *"human readability is something that's very important because it is AI readability too."* The model's ability to do implicit lexical inference depends on seeing names with semantic content, not opaque identifiers. The same applies to predicates: `P31` becomes `instance of`, `P279` becomes `subclass of`.

**Multilingual training is the plan; English is just first.** English is the practical starting point — labels are usually present, engineering is simpler — but the explicit intention is to train in many languages. The label-substitution mechanism generalizes to any language Wikidata covers.

**Trade-offs accepted:** label collisions (two distinct entities named "John Smith"), label drift over time, translation drift across languages. These are accepted because the alternative — opaque identifiers — defeats the purpose of training a model that can reason about content.

**What this approach is NOT trying to do:** establish name-equivalences. The model is not being trained to know that "Burma" and "Myanmar" refer to the same place, or that "Cassius Clay" became "Muhammad Ali." Name-equivalence resolution is a separate problem (handled by `owl:sameAs` triples that already exist in Wikidata, plus the deferred entity-resolution pipeline). This preprocessing is about giving the model semantic content to reason over, not synonymy lookups.

**Entity resolution is partially deferred.** Within Wikidata alone, every entity has a stable QID for deduplication — no entity resolution needed at preprocessing time. As soon as multi-source training begins (DBpedia, OpenAlex), entity resolution becomes load-bearing because the same person/place/thing appears under different IDs across sources. A real entity-resolution pipeline is needed at that point, not later. Full enrollment-style entity resolution against arbitrary non-RDF sources is part of the year-deferred enrollment work — see `planning/enrollment-v1.md`.

---

## 4. How the world-model layer composes with SutraDB internals

| SutraDB component | Role in world-model layer |
|---|---|
| RDF-star storage | Native reification — every inferred triple carries provenance via `<<s p o>> sutra:supports :evidence`. |
| SPO/POS/OSP indexes | Training-batch streaming and query-time graph context retrieval. |
| HNSW vector index | **Output decoder.** Model emits an embedding; HNSW NN resolves to a URI. Vectors are no longer just a search feature — they are the bridge from latent space back to symbolic space. |
| Struct literals (`sutra:num`, `sutra:date`, `sutra:coord`, `sutra:f32vec`) | Value-space substrate. Numbers, dates, coordinates don't get learned representations — their identity comes from canonical form. See `planning/structural-typing.md`. |
| Grounding-level metric (BASE / L1..Ln / ORPHAN) | **Confidence gradient.** Layer 1 = raw imports. L2 = first-order inference. L3 = inference-on-inference. Queryable per-triple. |
| `sutra:sourceExempt` annotation | Stops infinite provenance recursion on inference-chain edges. |
| Stance vocabulary (`supports` / `contradicts` / `isAbout` / `isUncertain` / `retracts`) | The connectives between an inferred triple and its evidence. |

---

## 5. Architectural commitments (load-bearing)

1. **RDF triples in, RDF triples out.** No flat token streams, no embeddings as output, no Turtle serialization as canonical. Output is structured triples.
2. **Train from scratch on RDF.** No fine-tuning of general-purpose language models. Pretrained text models are not the substrate.
3. **Role-aware S/P/O prediction.** Subject, predicate, object are predicted as structured slots, not as positions in a flat token sequence.
4. **Open-world.** The model assumes nothing about completeness. Masked prediction is the natural training objective for an open-world graph.
5. **Self-citing inference.** Every inferred triple carries its inference chain as first-class provenance. "Pramana's original vision but actually working" — Claude's framing in the source chat, accepted.
6. **Same SPARQL+ for both systems.** The model is queried the same way as the database. Federation is implicit, not a separate API.
7. **HNSW is the decoder.** Model output → vector → HNSW NN → URI. This makes the existing `sutra-hnsw` crate load-bearing for inference, not just search.
8. **Common concepts don't need IRIs.** Only proper nouns mint IRIs. Common concepts (`green`, `tree`, `alive`) get consistent embeddings via the model + HNSW. The schema policy follows from this.
9. **Small model is the default ambition.** Scale up only if the structured-data hypothesis fails empirically.
10. **Train on human-readable labels, not opaque IDs.** Whatever the source, the preprocessing pipeline replaces opaque identifiers with their human-readable labels (Wikidata QIDs/PIDs → label strings, etc.). Languages and per-source mechanics are working choices, not committed (see §3.1); the principle that the model sees names not IDs is committed. Human-readable = AI-readable.

## 6. Architectural rejections (negative space)

1. **JEPA / latent-as-output.** Embedding space is infrastructure, not the destination.
2. **TransE / KGE / `head + relation ≈ tail` as architecture.** Might emerge as a probe finding, fine. Not baked in.
3. **Naive text-tokenization of triples.** Triples are not flat sentences.
4. **OWL as engine reasoner.** Engine has no DL semantics. See §7.
5. **Closed-world.** The model is not trying to be a database of the entire world.
6. **Fine-tuning a general model.** Costs more, hallucinates more, learns the wrong priors.
7. **GraphQL / SQL / MongoQL interfaces.** Already in `CLAUDE.md`. Reaffirmed.

## 7. OWL as expected-triple templates

OWL has been the persistent open question. The user's framing settles it:

> *"OWL is more of a thing that would give expected triples for something and then the world model will fill them out."*

OWL is **not** a reasoner in SutraDB. OWL is a **prediction template**. An OWL class declaration says "an instance of class C is expected to have properties P1, P2, P3 with values matching constraints X, Y, Z." When the database has a partial entity (e.g., a Person with no birthplace), the world-model layer reads the OWL template, identifies the expected-but-missing predicates, and predicts values for them — citing the OWL template as the prompt for the inference.

This makes OWL a first-class part of the inference pipeline without committing the engine to OWL DL semantics.

| OWL feature | SutraDB engine treatment | World-model treatment |
|---|---|---|
| Class declarations | Stored as triples | Used as expected-triple templates |
| Property ranges/domains | Stored as triples | Used as constraints on predicted values |
| Cardinality restrictions | Stored as triples | Used as targets for "fill in N missing values" |
| Disjointness | Stored as triples | Used as contradictory-prediction filter |
| `subClassOf` chains | Stored as triples | Used to propagate expected-triples up the hierarchy |

The engine never reasons about OWL. SDKs validate against OWL client-side (already the SutraDB stance per `CLAUDE.md`). The world model uses OWL as input prompts.

For value-space classes (`sutra:num`, `sutra:date`, etc.), structural typing replaces OWL entirely. See `planning/structural-typing.md`.

## 8. Evaluation criteria — real vs. bubble

From `chats/ai-bubble.md`:

- **Demos must visibly do the loop.** A SutraDB demo without the inference-write-back cycle is bubble-coded. The cautionary tale: JEPA was research-framed but a vibe-coded weekend project matched its results.
- **Embeddings are infrastructure, not output.** Anything we ship that has "the output is an embedding" disqualifies us.
- **No reputation collateral.** Don't claim the system does X because someone famous said RDF/world-models are good.
- **Working > impressive.** A small slow loop that visibly produces cited inferred triples beats a large model with opaque outputs.

## 9. Open questions

- **Output tokenization for triples.** Empirical. Try several. Explicitly not committed.
- **Model size and training compute.** Depends on corpus size and tokenization. Estimate after the first scale tests.
- **High-degree node handling.** Example: a Wikidata node with 60k+ neighbors (e.g., "shrines worshipping Amaterasu"). Context-window assembly needs adjusted BFS, weighted by inference relevance. Hand-wave for now; concrete strategy needed before training at scale.
- **Multimodal extensions.** RDF→text, text→RDF, RDF→video, video→RDF. Pipelines that route through the symbolic layer for interpretability. Out of scope for v1; design later.
- **OWL-template-driven inference UI.** When the user asks "what should be true about this entity?" how does that surface in `sutra mcp` / Sutra Studio? Design later.

## 10. Implementation order (rough)

Subject to revision based on what's learned at each step:

1. **Symbolic layer foundations** — struct literals, source-exempt annotation, stance vocabulary, grounding metric. See `planning/symbolic-layer-and-naming.md`.
2. **Wikidata + DBpedia ingestion at scale** — already partially in flight; `tools/wikidata_bfs_import.py` is the starting point.
3. **First training run** — small model, single-GPU, bootstrap on a subset of Wikidata. Goal: produce one cited inferred triple end-to-end.
4. **Inference write-back** — close the loop. Inferred triples land in SutraDB with provenance.
5. **Scale up** — corpus size, model size, evaluation harness.
6. **Enrollment v1** — non-RDF source ingestion. See `planning/enrollment-v1.md`.

## 11. References

- `chats/world-models.md` — full conversation source for §1–§9
- `chats/ai-bubble.md` — positioning thesis for §8
- `planning/symbolic-layer-and-naming.md` — the four-ideas-from-Pramana plan, fifth idea added
- `planning/structural-typing.md` — combinatoric namespaces and value-space classes
- `planning/enrollment-v1.md` — non-RDF source ingestion (deferred)
- `docs/architecture.md` — current SutraDB architecture
- `CLAUDE.md` — workflow rules and core philosophy
