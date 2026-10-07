# Loka: Retractable Provenance for Model-Generated Triples in an RDF-star Store

**Code:** <https://github.com/EmmaLeonhart/Loka> (release `v0.4.4`: <https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.4>) &middot; **Model checkpoints:** <https://huggingface.co/datasets/EmmaLeonhart/loka> (snapshot tags `v3`–`v14`, plus per-epoch `v12.*` `v13.*` `v14.*`) &middot; **Normalized-Wikidata training corpora (v11+):** <https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata> (snapshot tags `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`) &middot; **Source dataset:** <https://huggingface.co/datasets/philippesaade/wikidata>

---

## Abstract

Once model-generated statements are written into a knowledge graph next to curated data, it is hard to tell them apart, to keep them out of the next model's training data, or to remove them when a statement they depended on turns out to be wrong. We describe Loka, an RDF-star triplestore that stores model-predicted triples alongside curated ones and annotates each with RDF-star statements in a reserved namespace: the generating model, a confidence, and quoted pointers to the stored statements the prediction procedure took as input, which we call selection provenance. The namespace is enforced at corpus extraction, candidate selection and write time, so generated triples never re-enter a training corpus. Because these dependencies are explicit graph edges, the store supports cascade retraction: removing a node also removes every generated triple that transitively depended on one of its statements, without following ordinary data edges. We exercise the loop with a series of small transformers trained from scratch on label-substituted Wikidata triples and use them to evaluate the provenance machinery on real data. Tested against an independent reference, retraction computes a 106k-triple removal from a 5M-row store in about 0.1 s. Selection provenance records what the procedure used, not what the model relied on, and on held-out triples the model does not beat a predicate-frequency baseline: our claims concern the provenance machinery, not the model. Code, all checkpoints and the cleaned corpora are released.

---

## 1. Introduction

Knowledge graphs are increasingly extended by models: link predictors propose missing edges, and language models extract or generate statements. Once a predicted statement is stored, a typical triplestore holds it exactly like a curated one. Whatever provenance exists lives outside the graph, in pipeline logs or a separate metadata table. Three problems follow. A query cannot ask which answers came from a model. A model's output can flow into the training corpus of the next model. And when a source statement is found to be wrong, nothing in the store says which generated statements were derived from it.

RDF-star lets a triple be the subject of other triples. Loka uses this to keep provenance in the same graph as the data it describes. Every model-predicted triple is written together with RDF-star annotations, in a reserved namespace, recording that it was generated, by which model, with what confidence, and from which stored statements. These annotations can be queried with ordinary SPARQL-star, used to exclude generated statements from training corpora, and followed to retract everything that depended on a withdrawn statement.

We are specific about what the dependency edge means. In our prediction procedure, a candidate (subject, predicate) pair is proposed from the subject's existing statements and those of its graph neighbours, and the model then predicts an object from the subject and predicate labels alone. The `propositionInferredFrom` edges record exactly the subject's statements whose match with a neighbour led to the proposal, plus one statement of each such neighbour. We call this *selection provenance*. It is a record of the procedure's inputs, which is what retraction needs; it is not evidence that the cited statements support the prediction, and not a record of what the model attended to.

### Contributions

1. **A reserved provenance namespace with three-layer exclusion.** Predicates under `http://loka.dev/provenance/` are system-only. The corpus extractor, the candidate selector and the writer each refuse them independently, so generated statements and their annotations never re-enter a training corpus even if one guard regresses. (§3.1)

2. **An RDF-star annotation schema for generated triples.** Each generated triple carries a fixed block of annotations on the quoted triple: a generated flag, the model version, a confidence, and selection-provenance edges whose objects are themselves quoted triples. (§3.2)

3. **Cascade retraction.** Removing a node removes its statements and every generated triple that transitively depended on them. Traversal follows only selection-provenance edges and stays inside the reserved namespace, so ordinary data edges are never treated as dependencies. We test it against an independent reference implementation and measure its cost. (§3.4, §6.1–6.2)

4. **A case study on Wikidata.** A from-scratch masked-triple transformer series (v3–v14, all checkpoints and corpora released) exercises the loop end to end on real data. (§5)

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

Retrieval-augmented generation conditions a language model on retrieved passages (Lewis et al., 2020), and attributed question answering asks a model to return evidence supporting its answer and evaluates whether the evidence does support it (Bohnet et al., 2022). Those lines of work aim at support: the cited source should justify the output. Selection provenance makes a weaker, procedural claim: the cited statements were the input to the procedure that produced the output. That is enough for retraction, which needs to know what an output depended on, and it is not a claim of support (§7.2).

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
   ... (one row per subject statement the proposal depended on)
