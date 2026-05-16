# Loka: Generative Citation in a Neuro-Symbolic World Model over RDF-Star Knowledge Graphs

**Code:** <https://github.com/EmmaLeonhart/Loka> (engine release `v0.4.0`: <https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.0>) &middot; **Model checkpoints:** <https://huggingface.co/datasets/EmmaLeonhart/loka> (snapshot tags `v3`–`v14`, plus per-epoch `v12.*` `v13.*` `v14.*`) &middot; **Normalized-Wikidata training corpora (v11+):** <https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata> (snapshot tags `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`) &middot; **Source dataset:** <https://huggingface.co/datasets/philippesaade/wikidata>

---

## Abstract

**Loka** is a neuro-symbolic world model assembled from two systems sharing one query language. The first is an RDF-star triplestore (the engine, formerly published as Loka) — explicit memory, exact answers. The second is a small role-aware transformer trained from scratch on the same triples, with English labels substituted for opaque entity identifiers — implicit memory, plausible answers. They compose at the SPARQL+ layer: a query reaches both systems and the caller does not pick which one answered, except by inspecting `propositionInferredFrom` provenance edges on each result.

The technical contribution is **generative citation**: a closed loop in which the transformer's predicted triples are written back into the triplestore as RDF-star annotations whose subject is the *quoted* generated triple and whose object is *another* quoted triple — a directly cited piece of context the prediction was conditioned on. A reserved system namespace (`http://loka.dev/provenance/`) marks every system-emitted predicate, which is enforced at three layers (corpus stripping, candidate filtering, emit-time guard) so the model never sees, learns to predict, or hallucinates a citation predicate. Hallucinated *citations* (the model picking the wrong context triple as the support) are auditable and filterable like any other generated triple — they degrade like other RDF rather than vanishing into opaque embeddings.

We demonstrate the end-to-end loop across a **twelve-version model progression (v3 → v14)**, each version shipped as a tagged Hugging Face snapshot with its training corpus. The progression has four phases: (i) word-level-tokenizer models v3–v5 on a 5 M-triple Wikidata slice, where capacity was the binding constraint (16 M → 44 M params, final perplexities 92.5 → 84.85 over five epochs) and predictions already emerged that were not memorized templates (`Comtesse de Die | educated at | university of halle`; `Abbas Mirza | has works in the collection | metropolitan museum of museum`); (ii) BPE-tokenizer models v6–v8 after a corpus-quality finding (below) took final perplexity to 64.65; (iii) cron-automated v9–v10 reaching **55.52** on a small clean slice; and (iv) the **normalized-wikidata scaling series v11–v14**, the current frontier, which holds architecture/tokenizer/config fixed and scales only the cleaned corpus. The headline result is now a *corpus-scale* result, not the earlier capacity result: across v11–v14 the cleaned corpus grows 350 k → 4.0 M triples and best-perplexity falls 279.12 → **202.01** (v14, final) — evidence the binding constraint on this from-scratch line is data scale, not architecture. v14 is finalized at its epoch-4 checkpoint (ppl 202.01, the series best); a bounded continuation experiment (epochs 6–7) confirmed ~202 is the practical floor for a from-scratch fit on this corpus and hardware, so deeper results are explicitly delegated to a clean-optimizer 10-epoch contributor run rather than chased with diminishing returns. Throughout, we characterize the failure modes — mode collapse on common connector tokens, mitigated at decode time by a *cumulative* repetition penalty rather than at training time — and document engine-level bugs surfaced by the data scale.

