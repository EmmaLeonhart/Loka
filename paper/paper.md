# Loka: Retractable Provenance for Model-Generated Triples in an RDF-star Store

**Code:** <https://github.com/EmmaLeonhart/Loka> (engine release `v0.4.0`: <https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.0>) &middot; **Model checkpoints:** <https://huggingface.co/datasets/EmmaLeonhart/loka> (snapshot tags `v3`–`v14`, plus per-epoch `v12.*` `v13.*` `v14.*`) &middot; **Normalized-Wikidata training corpora (v11+):** <https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata> (snapshot tags `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`) &middot; **Source dataset:** <https://huggingface.co/datasets/philippesaade/wikidata>

---

## Abstract

Once model-generated statements are written into a knowledge graph next to curated data, it is hard to tell them apart, to keep them out of the next model's training data, or to remove them when a statement they depended on turns out to be wrong. We describe Loka, an RDF-star triplestore that stores model-predicted triples alongside curated ones and annotates each with RDF-star statements in a reserved namespace: the generating model, a confidence, and quoted pointers to the stored statements the prediction procedure took as input, which we call selection provenance. The namespace is enforced at corpus extraction, candidate selection and write time, so generated triples never re-enter a training corpus. Because these dependencies are explicit graph edges, the store supports cascade retraction: removing a node also removes every generated triple that transitively depended on one of its statements, without following ordinary data edges. We exercise the loop with a series of small transformers trained from scratch on label-substituted Wikidata triples, and report a corpus-construction finding: catalog-identifier properties made up three quarters of our initial training corpus and caused identifier-shaped hallucinations, which excluding them removed. Selection provenance records what the procedure used, not what the model relied on, and we make no claim about completion accuracy. Code, all checkpoints and the cleaned corpora are released.

---

## 1. Introduction

Knowledge graphs are increasingly extended by models: link predictors propose missing edges, and language models extract or generate statements. Once a predicted statement is stored, a typical triplestore holds it exactly like a curated one. Whatever provenance exists lives outside the graph, in pipeline logs or a separate metadata table. Three problems follow. A query cannot ask which answers came from a model. A model's output can flow into the training corpus of the next model. And when a source statement is found to be wrong, nothing in the store says which generated statements were derived from it.

RDF-star lets a triple be the subject of other triples. Loka uses this to keep provenance in the same graph as the data it describes. Every model-predicted triple is written together with RDF-star annotations, in a reserved namespace, recording that it was generated, by which model, with what confidence, and from which stored statements. These annotations can be queried with ordinary SPARQL-star, used to exclude generated statements from training corpora, and followed to retract everything that depended on a withdrawn statement.

We are specific about what the dependency edge means. In our prediction procedure, a candidate (subject, predicate) pair is proposed from the subject's existing statements and those of its graph neighbours, and the model then predicts an object from the subject and predicate labels alone. The `propositionInferredFrom` edges record subject statements that fed the proposal step. We call this *selection provenance*. It is a record of the procedure's inputs, which is what retraction needs; it is not evidence that the cited statements support the prediction, and not a record of what the model attended to.

### Contributions

1. **A reserved provenance namespace with three-layer exclusion.** Predicates under `http://loka.dev/provenance/` are system-only. The corpus extractor, the candidate selector and the writer each refuse them independently, so generated statements and their annotations never re-enter a training corpus even if one guard regresses. (§3.1)

2. **An RDF-star annotation schema for generated triples.** Each generated triple carries a fixed block of annotations on the quoted triple: a generated flag, the model version, a confidence, and selection-provenance edges whose objects are themselves quoted triples. (§3.2)

3. **Cascade retraction.** Removing a node removes its statements and every generated triple that transitively depended on them. Traversal follows only selection-provenance edges and stays inside the reserved namespace, so ordinary data edges are never treated as dependencies. (§6.2)

4. **A case study on Wikidata.** A from-scratch masked-triple transformer series (v3–v14, all checkpoints and corpora released) exercises the loop end to end and yields a corpus-construction finding: Wikidata's catalog-identifier datatypes dominate a naive corpus and cause identifier-shaped hallucinations, which excluding them removes. (§5)

---

## 2. Background and related work

### 2.1 RDF-star

