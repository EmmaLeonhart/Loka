# Loka: Provenance for Retracting Model-Generated Triples in an RDF-star Store

**Code:** <https://github.com/EmmaLeonhart/Loka> (release `v0.4.7`: <https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.7>) &middot; **Model checkpoints:** <https://huggingface.co/datasets/EmmaLeonhart/loka> (`checkpoints/wikidata_v3.pt`–`wikidata_v14.pt`, plus per-epoch checkpoints for v13 and v14, snapshot tags `v13.*` `v14.*`) &middot; **Normalized-Wikidata training corpora (v11+):** <https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata> (snapshot tags `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`) &middot; **Source dataset:** <https://huggingface.co/datasets/philippesaade/wikidata>

---

## Abstract

When a model writes statements into a knowledge graph next to curated data, the graph should say which statements came from a model, from which model, and which stored statements each was produced from, so that generated statements can be found, filtered, and removed when a statement they depended on is withdrawn. We describe a small provenance vocabulary for model-generated statements and its implementation in Loka, an RDF-star triplestore. Each generated triple carries RDF-star annotations in a reserved namespace: a generated flag, the generating model, a confidence, and quoted pointers to the stored statements the prediction procedure took as input, which we call selection provenance. Because these dependencies are graph edges, the store can query them with ordinary SPARQL-star, leave flagged statements out of the corpora it extracts, and retract by dependency: removing a node also removes every generated triple that transitively depended on one of its statements, without following ordinary data edges. Retraction agrees with an independent reference implementation on generated graphs, computes the set for a 106k-triple removal from a 5M-row store in about 85 ms in memory (deleting it from disk is measured separately), and on a real 2M-triple Wikidata store with chained predictions it matched an independently computed closure on all 8,621 required removals, counted as (entity, generated triple) pairs. The generated triples come from small transformers trained from scratch on Wikidata, which serve as the workload; on held-out triples they do not beat a predicate-frequency baseline. Selection provenance records what the procedure used, not what the model relied on. Code, checkpoints and corpora are released.

---

## 1. Introduction

Knowledge graphs are increasingly extended by models: link predictors propose missing edges, and language models extract or generate statements. Once a predicted statement is stored, a typical triplestore holds it exactly like a curated one. Whatever provenance exists lives outside the graph, in pipeline logs or a separate metadata table. Three problems follow. A query cannot ask which answers came from a model. A corpus exported from the graph mixes model output with curated data. And when a source statement is found to be wrong, nothing in the store says which generated statements were derived from it.

RDF-star lets a triple be the subject of other triples. Loka uses this to keep provenance in the same graph as the data it describes. Every triple written by Loka's inference procedure (§4.4) carries RDF-star annotations, in a reserved namespace, recording that it was generated, by which model, with what confidence, and from which stored statements. These annotations form a small ontology of model-generated statements. They can be queried with ordinary SPARQL-star, used to leave generated statements out of corpora extracted from the store, and followed to retract everything that depended on a withdrawn statement.

We are specific about what the dependency edge means. In our prediction procedure, a candidate (subject, predicate) pair is proposed from the subject's existing statements and those of its graph neighbours, and the model then predicts an object from the subject and predicate labels alone. The `propositionSelectedFrom` edges record exactly the subject's statements whose match with a neighbour led to the proposal, plus one statement of each such neighbour. We call this *selection provenance*. It is a record of the procedure's inputs, which is what retraction needs; it is not evidence that the cited statements support the prediction, and not a record of what the model attended to.

### Contributions

1. **A provenance vocabulary for model-generated statements.** Predicates under a reserved namespace, `http://loka.dev/provenance/`, form a fixed annotation block on each quoted generated triple: a generated flag, the model version, a confidence, and selection-provenance edges whose objects are themselves quoted triples, the stored statements that led the procedure to propose the (subject, predicate) slot. They record the candidate's proposal, not a justification of the predicted object, which the model produces from the subject and predicate labels. (§3.1–3.2)

2. **Cascade retraction over that vocabulary.** Removing a node removes its statements and every generated triple that transitively depended on them. Traversal follows only selection-provenance edges and stays inside the reserved namespace, so ordinary data edges are never treated as dependencies. We test it against an independent reference implementation and measure its cost. (§3.4, §6.1–6.2)

3. **Store-side use of the flags.** When Loka extracts a corpus it drops every triple flagged generated and every reserved-namespace row, and its candidate selector and writer refuse reserved predicates. These guards cover what Loka's own pipeline writes and flags; the store accepts unflagged triples from other writers. (§3.1)

4. **A case study on Wikidata.** A series of small from-scratch masked-triple transformers (v3–v14, all checkpoints and corpora released) generates the triples the evaluation retracts. The model is the workload, not a contribution. (§5)

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

### 2.2 Provenance

Recording where statements come from is an old concern of the Semantic Web. Named graphs attach provenance and trust information to sets of triples (Carroll et al., 2005), and the W3C PROV-O ontology gives a vocabulary for entities, activities and agents and their derivation relations (Lebo et al., 2013). Nanopublications package a single assertion with its provenance and publication information as a set of named graphs (Groth et al., 2010). Inside a triplestore, TripleProv stores provenance next to the data and answers lineage queries natively (Wylot et al., 2014). In database theory, provenance has been formalised as *why-provenance*, the source tuples that witness an output, and *where-provenance*, the locations a value was copied from (Buneman et al., 2001), and generalised by provenance semirings, which annotate query results with polynomials over source annotations (Green et al., 2007). Selection provenance borrows the shape of why-provenance, one set of source statements per output, but not its meaning. A why-provenance witness is a set of source tuples from which the output follows; the statements we record are the ones that led the procedure to propose the (subject, predicate) slot, and the object value comes from the model, not from them. It is provenance of the proposal, not a witness for the value. All of these record provenance; none of the RDF ones defines how a store should act on it when a source is withdrawn. Most operate at the granularity of a graph or an explicit provenance resource. We use RDF-star to attach provenance to individual triples directly, with a small fixed vocabulary for one case, model-generated statements; mapping it onto PROV-O is left to consumers, and `prov:used` on the generating activity fits `propositionSelectedFrom` better than `prov:wasDerivedFrom`, which would claim the value was derived from the cited statements. What we add is not a provenance vocabulary but an operational use of it inside the store: exclusion from training corpora and cascade retraction.

