# Loka: Generative Citation in a Neuro-Symbolic World Model over RDF-Star Knowledge Graphs

**Code:** <https://github.com/EmmaLeonhart/Loka> (engine release `v0.4.0`: <https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.0>) &middot; **Model checkpoints:** <https://huggingface.co/datasets/EmmaLeonhart/loka> (snapshot tags `v3`–`v14`, plus per-epoch `v12.*` `v13.*` `v14.*`) &middot; **Normalized-Wikidata training corpora (v11+):** <https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata> (snapshot tags `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`) &middot; **Source dataset:** <https://huggingface.co/datasets/philippesaade/wikidata>

---

## Abstract

**Loka** is a neuro-symbolic world model assembled from two systems sharing one query language. The first is an RDF-star triplestore — explicit memory, exact answers. The second is a small role-aware transformer trained from scratch on the same triples, with English labels substituted for opaque entity identifiers — implicit memory, plausible answers. They compose at the SPARQL+ layer: a query reaches both systems and the caller does not pick which one answered, except by inspecting `propositionInferredFrom` provenance edges on each result.

The technical contribution is **generative citation**: a closed loop in which the transformer's predicted triples are written back into the triplestore as RDF-star annotations whose subject is the *quoted* generated triple and whose object is *another* quoted triple — a directly cited piece of context the prediction was conditioned on. A reserved system namespace (`http://loka.dev/provenance/`) marks every system-emitted predicate, enforced at three layers (corpus stripping, candidate filtering, emit-time guard) so the model never sees, learns to predict, or hallucinates a citation predicate. Hallucinated *citations* (the model picking the wrong context triple as support) are auditable and filterable like any other generated triple — they degrade like RDF rather than vanishing into opaque embeddings. Because the citation graph is explicit RDF-star, it is also *actionable*: a bounded cascade-retraction (§6.3) removes any node together with every generated inference that transitively cited it.

We demonstrate the end-to-end loop across a **twelve-version model progression (v3 → v14)**, each version shipped as a tagged Hugging Face snapshot with its training corpus. The four phases are: (i) word-level v3–v5 on a 5 M-triple Wikidata slice where capacity was the binding constraint (16 M → 44 M params, perplexities 92.5 → 84.85); (ii) BPE-tokenizer v6–v8, after a corpus-quality finding (below) took final perplexity to 64.65; (iii) cron-automated v9–v10 reaching **55.52** on a small clean slice; and (iv) the **normalized-wikidata scaling series v11–v14**, which holds architecture/tokenizer/config fixed and scales only the cleaned corpus from 350 k to 4.0 M triples, driving best-perplexity 279.12 → **202.01** (v14) — a corpus-scale result, not the earlier capacity result. v14 is finalized at its epoch-4 checkpoint; a bounded continuation (epochs 6–7) confirmed ~202 is the practical from-scratch floor on this hardware, with deeper results delegated to a clean-optimizer 10-epoch contributor run. Throughout, we characterize failure modes — mode collapse on connector tokens, mitigated by a *cumulative* repetition penalty at decode time — and document engine-level bugs surfaced by data scale.

The corpus-quality finding (§5.5): v6 produced confident catalog-format hallucinations (`ISNI -> 00000000`, `Freebase -> /m/0c__9`), and the catalog-format shape *leaked onto unrelated predicates* (`instance of -> + Ġof - 00 - 03 T 00`, a Wikidata date-prefix string, on 15 subjects in a single 30-source run). Investigation showed 49.6 % of the v6 corpus was Wikidata `external-id` predicates — ~80 % of all Wikidata property *types*. The v7 rebuild excludes external-ids plus 319 other catalog-shaped datatypes (`url`, `commonsMedia`, lexeme/sense/form, math, geo-shape) and normalises time/quantity literals; it is 24 % of v6 by volume but trains to a comparable perplexity *with the catalog-format hallucinations vanished* — the failure mode shifts from "confidently wrong" to "refuses to emit". v8 reaches ppl 64.65 with the loss curve still descending; v9 and v10 reach 57.15 and 55.52 on freshly-pulled 2 M-triple slices, with v10 emitting **zero catalog-format hallucinations** against v6's 21/52 on the standard Q42 propgen test, and v10 the first model shipped end-to-end by a 12-hour automated cron loop. v11–v14 then pivot pipeline shape: the corpus is built directly from `philippesaade/wikidata` (the SPARQL `LIMIT/OFFSET` extraction path was O(offset) on sled at multi-million scale) and published as a standalone Hugging Face dataset (`EmmaLeonhart/normalized-wikidata`).

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