RDF-star extends RDF so that a triple can appear, *quoted*, as the subject or object of another triple (Hartig, 2017; W3C RDF-star Community Group). The notation `<<s p o>>` denotes the triple s p o as a term, without asserting it. This admits direct annotation of statements:

```
:Tokyo  :population  "13929286" .
<<:Tokyo :population "13929286">>  :measuredAt  "2020-01-01" .
<<:Tokyo :population "13929286">>  :statedIn    :census2020 .
```

Wikidata expresses the same information through reified statement nodes carrying qualifiers and references (Vrandečić and Krötzsch, 2014); RDF-star collapses them into one structural primitive. Loka interns a quoted triple to a content-addressed identifier, `quoted_triple_id(s, p, o)`, a hash of the three component ids, and keeps a reverse index from that identifier to (s, p, o) so that a quoted triple can be rendered and dereferenced.

### 2.2 Provenance in RDF

Recording where statements come from is an old concern of the Semantic Web. Named graphs attach provenance and trust information to sets of triples (Carroll et al., 2005), and the W3C PROV-O ontology gives a vocabulary for entities, activities and agents and their derivation relations (Lebo et al., 2013). Both operate at the granularity of a graph or an explicit provenance resource. We use RDF-star to attach provenance to individual triples directly, with a small fixed vocabulary for one case, model-generated statements; mapping it onto PROV-O terms (e.g. `prov:wasDerivedFrom` for `propositionInferredFrom`) is straightforward and left to consumers. What we add is not a provenance vocabulary but an operational use of it inside the store: exclusion from training corpora and cascade retraction.

### 2.3 Knowledge-graph completion

Link prediction scores candidate completions of (subject, predicate, ?) queries. Embedding models such as TransE (Bordes et al., 2013) and RotatE (Sun et al., 2019) learn entity and relation vectors; transformer-based models score triples as text (KG-BERT; Yao et al., 2019) or generate the missing entity as a sequence (KGT5; Saxena et al., 2022). The standard evaluation reports mean reciprocal rank and Hits@k in the filtered setting, which removes other known true answers before ranking (Bordes et al., 2013); we follow it, with the tie-aware rank of Berrendorf et al. (2020), and use PyKEEN (Ali et al., 2021) for the TransE baseline. These systems output scores or ranked candidates. Writing the chosen completions back into the graph, with a record of what they were derived from, is outside their scope, and is the part this paper addresses.

### 2.4 Attribution for generated content

Retrieval-augmented generation conditions a language model on retrieved passages (Lewis et al., 2020), and attributed question answering asks a model to return evidence supporting its answer and evaluates whether the evidence does support it (Bohnet et al., 2022). Those lines of work aim at support: the cited source should justify the output. Selection provenance makes a weaker, procedural claim: the cited statements were the input to the procedure that produced the output. That is enough for retraction, which needs to know what an output depended on, and it is not a claim of support (§6.2).

### 2.5 Training from scratch

The models in our case study are trained from scratch on triples, not fine-tuned from a pretrained language model. With a pretrained model, a generated triple can draw on pretraining data the store knows nothing about, so its provenance record would be incomplete by construction; training only on the corpus keeps everything the model learned inside a known, released dataset. The schema itself does not depend on this choice.

---

## 3. Architecture

### 3.1 The reserved provenance namespace

Every predicate under `http://loka.dev/provenance/` is system-internal. The names are deliberately verbose — `propositionGeneratedFrom` rather than `generatedFrom` — so a human scanning raw triples spots them at a glance and accidental collision with real-world predicates is vanishingly unlikely. The full namespace currently holds:

| Predicate | Object type | Meaning |
|---|---|---|
| `propositionGenerated` | `xsd:boolean` | This triple was emitted by a model (not curated). |
| `propositionGeneratedBy` | string | The model version (e.g., `wikidata_v4`) that emitted it. |
| `propositionConfidence` | `xsd:decimal` | Mean per-token softmax probability of the prediction. |
| `propositionInferredFrom` | quoted triple | A stored statement the prediction procedure took as input (selection provenance). |
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

### 3.2 Annotating generated triples

Selection provenance records the stored statements that the prediction procedure took as input (§4.4), not statements the model attended to or that support the prediction (§1). The schema does not depend on that choice: a procedure that selected its evidence differently, or a model that retrieved it, would write the same block.

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

