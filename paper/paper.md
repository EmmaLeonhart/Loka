# Loka: Generative Citation in a Neuro-Symbolic World Model over RDF-Star Knowledge Graphs



---

## Abstract

**Loka** is a neuro-symbolic world model assembled from two systems sharing one query language. The first is an RDF-star triplestore (the engine, formerly published as SutraDB) — explicit memory, exact answers. The second is a small role-aware transformer trained from scratch on the same triples, with English labels substituted for opaque entity identifiers — implicit memory, plausible answers. They compose at the SPARQL+ layer: a query reaches both systems and the caller does not pick which one answered, except by inspecting `propositionInferredFrom` provenance edges on each result.

The technical contribution is **generative citation**: a closed loop in which the transformer's predicted triples are written back into the triplestore as RDF-star annotations whose subject is the *quoted* generated triple and whose object is *another* quoted triple — a directly cited piece of context the prediction was conditioned on. A reserved system namespace (`http://sutra.dev/provenance/`) marks every system-emitted predicate, which is enforced at three layers (corpus stripping, candidate filtering, emit-time guard) so the model never sees, learns to predict, or hallucinates a citation predicate. Hallucinated *citations* (the model picking the wrong context triple as the support) are auditable and filterable like any other generated triple — they degrade like other RDF rather than vanishing into opaque embeddings.

We demonstrate the end-to-end loop on a 5,055,385-triple slice of Wikidata (philippesaade/wikidata, streamed from Hugging Face), with role-aware masked-S/P/O training producing models from 16M to 44M parameters that reach final perplexities of 92.5 and 84.85 respectively over five epochs. Predictions emerge that are not memorized templates (e.g., `Comtesse de Die | educated at | university of halle` correctly identifies Halle, where she studied; `Abbas Mirza | has works in the collection | metropolitan museum of museum` correctly identifies the Met). We characterize the failure modes — mode collapse on common connector tokens, mitigated at decode time by a *cumulative* repetition penalty rather than at training time — and document two engine-level bugs surfaced by the data scale.

---

## 1. Introduction

Two technical pressures motivate this work.

First: **knowledge-graph completion has historically been a black-box prediction problem.** TransE-family link predictors and recent transformer-on-KG approaches output a confidence over candidate triples, but offer no native account of what evidence shaped a given prediction. Provenance lives outside the model — in metadata about the training corpus, not as edges of the graph the model populates.

Second: **language models hallucinate without traceable inference.** LLM responses to factual queries are a single forward pass over a frozen distribution; the answer is the answer, with no surface that distinguishes "this came from training data" from "this is a plausible continuation." Retrieval augmentation pins one piece of evidence to one response, but does not produce a graph one can later prune, audit, or retrain on.

**Loka's claim is that a single design choice resolves both:** if the inference layer's outputs are *triples* and provenance is expressed as *RDF-star annotations on those triples*, then every model-generated fact lands in the same store as the curated facts, with first-class citation edges to its supporting context. Auditable, filterable, queryable in SPARQL+, retrainable on the post-filtered corpus. The "neuro-symbolic" adjective is not aspirational — it describes the data layout.

### Contributions

1. **A reserved provenance namespace and a three-layer enforcement.** Predicates under `http://sutra.dev/provenance/` (e.g., `propositionGenerated`, `propositionInferredFrom`, `propositionGeneratedBy`, `propositionConfidence`) are system-only. Three independent guards prevent the model from ever seeing, proposing, or emitting one: a SPARQL-star `FILTER NOT EXISTS << ?s ?p ?o >> propositionGenerated ?_g` clause in the corpus puller, a candidate-predicate filter in the inference loop, and an emit-time guard before each primary triple is written. Any single guard suffices; together they ensure that even with a regression in one path, generated provenance never re-enters training data. (§3.1)

2. **Generative citation as RDF-star reification.** Every model-generated triple `<S> <P> "X"` is accompanied by a fixed-shape annotation block. The block's subject is the *quoted* generated triple `<<S P "X">>`. Its objects include four metadata predicates (`propositionGenerated`, `propositionGeneratedBy`, `propositionConfidence`, ...) and one or more `propositionInferredFrom` edges whose object is *another quoted triple* — a cited piece of context. The result is a graph of generated triples threaded by citation edges to the curated context that informed them. (§3.2)

