# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ⭐ FIRST — arXiv-readiness timeline for the Loka paper (planned 2026-10-06)

Goal (Emma, 2026-10-06): the bare-minimum `paper/` that gets **Accept / Strong Accept** on the
review site and can be posted to **arXiv**. Rules for every step: cut claims rather than invent
results; every number comes from a run actually performed and recorded; every reference is real
and checked; never fabricate a metric, baseline or citation. Decisions made while planning are in
`DEVLOG.md` (2026-10-06); the target title, claim, cuts and outline are in
`planning/arxiv-readiness.md`. Work these top to bottom, one per tick, each in its own commit.

**Review site:** the repo's review loop is clawRxiv (`https://clawrxiv.io`), driven by
`.github/workflows/papers-ci.yml` on any push touching `paper/paper.md`. Intermediate rewrite
commits carry the `Skip-Submit: true` trailer so a half-rewritten paper is never posted; only
step 9 submits.

4. **(target 2026-10-10) Held-out data check.** Find out, from the recorded corpora on HF
   (`EmmaLeonhart/normalized-wikidata` tags `v11-50k`…`v14-1M`), whether a set of triples
   exists that a shipped checkpoint was never trained on but whose entities it saw (needed for
   transductive link prediction). Record the answer in `planning/arxiv-readiness.md`.
5. **(target 2026-10-12) Link-prediction harness, no training.** `training/eval_linkpred.py`:
   for held-out (s, p, ?) queries, rank candidate object labels by the model's masked
   log-likelihood; report filtered MRR and Hits@1/3/10, plus a predicate-frequency baseline on
   the same split. Inference only. If step 4 found no usable split, the held-out set needs a
   retrain: **ask Emma with AskUserQuestion before any training run** (CLAUDE.md: no silent
   run escalation); if declined, the paper drops every completion-performance claim and keeps
   perplexity only as a training diagnostic.
6. **(target 2026-10-13) Standard baseline on the same split.** TransE (and DistMult if cheap)
   via PyKEEN on CPU over the identical split; results table next to step 5. Skip only if step
   5 resolved to the cut-claims route.
7. **(target 2026-10-14) Retraction evaluation.** Measure `retract_set` on synthetic provenance
   graphs of increasing size (correctness against a brute-force reference + latency), as a
   criterion bench or test. This is the systems contribution's evidence; record real numbers.
8. **(target 2026-10-15) Related work + references.** Write a Related Work section (KG
   completion, provenance/PROV-O, RDF-star annotation, retrieval attribution). Every reference
   checked against its publisher/arXiv page; drop anything that can't be verified.
9. **(target 2026-10-16) Clean build + submit for review.** Get `pandoc` + `latexmk` building
   `paper.tex` to PDF locally with no undefined refs or overfull-table disasters; check
   abstract length; then push `paper.md` **without** `Skip-Submit` so clawRxiv reviews it.
10. **(target 2026-10-17 → 2026-10-24) Review iteration.** Read each new review in
    `paper/reviews/`; fix what is fixable by cutting or by real runs; resubmit. Stop at Accept /
    Strong Accept, or at 2026-10-24 with a written list of what the remaining cons would need.
11. **(target 2026-10-25) arXiv package + handoff.** Build the arXiv source tarball
    (`paper.tex`, generated body, `neurips_2026.sty`, figures), verify it compiles from a clean
    directory, write the metadata (title, abstract, `cs.DB` + `cs.AI`, license). Ask Emma with
    AskUserQuestion who the arXiv author list and endorser are. Submitting is Emma's action.

---

## ACTIVE — computed values: stage 4 (projected expressions + ORDER BY)

**Stages 1–3 are done.** `BIND` over a computed string binds and renders in every result format —
JSON, CSV, TSV, XML, the CLI table, MCP and the FFI boundary — each with its own test asserting the
value appears AND that `_:id` does not. (Turtle/N-Triples turned out not to apply: that renderer only
serves `export_graph`, which reads the store, and a computed id can never be stored.) Design + status:
`planning/computed-values.md`.

**Stage 4:** `SELECT (expr AS ?v)` and `ORDER BY expr`. Parser work as well as executor —
`(expr AS ?var)` in the select clause does not parse today. ORDER BY on a computed value must compare
the STRING, never the id: ids are assigned in first-computed order, which is the same trap that made
negative-integer ordering wrong on 07-29.

**Stage 5** after it: `GROUP BY` on a computed value — nearly free, since interning is by value, so
equal strings already share an id. That one closes Pramana's type-count query, which currently groups
on the full IRI and folds local names client-side.

---

## ACTIVE — operator precedence inside FILTER arithmetic

The last piece of the SPARQL 1.1 `Expression` grammar. `parse_arith_operand` is one
left-associative loop over `+ - * /`, so `?a + 2 * 3` is `(?a + 2) * 3` where SPARQL means
`?a + (2 * 3)`. Pinned by `arithmetic_has_no_operator_precedence_yet`
(`loka-sparql/tests/filter_numeric_ordering.rs`) on a case where the readings select different
rows, so it is a known divergence rather than a silent one.

Same shape as the `&&`/`||` split done 07-29: an `AdditiveExpression` loop over a
`MultiplicativeExpression` loop. It re-associates queries that already parse, so it goes in its
own commit with the pinning test rewritten to assert precedence — not as a drive-by.

Worth doing together with **unary minus** (`FILTER(-?a > 5)`), which `parse_arith_operand` does
not accept at all: it calls `parse_term` first, so a leading `-` is only handled when it is part
of a numeric literal.

---

The rest of the queue is drained. Remaining work is either GPU-gated
(v11–v14 training, propgen tests, clean v12 retrain, donor clean-Adam v14) or
Emma-gated (SDK first publish). The autonomous work-loop cron promotes the next
genuinely-unblocked, bounded `TODO.md` item into this file each tick — see
`TODO.md` for the horizon and `planning/sdk-publish-readiness.md` for the
publish verdict.

---

## Pinned tail — autonomous-loop cron management

These two items are always the last in the queue (autonomous-loop playbook §d):

1. **Ensure the three crons are running** — work-loop (`3 * * * *`), auto-flush
   (`15 * * * *`), status-report (`42 * * * *`). Start them if this session
   never did; restart them if a planning burst / queue re-fill killed them.
2. **Run the status-report action once more, independently** — an end-of-session
   summary of everything that happened this session.

---

## Reference

- **`TODO.md`** — longer-horizon work (includes the now-relocated engine-bug #1
  ingest-verification watch and the GPU-gated training follow-ups).
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
- **`planning/cascade-retraction.md`** — spec for the shipped retraction system.
- **`planning/base-retrieval.md`** — spec for the shipped base+retrieval pivot.
- **`planning/sdk-publish-readiness.md`** — SDK publish verdict (Emma-gated).
