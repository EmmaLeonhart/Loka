# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ACTIVE — arithmetic in FILTER operand position (found closing the two items above)

`parse_comparison_expr` parses `?age + 5 > 30` and then **discards the arithmetic**, building
`Equals/GreaterThan(?age, 30)` from the left variable alone — so the filter silently evaluates
`?age > 30`. Wrong answers, no error. Full writeup in `TODO.md` (§ "arithmetic in FILTER operand
position is parsed and then thrown away").

Not a mechanical fix: `FilterExpr` has no node that can hold the operation, so it needs an AST
addition plus executor evaluation, and a decision on non-numeric operands. Steps:

1. Decide the AST shape — a dedicated `Arith(Term, ArithOp, Term)` usable as a comparison
   operand, vs. a general `Expr` node. Prefer the narrow one unless the general one is needed
   for something already queued.
2. Add executor evaluation over inline integers (the only numeric encoding that exists today);
   non-numeric operands evaluate the comparison to false, matching how unresolvable terms
   already behave rather than inventing an error path.
3. Tests asserting row counts for `?a + n <cmp> v` in all four operators and in both operand
   positions, plus one pinning what non-numeric operands do.

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