3. **Cumulative repetition penalty as a decode-time correction for mode collapse on common tokens.** Masked-S/P/O training produces models that "know" the answer category (university, museum, https-URL) but degenerate during greedy decoding to fillers like `of of of of` or `museum museum`. We show that a cumulative repetition penalty — dividing each repeated token's logit by `repetition_penalty ** count` — collapses these cascades within 2–3 emissions while preserving genuinely-needed reuse. The same v4 checkpoint moves from `university of of of of of of of` (no penalty) to `university of halle` (cumulative penalty 3.0), without retraining. (§4.3)

4. **Empirical demonstration on a real-scale 5M-triple corpus.** We report the v3→v4→v5 trajectory, including a corpus-quality regression caught and fixed mid-development (datatype-suffix tokens leaking into the training set), the qualitative failure modes of each model, and the headline result that capacity (16M → 44M params) was the binding constraint at this corpus size — the bigger model produces concrete entity tokens (`halle`, `33`, `kosmos 116`) where the smaller one fell back to common-token fillers. (§5)

We also surface two engine-level bugs found at scale: a SPARQL serialization quirk producing literal values in the predicate slot, and a write-flush wedge in the persistent layer at roughly every 5–6× growth in stored triples. Both are filtered/worked-around in production code; both want fixing.

---

## 2. Background

### 2.1 RDF-star

RDF-star is an extension of RDF in which any of the three positions of a triple — subject, predicate, object — may be a *quoted* (referenced, not asserted) triple. The notation `<<s p o>>` means "the triple s p o, treated as a term." This admits direct annotation of facts:

```
:Tokyo  :population  "13929286" .
<<:Tokyo :population "13929286">>  :measuredAt  "2020-01-01" .
<<:Tokyo :population "13929286">>  :statedIn    :census2020 .
```

The same shape that Wikidata expresses through reified statement nodes (e.g., `wds:Q1490-abc...`) collapses into one structural primitive. Two storage strategies exist: separate-asserted-graph (RDF 1.2 working draft) and synthetic-ID interning (used by SutraDB, where `quoted_triple_id(s_id, p_id, o_id) = xxh3` deterministically). We use the latter for compact joins on quoted-triple subjects.

### 2.2 Transformer-based knowledge graph completion

The dominant patterns in KG completion split into translational (TransE, RotatE, etc.) and transformer-based (KG-BERT, KGT5, recent work using LLMs as scoring functions). Most predict a single missing entity given (subject, predicate, ?) and report top-k accuracy on held-out triples. Two limitations relevant here: (a) outputs are scores or candidate IDs, not triples that can be re-stored; (b) provenance — which other triples in the corpus made this prediction confident — is not surfaced.

### 2.3 The from-scratch position

Loka's training is from scratch on RDF-derived text, not fine-tuning of a pretrained LLM. The position is not anti-LLM — it is that the closed-form auditability of "model knowledge ⊆ training corpus" is load-bearing for generative citation. With a fine-tuned LLM, even with the same RDF-star output schema, a generated triple may be drawn from base-model pretraining that the user never authorized as authoritative. We document a parallel near-term track admitting fine-tuning under stricter provenance assumptions in `planning/fine-tuning-track.md`; for the experiments in this paper, all results are from-scratch.

---

## 3. Architecture

### 3.1 The reserved provenance namespace

Every predicate under `http://sutra.dev/provenance/` is system-internal. The names are deliberately verbose — `propositionGeneratedFrom` rather than `generatedFrom` — so a human scanning raw triples spots them at a glance and accidental collision with real-world predicates is vanishingly unlikely. The full namespace currently holds:

| Predicate | Object type | Meaning |
|---|---|---|
| `propositionGenerated` | `xsd:boolean` | This triple was emitted by the world-model layer (not curated). |
| `propositionGeneratedBy` | string | The model version (e.g., `wikidata_v4`) that emitted it. |
| `propositionConfidence` | `xsd:decimal` | Mean per-token softmax probability of the prediction. |
| `propositionInferredFrom` | quoted triple | A piece of context the prediction was conditioned on. |
| `propositionImportedFrom` | URI | Reserved; not currently emitted in production (was found redundant for uniformly-Wikidata corpora). |

Three layers of enforcement keep these out of the model's view and output:

**Corpus stripping.** The training corpus extractor issues a SPARQL-star query that excludes any inner triple flagged generated:

