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

- **Interactive inference path vs §2.6.** The paper argues for from-scratch models because a
  pretrained one's provenance is incomplete; the shipped double-click now uses base Qwen +
  retrieval. Check what that path writes (provenance annotations or not) and decide whether
  the paper needs a sentence saying that path is outside its scope.
- **Licence decision.** METADATA leaves the arXiv licence to Emma. Make the call, record it in
  METADATA and why in DEVLOG.
- **Lift the ON HOLD block in TODO.md** and write the hand-off: the exact steps only Emma can
  take (arXiv account, endorsement for a first cs.DB submission if needed, upload, licence
  click), so the submission is one sitting.
- **Ensure the three crons are running** (work loop :03, flush :15, status :42).
- **End-of-burst status report**: everything this queue did, what is left for Emma.

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
