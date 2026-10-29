# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ACTIVE

arXiv submission of `paper/paper.md` (Emma, 2026-10-16: this is the whole queue). The paper
already has an Accept (clawRxiv v16) and a CI-built source package; these items check it is
still sound on today's `main` and get it ready for Emma to submit.

Gemini's review of the fixed build (2026-10-29), one item at a time:

1. §6.2: find why the importer rejected 2,139 of the 155,324 pulled lines (DEVLOG 2026-10-09)
   and state the cause in the paper. If the pulled file is gone and the cause cannot be found,
   keep "not recorded" and say so in DEVLOG.
2. Finish: Paper PDF CI green for the §9 fix (and item 1 if it changed the paper), then refresh
   `pages/arxiv/` (PDF, tarball; check the page count).

Not queued, for Emma (bigger than a text fix; INTENT says ask before new experiment runs): an
external retraction baseline (e.g. a SPARQL `inferredFrom+` closure), repeated timing trials,
renaming `propositionInferredFrom`, and the title wording. See TODO.md.

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