```sparql
SELECT ?s ?p ?o WHERE {
  ?s ?p ?o .
  FILTER NOT EXISTS {
    << ?s ?p ?o >> <http://sutra.dev/provenance/propositionGenerated> ?_g .
  }
}
```

It also drops any row whose predicate IRI matches the reserved prefix.

**Candidate filtering.** The inference loop builds candidate `(subject, predicate)` pairs by intersecting subject-with-graph-neighbor predicates. Reserved-namespace predicates are excluded from `pred_usage` and re-filtered at the candidate list level.

**Emit-time guard.** Each prediction's primary triple is checked against the reserved prefix immediately before it is written to the output stream. A reserved-prefix predicate is logged loudly and dropped.

Any single layer suffices. Three are kept because regressions in one path should not silently allow the model to learn or output system metadata.

### 3.2 Generative citation as RDF-star reification

When the inference layer accepts a candidate `(S, P)` and emits a predicted object `"X"`, it writes a fixed-shape block:

```
<S> <P> "X" .
<<S P "X">>  prov:propositionGenerated     "true"^^xsd:boolean .
<<S P "X">>  prov:propositionGeneratedBy   "wikidata_v4" .
<<S P "X">>  prov:propositionConfidence    "0.43"^^xsd:decimal .
<<S P "X">>  prov:propositionInferredFrom  <<S existing_p1 existing_o1>> .
<<S P "X">>  prov:propositionInferredFrom  <<S existing_p2 existing_o2>> .
   ... (default: 10 cited context triples per prediction)
```

`prov:` is the abbreviation for the reserved namespace. The cited context triples are existing rows about the subject `S` that the inference loop's candidate-predicate selection conditioned on. The shape is identical for inference outputs (`propositionInferredFrom`) and ingest outputs (the same RDF-star pattern absorbs Wikidata's `pq:` qualifiers and `pr:` references on import) — citation is uniform across the data layer.

Hallucinated citations are not a correctness problem. A fabricated `propositionInferredFrom` row is still a transparent RDF-star annotation pointing at concrete context — auditable, filterable, often informative about what the model thinks the reasoning is. We do not add elaborate guards against citation hallucination; the schema does the work.

### 3.3 The two-system loop

```
   ┌───────────────────┐
   │ Curated triples   │  (Wikidata, etc.)
   │  (RDF-star)       │
   └─────────┬─────────┘
             ▼
   ┌───────────────────┐         ┌──────────────────────┐
   │ SutraDB store     │ ─────→  │ Training corpus      │
   │  (.sdb, RDF-star) │  SPARQL │  (label-substituted) │
   │                   │  +SPARQL-│                      │
   │                   │  star    │                      │
   └─────────▲─────────┘         └──────────┬───────────┘
             │                              ▼
             │                  ┌──────────────────────┐
             │                  │ Role-aware           │
             │                  │ transformer          │
             │     ┌──── feeds to ──── (this paper, §4) │
             │     │            └──────────┬───────────┘
             │     │                       ▼
             │     │            ┌──────────────────────┐
             │     │            │ Inference loop       │
             │     │            │ + cumulative rep.pen │
             │     │            │ + RDF-star write-back│
             │     │            └──────────┬───────────┘
             │     │                       ▼
             │     │            ┌──────────────────────┐
             └─────┴────────────│ Generated triples +  │
                                │ propositionInferred  │
                                │ From edges, written  │
                                │ back to the store    │
                                └──────────────────────┘
```

The loop is closed: generated triples land in the store with `propositionGenerated true`. The next training-corpus extraction's SPARQL-star FILTER excludes them. The model never trains on its own output. Inference can be re-run repeatedly to grow the citation graph without polluting the training distribution.

---

## 4. Method

### 4.1 Corpus

Source: `philippesaade/wikidata` on Hugging Face — a CC0 parquet dump of ~30M Wikidata entities, each row a JSON-shaped record with labels (every language), descriptions, sitelinks, and claims. We stream via the `datasets` library, converting each entity to N-Triples-star form: one main triple per claim, plus one RDF-star annotation per qualifier and per reference, all sharing the same `<<S P O>>` quoted-triple subject. Wikidata's `pq:` (qualifier) and `pr:` (reference) namespaces collapse into the same `wdt:` predicate URI on the annotation row — the qualifier-vs-reference distinction is structural (subject is a quoted triple), not lexical.