4. **A case study on Wikidata.** A from-scratch masked-triple transformer series (v3–v14, all checkpoints and corpora released) exercises the loop end to end and yields a corpus-construction finding: Wikidata's catalog-identifier datatypes dominate a naive corpus and cause identifier-shaped hallucinations, which excluding them removes. (§5)

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

A note on what "citation" claims here. At v0, the cited context triples are not selected by the model's internal attention or by a learned retrieval head; they are the rows the inference loop's candidate-predicate selection (§4.4 step 1) conditioned on. The contribution we claim is the *schema* — the data shape that lets a model emit a triple together with a transparent, queryable, post-hoc-auditable record of which curated rows were considered for that prediction — not a learned mapping from prediction to evidence. We treat this as a v0 design choice, not a final position; §6.2 records the gap and §7 sketches the OWL-template and HNSW-decoder paths that would make the link mechanistic. The schema makes the gap auditable: a downstream consumer can SPARQL-star over the `propositionInferredFrom` edges and decide for themselves whether each citation is informative, regardless of what the model "actually" attended to.

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

The v3–v6 store (the corpus this section describes): 5,055,385 triples / 1,695,402 RDF-star annotations / 27,780 entities / 770 MB on-disk Loka store, every language label and description Wikidata has. This slice trained v3–v6. Later corpora were rebuilt with catalog datatypes excluded (§5.3), and from v11 on built by streaming the source dump directly (§5.5).

### 4.2 Label substitution

The model is trained on text, not URIs. The corpus extractor walks all `rdfs:label "..."@en` triples, builds a URI → English-label map, then writes each triple with each component resolved through the map:

| Raw triple | After substitution |
|---|---|
| `<wd:Q42> <wdt:P31> <wd:Q5>` | `Douglas Adams <TAB> instance of <TAB> human` |
| `<wd:Q1490> <wdt:P1448> "Tokyo"@en` | `Tokyo <TAB> official name <TAB> Tokyo` |
| `<wd:Q24> <wdt:P40> <wd:Q1049347>` | `Jack Bauer <TAB> child <TAB> Kim Bauer` |

Property labels missing from the store are fetched from Wikidata's public SPARQL endpoint and cached. Two preprocessing steps matter:

1. **Strip `^^<datatype>` suffixes from typed literals.** Loka's SPARQL serialization embeds the datatype URI in the literal value string (e.g., `"+1966-02-18T00:00:00Z\"^^<http://www.w3.org/2001/XMLSchema#dateTime>"`) rather than separating it as `datatype` metadata. Without stripping, datatype-URI fragments (`xmlschema`, `decimal`, `org`) reach the tokenizer as if they were entity content and dominate certain predictions (§5.2).

2. **Drop rows with non-IRI predicates.** RDF does not allow a literal in the predicate position, so any such row is malformed and is dropped.

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
| v6–v14 | 512 | 8 | 6 | 44,531,712 | 2–20 | Appendix A |

v3's low perplexity comes from memorising datatype-suffix tokens (§5.2). From v5 on the architecture is fixed, so later versions differ only in corpus, tokenizer (from v6) and training length.

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

## 5. Case study: a model series on cleaned Wikidata

We exercised the provenance loop with a series of from-scratch models (v3–v14) trained on progressively rebuilt Wikidata corpora. Every checkpoint and corpus is released (links at the top of the paper); Appendix A lists each version's tokenizer, corpus size, training length and perplexity. This section reports what the series taught about *corpus construction*, which is the result that transfers to other work training on Wikidata. It does not report completion accuracy; §6.3 explains why, and what that evaluation needs.

