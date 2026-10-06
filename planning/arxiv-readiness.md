# arXiv readiness — framing memo

Step 1 of the arXiv-readiness timeline in `queue.md`. This file fixes what the rewritten paper
claims, what it stops claiming, and its shape, so steps 2–11 edit toward one target. Source
material: `paper/paper.md` as of `ccb0a6e` and the eight clawRxiv reviews in `paper/reviews/`.

## Title

**Loka: Retractable Provenance for Model-Generated Triples in an RDF-star Store**

Dropped from the old title: "Generative Citation" (names a mechanism the model does not perform)
and "Neuro-Symbolic World Model" (every reviewer called it superficial; the coupling is a shared
store and query interface, which the new title describes directly).

## Renaming the citation mechanism

Old: **generative citation**. New: **selection provenance**.

What the system records under `propositionInferredFrom` is up to ten of the subject's existing
statements, which are the input to the candidate selector (it proposes a predicate from graph
neighbours sharing a (predicate, object) key). The model never sees them: its input is the
subject and predicate labels only (`training/infer_with_citations.py`). So the edge is not
attention, not a learned retrieval head, not model conditioning, and not a claim that the cited
triple supports the prediction. "Selection provenance" means exactly "input to the procedure".
(Corrected 2026-10-06 in step 3: the first draft of this memo said the model's input was
conditioned on the cited triples. The code shows it is not.)
The predicate IRIs (`propositionInferredFrom`, etc.) stay as they are — they are shipped API —
and the paper states in one sentence what the edge means and does not mean.

## The claim the paper defends

> Writing model-generated triples into the same RDF-star store as curated data, each annotated
> under a reserved namespace with its generator, its confidence, and quoted pointers to the
> stored statements the prediction procedure took as input, makes model output (i) queryable and
> filterable with ordinary SPARQL-star, (ii) structurally excluded from future training corpora,
> and (iii) retractable: withdrawing a source removes every generated triple that transitively
> depended on it, without touching curated data.

This is a data-management contribution. The model is a case study that exercises the schema; it
is not the contribution and its quality is not a headline.

## Claims being cut

- "Generative citation" as a model capability (renamed, see above).
- "Neuro-symbolic world model", "explicit + implicit memory" as framing.
- Perplexity as a headline number. It stays only as a training diagnostic, in the appendix table.
- The v3→v14 progression as a contribution. It becomes one appendix table.
- "Corpus scale is the binding constraint" — a perplexity trend across corpora of different
  size *and* different content; not a controlled result. Cut.
- The cumulative repetition penalty as a contribution. It is the CTRL-style penalty applied per
  occurrence; it becomes an implementation detail in the decoding paragraph.
- Engine bug history (§6.1), hardware, commit hashes, cron automation, GPU crashes, contributor
  runs, dates, and the paragraph arguing with a reviewer (§6.3 "Position taken").

Kept, compressed: the catalog-noise observation (external-id predicates were ~half the raw
corpus and produced identifier-shaped hallucinations; excluding them removed those) as a
corpus-construction note, because it is real, recorded, and useful to anyone training on
Wikidata.

## Category

`cs.DB` primary, `cs.AI` cross-list.

## Target outline

1. Introduction — problem (model output mixed into a KG is unauditable and unremovable), claim,
   contributions (schema + namespace enforcement; cascade retraction; case study + evaluation).
2. Related Work — KG completion (TransE, KG-BERT, KGT5); provenance (PROV-O, W3C RDF-star);
   attribution in retrieval-augmented generation.
3. Provenance schema — reserved namespace, the annotation block, three-layer exclusion.
4. Retraction — semantics (follow only `propositionInferredFrom`, bounded to the namespace,
   cycle-safe), implementation, dry-run default.
5. Case study — corpus construction (label substitution, catalog-noise exclusion), model,
   inference procedure with selection provenance.
6. Evaluation — retraction correctness + latency (step 7); link prediction with baselines if a
   held-out split exists without new training (steps 4–6), otherwise no completion claim.
7. Limitations — selection provenance is not support; label-space prediction; scale.
8. Conclusion.
Appendix — model versions table (params, corpus size, epochs, perplexity).

## Held-out split (step 4 result, 2026-10-06)

**A transductive held-out set exists for the v13 checkpoint without any new training.**

- `train.py` has no validation split; its reported perplexity is exp(mean training loss) over
  the epoch. So no checkpoint has a built-in held-out set.
- The corpus tiers are prefixes of one unshuffled stream (`tools/preprocess_from_hf.py
  --max-rows N`), so `v14-1M` contains triples `v13-500k` does not, and the v13 model never
  trained on them.
- Measured with `tools/heldout_split_check.py` over both corpus files:
  - v13-500k: 2,511,771 lines, **1,663,040 unique triples**, 816,826 distinct entity/literal
    labels, 1,348 predicates.
  - v14-1M: 4,021,409 lines; 1,058,279 unique triples not in v13, of which 925,171 have a
    subject label v13 never saw (inductive, unusable for TransE).
  - **Transductive held-out (subject, predicate and object labels all occur in v13): 28,448
    unique triples over 542 predicates.** (Step 4 first reported 29,893 and 1,142,131; both
    counted duplicate lines in the v14 file. Corrected in step 5; the script de-duplicates now.)
    6,943 have an (s, p) that already has another object in v13's training data.
  - Dominated by literal-valued predicates (population 3,905; date of birth 1,839; publication
    date 1,646; inception 1,422; elevation 1,202), then entity-valued ones (located in the
    administrative territorial entity 610; instance of 557; given name 511).