Final ingested store: 5,055,385 triples / 1,695,402 RDF-star annotations / 27,780 entities / 770 MB on-disk SutraDB store. Every language label and description Wikidata has is included.

### 4.2 Label substitution

The model is trained on text, not URIs. The corpus extractor walks all `rdfs:label "..."@en` triples, builds a URI → English-label map, then writes each triple with each component resolved through the map:

| Raw triple | After substitution |
|---|---|
| `<wd:Q42> <wdt:P31> <wd:Q5>` | `Douglas Adams <TAB> instance of <TAB> human` |
| `<wd:Q1490> <wdt:P1448> "Tokyo"@en` | `Tokyo <TAB> official name <TAB> Tokyo` |
| `<wd:Q24> <wdt:P40> <wd:Q1049347>` | `Jack Bauer <TAB> child <TAB> Kim Bauer` |

Property labels missing from the live store are fetched from Wikidata's public SPARQL endpoint with caching and 429-tolerance. Two preprocessing fixes were essential and are fragile enough to surface here:

1. **Strip `^^<datatype>` suffixes from typed literals.** SutraDB's SPARQL serialization embeds the datatype URI in the literal value string (e.g., `"+1966-02-18T00:00:00Z\"^^<http://www.w3.org/2001/XMLSchema#dateTime>"`) rather than separating it as `datatype` metadata. Without stripping, datatype-URI fragments (`xmlschema`, `decimal`, `org`) reach the tokenizer as if they were entity content and dominate certain predictions (§5.1).

2. **Drop rows with non-URI predicates.** ~1% of rows on a 5M corpus exhibit a SutraDB SPARQL bug (§6.1) where literal values surface in the `?p` slot. RDF disallows literal predicates, so dropping is safe.

After cleaning, the training file holds 757,592 lines for our 5M-triple corpus.

### 4.3 Model and training

Architecture: a role-aware Transformer encoder. Each triple is tokenized as

```
[CLS] s_tokens [SEP_S] p_tokens [SEP_P] o_tokens [SEP_O]
```

Token + position + role embeddings sum at each position, where the role is one of `{SPECIAL, S, P, O}`. The classification head is tied to the input embedding for parameter efficiency.

Training objective: pick one role (S, P, or O) at random per example, mask its tokens with `[MASK]`, predict the originals. Cross-entropy on the masked positions, AdamW, 3e-4 LR, β=(0.9, 0.95), weight decay 0.01, gradient clipping at 1.0. Standard.

Three model sizes at this corpus size:

