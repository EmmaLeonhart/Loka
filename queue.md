# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ⭐ FIRST — make the Loka paper submittable to arXiv (Emma, 2026-10-06)

**Goal, in Emma's words:** the bare-minimum version of `paper/` that would get an **accept or
strong accept on the Claude4S review site** and could be **posted on arXiv**. This item comes
before the engineering items below.

**This item's first job is to PLAN, not to edit the paper.** Read `paper/paper.md`,
`paper/paper.tex` and every review in `paper/reviews/` (v1 to v8; v8, post 2601, is a **Reject**).
Then replace this item with a concrete, ordered queue of steps, **each with a target date**,
forming a timeline from today to "submitted to arXiv". Commit and push that queue on its own
before starting step one. After that, work the steps top to bottom like any other item.

**What the reviews already say needs fixing** (v8; check the earlier ones for anything else):
- No standard KG-completion metrics (MRR, Hits@k) and no comparison with existing baselines.
- A small experiment (44M parameters, 4M triples), with perplexity as the main metric.
- The writing reads like a dev log: hardware, commit hashes, cron loops. arXiv needs an academic
  paper.
- "Generative citation" comes from a heuristic candidate selector, not the model, so the name
  overclaims. Either rename it or back it up.
- The "neuro-symbolic" claim is shallow.

**The bare minimum is the point.** Cut claims rather than inventing results. Every number in the
paper must come from a run that was actually performed and recorded. Never fabricate a metric, a
baseline or a citation. arXiv also needs: the LaTeX building cleanly from `paper/paper.tex` to a
PDF, a references list where every entry is real, an abstract within arXiv's length limit, and a
category, probably `cs.AI` or `cs.DB`.

**Unknowns, to ask Emma about with `AskUserQuestion` when they come up, one at a time:** the
Claude4S site's URL and how to submit to it, if nothing in the repo says; and who the arXiv
author and endorser will be. Submitting to arXiv itself is Emma's action, not the session's.

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