### 5.1 Setup

From v5 on, the architecture is fixed at the 44.5 M-parameter configuration of §4.3; from v6 on, the tokenizer is a fixed byte-level BPE vocabulary. Later versions therefore differ only in training corpus and training length. Perplexity is reported per version as a training diagnostic only. It is not comparable across the word-level (v3–v5) and BPE (v6+) tokenizers, and it is not a controlled measure across corpora, because successive corpora differ in content as well as size.

### 5.2 Datatype leakage

The first corpus serialised typed literals with their datatype IRI inside the literal string. A model trained on it (v3) learned to emit those fragments: `Abbas Mirza | has works in collection | 1 http www w3 org 2001 xmlschema decimal` at confidence 0.93. Stripping datatype suffixes before tokenization (§4.2) removed the pattern; the retrained model (v4) gives `metropolitan museum of museum` for the same query. v4's perplexity is *higher* than v3's (92.5 vs 53.4), because v3 had been earning cheap loss on high-frequency datatype tokens. Lower perplexity here signalled a worse corpus, which is why we treat perplexity as a diagnostic rather than an outcome.

### 5.3 Catalog noise

A behavioural test on the v6 model exposed a larger corpus problem. The test seeds a breadth-first, depth-3 Wikidata neighbourhood at `Q42` (183 entities, 14,586 triples), takes 30 high-PageRank source triples, generates up to 10 candidate predictions per source, and keeps those at confidence ≥ 0.25. It is a small, fixed probe, used to compare versions with each other, not a benchmark.

v6 produced confident identifier-shaped hallucinations: `British Broadcasting Corporation | ISNI -> "00000000"` (0.75), `Douglas Adams | Freebase ID -> "/ m / 0 c _ _ 9"` (0.43). The identifier *shape* also leaked onto unrelated predicates: `instance of -> "+ Ġof - 00 - 03 T 00"`, a Wikidata date prefix, appeared on 15 different subjects in one run.

The cause was corpus composition. Wikidata's `external-id` datatype, used for catalog cross-references (ISNI, GND, Freebase, LCCN, ...), covers 10,206 properties, about 80 % of all Wikidata properties, and 75.7 % of the rows in the v6 training file. We rebuilt the corpus excluding `external-id` and the other datatypes whose values carry no transferable content (`url`, `commonsMedia`, `math`, lexeme/sense/form, `globe-coordinate`, `geo-shape`, `musical-notation`, `tabular-data`, `wikibase-entity-schema`): 10,525 properties dropped, 2,231 kept (`wikibase-item`, `wikibase-property`, `quantity`, `string`, `time`, `monolingualtext`). Time and quantity literals were normalised at the same time (`+2012-10-15T00:00:00Z` → `2012-10-15`, `+1234` → `1234`). The full per-datatype specification ships with the code (`training/wikidata_excluded_predicates.json`).

The same probe across the versions trained on the cleaned corpora:

| | v6 (uncleaned) | v7 | v8 | v9 | v10 |
|---|---|---|---|---|---|
| Emissions at confidence ≥ 0.25 | 52 | 14 | 47 | 35 | 60 |
| on catalog-identifier predicates | 21 (40 %) | 9 (64 %) | 7 (15 %) | 1 (3 %) | 0 (0 %) |
| on semantic predicates | 31 (60 %) | 5 (36 %) | 40 (85 %) | 34 (97 %) | 60 (100 %) |
| `instance of` date-shape leak | 15 | 0 | 0 | 0 | 0 |

The date-shape leak disappears with the rebuild (v7) and does not return. v7 emits far less, because it no longer produces confident format strings; the failure mode moves from "confidently wrong" to "declines to emit", which is the right direction for a system whose output is stored. Longer training (v8) and fresh corpus slices (v9, v10) recover emission volume while the catalog share falls to zero. The v6 → v7 comparison is controlled (same architecture, tokenizer and epochs; only the corpus changed); the later columns also change training length or corpus slice, so they show the trend without isolating its cause.

