# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ACTIVE

Empty. Emma's 2026-10-08 queue is done (DEVLOG, 2026-10-08), and the crons are shut down at her
request. Next work: Emma's decisions (release, paper submission) or a new queue.

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
