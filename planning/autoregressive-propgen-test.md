# Auto-regressive proposition generation test

Post-training evaluation. The model and corpus are frozen; this measures
*generative behavior* under a specific protocol.

## Question

> Given a real Wikidata seed graph, if we generate ~10 child triples per
> source triple, with each child citing the prior context (including its
> already-emitted siblings), what do we actually get?

The interesting thing being tested is the *protocol*, not the model. The model
itself is the existing role-aware transformer (S, P, masked O → predicted O);
it doesn't natively condition on prior triples. "Auto-regressive" here means
the **context pool** at generation time grows as siblings are emitted, and the
**citation set** on each new triple records the pool's contents at emit time.

## Protocol

### 1. Seed graph
- Pick a random Wikidata QID (uniform over a curated short-list, or via
  `Special:Random`).
- BFS up to depth 10 with a hard cap (default 200 entities, 30s budget).
- Save as N-Triples in `training/data/seed_<qid>.nt`.

### 2. Per-source-triple context construction

For each triple `T = <s, p, o>` in the seed graph:

**a. Adjacency BFS.** Pull all triples that share `s` or `o` with `T` (1-hop
neighborhood in either direction). Cap raw output at 50 candidates.

**b. Asymmetry filter.** Drop a candidate triple `<s', p', o'>` from the
context if:
- `count(* | p', o') > N_PRED_OBJ_MAX` (default 10) — e.g. drops the 5,000
  shrines that worship Amaterasu; flagged separately for reporting.
- `count(p' | s') > N_SUBJ_PRED_MAX` (default 20) — drops subjects with
  pathological outgoing fanout on a single predicate.

After filtering, keep up to 10 adjacent triples → `C_adj`.

**c. Parallel-subgraph extension.** Approximation of the SIMD/pseudotable
sibling lookup, computed in Python because the Rust pseudotable registry
isn't exposed via HTTP yet:
- For each subject `s_i` appearing in `C_adj`, compute its predicate-set
  `P(s_i)`.
- Find all other subjects in the seed graph with the same predicate-set
  (a Python-side pseudotable approximation).
- If a group has ≥3 members, sample up to 10 sibling subjects and add their
  triples to context → `C_simd`.

**d. Initial pool.** `C_0 = C_adj ∪ C_simd`. This is the model-blind context
that triple #1 will cite.

### 3. Auto-regressive generation

For `i in 1..K` (default K = 10) per source triple `T`:

- **Predicate selection.** Look at all predicates appearing in `C_{i-1}`,
  filter out reserved-namespace predicates and predicates already on `s` in
  `C_{i-1}`. Pick the highest-frequency one for which we have a label.
- **Object prediction.** Run the model over `(s_label, p_label, MASK)` →
  `(o_label, confidence)`.
- **Threshold.** If `confidence < τ` (default 0.4), skip and advance to the
  next predicate candidate.
- **Emit.** Write:
  ```
  <s> <p_i> "o_label" .
  << <s> <p_i> "o_label" >> loka:generated     "true"^^xsd:boolean .
  << <s> <p_i> "o_label" >> loka:generatedBy   "<model_version>" .
  << <s> <p_i> "o_label" >> loka:confidence    "0.xx"^^xsd:decimal .
  << <s> <p_i> "o_label" >> loka:inferredFrom  << <s_c> <p_c> <o_c> >> .
    [...one inferredFrom per triple in C_{i-1}, capped at MAX_CITATIONS]
  ```
- **Pool extension.** `C_i = C_{i-1} ∪ {<s, p_i, "o_label">}`. Subsequent
  generations from `T` see the new triple in their context and can cite it.

### 4. Output

- `training/data/test_propgen_<qid>.nt` — all generated triples + citations.
- Console: per-source-triple summary (predicates attempted, accepted,
  citation count, depth-of-recursion).

## What "good" looks like

This is exploratory. We're looking for:

1. **Coherence growth/decay.** Do later generations from the same source
   triple drift toward nonsense, or do they stay anchored?
2. **Citation graph shape.** When a sibling cites another sibling, does the
   citation correspond to a *useful* informational link, or just bookkeeping?
3. **Pseudotable contribution.** Do parallel-subgraph siblings actually
   change the predicate selection meaningfully, or is `C_simd` ignored in
   practice?
4. **Asymmetric-filter effect.** Compare with/without the filter on the same
   seed — does removing the high-cardinality predicate-objects produce
   noticeably more diverse generations?

## Out of scope for v0

- Conditioning the model itself on context. The transformer takes one triple
  at a time. Real context-conditioning would need an architectural change.
- Real pseudotable lookup via the engine. Python approximation only until
  there's an HTTP/SPARQL surface for it.
- Confidence calibration. We use the existing per-token confidence threshold;
  no recalibration based on citation depth.
- Cascade retraction (the queue.md item). Generated triples stay where they
  land; we don't propagate retraction upward when an early sibling is judged
  bad.