### 2.3 Knowledge-graph completion

Link prediction scores candidate completions of (subject, predicate, ?) queries. Embedding models such as TransE (Bordes et al., 2013) and RotatE (Sun et al., 2019) learn entity and relation vectors; transformer-based models score triples as text (KG-BERT; Yao et al., 2019) or generate the missing entity as a sequence (KGT5; Saxena et al., 2022). The standard evaluation reports mean reciprocal rank and Hits@k in the filtered setting, which removes other known true answers before ranking (Bordes et al., 2013); we follow it, with the tie-aware rank of Berrendorf et al. (2020), and use PyKEEN (Ali et al., 2021) for the TransE baseline. These systems output scores or ranked candidates. Writing the chosen completions back into the graph, with a record of what they were derived from, is outside their scope, and is the part this paper addresses.

### 2.4 Attribution for generated content

Retrieval-augmented generation conditions a language model on retrieved passages (Lewis et al., 2020), and attributed question answering asks a model to return evidence supporting its answer and evaluates whether the evidence does support it (Bohnet et al., 2022). Those lines of work aim at support: the cited source should justify the output. Selection provenance makes a weaker, procedural claim: the cited statements were the input to the procedure that produced the output. That is enough for retraction, which needs to know what an output depended on, and it is not a claim of support (§7.2). Machine unlearning addresses the complementary problem of removing a training example's influence from the model itself (Bourtoule et al., 2021); cascade retraction removes the model's *stored outputs* that depended on a withdrawn source, and needs no change to the model. It does not remove whatever the model learned from that source during training.

### 2.5 Truth maintenance and deletion propagation

Cascade retraction is closest in spirit to truth maintenance. A justification-based truth maintenance system records, for each belief, the justifications that support it and withdraws a belief when none of its justifications remains valid (Doyle, 1979). Deductive and relational databases face the same problem for materialised views: Gupta et al. (1993) give incremental algorithms that propagate deletions to derived tuples, counting alternative derivations for each derived tuple and, for recursive views, deleting an over-estimate and re-deriving. Loka's setting differs in two ways that keep the mechanism simple. Each generated triple has exactly one justification, the set of stored statements its procedure consumed, so removing any one of them removes the triple, and no alternative-derivation counting is needed. And the derived statements are not recomputable from a rule set: they are model outputs, so the dependency record must be stored at write time, which is what the RDF-star annotations do. What we contribute is not the propagation idea but its realisation inside an RDF-star store, bounded to a reserved namespace so that curated data is never treated as derived.

### 2.6 Training from scratch

The models in our case study are trained from scratch on triples, not fine-tuned from a pretrained language model. With a pretrained model, a generated triple can draw on pretraining data the store knows nothing about, so its provenance record would be incomplete by construction; training only on the corpus keeps what the model learned inside a known, released dataset, apart from the property labels fetched from the live Wikidata endpoint (§4.2) and the unpinned source revision (§7.3). Selection provenance does not record that training-time dependence; it records only what the procedure consulted at inference. The schema itself does not depend on this choice.

---

## 3. Architecture

### 3.1 The reserved provenance namespace

Every predicate under `http://loka.dev/provenance/` is system-internal. The names are deliberately verbose — `propositionGeneratedBy` rather than `generatedBy` — so a human scanning raw triples spots them at a glance and accidental collision with real-world predicates is vanishingly unlikely. The full namespace currently holds:

| Predicate | Object type | Meaning |
|---|---|---|
| `propositionGenerated` | `xsd:boolean` | This triple was emitted by a model (not curated). |
| `propositionGeneratedBy` | string | The model version (e.g., `wikidata_v4`) that emitted it. |
| `propositionConfidence` | `xsd:decimal` | Mean post-penalty token probability of the prediction; a decoding score, not calibrated (§4.4). |
| `propositionSelectedFrom` | quoted triple | A stored statement the prediction procedure took as input (selection provenance). |
| `propositionPredictedLabel` | string | The raw model output, kept when the object was resolved to an entity IRI (§4.4). |
| `propositionImportedFrom` | URI | Reserved; not currently emitted in production (was found redundant for uniformly-Wikidata corpora). |

`propositionSelectedFrom` was called `propositionInferredFrom` until shortly before this paper was submitted. We renamed it because the edge records which statements the selector chose as input, not an inference from them (§1). The stores used for the real-data runs in §6 were written under the old name. Only the name changed: the engine's retraction still follows the old IRI, so those stores behave the same under the current code.

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

It also drops any row whose predicate IRI matches the reserved prefix. Both this filter and the retraction sweep of §3.4 match the prefix, not a list of names, so a predicate added to the namespace is covered without further changes.

**Candidate filtering.** The inference loop builds candidate `(subject, predicate)` pairs by intersecting subject-with-graph-neighbor predicates. Reserved-namespace predicates are excluded from `pred_usage` and re-filtered at the candidate list level.

**Emit-time guard.** Each prediction's primary triple is checked against the reserved prefix immediately before it is written to the output stream. A reserved-prefix predicate is logged loudly and dropped.

The layers protect two different invariants, with different coverage:

- *Generated triples stay out of an extracted corpus*: the extractor's generated-flag filter alone. A generated triple uses an ordinary data predicate, so the prefix checks do not see it.
- *Annotation rows stay out of an extracted corpus*: the extractor's prefix filter alone.
- *The model does not propose or write a reserved predicate*: the selector and the emit-time guard, either of which suffices. They are not covered by a test that disables the other.

All of these guards sit in Loka's extractor and inference pipeline, not in the store. In keeping with storing whatever is written, the store accepts any triple over `POST /triples`, so a generated triple that another tool writes without the generated flag is not excluded from a corpus.

### 3.2 Annotating generated triples

