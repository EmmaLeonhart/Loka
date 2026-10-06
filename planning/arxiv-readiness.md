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

## Open items carried by later steps

- Held-out split availability (step 4) decides whether section 6 has a link-prediction table.
- Author list and arXiv endorser: ask Emma at step 11.
