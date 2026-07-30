# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ACTIVE — BIND over a string-valued expression needs an interning path

`BIND(REPLACE(STR(?type), "^.*/", "") AS ?local)` now parses and returns an explicit
"not supported yet" execution error. Numeric BIND (`BIND(STRLEN(?x) + 1 AS ?n)`) works, because an
inline integer is encoded in the id. A string result has to be interned, and `execute` takes
`&TermDictionary`.

Two designs, both real changes:

1. **`&mut TermDictionary` in the executor.** Honest and simple, but it is a public-API break
   across loka-proto / loka-ffi / loka-cli, and it makes every query a potential writer — which
   interacts with the concurrency story (`search is &self`) and with persistence.
2. **A per-query value overlay** — computed values live in a side table for the duration of the
   query, with ids from a reserved range that cannot collide with dictionary ids. No API break, no
   dictionary growth from queries. Needs the reserved-range guarantee to be watertight and every
   render path (`resolve`, CSV/Turtle/N-Triples output) to consult the overlay.

Option 2 looks right; it also generalises to `SELECT (?a + 1 AS ?b)` and aggregates over computed
values. Do the range design first and write it into `planning/` before touching code.

**Pramana does not need to wait for this** — its entity page can take the local name from `?type`
client-side, which is a 3-line change in `web/data_access.py`, and it should not carry a workaround
for an engine gap that is being fixed anyway.

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