`prov:` abbreviates the reserved namespace. The `propositionInferredFrom` objects are up to ten of the subject's existing statements, the input to the candidate selector (§4.4). Wikidata qualifiers and references are imported with the same RDF-star pattern (a quoted statement as subject), so curated and generated annotations share one shape.

Because the `propositionInferredFrom` objects are written by the procedure, not generated by the model, they always point at statements that exist in the store. What they cannot guarantee is relevance: a cited statement may have had no bearing on the predicted object (§6.2).

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

Source: `philippesaade/wikidata` on Hugging Face, a CC0 conversion of a Wikidata JSON dump with one row per entity, each row a JSON-shaped record with labels (every language), descriptions, sitelinks, and claims. We stream via the `datasets` library, converting each entity to N-Triples-star form: one main triple per claim, plus one RDF-star annotation per qualifier and per reference, all sharing the same `<<S P O>>` quoted-triple subject. Wikidata's `pq:` (qualifier) and `pr:` (reference) namespaces collapse into the same `wdt:` predicate URI on the annotation row — the qualifier-vs-reference distinction is structural (subject is a quoted triple), not lexical.

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

Architecture: a role-aware Transformer encoder (Vaswani et al., 2017). Each triple is tokenized as

```
[CLS] s_tokens [SEP_S] p_tokens [SEP_P] o_tokens [SEP_O]
```

Token + position + role embeddings sum at each position, where the role is one of `{SPECIAL, S, P, O}`. The classification head is tied to the input embedding for parameter efficiency.

Training objective: masked-token prediction in the style of BERT (Devlin et al., 2019), applied per role: pick one role (S, P, or O) at random per example, mask its tokens with `[MASK]`, predict the originals. Cross-entropy on the masked positions, AdamW, 3e-4 LR, β=(0.9, 0.95), weight decay 0.01, gradient clipping at 1.0. Standard.

Three model sizes at this corpus size:

| Model | d_model | nhead | layers | params | epochs | final ppl |
|---|---|---|---|---|---|---|
| v3 (early; pre-cleanup) | 256 | 8 | 4 | 16,012,800 | 5 | 53.43 |
| v4 (early; cleaned) | 256 | 8 | 4 | 15,967,744 | 5 | 92.48 |
| v5 (early; capacity scale-up) | 512 | 8 | 6 | 44,531,712 | 5 | 84.85 |
| v6–v14 | 512 | 8 | 6 | 44,531,712 | 2–20 | Appendix A |

v3's low perplexity comes from memorising datatype-suffix tokens (§5.2). From v5 on the architecture is fixed, so later versions differ only in corpus, tokenizer (from v6) and training length.

### 4.4 Inference with selection provenance

For each candidate subject in the corpus:

1. **Candidate predicate selection.** Find graph-neighbors — subjects sharing at least one (predicate, object-key) tuple with this one — and rank predicates they have but the candidate subject lacks. Cap at *N* candidates per subject (default 5).
2. **Masked decoding with cumulative repetition penalty.** Build the input as `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] [MASK]^k [SEP_O]`. At each masked position, the model emits a logit distribution. We apply:

   - Hard skip-set: special tokens never win.
   - Cumulative repetition penalty, a per-occurrence variant of the penalty of Keskar et al. (2019): `logit[t] /= penalty^count[t]` where `count[t]` is the number of times `t` has already been emitted in this sequence. Default `penalty = 3.0`.
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

- **Selection provenance is not support.** A `propositionInferredFrom` row points at a concrete context triple, which is auditable, but the *choice* of which context triples to cite is heuristic (§4.4 step 1). The model does not see these statements during prediction; it sees only the subject and predicate labels. The cited set is the first ten of the subject's statements, while the selector reads all of them. So for a subject with more than ten statements, retraction can miss a real dependency, and for any subject a cited statement may have played no part in the proposal. Both are properties of the current procedure, not of the schema; citing exactly the statements that matched a neighbour would fix both.