Selection provenance records the stored statements that the prediction procedure took as input (§4.4): the ones that led it to propose the (subject, predicate) slot. It is a dependency of the proposal, not a justification of the predicted value; it does not name statements the model attended to or that support the prediction (§1). The schema does not depend on that choice: a procedure that selected its evidence differently, or a model that retrieved it, would write the same block.

When the inference layer accepts a candidate `(S, P)` and emits a predicted object `"X"`, it writes a fixed-shape block:

```
<S> <P> "X" .
<<S P "X">> lprov:propositionGenerated     "true"^^xsd:boolean .
<<S P "X">> lprov:propositionGeneratedBy   "wikidata_v4" .
<<S P "X">> lprov:propositionConfidence    "0.43"^^xsd:decimal .
<<S P "X">> lprov:propositionSelectedFrom  <<S existing_p1 existing_o1>> .
<<S P "X">> lprov:propositionSelectedFrom  <<S existing_p2 existing_o2>> .
   ... (one row per subject statement the proposal depended on)
```

`lprov:` abbreviates the reserved namespace `http://loka.dev/provenance/` (not PROV-O's `prov:`). The `propositionSelectedFrom` objects are the subject's statements whose (predicate, object) pair matched a neighbour that has the proposed predicate, plus, for each such neighbour, its statement carrying the proposed predicate (§4.4). Wikidata qualifiers and references are imported with the same RDF-star pattern (a quoted statement as subject), so curated and generated annotations share one shape.

Because the `propositionSelectedFrom` objects are written by the procedure, not generated by the model, they always point at statements that exist in the store. What they cannot guarantee is relevance: a cited statement may have had no bearing on the predicted object (§7.2).

### 3.3 The loop

The store and the model form a closed loop:

1. Curated triples (here, Wikidata) are loaded into the store as RDF-star.
2. A training corpus is extracted from them, with labels substituted for identifiers (§4.2) and every generated triple excluded (§3.1).
3. A role-aware transformer is trained on the corpus (§4.3).
4. The inference procedure proposes (subject, predicate) pairs, the model predicts objects, and each accepted prediction is written back to the store with its annotation block (§3.2, §4.4).

Generated triples land in the store flagged `propositionGenerated true`, and the next corpus extraction's SPARQL-star filter excludes them, so the model never trains on its own output. Inference can be re-run to add predictions without polluting the training distribution.

### 3.4 Cascade retraction

Because every triple written by the §4.4 procedure carries `propositionSelectedFrom` edges to the statements that procedure took as input, the store can retract by dependency. Given a node to remove, `retract_set` computes:

1. **Depth 0:** every triple whose subject or object is the node. A generated triple among them takes its reserved-namespace annotation rows with it. Curated annotations on a removed triple (Wikidata qualifier and reference rows, whose predicates are outside the reserved namespace) are not swept; they remain as annotations on a quoted triple that is no longer asserted (§7.2).
2. **Closure:** for each removed triple *T*, every generated triple *G* with an annotation `<<G>> propositionSelectedFrom <<T>>` is removed together with all of *G*'s reserved-namespace annotations, and the step repeats on *G*.

Stated as one definition: for a root node *n*, let $R_0$ be the triples with *n* as subject or object, and let *R* be the least superset of $R_0$ that contains every generated triple *G* with some `<<G>> propositionSelectedFrom <<T>>` row for a *T* in *R*. The retraction set is *R* together with the reserved-namespace annotation rows of every generated triple in *R*. Three consequences follow from this definition and are deliberate. Retraction is node-rooted: the operation removes a node, not a single statement, so a statement is withdrawn only as part of a node's rows. Statement-level retraction would need two changes. The closure itself already works per triple (it starts from a set of removed triples and looks up `<<G>> propositionSelectedFrom <<T>>` by T through the POS index), so the root would simply be the one statement. The record, however, cites only one statement of each contributing neighbour (§4.4), which is enough when whole neighbours are removed but would miss a prediction that depended on another of that neighbour's statements; statement-level retraction would require citing every matching statement, at the cost the unbounded selector showed (a median of 105 citations instead of 9, §4.4). A generated triple goes as soon as any one of its cited sources is in *R*; where one triple carries the merged citations of several derivations, losing a source of either removes it (§7.2). And the only curated triples removed are those in $R_0$: beyond the node's own rows, everything removed is a generated triple or its reserved-namespace annotation.

Traversal follows only `propositionSelectedFrom` and only sweeps predicates under the reserved namespace, so an ordinary data edge is never treated as a dependency: retracting a curated entity does not chase its curated neighbours. A triple is processed at most once, so cycles in the citation graph terminate. Dereferencing `<<T>>` requires the reverse index from content-addressed quoted-triple ids to (s, p, o) described in §2.1. The computation is read-only; the engine exposes it as a preview, and deletion is a separate, explicitly confirmed operation (dry run is the default at every interface). §6.1–6.2 evaluate its correctness and cost.

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

Training objective: masked-token prediction in the style of BERT (Devlin et al., 2019), applied per role: pick one role (S, P, or O) at random per example, mask its tokens with `[MASK]`, predict the originals. Cross-entropy on the masked positions, AdamW, 3e-4 LR, β=(0.9, 0.95), weight decay 0.01, gradient clipping at 1.0.

Two architectures were used; v3 and v4 share one, and their parameter counts differ only through vocabulary size:

| Model | d_model | nhead | layers | params | epochs | final ppl |
|---|---|---|---|---|---|---|
| v3 (early; pre-cleanup) | 256 | 8 | 4 | 16,012,800 | 5 | 53.43 |
| v4 (early; cleaned) | 256 | 8 | 4 | 15,967,744 | 5 | 92.48 |
| v5 (early; capacity scale-up) | 512 | 8 | 6 | 44,531,712 | 5 | 84.85 |
| v6–v14 | 512 | 8 | 6 | 44,531,712 | 2–20 | Appendix A |

v3's low perplexity comes from memorising datatype-suffix tokens (§5.2). From v5 on the architecture is fixed, so later versions differ only in corpus, tokenizer (from v6) and training length.

### 4.4 Inference with selection provenance

For each candidate subject in the corpus:

1. **Candidate predicate selection.** Find graph-neighbors — subjects sharing at least one (predicate, object-key) tuple with this one, where an object's key is its IRI, or for a literal its normalised value without datatype or language tag — and rank predicates they have but the candidate subject lacks. Cap at *N* candidates per subject (default 5). For each candidate, record which of the subject's statements produced a matching neighbour, and one statement of each contributing neighbour (its first statement with the proposed predicate); these become its selection provenance. One statement per neighbour suffices because retraction removes whole nodes: retracting a neighbour removes the cited statement and so reaches the prediction. To keep that record bounded, the selector itself is bounded: it consults at most 20 of the subject's statements (in store order) and at most 20 distinct neighbours (in the order first matched), and cites the statements described above from within those bounds. It does not record the predicate ranking, the top-*N* cutoff, or the negative test that the subject lacks the proposed predicate, so the record is not a full trace of the procedure. What it guarantees is narrower: retracting the subject, or any neighbour that contributed the proposed predicate, reaches the prediction, because each of those has a cited statement. A neighbour the selector consulted that did not contribute this predicate is not cited, although it may have affected the ranking. Its size is capped by construction at 40 citations per prediction: at most 20 subject statements plus one statement from each of at most 20 neighbours. The maxima observed are 26 for the v13 model and 36 for the predicate-frequency generator on the 153k-triple graph (§6.5), and 39 on the 2M-triple store (§6.2). Without the bounds the volume grew with the neighbourhood: on the 153k-triple graph of §6.2 a prediction cited a median of 105 statements (23 subject-side, 82 neighbour-side), because almost every statement of a subject matched some neighbour; with them it cites a median of 9 (maximum 26).
2. **Masked decoding with cumulative repetition penalty.** Build the input as `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] [MASK]^k [SEP_O]`. At each masked position, the model's logits are turned into a softmax distribution `p`. We apply:

   - Hard skip-set: special tokens never win.
   - Cumulative repetition penalty, a per-occurrence variant of the penalty of Keskar et al. (2019): `p[t] /= penalty^count[t]` where `count[t]` is the number of times `t` has already been emitted in this sequence. Default `penalty = 3.0`. The penalty divides probabilities, which are non-negative, so it can only lower a repeated token; `p` is not renormalised afterwards.
   - Per-token confidence floor: emission halts when the top-token probability falls below 0.05.

   Greedy top-1 selection, no beam search. The prediction's `propositionConfidence` is the mean of the selected tokens' post-penalty probabilities. It is a decoding score used for thresholding, not a calibrated probability that the triple is true; we have not measured its calibration.
3. **Confidence-thresholded emit.** If the confidence of step 2 is ≥ threshold (default 0.4) and the predicted object is not a duplicate of an existing fact for this (S, P), emit the RDF-star block (§3.2).
4. **Entity resolution.** If the predicted label, normalised (BPE word-boundary markers to spaces, case folded), equals the label of exactly one entity in the store, the object is written as that entity's IRI and the raw output is kept under `propositionPredictedLabel`; otherwise the object stays a literal. On the real graphs of §6.2 this resolved 5 of 356 predictions (153k triples) and 8 of 672 (2M triples): the model's outputs rarely equal an entity label exactly.
5. **Optional `--post`.** Write the emitted N-Triples-star to the live Loka store via `POST /triples`. Subsequent training-corpus extractions exclude these via the SPARQL-star FILTER from §3.1.

The cumulative penalty matters: a *non*-cumulative penalty (set membership) was tested first and failed to break loops on dominant common tokens because the penalty applied only once regardless of how many times the token had already won. With cumulative, three emissions of `of` at penalty 3.0 multiply its divisor by 27 and reliably drop it below the floor, breaking the cascade.

---

## 5. Case study: a model series on cleaned Wikidata

We exercised the provenance loop with a series of from-scratch models (v3–v14) trained on progressively rebuilt Wikidata corpora. Every checkpoint and corpus is released (links at the top of the paper); Appendix A lists each version's tokenizer, corpus size, training length and perplexity. The models matter here only as generators for the provenance machinery, and their completion accuracy is evaluated against baselines in §6.4. This section summarises the corpus decisions that the released models and corpora embody; the examples and probe results behind them are in the repository (`docs/model-history.md`, <https://github.com/EmmaLeonhart/Loka/blob/main/docs/model-history.md>).

### 5.1 Setup

From v5 on, the architecture is fixed at the 44.5 M-parameter configuration of §4.3; from v6 on, the tokenizer is a fixed byte-level BPE vocabulary. Later versions therefore differ only in training corpus and training length. Perplexity is a training diagnostic only: it is not comparable across the word-level (v3–v5) and BPE (v6+) tokenizers, nor across corpora that differ in content as well as size.

### 5.2 Datatype leakage

The first corpus kept datatype IRIs inside literal strings, and the model trained on it (v3) learned to emit them. Stripping datatype suffixes before tokenization (§4.2) removed the pattern and *raised* perplexity (92.5 vs 53.4), because v3 had been earning cheap loss on those tokens; this is why we treat perplexity as a diagnostic, not an outcome.

### 5.3 Catalog noise

Catalog cross-reference identifiers (Wikidata's `external-id` datatype: about 80 % of properties and 75.7 % of the v6 training rows) taught v6 to emit confident identifier-shaped strings, including on unrelated predicates. From v7 on, the corpus excludes the datatypes whose values carry no transferable content, keeping 2,231 of 12,756 properties, and normalises time and quantity literals; the specification ships as `training/wikidata_excluded_predicates.json`. On a fixed behavioural probe, the share of emissions on catalog-identifier predicates fell from 40 % (v6) to 0 % (v10), though not steadily: it was 64 % at v7 (9 of only 14 emissions), which we have not explained, then 15 %, 3 % and 0 %; and a date-shaped leak onto `instance of` disappeared from v7 on (`docs/model-history.md`).

### 5.4 Residual failure modes

The cleaned models still fail in narrow, consistent ways: copying the subject as the object on mostly reflexive predicates, following the most frequent predicate template, giving a value of the right type but wrong identity, and numeric-format degeneration. Raw examples are in `docs/model-history.md`.

### 5.5 Building the corpus without the store

Extracting training triples from the store with paged SPARQL (`LIMIT`/`OFFSET`) costs time linear in the offset, which made a 50 M-triple extraction impractical. From v11 on, the corpus is built by streaming the source parquet directly: a first pass caches English labels, a second emits one tab-separated `subject predicate object` line per kept claim, with the same exclusions and normalisations as above and with reserved-namespace triples stripped. The resulting corpora are released as a separate dataset in four sizes (Appendix A). These corpora contain no generated triples because the source dump contains none; the extractor's generated-flag filter of §3.1 applies only when a corpus is extracted from a store.

---

## 6. Evaluation

### 6.1 Retraction correctness

We test `retract_set` against an independent reference on randomly generated provenance graphs. The generator creates curated triples and generated triples; each generated triple cites one to three earlier triples (curated or generated), about 5 % also cite a *later* generated triple so that the citation graph contains cycles, and each carries a `propositionGeneratedBy` annotation. This is a smaller schema than the inference pipeline writes (§3.2 also lists the generated flag, confidence and predicted label); it suffices here because the engine follows only `propositionSelectedFrom` and sweeps every annotation under the reserved prefix alike, and the real-data checks of §6.2 use the full production schema. The reference computes the intended closure (§3.4) by fixpoint iteration over the generator's own lists, without using the store's indexes. We compare the two on 50 small graphs (60 curated and 60 generated triples, 10 roots each) and 10 medium graphs (2,000 and 2,000, 20 roots each).

The comparison found a defect on its first run. When the retracted node was itself the subject or object of a generated triple, that triple was removed at depth 0, but its annotation rows were swept only for generated triples reached through a provenance edge, so they remained as annotations on a triple no longer in the store. Every discrepancy was of this kind. After the fix (depth 0 now sweeps the annotations of its own generated rows, as §3.4 states) the engine and the reference agree on all 700 roots, and a unit test covering the case fails without the fix.

### 6.2 Retraction cost

We time `retract_set` with criterion on generated graphs of four sizes, using the same generator (curated and generated triples in equal number, entities = a quarter of that number). The root is the entity with the largest retraction set among a fixed sample of 50. Single laptop (AMD Ryzen 7 8845HS, 32 GB RAM, NVMe SSD, Windows 11; all timings in §6 are from this machine), release build; the whole benchmark was run five times back to back, and the table gives the median of the five per-run medians with the range across runs. The store is the in-memory index that the engine computes retractions against in every mode, including the server, which mirrors writes to its persistent store:

| Generated triples | Store rows | Triples removed | Max depth | Median time (5 runs) | Range across runs |
|---|---|---|---|---|---|
| 1,000 | 5,034 | 1,439 | 14 | 0.45 ms | 0.44–0.47 ms |
| 10,000 | 50,484 | 7,367 | 24 | 2.84 ms | 2.57–3.33 ms |
| 100,000 | 504,389 | 120,461 | 34 | 86.3 ms | 82.0–104.6 ms |
| 1,000,000 | 5,050,435 | 106,255 | 39 | 85.0 ms | 80.5–86.2 ms |

These results are consistent with time following the size of the removed set rather than the size of the store, as the algorithm's per-triple index lookups predict, but they do not establish it in general: the 5M-row store and the 0.5M-row store take the same time for removals of similar size, about 0.7–0.8 µs per removed triple (86.3 ms for 120,461 and 85.0 ms for 106,255 removed triples). The per-triple cost is not constant across all four rows: it is 0.31 µs and 0.39 µs on the two small graphs, whose removal sets are also shallower (depth 14 and 24 against 34 and 39); we have not isolated which factor accounts for the difference. The comparison that isolates store size is therefore the pair of large rows, which is two data points. Across the five runs the slowest and fastest median differ by at most 1.3× at any size (the widest ranges are at 10k and 100k, each from one slow run), and the ranges of the two large rows overlap, so their equal times are not an artefact of a single run. Absolute times vary more between sessions than within one: an earlier single run of the code on the same laptop measured the three smaller sizes about 1.6× faster than another, so the numbers are comparable within the table but not as absolute figures. Raw criterion output for all five runs is in the repository under `paper/supplementary/retract-trials/`.

As an external baseline, we computed the same retraction set with plain SPARQL-star queries through the engine's general query executor, on the same four graphs and roots: the rows touching the root, their reserved annotations, and the closure `?g propositionSelectedFrom* ?h`, where `?h` cites a triple touching the root, together with `?g`'s reserved annotations and its dereferenced triple. The path is written as one hop into a quoted pattern followed by `*` between variables, because the engine does not evaluate a property path whose end is a quoted pattern. Before timing, the benchmark checks that the SPARQL result is exactly `retract_set`'s set; it was at every size, in every run. Both were then timed side by side in the same process, five runs each (median of per-run medians, range across runs):

| Generated triples | `retract_set` | SPARQL-star closure | Ratio |
|---|---|---|---|
| 1,000 | 0.37 ms (0.36–0.41) | 3.73 ms (3.66–4.00) | 10.0× |
| 10,000 | 2.36 ms (2.30–2.45) | 20.8 ms (20.5–29.8) | 8.8× |
| 100,000 | 71.5 ms (68.7–73.1) | 469 ms (451–483) | 6.6× |
| 1,000,000 | 71.3 ms (70.2–95.1) | 429 ms (415–443) | 6.0× |

The query-based closure is 6–10× slower and, like `retract_set`, takes about the same time on the 0.5M-row and 5M-row stores. The retraction semantics therefore do not depend on the dedicated traversal: in principle, any store that holds the annotation block can compute the same set with standard SPARQL-star queries, although we ran them only on Loka's own executor, which needed the rewrite above. What `retract_set` adds is speed, 6–10× over the general query path of the same engine. The two tables in this section measure `retract_set` on the same graphs; its times here are about 15–20 % below the first table's because the two were measured by separate benchmark binaries minutes apart, which is the run-to-run variation in absolute times noted above. The ratio, measured within one process, is the comparable figure. Raw output is under `paper/supplementary/retract-baseline/`, and the benchmark is `loka-sparql/benches/retract_baseline.rs`.

On real data, we pulled a larger breadth-first Wikidata neighbourhood of `Q42` (155,324 triples, 983 entities; 153,185 imported: the importer rejected 2,139 lines, 1.4 %, and since the pulled file was not kept their cause can no longer be determined; every check below runs on the imported store, so the rejected lines do not enter any result), ran the v13 model's inference over every subject, posted its 263 predictions, and ran inference a second time with those predictions visible as context, which produced 93 more, 83 of them citing a first-pass prediction. The store then held real dependency chains of two generated hops (356 generated triples, 4,817 annotation rows). For each of the 983 entities we called the server's retraction preview and compared it with an independently computed closure: a generated triple must be removed if it touches the entity, cites a statement touching the entity, or cites a removed generated triple. Summed over the 983 previews there were 4,354 required removals (entity, generated triple pairs); in every case the generated triple and every one of its annotation rows was in the returned set, and no generated triple outside the closure was. Over HTTP the preview took a median of 1.4 ms (p95 5.6 ms, maximum 42 ms) and removed a median of 83 triples (maximum 4,451); these counts include the entity's own curated triples and the annotation rows, not only generated triples. An earlier run with an unbounded selector (§4.4), 371 generated triples carrying 39,470 annotation rows, gave the same result: 27,142 required removals, none missed, none extra.

Committing a retraction also deletes the set from the persistent (sled) store and flushes it before replying. We committed retractions of 30 randomly chosen entities in turn on a copy of a store. With one storage transaction per removed triple, as the engine first did, the unbounded-selector store took a median of 140 ms per commit and up to 30 s (1.28 ms per removed triple; 43,085 rows removed in all); removing the whole set in a single transaction brought the same 30 commits to a median of 8.3 ms and a maximum of 0.31 s (0.06 ms per removed triple). On the bounded-selector store above, the 30 commits removed 6,380 rows, at a median of 4.6 ms and a maximum of 31 ms. Every restart reloaded exactly the expected number of rows.

At larger scale, we imported 2,000,623 real triples (8,806 entities with every claim, label, qualifier and reference) from the same Wikidata dump into a fresh store, ran both inference passes over 3,000 subjects sampled with a fixed seed, and repeated the checks. The passes produced 526 and 146 predictions, 100 of the second citing a first-pass prediction (672 generated triples, 13,318 annotation rows; a median of 17 citations each, maximum 39). Over 8,621 required removals across 2,667 entities, the retraction preview matched the independent closure exactly, with nothing missed and nothing extra; it took a median of 2.3 ms (p95 6.2 ms) and removed a median of 140 triples (maximum 20,600). Before the commits the store held 2,015,262 rows; this is 649 more than the imported, generated and annotation counts above add up to, and the run's logs do not break the store down further. Thirty committed retractions removed 5,377 rows at a median of 24 ms (maximum 43 ms), and a restart reloaded exactly the expected 2,015,262 − 5,377 = 2,009,885 rows. Inference itself was the slow step at this scale: about 5 s per subject on CPU, against about 1 s on the 153k-triple graph.

### 6.3 Encoding cost against reification and named graphs

We compare the annotation block with the two standard ways of attaching provenance to individual statements in plain RDF, using real data. We pulled a breadth-first Wikidata neighbourhood of `Q42` (14,819 triples, 169 entities), loaded it, and ran the v13 model's inference over every subject, which wrote 38 generated triples with 181 selection-provenance edges (113 distinct cited statements). The same generated set was then re-encoded (a) as standard RDF reification with PROV-O, where each generated triple gets an `rdf:Statement` node carrying the metadata and `prov:wasDerivedFrom` links to `rdf:Statement` nodes for the cited statements, and (b) as one named graph per generated triple, with the cited statements still reified so they can be pointed at. The reified baseline uses `prov:wasDerivedFrom` because it links one statement node directly to another; the relation §2.2 recommends, `prov:used`, runs from an activity to an entity, so using it would add a generating-activity node and its links for every generated triple, and the baseline as run is the smaller of the two. Variant (b) could also put each cited statement in a singleton graph instead of reifying it, which needs fewer rows, so its counts below are an upper bound for named graphs. Named graphs are compared by size only (see below).

| Encoding | Rows | Bytes (N-Triples / N-Quads) |
|---|---|---|
| RDF-star annotation block (ours) | **333** | **82,839** |
| Reification + PROV-O | 937 | 125,570 |
| Named graph per prediction | 785 quads | 106,767 |

In rows the RDF-star block is 2.8× smaller than reification and 2.4× smaller than named graphs; in bytes the gaps are 1.5× and 1.3×, because the extra rows are short. Most of the difference is the cost of making a curated statement citable: RDF-star quotes it in place, while the other encodings need four extra rows per cited statement.

We also asked both triple-based stores the same question for each of the 169 entities X, "which generated triples cite a statement whose object is X?" (a nested SPARQL-star pattern on one side, a five-pattern join over reification nodes on the other). Both returned identical answers for all 169 entities, with the stores reloaded from disk. Median latency over HTTP was 0.66 ms for the RDF-star query and 0.56 ms for the reified join (p95 1.08 and 0.97 ms). An earlier executor took 5.14 ms on the RDF-star query, because it found quoted subjects by hashing every stored triple; it now walks the rows of the bound annotation predicate and dereferences each quoted subject through the reverse index (§2.1). The RDF-star encoding needs 2.8 times fewer rows (1.5 times fewer bytes) than reification, but on this query the reified join was about 0.1 ms faster: the advantage shown here is in storage, not query speed. Loka has no named-graph support, so (b) was counted but not queried, and the comparison with it is of rows and bytes only.

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

Entity-valued objects are those whose label also occurs as a subject in training. Neither learned model beats predicate frequency on this split, with one exception within noise: on literal-valued objects the v13 transformer's Hits@1 is 0.020 against 0.018 (about 27 more queries of 13,596 ranked first), while its MRR and every other column on that subset are lower. The v13 transformer ranks above the untuned TransE, but with no tuning for either model we do not read that as a comparison of methods. The result is consistent with the model's training perplexity (Appendix A): it is a weak completion model. This is why the paper's claims concern the provenance machinery and not the quality of what the model predicts; the machinery is independent of the model it records.

### 6.5 Independence from the generator

The provenance record depends on the candidate selector, not on the model that fills in the object. To check this, we reran the two-pass pipeline of §6.2 on the 153k-triple graph with the v13 model replaced by the predicate-frequency predictor of §6.4 (for a proposed (s, p), the most frequent object of p that s does not already have), emitting for every proposed pair. Selection, filtering and annotation code were unchanged.

| Generator | Generated triples | Citations per prediction (median / mean / p90 / max) | Required removals checked | Missed / extra |
|---|---|---|---|---|
| v13 model | 356 | 9 / 10.5 / 20 / 26 | 4,354 | 0 / 0 |
| Predicate frequency | 8,637 | 9 / 10.3 / 19 / 36 | 127,958 | 0 / 0 |

Per prediction, the provenance cost is nearly the same under both generators: the medians are equal, the means and 90th percentiles differ by less than one citation, and the maxima are 26 and 36. What differs is mainly the number of predictions. The frequency run produced 3,818 second-pass predictions citing first-pass ones, so its retraction check covered far more chained dependencies, and committed retractions of 30 entities on it (44,131 rows) took a median of 34 ms (maximum 0.25 s), with a restart reloading exactly the expected rows. We did not test stronger learned generators; by this result, a generator changes what is predicted, not what the record costs.

---

## 7. Limitations

### 7.1 Model and decoding

- **Weak completion accuracy.** The case-study model does not beat a predicate-frequency baseline on held-out triples (§6.4).
- **Mode collapse on common tokens.** Even with the cumulative penalty, predictions for predicates the model knows weakly fall back to connectors (`of`, `and`) or to format placeholders (`spouse -> "1 ."`, §5.4). The corpus cleanup removed the worst of these (§5.3) but not all.
- **Label-space output.** The model emits subword tokens of an English label. Exact resolution to an entity IRI (§4.4) succeeded for 5 of 356 and 8 of 672 predictions on the two real graphs, so nearly all stored predictions are literals that can contain BPE fragments.
- **Greedy decoding only.** No beam search or sampling.

### 7.2 Provenance

- **Selection provenance is not support.** A `propositionSelectedFrom` row points at a concrete stored statement, which is auditable, but the statement was chosen by the candidate-selection heuristic (§4.4 step 1), and the model does not see it: the model's input is the subject and predicate labels only. A cited statement may have played no part in the predicted value.
- **One statement per neighbour.** A proposal depends on several statements of each contributing neighbour; only one is cited (§4.4). Retracting the neighbour node reaches the prediction, but deleting a single other statement of that neighbour would not, if Loka supported statement-level retraction (it retracts nodes).
- **One justification per triple.** The procedure proposes only predicates a subject does not already have, and later passes see earlier predictions in the store, so it never writes the same generated triple twice. If another writer stores an identical triple with its own annotation block, the two citation sets merge, and retracting a source of either removes the triple. That over-deletes relative to a system that counts alternative derivations (Gupta et al., 1993; §2.5), which Loka does not do.
- **Curated annotations outlive their triple.** Retraction sweeps only reserved-namespace annotations, so when it removes a curated statement that carries Wikidata qualifiers or references, those rows stay behind on the quoted, no longer asserted statement. RDF-star allows annotating an unasserted quoted triple, so the store remains well-formed, but the rows describe a statement the store no longer holds. Sweeping them as well would not change the agreement checks of §6.1–6.2, which compare generated triples and their annotations, but it would change the removal totals, committed row counts and commit timings reported in §6.2; the behaviour is pinned by a unit test and left as is for this paper.
- **Bounded selector.** Provenance is complete relative to what the bounded selector consulted (§4.4); a statement or neighbour beyond the first 20 is not consulted, so it neither influences a proposal nor is cited.
- **The interactive generator is outside this record.** The released engine also has an interactive path (`tools/retrieval_generate.py`) in which a pretrained language model continues a list of retrieved neighbourhood triples. It writes the generated flag, model and confidence, so its output is excluded from training corpora (§3.1), but it writes no `propositionSelectedFrom` edges, so cascade retraction does not reach it, and by §2.6 its provenance would be incomplete in any case. Everything evaluated in this paper uses the procedure of §4.4.
- **Earlier outputs used a cruder rule.** Earlier versions of the procedure cited the first ten of the subject's statements regardless of which ones matched, so it could both cite irrelevant statements and miss the one that mattered. Generated triples produced under that rule should be re-generated before relying on retraction.

### 7.3 Evaluation scope

- The link-prediction set is small (19,686 rankable queries), skewed toward literal-valued predicates, and drawn from the triples a larger label cache newly resolved rather than sampled uniformly from Wikidata. Entities are identified by English label, so distinct entities with the same label are merged, which affects the transformer and TransE alike.
- No model was tuned, and only one checkpoint (v13) has a held-out set that requires no retraining.
- Retraction was evaluated on synthetic graphs of up to 5M rows and on real Wikidata stores of 153k and 2M triples with dependency chains of two generated hops, on one machine; longer real chains were not built, and inference over the 2M-triple store covered a sample of 3,000 subjects.
- The source dataset revision was not pinned when the corpora were built (References). The released corpora are fixed, but the path from source dump to corpus cannot be replayed exactly.

---

## 8. Discussion

Two directions would strengthen the provenance record itself.

**Ontology templates as the selector.** OWL ontologies can be stored in the engine as triples, though the engine does not reason over them. An ontology could serve as the candidate selector: a class declares the properties its instances are expected to have, the inference loop proposes the missing ones, and `propositionSelectedFrom` cites the class declaration alongside the subject's statements. The citation would then name the reason a predicate was proposed, which the current neighbour heuristic only approximates.

**An entity-space decoder.** The engine's HNSW vector index could resolve a predicted embedding to the nearest known IRI, so that predictions are entities rather than label strings. That would make completion directly comparable with entity-ranking methods and remove BPE artifacts from stored output.

Beyond these, the same store could serve as a harness for training models on its own graph, with the vocabulary keeping each generation's output distinguishable from the data it was trained on. We leave that open; this paper claims only the provenance record and what the store does with it.

---

## 9. Conclusion

We described how a triplestore can hold model-generated statements next to curated ones without losing track of them: each generated triple carries RDF-star annotations, in a reserved namespace, naming its model, its confidence and the stored statements its procedure used. The flags keep generated statements out of the corpora Loka extracts, and the dependency edges support cascade retraction, which we tested against an independent reference (finding and fixing one defect) and timed at about 85 ms to compute a 106k-triple retraction set in a 5M-row store, in memory, with cost that, on the two large synthetic data points that isolate store size, follows the size of the removal rather than of the store. The case-study model is weak at completion, below a frequency baseline, which is why our claims rest on the provenance machinery rather than on the model. Code, checkpoints and corpora are released.

---

## References

- Ali, M., Berrendorf, M., Hoyt, C. T., Vermue, L., Sharifzadeh, S., Tresp, V., Lehmann, J. *PyKEEN 1.0: A Python Library for Training and Evaluating Knowledge Graph Embeddings.* Journal of Machine Learning Research 22(82):1–6, 2021.
- Berrendorf, M., Faerman, E., Vermue, L., Tresp, V. *On the Ambiguity of Rank-Based Evaluation of Entity Alignment or Link Prediction Methods.* arXiv:2002.06914, 2020.
- Bohnet, B., Tran, V. Q., Verga, P., Aharoni, R., Andor, D., Baldini Soares, L., Ciaramita, M., et al. *Attributed Question Answering: Evaluation and Modeling for Attributed Large Language Models.* arXiv:2212.08037, 2022.
- Bordes, A., Usunier, N., Garcia-Durán, A., Weston, J., Yakhnenko, O. *Translating Embeddings for Modeling Multi-relational Data.* Advances in Neural Information Processing Systems 26 (NIPS 2013).
- Bourtoule, L., Chandrasekaran, V., Choquette-Choo, C. A., Jia, H., Travers, A., Zhang, B., Lie, D., Papernot, N. *Machine Unlearning.* IEEE Symposium on Security and Privacy, 2021.
- Buneman, P., Khanna, S., Tan, W.-C. *Why and Where: A Characterization of Data Provenance.* ICDT 2001, LNCS 1973.
- Carroll, J. J., Bizer, C., Hayes, P., Stickler, P. *Named Graphs, Provenance and Trust.* Proceedings of the 14th International World Wide Web Conference (WWW 2005), 613–622.
- Devlin, J., Chang, M.-W., Lee, K., Toutanova, K. *BERT: Pre-training of Deep Bidirectional Transformers for Language Understanding.* NAACL 2019. arXiv:1810.04805.
- Doyle, J. *A Truth Maintenance System.* Artificial Intelligence 12(3):231–272, 1979.
- Green, T. J., Karvounarakis, G., Tannen, V. *Provenance Semirings.* PODS 2007.
- Groth, P., Gibson, A., Velterop, J. *The Anatomy of a Nanopublication.* Information Services & Use 30(1–2):51–56, 2010.
- Gupta, A., Mumick, I. S., Subrahmanian, V. S. *Maintaining Views Incrementally.* SIGMOD 1993, 157–166.
- Hartig, O. *Foundations of RDF\* and SPARQL\* — An Alternative Approach to Statement-Level Metadata in RDF.* Proceedings of the 11th Alberto Mendelzon International Workshop on Foundations of Data Management (AMW 2017).
- Keskar, N. S., McCann, B., Varshney, L. R., Xiong, C., Socher, R. *CTRL: A Conditional Transformer Language Model for Controllable Generation.* arXiv:1909.05858, 2019.
- Lebo, T., Sahoo, S., McGuinness, D. (eds.). *PROV-O: The PROV Ontology.* W3C Recommendation, 30 April 2013. https://www.w3.org/TR/prov-o/.
- Lewis, P., Perez, E., Piktus, A., Petroni, F., Karpukhin, V., Goyal, N., et al. *Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks.* NeurIPS 2020. arXiv:2005.11401.
- Saxena, A., Kochsiek, A., Gemulla, R. *Sequence-to-Sequence Knowledge Graph Completion and Question Answering.* Proceedings of ACL 2022 (Volume 1: Long Papers), 2814–2828.
- Sun, Z., Deng, Z.-H., Nie, J.-Y., Tang, J. *RotatE: Knowledge Graph Embedding by Relational Rotation in Complex Space.* ICLR 2019. arXiv:1902.10197.
- Vaswani, A., Shazeer, N., Parmar, N., Uszkoreit, J., Jones, L., Gomez, A. N., Kaiser, Ł., Polosukhin, I. *Attention Is All You Need.* NeurIPS 2017. arXiv:1706.03762.
- Vrandečić, D., Krötzsch, M. *Wikidata: A Free Collaborative Knowledgebase.* Communications of the ACM 57(10):78–85, 2014.
- W3C RDF-star Community Group. *RDF-star and SPARQL-star.* Final Community Group Report. https://w3c.github.io/rdf-star/cg-spec/.
- Wylot, M., Cudré-Mauroux, P., Groth, P. *TripleProv: Efficient Processing of Lineage Queries in a Native RDF Store.* WWW 2014.
- Yao, L., Mao, C., Luo, Y. *KG-BERT: BERT for Knowledge Graph Completion.* arXiv:1909.03193, 2019.

**Software and data.**

- Loka engine, release `v0.4.7` (includes the retraction fix of §6.1, the `propositionSelectedFrom` name and the §6.2 baseline benchmark). https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.7. AGPL-3.0-or-later.
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

v11–v14 are trained on the four released tiers of the streamed corpus (§5.5: `v11-50k`, `v12-100k`, `v13-500k`, `v14-1M`, named by entity rows read). The released checkpoint is the lowest-perplexity epoch kept, except v12, whose lower epoch-4 checkpoint (226.86) was not retained. Per-epoch checkpoints for v13 and v14 are published separately; v12's were not kept.