```

`prov:` abbreviates the reserved namespace. The `propositionInferredFrom` objects are the subject's statements whose (predicate, object) pair matched a neighbour that has the proposed predicate, plus, for each such neighbour, its statement carrying the proposed predicate (§4.4). Wikidata qualifiers and references are imported with the same RDF-star pattern (a quoted statement as subject), so curated and generated annotations share one shape.

Because the `propositionInferredFrom` objects are written by the procedure, not generated by the model, they always point at statements that exist in the store. What they cannot guarantee is relevance: a cited statement may have had no bearing on the predicted object (§7.2).

### 3.3 The loop

The store and the model form a closed loop:

1. Curated triples (here, Wikidata) are loaded into the store as RDF-star.
2. A training corpus is extracted from them, with labels substituted for identifiers (§4.2) and every generated triple excluded (§3.1).
3. A role-aware transformer is trained on the corpus (§4.3).
4. The inference procedure proposes (subject, predicate) pairs, the model predicts objects, and each accepted prediction is written back to the store with its annotation block (§3.2, §4.4).

Generated triples land in the store flagged `propositionGenerated true`, and the next corpus extraction's SPARQL-star filter excludes them, so the model never trains on its own output. Inference can be re-run to add predictions without polluting the training distribution.

### 3.4 Cascade retraction

Because every generated triple carries `propositionInferredFrom` edges to the statements its prediction procedure took as input, the store can retract by dependency. Given a node to remove, `retract_set` computes:

1. **Depth 0:** every triple whose subject or object is the node. A generated triple among them takes its reserved-namespace annotation rows with it.
2. **Closure:** for each removed triple *T*, every generated triple *G* with an annotation `<<G>> propositionInferredFrom <<T>>` is removed together with all of *G*'s reserved-namespace annotations, and the step repeats on *G*.

Traversal follows only `propositionInferredFrom` and only sweeps predicates under the reserved namespace, so an ordinary data edge is never treated as a dependency: retracting a curated entity does not chase its curated neighbours. A triple is processed at most once, so cycles in the citation graph terminate. Dereferencing `<<T>>` requires the reverse index from content-addressed quoted-triple ids to (s, p, o) described in §2.1. The computation is read-only; the engine exposes it as a preview, and deletion is a separate, explicitly confirmed operation (dry run is the default at every interface). §6.1–6.2 evaluate its correctness and cost.

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

1. **Candidate predicate selection.** Find graph-neighbors — subjects sharing at least one (predicate, object-key) tuple with this one — and rank predicates they have but the candidate subject lacks. Cap at *N* candidates per subject (default 5). For each candidate, record which of the subject's statements produced a matching neighbour, and one statement of each contributing neighbour (its first statement with the proposed predicate); these become its selection provenance. One statement per neighbour suffices because retraction removes whole nodes: retracting a neighbour removes the cited statement and so reaches the prediction. The resulting volume grows with the neighbourhood: on the 15k-triple graph of §6.3 a prediction cites a median of 2 subject-side and a few neighbour statements, but on the 153k-triple graph of §6.2 the medians are 23 subject-side and 82 neighbour-side citations (means 67 and 54), because in a larger graph almost every statement of a subject matches some neighbour. We keep the record complete rather than capping it, since a cap would make retraction silently incomplete; a cheaper complete encoding would point at neighbour nodes rather than statements (§8).
2. **Masked decoding with cumulative repetition penalty.** Build the input as `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] [MASK]^k [SEP_O]`. At each masked position, the model emits a logit distribution. We apply:

   - Hard skip-set: special tokens never win.
   - Cumulative repetition penalty, a per-occurrence variant of the penalty of Keskar et al. (2019): `logit[t] /= penalty^count[t]` where `count[t]` is the number of times `t` has already been emitted in this sequence. Default `penalty = 3.0`.
   - Per-token confidence floor: emission halts when the top-token probability falls below 0.05.

   Greedy top-1 selection, no beam search.
3. **Confidence-thresholded emit.** Mean per-token probability is the prediction's confidence. If confidence ≥ threshold (default 0.4) and the predicted object is not a duplicate of an existing fact for this (S, P), emit the RDF-star block (§3.2).
4. **Entity resolution.** If the predicted label, normalised (BPE word-boundary markers to spaces, case folded), equals the label of exactly one entity in the store, the object is written as that entity's IRI and the raw output is kept under `propositionPredictedLabel`; otherwise the object stays a literal. On the 153k-triple graph this resolved 1 of the 371 predictions over both passes of §6.2: the model's outputs rarely equal an entity label exactly.
5. **Optional `--post`.** Write the emitted N-Triples-star to the live Loka store via `POST /triples`. Subsequent training-corpus extractions exclude these via the SPARQL-star FILTER from §3.1.

The cumulative penalty matters: a *non*-cumulative penalty (set membership) was tested first and failed to break loops on dominant common tokens because the penalty applied only once regardless of how many times the token had already won. With cumulative, three emissions of `of` at penalty 3.0 multiply its divisor by 27 and reliably drop it below the floor, breaking the cascade.

---

## 5. Case study: a model series on cleaned Wikidata

We exercised the provenance loop with a series of from-scratch models (v3–v14) trained on progressively rebuilt Wikidata corpora. Every checkpoint and corpus is released (links at the top of the paper); Appendix A lists each version's tokenizer, corpus size, training length and perplexity. This section records how the training corpus was built and what went wrong along the way, since the models and corpora are released and reused below. Completion accuracy is evaluated separately, against baselines, in §6.4.

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

## 6. Evaluation

### 6.1 Retraction correctness

We test `retract_set` against an independent reference on randomly generated provenance graphs. The generator creates curated triples and generated triples; each generated triple cites one to three earlier triples (curated or generated), about 5 % also cite a *later* generated triple so that the citation graph contains cycles, and each carries a `propositionGeneratedBy` annotation. The reference computes the intended closure (§3.4) by fixpoint iteration over the generator's own lists, without using the store's indexes. We compare the two on 50 small graphs (60 curated and 60 generated triples, 10 roots each) and 10 medium graphs (2,000 and 2,000, 20 roots each).

The comparison found a defect on its first run. When the retracted node was itself the subject or object of a generated triple, that triple was removed at depth 0, but its annotation rows were swept only for generated triples reached through a provenance edge, so they remained as annotations on a triple no longer in the store. Every discrepancy was of this kind. After the fix (depth 0 now sweeps the annotations of its own generated rows, as §3.4 states) the engine and the reference agree on all 700 roots, and a unit test covering the case fails without the fix.

### 6.2 Retraction cost

We time `retract_set` with criterion on generated graphs of four sizes, using the same generator (curated and generated triples in equal number, entities = a quarter of that number). The root is the entity with the largest retraction set among a fixed sample of 50. Single laptop, release build. The store is the in-memory index that the engine computes retractions against in every mode, including the server, which mirrors writes to its persistent store:

| Generated triples | Store rows | Triples removed | Max depth | Median time |
|---|---|---|---|---|
| 1,000 | 5,034 | 1,439 | 14 | 0.41 ms |
| 10,000 | 50,484 | 7,367 | 24 | 2.97 ms |
| 100,000 | 504,389 | 120,461 | 34 | 95.1 ms |
| 1,000,000 | 5,050,435 | 106,255 | 39 | 92.1 ms |

Time follows the size of the removed set, not the size of the store, as the algorithm's per-triple index lookups predict: the 5M-row store and the 0.5M-row store take the same time for removals of similar size, about 0.8 µs per removed triple. All four sizes come from one run; an earlier run of the same code on the same laptop measured the three smaller sizes about 1.6× faster, so absolute times vary with machine state by that much.

On real data, we pulled a larger breadth-first Wikidata neighbourhood of `Q42` (155,324 triples, 983 entities; 153,185 imported), ran the v13 model's inference over every subject, posted its 281 predictions, and ran inference a second time with those predictions visible as context, which produced 90 more, 83 of them citing a first-pass prediction. The store then held real dependency chains of two generated hops (371 generated triples, 39,470 annotation rows). For each of the 983 entities we called the server's retraction preview and compared it with an independently computed closure: a generated triple must be removed if it touches the entity, cites a statement touching the entity, or cites a removed generated triple. Over 27,142 such required removals, every generated triple and every one of its annotation rows was in the returned set, and no generated triple outside the closure was. Over HTTP the preview took a median of 3.0 ms (p95 306 ms, maximum 1.04 s) and removed a median of 182 triples (maximum 38,122).

### 6.3 Encoding cost against reification and named graphs

We compare the annotation block with the two standard ways of attaching provenance to individual statements in plain RDF, using real data. We pulled a breadth-first Wikidata neighbourhood of `Q42` (14,819 triples, 169 entities), loaded it, and ran the v13 model's inference over every subject, which wrote 59 generated triples with 286 selection-provenance edges (208 distinct cited statements). The same generated set was then re-encoded (a) as standard RDF reification with PROV-O, where each generated triple gets an `rdf:Statement` node carrying the metadata and `prov:wasDerivedFrom` links to `rdf:Statement` nodes for the cited statements, and (b) as one named graph per generated triple, with the cited statements still reified so they can be pointed at.

| Encoding | Rows | Bytes (N-Triples / N-Quads) |
|---|---|---|
| RDF-star annotation block (ours) | **522** | **122,861** |
| Reification + PROV-O | 1,590 | 211,419 |
| Named graph per prediction | 1,354 quads | 182,910 |

Most of the difference is the cost of making a curated statement citable: RDF-star quotes it in place, while the other encodings need four extra rows per cited statement.

We also asked both triple-based stores the same question for each of the 169 entities X, "which generated triples cite a statement whose object is X?" (a nested SPARQL-star pattern on one side, a five-pattern join over reification nodes on the other). Both returned identical answers for all 169 entities, with the stores reloaded from disk. Median latency over HTTP was 0.92 ms for the RDF-star query and 0.79 ms for the reified join (p95 1.29 and 1.18 ms). An earlier executor took 5.14 ms on the RDF-star query, because it found quoted subjects by hashing every stored triple; it now walks the rows of the bound annotation predicate and dereferences each quoted subject through the reverse index (§2.1). The RDF-star encoding is three times smaller and answers this query within 0.13 ms of the reified one. Loka has no named-graph support, so (b) was counted but not queried.

### 6.4 Link prediction

To place the case-study model against standard baselines, we evaluate it on held-out triples. The corpus tiers are prefixes of one stream, so the `v14-1M` corpus contains triples the v13 model never trained on. We keep those whose subject, predicate and object labels all occur in v13's training corpus (the transductive setting): 28,448 unique triples over 542 predicates. For each held-out (s, p, o) we rank candidate objects for (s, p, ?). Candidates are the objects seen with p in training, which excludes 8,762 queries whose true object never occurs with p in training and leaves 19,686. We use the filtered setting (other known true objects of (s, p), from training or held-out data, are removed) and the tie-aware rank of Berrendorf et al. (2020).

The model scores a candidate of L tokens by masking L object positions, exactly as in training, and summing the candidate tokens' log-probabilities; one forward pass per (query, L) scores all candidates of that length. We compare with a predicate-frequency baseline (rank by how often the candidate occurs as an object of p in training) and with TransE (Bordes et al., 2013) trained with PyKEEN (Ali et al., 2021) on the same training triples, with entities identified by label as in the transformer's corpus (dimension 128, 20 epochs, Adam, learning rate 0.001, batch 4,096, PyKEEN defaults otherwise). No model was tuned; there is no validation split.

| Model | Objects | n | MRR | Hits@1 | Hits@3 | Hits@10 |
|---|---|---|---|---|---|---|
| Predicate frequency | all | 19,686 | **0.129** | **0.090** | **0.131** | **0.202** |
| v13 transformer | all | 19,686 | 0.115 | 0.085 | 0.118 | 0.170 |
| TransE | all | 19,686 | 0.074 | 0.044 | 0.079 | 0.133 |
| Predicate frequency | entity-valued | 6,090 | **0.318** | **0.250** | **0.333** | **0.451** |
| v13 transformer | entity-valued | 6,090 | 0.287 | 0.230 | 0.308 | 0.399 |
| TransE | entity-valued | 6,090 | 0.202 | 0.128 | 0.224 | 0.358 |
| Predicate frequency | literal-valued | 13,596 | **0.044** | 0.018 | **0.040** | **0.090** |
| v13 transformer | literal-valued | 13,596 | 0.038 | **0.020** | 0.033 | 0.068 |
| TransE | literal-valued | 13,596 | 0.017 | 0.006 | 0.015 | 0.032 |

Entity-valued objects are those whose label also occurs as a subject in training. Neither learned model beats predicate frequency on this split. The v13 transformer ranks above the untuned TransE, but with no tuning for either model we do not read that as a comparison of methods. The result is consistent with the model's training perplexity (Appendix A): it is a weak completion model. This is why the paper's claims concern the provenance machinery and not the quality of what the model predicts; the machinery is independent of the model it records.

---

## 7. Limitations

### 7.1 Model and decoding

- **Weak completion accuracy.** The case-study model does not beat a predicate-frequency baseline on held-out triples (§6.4).
- **Mode collapse on common tokens.** Even with the cumulative penalty, predictions for predicates the model knows weakly fall back to connectors (`of`, `and`) or to format placeholders (`spouse -> "1 ."`, §5.4). The corpus cleanup removed the worst of these (§5.3) but not all.
- **Label-space output.** The model emits subword tokens of an English label. Exact resolution to an entity IRI (§4.4) succeeded for 1 of 371 predictions on real data, so nearly all stored predictions are literals that can contain BPE fragments.
- **Greedy decoding only.** No beam search or sampling.

### 7.2 Provenance

- **Selection provenance is not support.** A `propositionInferredFrom` row points at a concrete stored statement, which is auditable, but the statement was chosen by the candidate-selection heuristic (§4.4 step 1), and the model does not see it: the model's input is the subject and predicate labels only. A cited statement may have played no part in the predicted value.
- **One statement per neighbour.** A proposal depends on several statements of each contributing neighbour; only one is cited (§4.4). Retracting the neighbour node reaches the prediction, but deleting a single other statement of that neighbour would not, if Loka supported statement-level retraction (it retracts nodes).
- **Annotation volume grows with the neighbourhood.** Complete selection provenance cost about 120 citation rows per prediction on the 153k-triple graph (§4.4).
- **Earlier outputs used a cruder rule.** Earlier versions of the procedure cited the first ten of the subject's statements regardless of which ones matched, so it could both cite irrelevant statements and miss the one that mattered. Generated triples produced under that rule should be re-generated before relying on retraction.

### 7.3 Evaluation scope

- The link-prediction set is small (19,686 rankable queries), skewed toward literal-valued predicates, and drawn from the triples a larger label cache newly resolved rather than sampled uniformly from Wikidata. Entities are identified by English label, so distinct entities with the same label are merged, which affects the transformer and TransE alike.
- No model was tuned, and only one checkpoint (v13) has a held-out set that requires no retraining.
- Retraction was evaluated on synthetic graphs of up to 5M rows and on a 153k-triple real graph with dependency chains of two generated hops, on one machine; longer real chains were not available without more inference passes. The in-memory store is the one retraction runs against in every deployment mode (the server keeps its indexes in memory and mirrors writes to the persistent store), but the commit step that deletes the computed set from the persistent store was not timed.
- The source dataset revision was not pinned when the corpora were built (References). The released corpora are fixed, but the path from source dump to corpus cannot be replayed exactly.

---

## 8. Discussion

Three directions would strengthen the provenance record itself.

**Ontology templates as the selector.** OWL ontologies can be stored in the engine as triples, though the engine does not reason over them. An ontology could serve as the candidate selector: a class declares the properties its instances are expected to have, the inference loop proposes the missing ones, and `propositionInferredFrom` cites the class declaration alongside the subject's statements. The citation would then name the reason a predicate was proposed, which the current neighbour heuristic only approximates.

**Node-level dependencies.** Recording one `propositionDependsOn` edge per contributing node, and having retraction follow it, would keep provenance complete at a fraction of the statement-level volume of §4.4. It needs a change to `retract_set`.

**An entity-space decoder.** The engine's HNSW vector index could resolve a predicted embedding to the nearest known IRI, so that predictions are entities rather than label strings. That would make completion directly comparable with entity-ranking methods and remove BPE artifacts from stored output.

---

## 9. Conclusion

We described how a triplestore can hold model-generated statements next to curated ones without losing track of them: each generated triple carries RDF-star annotations, in a reserved namespace, naming its model, its confidence and the stored statements its procedure used. The namespace keeps generated statements out of training corpora, and the dependency edges support cascade retraction, which we tested against an independent reference (finding and fixing one defect) and timed at about 0.1 s for a 106k-triple retraction in a 5M-row store, with cost following the size of the removal rather than of the store. The case-study model is weak at completion, below a frequency baseline, which is why our claims rest on the provenance machinery rather than on the model. Code, checkpoints and corpora are released.

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

- Loka engine, release `v0.4.4` (includes the retraction fix of §6.1). https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.4. AGPL-3.0-or-later.
- Model checkpoints: https://huggingface.co/datasets/EmmaLeonhart/loka. Training corpora: https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata.
- Source data: philippesaade, *wikidata*, Hugging Face dataset, CC0. https://huggingface.co/datasets/philippesaade/wikidata. The dataset revision used for the v11–v14 corpora was not pinned, and the dataset has been updated since.

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