- **The provenance graph is actionable, not just auditable: cascade-retraction.** Because every generated triple carries `propositionInferredFrom` edges to the statements its prediction procedure took as input, the provenance graph supports a *retraction* operation: remove a node — real data or model-generated — and every generated inference that transitively cited it disappears with it. Propagation follows **only** `propositionInferredFrom` edges and is bounded to the reserved `http://loka.dev/provenance/` namespace, so an ordinary data edge is never mistaken for a derivation (real→real is not a dependency) and the traversal is cycle-safe. This ships end-to-end as a pure engine function (`retract_set`), a read-only preview endpoint, a commit-gated `POST /retract` + `retract_node` MCP tool, and a Loka Studio confirm action; the destructive path is opt-in (dry-run is the default at every surface). It depends on the store keeping a reverse index from each content-addressed quoted-triple id to its (s, p, o), without which a `propositionInferredFrom` source could not be dereferenced. This is the concrete payoff of the provenance schema beyond inspection: when a source is found to be wrong, the contaminated model output can be excised precisely rather than left as orphaned hallucination.

### 6.3 What we are *not* claiming, and why we do not report MRR / Hits@k

The dominant evaluation regime in transformer-on-KG completion (KG-BERT, KGT5, et al.) reports MRR and Hits@k against held-out triples on closed benchmarks like FB15k-237 or WN18RR. We do not report these numbers, and we want to be explicit about why — both so the gap is visible and so future work in the regime is well-scoped.

1. **Prediction space, not entity space.** Loka v0 emits *labels*, not entity IRIs. The model produces `"university of halle"` token-by-token, not `<wd:Q156667>`. MRR and Hits@k assume a finite candidate set of entities to rank; we have a vocabulary over English subword pieces (BPE in v6, word-level in v3–v5). The HNSW-as-decoder direction sketched in §7 would close this gap and is a precondition for a meaningful Hits@k number — until then, comparing to a benchmark that ranks entities is category-mistaken, not just unflattering.
2. **Open-world Wikidata, not closed-world benchmarks.** The 5M-triple slice has no held-out test set in the FB15k sense, and constructing one is non-trivial without leakage: Wikidata is open-world, the corpus is updated continuously, and the same predicate often has many correct values (a city has many `instance of` claims, all valid). The held-out set we *would* construct would be a soft top-k accuracy rather than a hard "correct/incorrect" split.
3. **What we report instead.** Perplexity (Appendix A) is a training diagnostic, not a completion metric. The behavioural probe of §5.3 is a small fixed test for comparing versions, not a benchmark. The right systematic evaluation, after the entity-decoder lands, is filtered Hits@k against a held-out wikidata snapshot constructed as the symmetric difference between two dump dates.

We treat MRR / Hits@k as *blocked future work*, gated on the entity-decoder, not as a comparison the paper sidesteps. The reproducibility supplement records the held-out construction we would run.

---

## 7. Discussion

The from-scratch training position (§2.5) coexists with a documented parallel near-term track admitting fine-tuning of a small base model (e.g., Qwen 2.5 1.5B-Instruct + QLoRA) under the same `propositionInferredFrom` output schema. The corpus cleanup (§5.3) removed the catalog-format hallucinations; the remaining failure modes (§5.4, §6.1: numeric-placeholder degenerations like `spouse -> "1 ."` and BPE-artifact leakage on Commons-category templates) might still be addressed faster by a fine-tuned 1B–3B parameter base model with English already encoded than by the from-scratch path waiting for corpus scale. We accept the provenance tradeoff this introduces — base-model pretraining is opaque — and record `propositionGeneratedBy "qwen-2.5-1.5b-loka-v1"` to track what was emitted by what.

Two larger questions are open:

**Where does the OWL layer live?** OWL ontologies are stored in the engine as triples but the engine does not reason. A reasonable role for OWL in the prediction loop is as a *prediction template*: an ontology declares "an instance of class C is expected to have properties P1, P2, P3 with values matching constraints X, Y, Z," and the inference loop reads the template, identifies expected-but-missing predicates for an entity, and predicts values for them. The OWL template becomes the *prompt* of a generative-citation inference call, and `propositionInferredFrom` cites the OWL declaration alongside the supporting context triples. We have not implemented this; it is the cleanest next step.