### 5.4 Residual failure modes

The cleaned models fail in narrower, more consistent ways. Raw outputs below keep BPE artifacts visible (`Ġ` marks a word boundary; `âĢĵ` is a mis-decoded en dash):

| Subject / predicate | Output | Confidence | Failure |
|---|---|---|---|
| `Adams / different from` | `"Adams"` | 0.960 | circular: subject copied as object (v8) |
| `Joan of Arc / Commons category` | `"Joan Ġof ĠAr c Ġ( Ġ("` | 0.654 | template overfit plus BPE fragments (v8) |
| `Leonardo da Vinci / country of citizenship` | `"Polish âĢĵ"` | 0.677 | right type (nationality), wrong value (v8) |
| `– / spouse` | `"1 ."` | 0.50 | numeric-format degeneration (v10) |

Circular `different from` follows from the masked-role objective on a predicate that is mostly reflexive in the corpus. The Commons-category outputs follow the most frequent predicate template. Neither argues for restoring the excluded datatypes.

### 5.5 Building the corpus without the store

Extracting training triples from the store with paged SPARQL (`LIMIT`/`OFFSET`) costs time linear in the offset, which made a 50 M-triple extraction impractical. From v11 on, the corpus is built by streaming the source parquet directly: a first pass caches English labels, a second emits one tab-separated `subject predicate object` line per kept claim, with the same exclusions and normalisations as above and with reserved-namespace triples stripped. The resulting corpora are released as a separate dataset in four sizes (Appendix A). The provenance guarantee of §3.1 is unchanged: generated triples never enter a corpus, whichever path builds it.

---

## 6. Limitations

### 6.1 Model and decoding

- **Mode collapse on common tokens.** Even with the cumulative penalty, predictions for predicates the model knows weakly fall back to connectors (`of`, `and`) or to format placeholders (`spouse -> "1 ."`, §5.4). The corpus cleanup removed the worst of these (§5.3) but not all.
- **Label-space output.** The model emits subword tokens of an English label, not an entity IRI, so outputs can contain BPE fragments and cannot be checked against an entity identifier directly.
- **No beam search or top-p sampling.** Greedy top-1 only. Some failure cases would resolve with beam-2.

### 6.2 Provenance

- **Citation hallucination is structurally bounded but not zero.** A `propositionInferredFrom` row points at a concrete context triple, which is auditable, but the *choice* of which context triples to cite is heuristic (§4.4 step 1). The model is not actually inspecting these specific triples during prediction; the citation is "the candidate-predicate selection considered these triples." We document this tradeoff as accepted: the schema is honest about what it represents.

- **The provenance graph is actionable, not just auditable: cascade-retraction.** Because every generated triple carries `propositionInferredFrom` edges to the context it was conditioned on, the provenance graph supports a *retraction* operation: remove a node — real data or model-generated — and every generated inference that transitively cited it disappears with it. Propagation follows **only** `propositionInferredFrom` edges and is bounded to the reserved `http://loka.dev/provenance/` namespace, so an ordinary data edge is never mistaken for a derivation (real→real is not a dependency) and the traversal is cycle-safe. This ships end-to-end as a pure engine function (`retract_set`), a read-only preview endpoint, a commit-gated `POST /retract` + `retract_node` MCP tool, and a Loka Studio confirm action; the destructive path is opt-in (dry-run is the default at every surface). It depends on the store keeping a reverse index from each content-addressed quoted-triple id to its (s, p, o), without which a `propositionInferredFrom` source could not be dereferenced. This is the concrete payoff of the provenance schema beyond inspection: when a source is found to be wrong, the contaminated model output can be excised precisely rather than left as orphaned hallucination.

### 6.3 What we are *not* claiming, and why we do not report MRR / Hits@k

The dominant evaluation regime in transformer-on-KG completion (KG-BERT, KGT5, et al.) reports MRR and Hits@k against held-out triples on closed benchmarks like FB15k-237 or WN18RR. We do not report these numbers, and we want to be explicit about why — both so the gap is visible and so future work in the regime is well-scoped.

