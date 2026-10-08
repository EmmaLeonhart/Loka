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

10. **(reopened 2026-10-08) Review iteration toward Strong Accept.** Emma: aim for Strong
    Accept, not Weak Accept (v12). Model stays as is, no training (Emma, 2026-10-08). Work the
    other v12 cons:
    Review v13 (post 2912): **Accept**. Still aiming for Strong Accept (Emma). Next, no training:
    Review v14 (post 2913): **Accept** again. Remaining non-model cons, no training:
    Review v15 (post 2914): Weak Accept (v13/v14 Accept). Emma (2026-10-07): "More background
    research". Read as: position the contribution against the closest prior work.
    Review v16 (post 2915): **Accept**. Remaining cons: weak model, exact-match entity
    resolution, heuristic selector, real-data scale up to 2M (billion-triple untested),
    label-space output: all model-bound or out of laptop reach. Waiting on Emma: push further
    (would need the model) or submit. The arXiv package is ready (paper/arxiv/METADATA.md); the
    upload itself is Emma's action.

---

The rest of the queue is drained. Remaining work is either GPU-gated
(v11–v14 training, propgen tests, clean v12 retrain, donor clean-Adam v14) or
Emma-gated (SDK first publish). The autonomous work-loop cron promotes the next
genuinely-unblocked, bounded `TODO.md` item into this file each tick — see
`TODO.md` for the horizon and `planning/sdk-publish-readiness.md` for the
publish verdict.

---

## ACTIVE — large feature work (Emma, 2026-10-07: "Do the large feature work")

Plan, scope and tests per phase: `planning/large-features.md`. In order:
4. **Phase 4 — cost-based choice of HNSW vs triple scan.**
5. **Phase 5 — background maintenance: low-usage detection, HNSW rebuild + atomic swap.**
6. **Phase 6 — pseudo-table invalidation + planner recognition.**
7. **Phase 7 — query-latency metrics in health.**

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