**What is the right output decoder?** The HNSW vector index in the engine is currently used for vector search (a separate feature) but could serve as a *decoder*: the model emits an embedding, HNSW resolves the nearest known IRI, and the IRI becomes the predicted object. This would close the gap between prediction in label-space and prediction in entity-space, eliminating cases like "metropolitan museum of museum" (decoded label) in favor of `<wd:Q160236>` (decoded entity). Open work.

---

## References

- Ali, M., Berrendorf, M., Hoyt, C. T., Vermue, L., Sharifzadeh, S., Tresp, V., Lehmann, J. *PyKEEN 1.0: A Python Library for Training and Evaluating Knowledge Graph Embeddings.* Journal of Machine Learning Research 22(82):1–6, 2021.
- Berrendorf, M., Faerman, E., Vermue, L., Tresp, V. *On the Ambiguity of Rank-Based Evaluation of Entity Alignment or Link Prediction Methods.* arXiv:2002.06914, 2020.
- Bohnet, B., Tran, V. Q., Verga, P., Aharoni, R., Andor, D., Baldini Soares, L., Ciaramita, M., et al. *Attributed Question Answering: Evaluation and Modeling for Attributed Large Language Models.* arXiv:2212.08037, 2022.
- Bordes, A., Usunier, N., Garcia-Durán, A., Weston, J., Yakhnenko, O. *Translating Embeddings for Modeling Multi-relational Data.* Advances in Neural Information Processing Systems 26 (NIPS 2013).
- Carroll, J. J., Bizer, C., Hayes, P., Stickler, P. *Named Graphs, Provenance and Trust.* Proceedings of the 14th International World Wide Web Conference (WWW 2005), 613–622.
- Devlin, J., Chang, M.-W., Lee, K., Toutanova, K. *BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding.* NAACL 2019. arXiv:1810.04805.
- Hartig, O. *Foundations of RDF\* and SPARQL\* — An Alternative Approach to Statement-Level Metadata in RDF.* Proceedings of the 11th Alberto Mendelzon International Workshop on Foundations of Data Management (AMW 2017).
- Keskar, N. S., McCann, B., Varshney, L. R., Xiong, C., Socher, R. *CTRL: A Conditional Transformer Language Model for Controllable Generation.* arXiv:1909.05858, 2019.
- Lebo, T., Sahoo, S., McGuinness, D. (eds.). *PROV-O: The PROV Ontology.* W3C Recommendation, 30 April 2013. https://www.w3.org/TR/prov-o/.
- Lewis, P., Perez, E., Piktus, A., Petroni, F., Karpukhin, V., Goyal, N., et al. *Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks.* NeurIPS 2020. arXiv:2005.11401.
- Saxena, A., Kochsiek, A., Gemulla, R. *Sequence-to-Sequence Knowledge Graph Completion and Question Answering.* Proceedings of ACL 2022 (Volume 1: Long Papers), 2814–2828.
- Sun, Z., Deng, Z.-H., Nie, J.-Y., Tang, J. *RotatE: Knowledge Graph Embedding by Relational Rotation in Complex Space.* ICLR 2019. arXiv:1902.10197.
- Vaswani, A., Shazeer, N., Parmar, N., Uszkoreit, J., Jones, L., Gomez, A. N., Kaiser, Ł., Polosukhin, I. *Attention Is All You Need.* NeurIPS 2017. arXiv:1706.03762.
- Vrandečić, D., Krötzsch, M. *Wikidata: A Free Collaborative Knowledgebase.* Communications of the ACM 57(10):78–85, 2014.
- W3C RDF-star Community Group. *RDF-star and SPARQL-star.* Final Community Group Report. https://w3c.github.io/rdf-star/cg-spec/.
- Yao, L., Mao, C., Luo, Y. *KG-BERT: BERT for Knowledge Graph Completion.* arXiv:1909.03193, 2019.

**Software and data.**

- Loka engine, release `v0.4.0`. https://github.com/EmmaLeonhart/Loka. Apache-2.0.
- Model checkpoints: https://huggingface.co/datasets/EmmaLeonhart/loka. Training corpora: https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata.
- Source data: philippesaade, *wikidata*, Hugging Face dataset, CC0. https://huggingface.co/datasets/philippesaade/wikidata. Streamed for the v11–v14 corpora in May 2026; the dataset revision was not pinned, and the dataset has been updated since.

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