1. **Prediction space, not entity space.** Loka v0 emits *labels*, not entity IRIs. The model produces `"university of halle"` token-by-token, not `<wd:Q156667>`. MRR and Hits@k assume a finite candidate set of entities to rank; we have a vocabulary over English subword pieces (BPE in v6, word-level in v3–v5). The HNSW-as-decoder direction sketched in §7 would close this gap and is a precondition for a meaningful Hits@k number — until then, comparing to a benchmark that ranks entities is category-mistaken, not just unflattering.
2. **Open-world Wikidata, not closed-world benchmarks.** The 5M-triple slice has no held-out test set in the FB15k sense, and constructing one is non-trivial without leakage: Wikidata is open-world, the corpus is updated continuously, and the same predicate often has many correct values (a city has many `instance of` claims, all valid). The held-out set we *would* construct would be a soft top-k accuracy rather than a hard "correct/incorrect" split.
3. **What we report instead.** Perplexity (Appendix A) is a training diagnostic, not a completion metric. The behavioural probe of §5.3 is a small fixed test for comparing versions, not a benchmark. The right systematic evaluation, after the entity-decoder lands, is filtered Hits@k against a held-out wikidata snapshot constructed as the symmetric difference between two dump dates.

We treat MRR / Hits@k as *blocked future work*, gated on the entity-decoder, not as a comparison the paper sidesteps. The reproducibility supplement records the held-out construction we would run.

---

## 7. Discussion

The from-scratch training position (§2.3) coexists with a documented parallel near-term track admitting fine-tuning of a small base model (e.g., Qwen 2.5 1.5B-Instruct + QLoRA) under the same `propositionInferredFrom` output schema. The corpus cleanup (§5.3) removed the catalog-format hallucinations; the remaining failure modes (§5.4, §6.1: numeric-placeholder degenerations like `spouse -> "1 ."` and BPE-artifact leakage on Commons-category templates) might still be addressed faster by a fine-tuned 1B–3B parameter base model with English already encoded than by the from-scratch path waiting for corpus scale. We accept the provenance tradeoff this introduces — base-model pretraining is opaque — and record `propositionGeneratedBy "qwen-2.5-1.5b-loka-v1"` to track what was emitted by what.

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

---

## Appendix A. Model versions

All versions share the role-aware masked-triple objective of §4.3. Perplexity is on the training corpus at the released checkpoint; it is not comparable across the word-level and BPE tokenizers, nor across corpora (§5.1).

| Version | Params | Tokenizer | Training corpus (triples) | Epochs at release | Perplexity |
|---|---|---|---|---|---|
| v3 | 16.0 M | word | same slice, pre-fix extraction (§5.2) | 5 | 53.43 |
| v4 | 16.0 M | word | 757,592 | 5 | 92.48 |
| v5 | 44.5 M | word | 757,592 | 5 | 84.85 |
| v6 | 44.5 M | BPE | 757,592 (uncleaned, §5.3) | 5 | 194.98 |
| v7 | 44.5 M | BPE | 184,458 | 5 | 192.63 |
| v8 | 44.5 M | BPE | 184,458 | 20 | 64.65 |
| v9 | 44.5 M | BPE | 94,202 | 20 | 57.15 |
| v10 | 44.5 M | BPE | 94,058 | 20 | 55.52 |
| v11 | 44.5 M | BPE | 350,428 | 3 | 279.12 |
| v12 | 44.5 M | BPE | 671,817 | 6 | 250.82 |
| v13 | 44.5 M | BPE | 2,511,771 | 2 | 242.75 |
| v14 | 44.5 M | BPE | 4,021,409 | 4 | 202.01 |

v11–v14 are trained on the four released tiers of the streamed corpus (§5.5: `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`, named by entity rows read). The released checkpoint is the lowest-perplexity epoch kept, except v12, whose lower epoch-4 checkpoint (226.86) was not retained. Per-epoch checkpoints for v12–v14 are published separately.