- Caveats to state in the paper: entities are identified by English label, so two entities with
  the same label merge (no QIDs in the corpus); the held-out set skews to whatever the larger
  label cache newly resolved, not a random sample of Wikidata.
- Step 5 evaluated **v13** on this set. Decisions: candidates for (s, p, ?) are the objects
  seen with p in v13 training (predicate-constrained); queries whose true object was never an
  object of p in training are dropped (8,762 of 28,448, leaving 19,686); results are reported
  overall and split into entity-valued objects (object label also occurs as a subject) and
  literal-valued ones, rather than dropping literals.

## Link-prediction result (step 5, 2026-10-06)

`training/eval_linkpred.py`, output `training/logs/linkpred_v13.json`. Filtered,
predicate-constrained, realistic rank. 19,686 queries (6,090 entity-valued, 13,596 literal).

| | MRR | Hits@1 | Hits@3 | Hits@10 |
|---|---|---|---|---|
| v13 model, all | 0.115 | 0.085 | 0.118 | 0.170 |
| predicate frequency, all | 0.129 | 0.090 | 0.131 | 0.202 |
| v13 model, entity-valued | 0.287 | 0.230 | 0.308 | 0.399 |
| predicate frequency, entity-valued | 0.318 | 0.250 | 0.333 | 0.451 |
| v13 model, literal-valued | 0.038 | 0.020 | 0.033 | 0.068 |
| predicate frequency, literal-valued | 0.044 | 0.018 | 0.040 | 0.090 |

Step 6 added TransE (`training/baseline_kge.py`, output `training/logs/linkpred_v13_transe.json`;
PyKEEN 1.11.1, dim 128, 20 epochs, Adam lr 0.001, batch 4096, sLCWA, untuned) on the same split
and protocol:

| | MRR | Hits@1 | Hits@3 | Hits@10 |
|---|---|---|---|---|
| TransE, all | 0.074 | 0.044 | 0.079 | 0.133 |
| TransE, entity-valued | 0.202 | 0.128 | 0.224 | 0.358 |
| TransE, literal-valued | 0.017 | 0.006 | 0.015 | 0.032 |

Ordering on this split: predicate frequency > v13 > TransE, on every row. TransE's
hyperparameters are fixed, not tuned (there is no validation split, same as for v13), so the
paper must say "an untuned TransE" and not read the gap as TransE being a weak method. Its
training loss fell to 0.012, so it fit the training graph; the held-out set is small, skewed
toward literals, and keys entities by English label, which hurts an embedding model that
cannot share information between labels. DistMult was dropped: about 3.5 min per CPU epoch made
it not "cheap", which was the plan's condition.


**The v13 model does not beat the predicate-frequency baseline** (only literal Hits@1, 0.020 vs
0.018, is higher). This is consistent with its training perplexity (~245, reproduced from the
checkpoint) and object-token NLL of 5.8 nats. The paper reports it as is: the model exercises
the provenance loop, and its completion accuracy is below a trivial baseline. That supports the
framing decision in this memo (a data-management contribution, not a completion one).

Harness checks done before the full run: the checkpoint reproduces its recorded perplexity
through `train.py`'s own `collate` (245 vs 242.75 recorded); the harness builds the masked input
the same way `collate` does; scoring 300 training triples gives MRR 0.071 vs frequency 0.077, so
even on seen triples the model is near the baseline, which rules out a held-out-specific bug.

## Retraction evaluation (step 7, 2026-10-06)

**Correctness.** `loka-core/tests/retract_reference.rs` generates random provenance graphs (real
triples; generated triples citing 1–3 earlier triples; ~5 % back-citations, so cycles occur;
a `propositionGeneratedBy` row each) and compares `retract_set` to an independent fixpoint over
the generator's own lists. 50 small graphs × 10 roots and 10 medium graphs (2,000 real + 2,000
generated) × 20 roots.

It found a real defect on its first run: a generated triple that touched the root directly was
removed at depth 0, but its provenance annotation rows were swept only when it was reached by a
provenance hop, so they survived as orphans (every missing row was an annotation). The spec in
`planning/cascade-retraction.md` says a removed generated triple goes with "ALL its prov
annotation rows" and has no depth-0 exception, so this was an omission. Fixed in
`loka-core/src/retract.rs`, with a unit test
(`depth_zero_generated_triple_takes_its_annotations`) that fails without the fix and passes
with it. After the fix both reference tests pass; workspace suite 479 passing.

**Latency.** `cargo bench -p loka-core --bench retract` (criterion, release profile, this
laptop, in-memory `TripleStore`). The root is the entity with the largest retraction set among
the first 50.

| Generated triples | Store rows | Triples removed | Max depth | Time (median) |
|---|---|---|---|---|
| 1,000 | 5,034 | 1,439 | 14 | 0.26 ms |
| 10,000 | 50,484 | 7,367 | 24 | 1.91 ms |
| 100,000 | 504,389 | 120,461 | 34 | 58.3 ms |

Paper caveats: synthetic graphs, in-memory store (not the sled-backed persistent one), a
single machine.

## Open items carried by later steps

- Author list and arXiv endorser: ask Emma at step 11.