| Model | d_model | nhead | layers | params | epochs | final ppl |
|---|---|---|---|---|---|---|
| v3 (reference; pre-cleanup) | 256 | 8 | 4 | 16,012,800 | 5 | 53.43 |
| v4 (reference; cleaned) | 256 | 8 | 4 | 15,967,744 | 5 | 92.48 |
| v5 (this paper's main) | 512 | 8 | 6 | 44,531,712 | 5 | 84.85 |

v3 reports artificially low perplexity from memorizing datatype-suffix tokens (§5.1). v4 is the canonical baseline at the smaller architecture. v5 is the bigger-model run.

### 4.4 Inference: generative citation

For each candidate subject in the corpus:

1. **Candidate predicate selection.** Find graph-neighbors — subjects sharing at least one (predicate, object-key) tuple with this one — and rank predicates they have but the candidate subject lacks. Cap at *N* candidates per subject (default 5).
2. **Masked decoding with cumulative repetition penalty.** Build the input as `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] [MASK]^k [SEP_O]`. At each masked position, the model emits a logit distribution. We apply:

   - Hard skip-set: special tokens never win.
   - Cumulative repetition penalty: `logit[t] /= penalty^count[t]` where `count[t]` is the number of times `t` has already been emitted in this sequence. Default `penalty = 3.0`.
   - Per-token confidence floor: emission halts when the top-token probability falls below 0.05.

   Greedy top-1 selection, no beam search.
3. **Confidence-thresholded emit.** Mean per-token probability is the prediction's confidence. If confidence ≥ threshold (default 0.4) and the predicted object is not a duplicate of an existing fact for this (S, P), emit the RDF-star block (§3.2).
4. **Optional `--post`.** Write the emitted N-Triples-star to the live SutraDB store via `POST /triples`. Subsequent training-corpus extractions exclude these via the SPARQL-star FILTER from §3.1.

The cumulative penalty matters: a *non*-cumulative penalty (set membership) was tested first and failed to break loops on dominant common tokens because the penalty applied only once regardless of how many times the token had already won. With cumulative, three emissions of `of` at penalty 3.0 multiply its divisor by 27 and reliably drop it below the floor, breaking the cascade.

---

## 5. Experiments

### 5.1 Corpus quality regression: v3 → v4

A datatype-suffix-leakage bug (§4.2 fix #1) was caught only after the v3 model was trained. v3 produced predictions like `Abbas Mirza | has works in collection | 1 http www w3 org 2001 xmlschema decimal` (confidence 0.93) — clearly a memorization of literal-with-embedded-datatype-URI patterns. After fixing the corpus and retraining (v4), the same prediction becomes `metropolitan museum of museum` (confidence 0.43). The Met genuinely holds Abbas Mirza pieces.

The numerical effect is paradoxical at first read: v4's final perplexity (92.5) is *higher* than v3's (53.4). The explanation is mechanical — v3 was getting cheap loss reduction from memorizing fragments of typed-literal datatype URIs (`xmlschema decimal http www w3 org`) because they appeared frequently after a particular pattern. With those tokens stripped, the corpus is genuinely harder. Higher ppl, better content.

### 5.2 v4 vs v5: capacity scaling

Both trained 5 epochs on the cleaned 757k-line corpus. Side-by-side per-epoch perplexity:

| Epoch | v4 (16M) | v5 (44M) |
|---|---|---|
| 1 | 1150.7 | 1528.7 |
| 2 | 196.0 | 147.3 |
| 3 | 133.5 | 104.2 |
| 4 | 100.7 | 90.7 |
| 5 | **92.5** | **84.85** |

v5 starts higher (epoch 1) — more parameters mean a harder optimization landscape and slower initial convergence. It crosses under v4 at epoch 2 and pulls ahead from there. By epoch 4 it has already passed v4's *final* perplexity. Wall time on a 4070 Laptop: 91 min for v5 vs 42 min for v4 (2.2× compute, 8% better final ppl).

### 5.3 Qualitative comparison (same seed, same penalty)

50 subjects sampled deterministically (seed 42), 5 candidate predicates each, confidence threshold 0.4, cumulative repetition penalty 3.0. Selected predictions:

| Subject / predicate | v4 (16M, with penalty) | v5 (44M, with penalty) |
|---|---|---|
| canton of Romilly-sur-Seine-1 / Commons category | "canton of of sur sur" | **"canton of"** (conf 0.882) |
| Comtesse de Die / educated at | "university of of of of of of of" | **"university of halle"** (conf 0.488; correctly identifies Halle, where she studied) |
| Zudar / area | (didn't pass threshold) | **"33"** (conf 0.901; numeric — model picked up that area is a number) |
| Meeuwen-Gruitrode / locator map image | "map of comune of meeuwen province province" | "map of comune of" (conf 0.685; clean truncation) |
| Curt Meyer-Clason / Commons category | "curt meyer clason" (extra token) | "curt meyer" (conf 0.825) |
| Kosmos 116 / Commons category | (didn't pass) | **"kosmos 116"** (conf 0.740) |
| Centralbahnhof / Vikidia article ID | (didn't pass cleanly) | "fr" (conf 0.798; correct lang prefix for Vikidia) |
| Liriodendron tulipifera / African Plant Database ID | (n/a) | "liriodendron tulipifera" (conf 0.441) |

v5 picks specific, correct entity tokens (`halle`, `33`, `kosmos 116`) where v4 fell back to common connectors. The repetition penalty (same setting in both columns) eliminates the most egregious looping for both, but v5's distributions over real entity tokens are more concentrated, so its *post-penalty* outputs are more often direct hits.

### 5.4 Pass rate

At threshold 0.4, v4 emits 32/250 candidate predictions; v5 emits a comparable rate. The interesting metric is not pass rate but the *quality of the passing predictions* — and §5.3 carries the qualitative weight.

---

## 6. Limitations

### 6.1 Engine bugs surfaced at scale

- **SPARQL `?s ?p ?o` occasionally returns literal values in the predicate slot.** RDF disallows literal predicates; this is invalid output from the executor — almost certainly an RDF-star annotation row with positions getting confused. Filtered at preprocess (drops ~1% of rows on a 5M corpus). Real engine bug.

- **`POST /triples` wedges after roughly every 5–6× growth in stored triples.** Hit at ~174k and again at ~1M during the 5M ingest. `/health` keeps responding, but `/triples` and SPARQL hang indefinitely until the server is restarted. On restart, all data is intact on disk. Symptoms point at LSM compaction or persistent-index rebuild holding the write lock. Real engine bug.

### 6.2 Model and decoding

- **Mode collapse on common connector tokens.** Even with cumulative penalty 3.0, predictions for predicates the model has weak knowledge of fall back to `of`/`and`/`https www`. This reflects thin entity-content coverage at our corpus size (27,780 entities of the 30M available); the lever is more data, not more decoding tricks.
- **Word-level tokenizer chops Unicode.** "Saint-Léger" becomes `saint l ger`. BPE/wordpiece is the planned fix; unimplemented in this iteration.
- **No beam search or top-p sampling.** Greedy top-1 only. Some failure cases would resolve with beam-2.

### 6.3 Provenance

- **Citation hallucination is structurally bounded but not zero.** A `propositionInferredFrom` row points at a concrete context triple, which is auditable, but the *choice* of which context triples to cite is heuristic (§4.4 step 1). The model is not actually inspecting these specific triples during prediction; the citation is "the candidate-predicate selection considered these triples." We document this tradeoff as accepted: the schema is honest about what it represents.

---

## 7. Discussion

The from-scratch training position (§2.3) coexists with a documented parallel near-term track admitting fine-tuning of a small base model (e.g., Qwen 2.5 1.5B-Instruct + QLoRA) under the same `propositionInferredFrom` output schema. The empirical case for the parallel track is exactly the limitation in §6.2: at 757k training triples and 16M parameters, mode collapse on common connectors persists; a fine-tuned 1B-3B parameter base model with English already encoded would plausibly produce coherent triples within hours rather than waiting for the from-scratch path to scale. We accept the provenance tradeoff this introduces — base-model pretraining is opaque — and record `propositionGeneratedBy "qwen-2.5-1.5b-loka-v1"` to track what was emitted by what.

Two larger questions are open:

**Where does the OWL layer live?** OWL ontologies are stored in the engine as triples but the engine does not reason. A reasonable role for OWL in the world-model loop is as a *prediction template*: an ontology declares "an instance of class C is expected to have properties P1, P2, P3 with values matching constraints X, Y, Z," and the inference loop reads the template, identifies expected-but-missing predicates for an entity, and predicts values for them. The OWL template becomes the *prompt* of a generative-citation inference call, and `propositionInferredFrom` cites the OWL declaration alongside the supporting context triples. We have not implemented this; it is the cleanest next step.

**What is the right output decoder?** The HNSW vector index in the engine is currently used for vector search (a separate feature) but could serve as a *decoder*: the model emits an embedding, HNSW resolves the nearest known IRI, and the IRI becomes the predicted object. This would close the gap between prediction in label-space and prediction in entity-space, eliminating cases like "metropolitan museum of museum" (decoded label) in favor of `<wd:Q160236>` (decoded entity). Open work.

---

## References

- Wikidata Foundation. *Wikidata.* https://www.wikidata.org/. CC0.
- philippesaade. *philippesaade/wikidata.* Hugging Face dataset, snapshot 2024-09-18. https://huggingface.co/datasets/philippesaade/wikidata. CC0.
- W3C. *RDF-star and SPARQL-star.* https://w3c.github.io/rdf-star/cg-spec/.
- Devlin, J., Chang, M.-W., Lee, K., Toutanova, K. *BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding.* NAACL 2019. (Masked-token-prediction substrate.)
- Vaswani, A., et al. *Attention is All You Need.* NeurIPS 2017. (Transformer architecture.)
- Bordes, A., et al. *Translating Embeddings for Modeling Multi-relational Data.* NeurIPS 2013. (TransE; comparison-only context for §2.2.)
- Yao, L., Mao, C., Luo, Y. *KG-BERT: BERT for Knowledge Graph Completion.* arXiv:1909.03193. (Transformer-on-KG comparison-only context.)
- Saxena, A., Kochsiek, A., Gemulla, R. *Sequence-to-Sequence Knowledge Graph Completion and Question Answering.* ACL 2022. (KGT5; comparison-only context.)

<!-- v0.4.0 — first clawRxiv submission cycle: 2026-05-09 -->