We then trace a substantial corpus-quality finding from a post-training behavioural test (§5.5): the v6 model produced confident catalog-format hallucinations (`ISNI -> 00000000`, `Freebase -> /m/0c__9`) on identifier predicates, and worse, the catalog-format shape *leaked onto unrelated predicates* (`instance of -> + Ġof - 00 - 03 T 00`, a Wikidata date-prefix string, on 15 different subjects in a single 30-source run). Investigation showed that 49.6 % of the v6 corpus was Wikidata `external-id` predicates (Freebase, ISNI, GND, LCCN, Dewey, etc.) — there are 10,206 such properties on Wikidata, ~80 % of all property *types*. We rebuild the corpus with these removed, drop a further 319 properties of other catalog-shaped datatypes (`url`, `commonsMedia`, lexeme/sense/form, math, geo-shape), and normalise time and quantity literals (strip `+` era prefix, drop `T00:00:00Z` on date-only values). The resulting v7 corpus is 184,458 triples (24 % of v6 by volume) and trains to a comparable perplexity (192.63 vs v6's 194.98) but with the catalog-format hallucinations *vanished* — the model's failure mode shifts from "confidently wrong" to "refuses to emit", which is what we want from a generative-citation system. We then trained v8 — the same architecture, 20 epochs from random initialisation on the v7 corpus — reaching perplexity **64.65**, a 3× reduction, with the loss curve still descending at epoch 20. The cleaned v7 corpus carries substantially more exploitable signal than 5 epochs can extract. v9 and v10 push further: each is trained on a freshly-pulled 2 M-triple Wikidata slice (under the per-data-dir state file design described in §5.7), reaching perplexity **57.15** and **55.52** respectively on roughly half the corpus size of v7 — a 3.5× total improvement over v6. On the standard Q42 propgen test, v10 emits 60 candidate triples, **zero of which are catalog-format hallucinations**, against v6's 21/52. v10 is also the first model shipped end-to-end by an automated 12-hour cron loop with no manual intervention (§5.8). v11–v14 then pivot the *pipeline shape* — the training corpus is built directly from `philippesaade/wikidata` without staging into a Loka store, both because the SPARQL `LIMIT/OFFSET` extraction path is O(offset) on sled at multi-million-triple scale and because mid-pipeline auditing caught a property-label corruption issue specific to RDF-star materialisation. The cleaned corpus is published as a standalone Hugging Face dataset (`EmmaLeonhart/normalized-wikidata`, currently four snapshot tiers from 50 k to 1 M entity rows). v11 (§5.9) through v14 (§5.12) are the four trained-model rungs of this series. Holding architecture, tokenizer, and training config fixed and scaling only the cleaned corpus from 350 k to 4.0 M triples drove best-perplexity from 279.12 (v11) to **202.01 (v14)** — a 28 % reduction, with v14 still descending where v13 had plateaued. This is the paper's strongest evidence that the binding constraint on this from-scratch model line is corpus scale, not architecture or training duration; v11–v13's higher perplexities were data-starved, not architecture-limited.

---

## 1. Introduction

Two technical pressures motivate this work.

First: **knowledge-graph completion has historically been a black-box prediction problem.** TransE-family link predictors and recent transformer-on-KG approaches output a confidence over candidate triples, but offer no native account of what evidence shaped a given prediction. Provenance lives outside the model — in metadata about the training corpus, not as edges of the graph the model populates.

Second: **language models hallucinate without traceable inference.** LLM responses to factual queries are a single forward pass over a frozen distribution; the answer is the answer, with no surface that distinguishes "this came from training data" from "this is a plausible continuation." Retrieval augmentation pins one piece of evidence to one response, but does not produce a graph one can later prune, audit, or retrain on.

**Loka's claim is that a single design choice resolves both:** if the inference layer's outputs are *triples* and provenance is expressed as *RDF-star annotations on those triples*, then every model-generated fact lands in the same store as the curated facts, with first-class citation edges to its supporting context. Auditable, filterable, queryable in SPARQL+, retrainable on the post-filtered corpus. The "neuro-symbolic" adjective is not aspirational — it describes the data layout.

### Contributions

1. **A reserved provenance namespace and a three-layer enforcement.** Predicates under `http://loka.dev/provenance/` (e.g., `propositionGenerated`, `propositionInferredFrom`, `propositionGeneratedBy`, `propositionConfidence`) are system-only. Three independent guards prevent the model from ever seeing, proposing, or emitting one: a SPARQL-star `FILTER NOT EXISTS << ?s ?p ?o >> propositionGenerated ?_g` clause in the corpus puller, a candidate-predicate filter in the inference loop, and an emit-time guard before each primary triple is written. Any single guard suffices; together they ensure that even with a regression in one path, generated provenance never re-enters training data. (§3.1)

2. **Generative citation as RDF-star reification.** Every model-generated triple `<S> <P> "X"` is accompanied by a fixed-shape annotation block. The block's subject is the *quoted* generated triple `<<S P "X">>`. Its objects include four metadata predicates (`propositionGenerated`, `propositionGeneratedBy`, `propositionConfidence`, ...) and one or more `propositionInferredFrom` edges whose object is *another quoted triple* — a cited piece of context. The result is a graph of generated triples threaded by citation edges to the curated context that informed them. (§3.2)

3. **Cumulative repetition penalty as a decode-time correction for mode collapse on common tokens.** Masked-S/P/O training produces models that "know" the answer category (university, museum, https-URL) but degenerate during greedy decoding to fillers like `of of of of` or `museum museum`. We show that a cumulative repetition penalty — dividing each repeated token's logit by `repetition_penalty ** count` — collapses these cascades within 2–3 emissions while preserving genuinely-needed reuse. The same v4 checkpoint moves from `university of of of of of of of` (no penalty) to `university of halle` (cumulative penalty 3.0), without retraining. (§4.3)

4. **Empirical demonstration across a twelve-version progression (v3→v14).** We report the full arc: the v3→v5 word-level trajectory (a corpus-quality regression caught and fixed mid-development; capacity 16M→44M as the *then*-binding constraint), the v6→v8 BPE rebuild after the catalog-noise finding (final ppl 64.65), the cron-automated v9→v10 (ppl 55.52), and — the current frontier — the **normalized-wikidata scaling series v11→v14**. Holding architecture/tokenizer/config fixed and scaling only the cleaned corpus 350k→4.0M triples, best-perplexity falls 279.12→**202.01**, v14 still descending where v13 plateaued: the binding constraint on this from-scratch line is now demonstrably *corpus scale*, not capacity or training duration. Every version is a tagged HF snapshot with its corpus; per-epoch snapshots (v12–v14) made the run resilient to three separate GPU-contention crashes with zero lost completed epochs. (§5)

We also surface two engine-level bugs found at scale: a SPARQL serialization quirk producing literal values in the predicate slot, and a write-flush wedge in the persistent layer at roughly every 5–6× growth in stored triples. The first is filtered at preprocess time and remains open; the wedge was diagnosed and fixed during v9 (§5.7) by batching the persistent-store writes into a single sled multi-tree transaction per HTTP request, verified at 2 M-triple sustained ingest with no recurrence.

---

## 2. Background

### 2.1 RDF-star

RDF-star is an extension of RDF in which any of the three positions of a triple — subject, predicate, object — may be a *quoted* (referenced, not asserted) triple. The notation `<<s p o>>` means "the triple s p o, treated as a term." This admits direct annotation of facts:

```
:Tokyo  :population  "13929286" .
<<:Tokyo :population "13929286">>  :measuredAt  "2020-01-01" .
<<:Tokyo :population "13929286">>  :statedIn    :census2020 .
```

The same shape that Wikidata expresses through reified statement nodes (e.g., `wds:Q1490-abc...`) collapses into one structural primitive. Two storage strategies exist: separate-asserted-graph (RDF 1.2 working draft) and synthetic-ID interning (used by Loka, where `quoted_triple_id(s_id, p_id, o_id) = xxh3` deterministically). We use the latter for compact joins on quoted-triple subjects.

### 2.2 Transformer-based knowledge graph completion

The dominant patterns in KG completion split into translational (TransE, RotatE, etc.) and transformer-based (KG-BERT, KGT5, recent work using LLMs as scoring functions). Most predict a single missing entity given (subject, predicate, ?) and report top-k accuracy on held-out triples. Two limitations relevant here: (a) outputs are scores or candidate IDs, not triples that can be re-stored; (b) provenance — which other triples in the corpus made this prediction confident — is not surfaced.

### 2.3 The from-scratch position

Loka's training is from scratch on RDF-derived text, not fine-tuning of a pretrained LLM. The position is not anti-LLM — it is that the closed-form auditability of "model knowledge ⊆ training corpus" is load-bearing for generative citation. With a fine-tuned LLM, even with the same RDF-star output schema, a generated triple may be drawn from base-model pretraining that the user never authorized as authoritative. We document a parallel near-term track admitting fine-tuning under stricter provenance assumptions in `planning/fine-tuning-track.md`; for the experiments in this paper, all results are from-scratch.

---

## 3. Architecture

### 3.1 The reserved provenance namespace

Every predicate under `http://loka.dev/provenance/` is system-internal. The names are deliberately verbose — `propositionGeneratedFrom` rather than `generatedFrom` — so a human scanning raw triples spots them at a glance and accidental collision with real-world predicates is vanishingly unlikely. The full namespace currently holds:

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
    << ?s ?p ?o >> <http://loka.dev/provenance/propositionGenerated> ?_g .
  }
}
```

It also drops any row whose predicate IRI matches the reserved prefix.

**Candidate filtering.** The inference loop builds candidate `(subject, predicate)` pairs by intersecting subject-with-graph-neighbor predicates. Reserved-namespace predicates are excluded from `pred_usage` and re-filtered at the candidate list level.

**Emit-time guard.** Each prediction's primary triple is checked against the reserved prefix immediately before it is written to the output stream. A reserved-prefix predicate is logged loudly and dropped.

Any single layer suffices. Three are kept because regressions in one path should not silently allow the model to learn or output system metadata.

### 3.2 Generative citation as RDF-star reification

A note on what "citation" claims here. At v0, the cited context triples are not selected by the model's internal attention or by a learned retrieval head; they are the rows the inference loop's candidate-predicate selection (§4.4 step 1) conditioned on. The contribution we claim is the *schema* — the data shape that lets a model emit a triple together with a transparent, queryable, post-hoc-auditable record of which curated rows were considered for that prediction — not a learned mapping from prediction to evidence. We treat this as a v0 design choice, not a final position; §6.3 records the gap and §7 sketches the OWL-template and HNSW-decoder paths that would make the link mechanistic. The schema makes the gap auditable: a downstream consumer can SPARQL-star over the `propositionInferredFrom` edges and decide for themselves whether each citation is informative, regardless of what the model "actually" attended to.

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
   │ Loka store     │ ─────→  │ Training corpus      │
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

The v3–v6 store (the corpus this section describes): 5,055,385 triples / 1,695,402 RDF-star annotations / 27,780 entities / 770 MB on-disk Loka store, every language label and description Wikidata has. *This is the early-phase corpus.* v7–v10 rebuilt it (catalog-noise removal, then fresh 2 M-triple cron slices); v11–v14 abandoned the Loka-store-as-corpus-source path entirely in favour of the streaming `normalized-wikidata` pipeline (§5.9, §5.12). The number above is historical, not the current training input.

### 4.2 Label substitution

The model is trained on text, not URIs. The corpus extractor walks all `rdfs:label "..."@en` triples, builds a URI → English-label map, then writes each triple with each component resolved through the map:

| Raw triple | After substitution |
|---|---|
| `<wd:Q42> <wdt:P31> <wd:Q5>` | `Douglas Adams <TAB> instance of <TAB> human` |
| `<wd:Q1490> <wdt:P1448> "Tokyo"@en` | `Tokyo <TAB> official name <TAB> Tokyo` |
| `<wd:Q24> <wdt:P40> <wd:Q1049347>` | `Jack Bauer <TAB> child <TAB> Kim Bauer` |

Property labels missing from the live store are fetched from Wikidata's public SPARQL endpoint with caching and 429-tolerance. Two preprocessing fixes were essential and are fragile enough to surface here:

1. **Strip `^^<datatype>` suffixes from typed literals.** Loka's SPARQL serialization embeds the datatype URI in the literal value string (e.g., `"+1966-02-18T00:00:00Z\"^^<http://www.w3.org/2001/XMLSchema#dateTime>"`) rather than separating it as `datatype` metadata. Without stripping, datatype-URI fragments (`xmlschema`, `decimal`, `org`) reach the tokenizer as if they were entity content and dominate certain predictions (§5.1).

2. **Drop rows with non-URI predicates.** ~1% of rows on a 5M corpus exhibit a Loka SPARQL bug (§6.1) where literal values surface in the `?p` slot. RDF disallows literal predicates, so dropping is safe.

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
| v3 (early; pre-cleanup) | 256 | 8 | 4 | 16,012,800 | 5 | 53.43 |
| v4 (early; cleaned) | 256 | 8 | 4 | 15,967,744 | 5 | 92.48 |
| v5 (early; capacity scale-up) | 512 | 8 | 6 | 44,531,712 | 5 | 84.85 |
| v6–v14 (current line) | 512 | 8 | 6 | 44,531,712 | 5–20 | see §5.5–§5.12 |

v3 reports artificially low perplexity from memorizing datatype-suffix tokens (§5.1). v4 is the canonical baseline at the smaller architecture. v5 established the 44.5 M-param architecture that every version v5 onward keeps fixed — so the v6→v14 progression isolates corpus and tokenizer effects from architecture. This table is the *early-phase* reference; the current results are the v11–v14 normalized-wikidata series (§5.9–§5.12), best perplexity 202.01 (v14) and still descending.

### 4.4 Inference: generative citation

For each candidate subject in the corpus:

1. **Candidate predicate selection.** Find graph-neighbors — subjects sharing at least one (predicate, object-key) tuple with this one — and rank predicates they have but the candidate subject lacks. Cap at *N* candidates per subject (default 5).
2. **Masked decoding with cumulative repetition penalty.** Build the input as `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] [MASK]^k [SEP_O]`. At each masked position, the model emits a logit distribution. We apply:

   - Hard skip-set: special tokens never win.
   - Cumulative repetition penalty: `logit[t] /= penalty^count[t]` where `count[t]` is the number of times `t` has already been emitted in this sequence. Default `penalty = 3.0`.
   - Per-token confidence floor: emission halts when the top-token probability falls below 0.05.

   Greedy top-1 selection, no beam search.
3. **Confidence-thresholded emit.** Mean per-token probability is the prediction's confidence. If confidence ≥ threshold (default 0.4) and the predicted object is not a duplicate of an existing fact for this (S, P), emit the RDF-star block (§3.2).
4. **Optional `--post`.** Write the emitted N-Triples-star to the live Loka store via `POST /triples`. Subsequent training-corpus extractions exclude these via the SPARQL-star FILTER from §3.1.

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

### 5.5 Catalog-noise discovery and corpus cleanup (v6 → v7)

A post-training behavioural test surfaced a corpus-level finding that reframes everything before it. We ran an auto-regressive proposition-generation protocol on the v6 model: pull a fresh BFS-depth-3 Wikidata neighborhood (183 entities, 14,586 triples, seeded at Q42), and for every source triple in the neighborhood generate up to 10 child triples whose context is the BFS-adjacent set after an asymmetric-cardinality filter, with parallel-subgraph extension. (Protocol details in `planning/autoregressive-propgen-test.md`.)

v6 emitted 52 triples on a 30-source run. The qualitative content is the finding:

- **Confident catalog-format hallucinations.** `British Broadcasting Corporation | ISNI -> "00000000"` (conf 0.754); `Joan of Arc | Library of Congress authority ID -> "n 85 - 8"` (LCCN-shaped, wrong content); `Douglas Adams | Freebase ID -> "/ m / 0 c _ _ 9"` (Freebase format `/m/...`, wrong content). The model has memorised the *shape* of these identifiers and emits format-shaped strings on prompt.
- **Catalog format leaking onto unrelated predicates.** `instance of -> "+ Ġof - 00 - 03 T 00"` appeared on 15 different subjects in the same 30-source run. This string is a Wikidata date-prefix shape (`+YYYY-MM-DDTHH:MM:SS`) being hallucinated for a predicate (`P31 instance of`) whose objects are entities, not dates. The model has so saturated on catalog/structured-literal patterns that it defaults to format strings on uncertain prompts.

The diagnosis is corpus composition. Wikidata defines an `external-id` datatype for properties whose objects are catalog cross-references (Freebase, ISNI, GND, LCCN, Dewey, etc.). A fresh SPARQL query against `wikibase:propertyType wikibase:ExternalId` returns **10,206** properties — roughly 80 % of all Wikidata property *types*. In the Q42 seed they are 49.6 % of triples by *volume*. On the v6 training corpus the share was similar: 75.7 % of the 757,592-triple file was external-id rows after re-filtering by predicate label. We had been training on a corpus that was three-quarters catalog cross-reference noise.

We rebuild the corpus as v7. The exclusion list is broadened beyond external IDs to include other Wikidata datatypes whose values have no transferable semantic content (`url`, `commonsMedia`, `math`, lexeme/sense/form, `globe-coordinate`, `geo-shape`, `musical-notation`, `tabular-data`, `wikibase-entity-schema`) — 10,525 properties total dropped, against a kept set of `wikibase-item`, `wikibase-property`, `quantity`, `string`, `time`, and `monolingualtext` (2,231 properties). Two other normalisations land at the same time:

1. **Time and quantity literals.** Wikidata serialises positive years as `+YYYY` and dates with a trailing `T00:00:00Z` regardless of precision. The leading `+` is a high-frequency BPE token implicated in the date-shape leak. v7 strips the `+` for positive years (BCE keeps the `-`), drops the `T00:00:00Z` suffix on date-only times, and drops the trailing `Z`. `+2012-10-15T00:00:00Z` → `2012-10-15`; `+1234` → `1234`.
2. **Monolingual text.** v6 dropped non-English `monolingualtext` values; v7 keeps them in all languages with the `@lang` tag stripped. The model now sees `Tokyo` and `東京` as plain string values (the language information is lost — see §6.2).

The full per-datatype processing spec, with kept/dropped decisions and normalisation rules, is in `planning/wikidata-datatype-processing.md` and in `training/wikidata_excluded_predicates.json`.

We retrain v7 with the same 44.5 M-parameter BPE architecture as v6 for 5 epochs on the cleaned 184,458-triple corpus. Final perplexity 192.63 (v6 was 194.98, statistically tied); wall time 22 min on the same 4070 (v6 took 91 min on the larger noisy corpus). The same 30-source / Q42 seed test:

| | v6 (noisy corpus) | v7 (cleaned) |
|---|---|---|
| Total emissions at conf ≥ 0.25 | 52 | 14 |
| `instance of -> "+ Ġof - 00 - 03 T 00"` (date-shape leak) | 15 instances | **0 instances** |
| `ISNI ->` confident hallucination | `"00000000"` (0.75) | `"0 ."` (0.71) |
| `Freebase ID ->` confident hallucination | `"/ m / 0 c _ _ 9"` (0.43) | below threshold |
| `country of citizenship ->` (semantic predicate) | did not pass | `"Polish âĢĵ Ġof -"` (0.36, da Vinci; right *type* — nationality adjective — wrong content) |

The catalog-format hallucinations are *gone*, not muted. The model's failure mode shifts from "confidently wrong with the right shape" to "refuses to emit", which is the correct direction for a generative-citation system. The price is volume: 14 emissions vs 52, because the model no longer confidently produces format-shaped strings. The loss curve says v7 is undertrained (5.36 → 5.26 still descending at epoch 5); we trained v8 on the same corpus for 20 epochs (§5.6).

### 5.6 v8: 20 epochs on the cleaned corpus

We trained v8 using the same 44.5 M-parameter BPE architecture as v6 and v7, but for 20 epochs from random initialisation on the v7 corpus. The 5 → 20 epoch increase produced a 3× perplexity reduction with no change to architecture or data:

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 13.0306 | 456,141.98 |
| 5 | 5.2607 | 192.63 (= v7 final) |
| 10 | 4.4257 | 83.57 (≈ v5 final) |
| 15 | 4.2540 | 70.38 |
| 20 | **4.1691** | **64.65** |

Wall time 88 min on the same 4070 (vs v7's 22 min for 5 epochs — linear in epoch count). Loss was still descending at epoch 20 (4.20 → 4.19 → 4.17), so the v7 corpus is not yet saturated at this model size — strong evidence that the cleanup left signal the 5-epoch v7 had not yet exploited.

We apply the same 30-source / Q42 seed test as §5.5, extending the comparison to all three generations:

| | v6 | v7 | v8 |
|---|---|---|---|
| Final perplexity | 194.98 | 192.63 | **64.65** |
| Total emissions at conf ≥ 0.25 | 52 | 14 | **47** |
| — on catalog-shaped predicates | 21 (40 %) | 9 (64 %) | 7 (15 %) |
| — on semantic predicates | 31 (60 %) | 5 (36 %) | **40 (85 %)** |
| `instance of -> "+ Ġof - 00 - 03 T 00"` (date-shape leak) | 15 instances | 0 | 0 |

Selected v8 outputs (raw, BPE artifacts left visible — `Ġ` is the BPE space marker, `âĢĵ` is a mis-decoded em-dash):

| Subject / predicate | v8 output | Confidence |
|---|---|---|
| `English / different from` | `"English"` | 0.876 |
| `Adams / different from` | `"Adams"` | 0.960 |
| `Joan of Arc / Commons category` | `"Joan Ġof ĠAr c Ġ( Ġ("` | 0.654 |
| `British Broadcasting Corporation / Commons category` | `"British ĠBroadcasting ĠCorporation Ġ( Ġ("` | 0.791 |
| `myocardial infarction / Commons category` | `"my ocard ial Ġin far"` | 0.639 |
| `Leonardo da Vinci / country of citizenship` | `"Polish âĢĵ"` | 0.677 |
| `Leonardo da Vinci / date of birth` | `"- 00 000000 - 00 - 00 T"` | 0.322 |

Three patterns emerge. The model has discovered the high-frequency Wikipedia Commons-category template — `"X ( ..."` — and applies it confidently across many subjects; since Commons-category is among the most common predicates in the v7 corpus, this is frequency-appropriate behaviour. The `different from` outputs are circular: the model emits the subject as the object, a known pathology of the masked-S/P/O objective when the predicate is predominantly reflexive in the corpus (disambiguation pages couple each entity to itself). The failure is *consistently* wrong rather than arbitrarily wrong — a more tractable surface than v6's hallucinated catalog formats. Finally, the catalog-format leak that defined v6 remains entirely absent: `instance of` produces no date-prefix strings on any subject in the test.

The remaining failure modes — circular `different from`, BPE artifact leakage, residual catalog hallucination on the few external-id predicates the seed still includes — are all addressable downstream of the corpus cleanup: the first wants a structural change to the masked-prediction objective, the second is a tokenizer post-decode pass, and the third disappears once the inference layer also drops excluded predicates from its candidate pool. None of them argue for re-introducing catalog noise into training.

The wider implication of v8 is that the v7 corpus is small for the model. At ~600 k tokens after BPE on a 44.5 M-parameter model we are at 0.013 tokens per parameter, against a Chinchilla-optimal target of ~20. The v8 result — that 4× more epochs on the same corpus produces a 3× perplexity improvement — is consistent with a model that still has room to fit. The next step is therefore data scale, not more epochs: a fresh `tools/wikidata_hf_import.py` run targeting ~5 M useful triples after filtering, followed by v9 from scratch on that corpus. `tools/training_cron.py` (a 12-hour local cycle loop) automates the train-test-ship-retrain pipeline so this can run unattended.

### 5.7 v9: wedge fix exposed at 4M triples, model trained on a fresh slice

v9 carries two independent results: the `/triples`-wedge engine bug (§6.1) is fixed, and the model trained on a freshly-pulled Wikidata slice reaches perplexity **57.15**, the best of any version, on a corpus that is in fact *smaller* than v7's.

**The wedge.** Paper §6.1 has documented since v3 that the engine wedges after roughly every 5–6× growth in stored triples — the `POST /triples` handler stalls indefinitely while `/health` keeps responding. The wedge fired again on the v9 ingest at the now-routine ~90 k-triple threshold. Root cause traced (via `planning/triples-wedge-investigation.md` and inspection of `loka-core/src/persistent.rs` and `loka-proto/src/server.rs`): the handler made 3–4 sled write-transactions per N-Triples line (three term-interns plus one SPO/POS/OSP triple-insert) and called `PersistentStore::flush()` synchronously at the end of every request. Under sustained ingest of ~100 k+ triples in one POST, sled accumulated write-ahead-log entries faster than its internal compactor could drain, and the writer thread eventually stalled. Fix in commit `39effbb`: `PersistentStore::insert_batch` writes a whole HTTP request's worth of triples (and their term-interns) in a *single* sled multi-tree transaction. The synchronous `flush()` is gone — sled's periodic flush + Drop-time flush is sufficient durability. The handler in `loka-proto/src/server.rs` collects all triples into a `Vec<BatchInsert>` and calls `insert_batch` once. Verified: 2,000,049 triples in 4 003 s at 500 triples/sec sustained, no timeouts; cumulative 4M+ triples across the v9 cycle's two import phases, no wedge. Previous wedges at 90 k, 174 k, and ~1 M are all cleared by 20× or more.

**The corpus.** The v9 cycle pulled 2 090 640 raw triples into a fresh per-cycle Loka data dir from `philippesaade/wikidata`. Preprocessing through `training/preprocess.py` (same datatype filter as v7 + the v7 quantity/time normalisations) produced a 94 202-triple training file — *smaller* than v7's 184 458. The reason: `philippesaade/wikidata` is structured one row per entity, with all of an entity's claims in a single JSON blob. The v9 import consumed 9 647 rows for 2 M raw triples (217 triples/entity average), so most `wikibase-item` objects refer to entities whose own rows haven't been streamed yet, and the preprocess step drops a triple when it can't resolve the object's English label — 1 049 881 rows dropped that way. v7's corpus, built from a 50× larger raw-triple pool over the original 5 M-triple slice, had more closed label cycles per entity and a higher retention rate. This is a corpus-construction artifact, not a model issue; the fix (cross-cycle label caching, or streaming many more rows before preprocessing) is on the v10 roadmap.

**Training and perplexity.** 20 epochs, same 44.5 M-parameter BPE architecture, 44 min wall time on the 4070 (∝ to the 94 k vs 184 k corpus size relative to v8's 88 min).

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 17.4379 | 37,426,431 |
| 5 | 5.3740 | 215.71 (≈ v7 final) |
| 10 | 4.8977 | 134.0 |
| 15 | 4.2466 | 69.9 |
| 20 | **4.0457** | **57.15** |

v9 perplexity (57.15) beats v8 (64.65) by 12 % despite training on half the data. Two plausible explanations: the v9 corpus is preferentially the entities with closed reference graphs (i.e. structurally well-connected); and the v9 corpus is freshly drawn from a different slice of Wikidata, so the model isn't being asked to fit the same long-tail noise v8 had.

**Q42 propgen test, all four versions.**

| | v6 | v7 | v8 | v9 |
|---|---|---|---|---|
| Final perplexity | 194.98 | 192.63 | 64.65 | **57.15** |
| Emissions at conf ≥ 0.25 | 52 | 14 | 47 | 35 |
| — on catalog predicates | 21 (40 %) | 9 (64 %) | 7 (15 %) | **1 (3 %)** |
| — on semantic predicates | 31 (60 %) | 5 (36 %) | 40 (85 %) | **34 (97 %)** |
| `instance of` date-shape leak | 15 | 0 | 0 | 0 |

97 % semantic-predicate share on v9 is the cleanest signal yet that the v7 datatype cleanup has been internalised. A residual catalog-format hallucination remains on `Template:* | different from` predicates, where the model emits short URL-prefix strings (`"T ://"`, `"M ://"`) instead of entity references — same shape-leak class as v6's date-shape leak, but in URL format and on a narrower predicate set. Not yet diagnosed; characterised in `DEVLOG.md` for v10 follow-up.

**Implication.** The wedge fix removes a long-standing infrastructure ceiling. The model now scales with data without engine-side limits in the way. v10 work focuses on the corpus side: stream enough rows from the HF dataset to close the entity-label reference graph (target ≥ 50 k entities, several hundred thousand resolved-label triples after preprocess), and apply the same propgen test to see whether semantic-content quality continues to improve.

### 5.8 v10: the first fully-automated cron cycle

v10 is the first model shipped end-to-end by `tools/training_cron.py` without any manual intervention. The 12-hour local cron loop ran HF import → preprocess → train → propgen test → DEVLOG entry → MODEL.json pin → HF push → commit + push → sleep, all autonomously. Trained 20 epochs on a 94 058-triple corpus extracted from a fresh 2 M-triple HF slice, the same shape as v9. Final perplexity **55.52** — a further 3 % improvement over v9, with the loss curve still descending at epoch 20 (4.06 → 4.02).

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 17.6480 | 46,179,373 |
| 5 | 5.3723 | 215.37 |
| 10 | 4.9168 | 136.57 |
| 15 | 4.1988 | 66.61 |
| 20 | **4.0168** | **55.52** |

**Q42 propgen test.** 60 emissions at conf ≥ 0.25 (up from v9's 35), **0 catalog hallucinations**, **100 % semantic-predicate share** — the cleanest signal of the run.

| | v6 | v7 | v8 | v9 | v10 |
|---|---|---|---|---|---|
| Final perplexity | 194.98 | 192.63 | 64.65 | 57.15 | **55.52** |
| Emissions at conf ≥ 0.25 | 52 | 14 | 47 | 35 | 60 |
| — on catalog predicates | 21 (40 %) | 9 (64 %) | 7 (15 %) | 1 (3 %) | **0 (0 %)** |
| — on semantic predicates | 31 (60 %) | 5 (36 %) | 40 (85 %) | 34 (97 %) | **60 (100 %)** |

**Selected v10 outputs.**

| Subject / predicate | v10 output | Confidence |
|---|---|---|
| `– / Commons category` | `"man Ġ("` | 0.92 |
| `– / Commons category` | `"Category : Dramatists and play"` | 0.60 |
| `– / instance of` | `"municipality Ġof Ġthe"` | 0.52 |
| `– / country` | `"People 's ĠRepublic : ĠRepublic"` | 0.51 |
| `– / spouse` | `"1 ."` | 0.50 |

The Commons-category outputs are template-correct (`"Category : Dramatists and play[wrights]"` is the exact Wikimedia Commons naming convention). `instance of -> "municipality of the"` is a plausible-type semantic answer (would be correct for many entities in the Wikidata long tail). `country -> "People's Republic"` is right for PRC entities and right-type for non-PRC; `spouse -> "1 ."` is a numeric-format degeneration, a residual failure mode worth tracking but smaller than v9's `Template:* | different from -> "T ://"` URL-shape leak (now gone).

**What changed in the infrastructure.** v10 is the first model produced under the steady-state regime of:

1. The `/triples` wedge fix (§5.7), so a 2 M-triple HF import completes cleanly in ~67 min at 500 triples/s without retries.
2. The per-data-dir state file (commit `95f56f7`), so each cycle's HF import resumes correctly when a cycle is restarted, and crash-recovery doesn't redo work.
3. PageRank-weighted source selection in the propgen test (commits `e2809e3`, `0734e40`), so the evaluation focuses on structurally-important entities.
4. The auto-discovered `MODEL_FILES` list in `tools/hf_snapshot.py` (commit `4c996b9`), so new checkpoints are uploaded without editing the script.
5. The dynamically-rendered HF README (commit `758b6ff`), so the dataset page on Hugging Face refreshes its description on every push.

The cron's `ship()` step covers DEVLOG, MODEL.json, propgen test artifacts, HF push, and the local commit + push, but it deliberately does *not* edit the paper — paper revisions happen here (or via the remote `schedule`-skill cron jobs that polish paper prose for the AI peer reviewer). This is the v10 paper revision.

### 5.9 v11: pipeline pivot — no Loka in the training data path, and a standalone `normalized-wikidata` dataset

The v11 cycle pivoted the *shape* of the preprocessing pipeline. Earlier versions extracted training triples from Loka via SPARQL `?s ?p ?o` paging. At the v11 scale (a fresh 50 M-triple Loka slice) this hit two structural problems:

1. **`LIMIT/OFFSET` is O(offset) on sled.** Page 1 took 8 s, page 100 took 235 s, pass 1 alone projected to ~25 hours.
2. **Engine bug #2 corrupts `rdfs:label` rows for properties.** RDF-star annotation rows in the Wikidata source dump are surfaced by Loka's SPARQL executor with the property IRI keyed against the inner triple's *object* value, not the property's actual label — so `wdt:P20 rdfs:label "Belgium"@en` instead of `"place of death"`, and similarly for every property we audited. Entity labels were unaffected, but for training-input use this would have made every predicate label arbitrary garbage. The fix preloads `training/property_label_cache.json` (7 312 curated entries) as `source='curated'` and skips corpus rdfs:label rows whose subject is a property.

Both problems vanish if we go straight from the source HF parquet to the training file with Loka not in the loop. `tools/preprocess_from_hf.py` streams `philippesaade/wikidata`, builds an English-label cache in SQLite during a first iteration, then emits one tab-separated `subject\tpredicate\tobject\n` line per kept claim during a second iteration. Same filter set as the canonical preprocess (`training/wikidata_excluded_predicates.json` removes ~82 % of property types by datatype class, time/quantity values are normalised, RDF-star annotation rows are stripped, system-reserved `loka:propositionInferredFrom`-namespace triples are stripped). The resulting corpus is independently useful, so it gets its own HF dataset repo: `EmmaLeonhart/normalized-wikidata`, license CC-BY-SA 4.0 (inherits from Wikidata).

This is also the start of a multi-rung dataset / model series:

| HF dataset tag | Entity rows | Output triples | Trained model |
|---|---|---|---|
| `v11-50k` (released) | 50 000 | 350 428 | v11 |
| `v12-100k` | 100 000 | ~700 000 (est.) | v12 |
| `v13-500k` | 500 000 | ~3.5 M (est.) | v13 |
| `v14-1M` | 1 000 000 | ~7 M (est.) | v14 |

**v11 training.** Same 44.5 M-parameter BPE architecture as v6 onward. Hardware constraint: the training box is a laptop 4070 with **8 GB VRAM** (not the 12 GB of the desktop 4070). At `--batch-size 32` the model fits forward but Adam's optimizer state + gradient peaks in later epochs cross the line. v11 completed 3 of 20 planned epochs before `torch.AcceleratorError: CUDA error: out of memory` in epoch 4's backward pass; the per-epoch checkpoint mechanism in `train.py` means the **epoch-3 weights are the v11 release**. Future training runs (v12, v13, v14) use `--batch-size 16`.

| Epoch | Loss | Perplexity | Wall |
|---|---|---|---|
| 1 | 8.7914 | 6 577.15 | 1 102 s |
| 2 | 5.8514 | 347.71 | 1 100 s |
| 3 | 5.6316 | **279.12** | 978 s |
| 4 | — | **CUDA OOM** | — |

**Headline number 279.12 is not directly comparable to v10's 55.52.** v10 was trained 20 epochs on a 94 k-triple corpus; v11 was trained 3 epochs on a 350 k-triple corpus. Loss curve is descending well at epoch 3 (5.85 → 5.63 in one epoch), so the headroom is real — a future continuation run at batch 16 would land somewhere between v10's 55.52 and the trajectory's projection of ~50–80 at epoch 20. We choose to ship v11 at epoch 3 rather than retrain because the next bottleneck is *corpus quality and scale*, which `v12-100k` / `v13-500k` / `v14-1M` are set up to push on. The corpus shape is what's new about v11; the model trained on it is a first measurement against that shape.

**Propgen test deferred.** The same GPU fragility that caused the OOM makes us wary of running a sustained autoregressive inference loop on this checkpoint immediately afterward. Defer to a follow-up cycle when the laptop is genuinely idle. The §5.6–§5.8 trajectory of catalog-predicate share collapsing to zero is expected to hold on v11 because the cleaning is dataset-side, not model-side; the v11-50k corpus has no external-id / catalog datatypes by construction.

### 5.10 v12: bigger corpus pays despite a botched training trajectory

v12 is the second rung of the normalized-wikidata series: 671 817 triples from the `v12-100k` corpus (100 000 entity rows, ~1.9× v11's size), same 44.5 M-parameter BPE architecture, `--batch-size 16` to live within 8 GB VRAM.

The training run did not go cleanly. Five hours in, an unrelated LLaMA-3.1-8B experiment (`scripts/run_five_condition_experiment.py`) started using the laptop's GPU concurrently. Per-epoch wall time jumped from ~37 min to ~180 min, and the loss curve started *climbing* instead of descending — Adam's momentum estimates corrupted under shared-GPU contention (CUDA stream re-ordering, memory fragmentation, thermal throttling, in some combination). When the contending experiment was killed, the next epoch finished with even worse perplexity than the one before it — optimizer-state corruption is sticky. The v12 trainer was then killed externally during what would have been epoch 8.

| Epoch | Loss | Perplexity | Wall |
|---|---|---|---|
| 2 | 5.8383 | 343.18 | 38 min |
| 3 | 5.4725 | 238.05 | 32 min |
| 4 | 5.4243 | **226.86** ← best | 41 min |
| 5 | 5.4955 | 243.59 | 175 min (shared GPU) |
| 6 | 5.5247 | **250.82** ← shipped | 191 min |
| 7 | 5.5521 | 257.77 | 147 min |

We snapshot-saved the epoch-6 checkpoint mid-run (foresight: `train.py` writes to the same path every epoch, overwriting), so the v12 release is ppl **250.82**, not the worse epoch-7 value the unsnapshotted run would have produced.

Two findings from the cycle worth keeping:

1. **The corpus shape matters more than 3 epochs of training**. v12 at ppl 250.82 (six botched epochs on a 672 k-triple corpus) beats v11 at ppl 279.12 (three clean epochs on a 350 k-triple corpus). v12's clean epoch-4 result (226.86, 31 points below v11's 3-epoch endpoint) is a stronger version of the same point — but even the regression-damaged epoch-6 result holds it.

2. **Shared GPU is poison for Adam under sustained CUDA load.** This isn't merely "training is slow when the GPU is busy" — the *direction* of the loss curve flips. On a thermally-marginal consumer laptop, batched-transformer training requires exclusive GPU access; any other CUDA process on the same card will corrupt training over time, not just delay it.

`v13-500k` (2 511 771 triples, 3.6× v12) and `v14-1M` (4 021 409 triples) both completed: v13 shipped at epoch-2 ppl 242.75 (§5.11), v14 final at epoch-4 ppl 202.01 (§5.12). The corpus-scale trend held to the end of the series.

### 5.11 v13: shipped early at epoch-2 plateau (ppl 242.75), partial-local v14 follow-up

v13 trained on the `v13-500k` corpus (2 511 771 triples, 3.6× v12) on an exclusive laptop GPU — no other CUDA processes, no contention. The loss curve descended cleanly to a strong epoch-2 result (ppl **242.75**) and then plateaued: epochs 3–5 walked inside a 248–258 band with stable 112 min/epoch wall-time. `nvidia-smi` showed the GPU power-cap-bound at 74 W actual / 80 W cap with 83 % util, 74 °C. Not divergence (the v12 pattern broke both ppl and wall-time); not contention (verified GPU exclusive). Adam's first/second-moment estimates appear to have settled around a local optimum at epoch 2 and the subsequent epochs are random-walking inside the basin.

| Epoch | Loss | Perplexity | HF tag |
|---|---|---|---|
| 1 | 5.8134 | 334.76 | `v13.1` |
| 2 | 5.4920 | **242.75** ← shipped | `v13.2` |
| 3 | 5.5146 | 248.29 | `v13.3` |
| 4 | 5.5546 | 258.42 | `v13.4` |
| 5 | 5.5453 | 256.04 | `v13.5` |

The decision to ship at epoch 2 rather than continue is enabled by `tools/epoch_snapshot_pusher.py`, a watcher introduced after the v12 disaster: every completed epoch is snapshotted to its own HF tag, so a divergent late epoch cannot lose the earlier ones. Shipping the canonical `v13` tag as the epoch-2 checkpoint costs nothing — every epoch through 5 is still pullable on Hugging Face for empirical comparison.

**Cross-version comparison.** v13 at 242.75 lands between v12's lost best (epoch-4 ppl 226.86, never shipped) and v12's actual shipped value (epoch-6 ppl 250.82, training-disruption-degraded). The corpus-quality lever (more data, same architecture) is therefore still working: v11 (350 k) → v12 (672 k) → v13 (2 511 k) trained best results 279.12 → 226.86 → 242.75 — but the trained-headroom-on-this-laptop floor has clearly bottomed out around 240. Contributors with desktop GPUs and bigger batch sizes are the natural next axis; see the GPU-donation contributor path documented at <https://loka.emmaleonhart.com/contribute/> and `tools/contribute_v14_training.py`.

**Power-cap observation.** The laptop runs power-cap-bound during training. A 24 GB-card contributor at `--batch-size 64` is a meaningfully different empirical regime (lower gradient noise, deeper basins are practical), not just a faster version of the same run. We expect the contributor v14 result to push below 240 — but we can't make that observation on this hardware.

**v14 follow-up.** A partial-local v14 run (5 epochs, batch 16, ~16 h estimated) started immediately after v13 stopped. Each epoch will land on Hugging Face as `v14.N` regardless of whether the run completes. Full 10-epoch contributor path is published separately.

### 5.12 v14: corpus scale confirmed as the binding lever (ppl 202.01, series best)

v14 trained on the `v14-1M` corpus (4 021 409 triples, 1.6× v13, ~11× v11) — same 44.5 M-parameter architecture, batch 16, partial-local 5-epoch run. It is the best model in the entire series and the cleanest demonstration of the central empirical claim.

| Epoch | Loss | Perplexity | HF tag | Notes |
|---|---|---|---|---|
| 1 | 5.6457 | 283.07 | `v14.1` | |
| 2 | 5.3727 | 215.45 | `v14.2` | |
| 3 | 5.3449 | 209.55 | `v14.3` | |
| 4 | 5.3083 | **202.01** ← canonical | `v14.4` | series best |
| 5 | 5.3216 | 204.70 | `v14.5` | end of partial-local run |
| 6 | 5.3284 | 206.12 | `v14.6` | continuation; fresh-Adam restart from epoch-4 weights (epoch-4 ckpt predates optimizer-state saving) — expected one-epoch bump |
| 7 | 5.3465 | 209.87 | `v14.7` | Adam *not* recovering below 202; drifting up like v13's epochs 3–5 |

**v14 is final at epoch 4 (ppl 202.01).** The 5-epoch partial run had not plateaued (ep4 202.01 best, ep5 204.70), so we ran a *bounded continuation experiment* — epochs 6–7 resumed from the epoch-4 weights. Because the epoch-4 checkpoint predates optimizer-state persistence, this restarts Adam from zero state, and the result (ep6 206.12, ep7 209.87) is the same Adam-plateau signature as v13 §5.11: a fresh optimizer on already-well-fit weights random-walks slightly *outward* rather than re-finding a deeper basin. That experiment answered its question — **~202 is the practical floor for a from-scratch fit on this corpus and this 8 GB power-capped laptop GPU** — so we finalize v14 at the epoch-4 checkpoint rather than chase diminishing returns with a fresh mid-line optimizer. Every epoch is preserved as its own HF tag (`v14.1`…`v14.7`); the canonical `v14` is epoch-4. Pushing materially below 202 is explicitly the job of a clean-optimizer 10-epoch run on larger hardware (the GPU-donation contributor path, §contributor), not of this hardware.

**What v14 trained on.** The `v14-1M` corpus is clean English-label triples — no opaque QIDs, no catalog-identifier noise, normalised time/quantity values. A uniform random sample:

```
Joseph Roger de Benoist  | educated at        | Paris Diderot University
1994 FIFA World Cup       | statistical leader | Hristo Stoichkov
Stanley Clarke            | social media followers | 44568
Berkanush                 | inception          | 1828-00-00
Meigs County              | population         | 23257
Bernhard Severin Ingemann | Commons gallery    | Bernhard Severin Ingemann
```

This is the data shape the corpus-scale result rests on: every position a resolved English label, the noise datatypes (~82 % of Wikidata property *types*) stripped at preprocess. A v14 propgen behavioural test (the Q42-seeded generation eval reported for v6–v10 in §5.5–§5.8) was deliberately *not* run on this hardware — the laptop GPU fragility that capped training also makes a sustained inference loop risky — and is documented as a contributor-path follow-up, not reported here as a v14 number we do not have.

**The headline result.** Holding architecture, tokenizer, batch size and optimizer fixed and varying only the cleaned-corpus size:

| Model | Corpus triples | Best ppl |
|---|---|---|
| v11 | 350 428 | 279.12 |
| v12 | 671 817 | 250.82 (226.86 lost) |
| v13 | 2 511 771 | 242.75 |
| **v14** | **4 021 409** | **202.01** |

An 11× increase in clean training data drove a 28 % perplexity reduction, and — critically — v14 was *still descending at epoch 4*, where v13 (2.5 M) had plateaued by epoch 2. More data did not merely lower the floor; it extended the number of epochs of extractable signal before Adam settled. This is the strongest evidence in the paper that the binding constraint on this model line is **corpus scale**, not architecture, decoder, or training duration on the available hardware. It also retroactively reframes the v11–v13 perplexities: they were data-starved, not architecture-limited.

**Why we ship epoch 4 rather than a 10-epoch run.** v14 had not plateaued at 5 epochs, so a 10-epoch continuation was attempted. `train.py` was extended with `--resume-from` / `--start-epoch` and optimizer-state persistence for this. The continuation OOM'd in epoch 6's backward pass when an unrelated `pytest` suite acquired the 8 GB laptop GPU concurrently — the same concurrent-CUDA failure class that corrupted v12 (§5.10). Rather than re-run 16 h locally on hardware where a stray process can kill training, we ship the epoch-4 checkpoint (already the series best, already on HF as both `v14` and `v14.4`) and publish the full clean 10-epoch run as a GPU-donation contributor path (`tools/contribute_v14_training.py`, <https://loka.emmaleonhart.com/contribute/>). We expect a contributor run on a 24 GB card at `--batch-size 64`, run to 10 epochs without wall-clock pressure, to push meaningfully below 202 — but that is a different empirical regime than this laptop can produce, and we are explicit about the boundary rather than extrapolating past it.

**Series summary.** Twelve checkpoints (v3–v14) on `EmmaLeonhart/loka`; four normalized-corpus tiers (`v11-50k` → `v14-1M`) on `EmmaLeonhart/normalized-wikidata`; per-epoch tags for v12–v14. The per-epoch snapshot discipline (`tools/epoch_snapshot_pusher.py`) preserved every epoch across three separate training disruptions (v11 batch-32 OOM, v12 shared-GPU contention, v14 pytest OOM) — no result was ever lost to a failed run, only wall-clock.

---

## 6. Limitations

### 6.1 Engine bugs surfaced at scale

- **SPARQL `?s ?p ?o` occasionally returns literal values in the predicate slot.** RDF disallows literal predicates; this is invalid output from the executor — almost certainly an RDF-star annotation row with positions getting confused. Filtered at preprocess (drops ~1% of rows on a 5M corpus). Real engine bug.

- **`POST /triples` wedges after roughly every 5–6× growth in stored triples.** *(Resolved during v9; see §5.7.)* Originally hit at ~90 k, ~174 k, and ~1 M during the v6 ingest of the 5 M-triple slice — `/health` keeps responding, `/triples` and SPARQL hang indefinitely until the server is restarted, data intact on disk. Root cause: the handler ran 3–4 sled write-transactions per N-Triples line (three term-interns + one SPO/POS/OSP triple-insert) plus a synchronous `flush()` per request, accumulating WAL entries faster than sled's compactor could drain. Fix in commit `39effbb`: `PersistentStore::insert_batch` writes the whole request in one sled multi-tree transaction; synchronous flush removed (sled's periodic + on-Drop flush is sufficient). Verified across two independent 2 M-triple imports (4 M cumulative) at 500 triples/s sustained, no recurrence.

### 6.2 Model and decoding

- **Mode collapse on common connector tokens.** Even with cumulative penalty 3.0, predictions for predicates the model has weak knowledge of fall back to `of`/`and`/`https www`. This reflected thin entity-content coverage at v5's corpus size (27,780 entities of the ~100 M available on Wikidata). v6–v10 trace the lever side: the corpus-cleanup work in v7 (§5.5) eliminated the catalog-format leak that drove the worst connector-token loops, and v10's 100 % semantic-predicate share on the Q42 propgen test (§5.8) is the strongest evidence to date that the binding constraint shifted from "model can't distinguish predicates" to "model needs more diverse entity-context coverage to leave its current mode-collapse residue (numeric placeholders on under-specified predicates like `spouse -> "1 ."`)". The next lever remains more *useful* data, where "useful" now requires closed entity-label reference graphs (§5.7's discovery).
- **Word-level tokenizer chops Unicode.** "Saint-Léger" becomes `saint l ger`. BPE/wordpiece is the planned fix; unimplemented in this iteration.
- **No beam search or top-p sampling.** Greedy top-1 only. Some failure cases would resolve with beam-2.

### 6.3 Provenance

- **Citation hallucination is structurally bounded but not zero.** A `propositionInferredFrom` row points at a concrete context triple, which is auditable, but the *choice* of which context triples to cite is heuristic (§4.4 step 1). The model is not actually inspecting these specific triples during prediction; the citation is "the candidate-predicate selection considered these triples." We document this tradeoff as accepted: the schema is honest about what it represents.

- **Position taken: the contribution is the schema, not the mechanistic link.** A round-one AI peer reviewer (Gemini 3 Flash, post 2378 v1) flagged this exact heuristic-vs-learned gap as invalidating the "generative citation" framing. We disagree on the framing but accept the empirical reading: at v0, citations describe *what context was selected for prediction*, not *what the model attended to during prediction*. The contribution we claim is the data shape — predicted triples and their context appear in the same store, in RDF-star, with citation predicates that any consumer can audit and filter via SPARQL — not a learned retrieval head that proves each citation supports its target. §7 sketches the OWL-template and HNSW-decoder paths that would close this gap; until those land, the right framing for citations is "auditable post-hoc evidence pointer," which is still strictly better than the standard situation (no citation at all).

### 6.4 What we are *not* claiming, and why we do not report MRR / Hits@k

The dominant evaluation regime in transformer-on-KG completion (KG-BERT, KGT5, et al.) reports MRR and Hits@k against held-out triples on closed benchmarks like FB15k-237 or WN18RR. We do not report these numbers, and we want to be explicit about why — both so the gap is visible and so future work in the regime is well-scoped.

1. **Prediction space, not entity space.** Loka v0 emits *labels*, not entity IRIs. The model produces `"university of halle"` token-by-token, not `<wd:Q156667>`. MRR and Hits@k assume a finite candidate set of entities to rank; we have a vocabulary over English subword pieces (BPE in v6, word-level in v3–v5). The HNSW-as-decoder direction sketched in §7 would close this gap and is a precondition for a meaningful Hits@k number — until then, comparing to a benchmark that ranks entities is category-mistaken, not just unflattering.
2. **Open-world Wikidata, not closed-world benchmarks.** The 5M-triple slice has no held-out test set in the FB15k sense, and constructing one is non-trivial without leakage: Wikidata is open-world, the corpus is updated continuously, and the same predicate often has many correct values (a city has many `instance of` claims, all valid). The held-out set we *would* construct would be a soft top-k accuracy rather than a hard "correct/incorrect" split.
3. **What we report instead.** Perplexity (§5.2) is a per-token quantity that says how surprised the model is by the corpus on average; it is a substrate property, not a completion metric. The qualitative comparison (§5.3) is a 50-subject hand-audit; we acknowledge this is anecdotal and is presented as such. The right systematic evaluation, after the entity-decoder lands, is filtered Hits@k against a held-out wikidata snapshot constructed as the symmetric difference between two dump dates.

We treat MRR / Hits@k as *blocked future work*, gated on the entity-decoder, not as a comparison the paper sidesteps. The reproducibility supplement records the held-out construction we would run.

---

## 7. Discussion

The from-scratch training position (§2.3) coexists with a documented parallel near-term track admitting fine-tuning of a small base model (e.g., Qwen 2.5 1.5B-Instruct + QLoRA) under the same `propositionInferredFrom` output schema. The empirical case for the parallel track has softened across v6→v10: catalog-format hallucinations are gone, semantic-predicate share is at 100 % on the propgen test, and the cron loop ships new models without human intervention. The remaining failure mode in §6.2 (numeric-placeholder degenerations like `spouse -> "1 ."` and BPE-artifact leakage on Commons-category templates) might still be addressed faster by a fine-tuned 1B–3B parameter base model with English already encoded than by the from-scratch path waiting for corpus scale. We accept the provenance tradeoff this introduces — base-model pretraining is opaque — and record `propositionGeneratedBy "qwen-2.5-1.5b-loka-v1"` to track what was emitted by what.

Two larger questions are open:

**Where does the OWL layer live?** OWL ontologies are stored in the engine as triples but the engine does not reason. A reasonable role for OWL in the world-model loop is as a *prediction template*: an ontology declares "an instance of class C is expected to have properties P1, P2, P3 with values matching constraints X, Y, Z," and the inference loop reads the template, identifies expected-but-missing predicates for an entity, and predicts values for them. The OWL template becomes the *prompt* of a generative-citation inference call, and `propositionInferredFrom` cites the OWL declaration alongside the supporting context triples. We have not implemented this; it is the cleanest next step.

**What is the right output decoder?** The HNSW vector index in the engine is currently used for vector search (a separate feature) but could serve as a *decoder*: the model emits an embedding, HNSW resolves the nearest known IRI, and the IRI becomes the predicted object. This would close the gap between prediction in label-space and prediction in entity-space, eliminating cases like "metropolitan museum of museum" (decoded label) in favor of `<wd:Q160236>` (decoded entity). Open work.

---

## References

- Loka. *Loka / Loka — RDF-star triplestore with native HNSW vector indexing.* GitHub release `v0.4.0`, 2026. https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.0. Apache-2.0.
- Wikidata Foundation. *Wikidata.* https://www.wikidata.org/. CC0.
- philippesaade. *philippesaade/wikidata.* Hugging Face dataset, snapshot 2024-09-18. https://huggingface.co/datasets/philippesaade/wikidata. CC0.
- W3C. *RDF-star and SPARQL-star.* https://w3c.github.io/rdf-star/cg-spec/.
- Devlin, J., Chang, M.-W., Lee, K., Toutanova, K. *BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding.* NAACL 2019. (Masked-token-prediction substrate.)
- Vaswani, A., et al. *Attention is All You Need.* NeurIPS 2017. (Transformer architecture.)
- Bordes, A., et al. *Translating Embeddings for Modeling Multi-relational Data.* NeurIPS 2013. (TransE; comparison-only context for §2.2.)
- Yao, L., Mao, C., Luo, Y. *KG-BERT: BERT for Knowledge Graph Completion.* arXiv:1909.03193. (Transformer-on-KG comparison-only context.)
- Saxena, A., Kochsiek, A., Gemulla, R. *Sequence-to-Sequence Knowledge Graph Completion and Question Answering.* ACL 2022. (KGT5; comparison-only context.)

<!-- v0.4.0 — first clawRxiv submission cycle: 2026-05-09 -->