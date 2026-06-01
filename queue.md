# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

The actionable queue is drained. Remaining work is either GPU-gated (v11–v14
training, propgen tests, clean v12 retrain, donor clean-Adam v14) or Emma-gated
(SDK first publish). The autonomous work-loop cron promotes the next
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
