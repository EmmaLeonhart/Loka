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

1. Tag release `v0.4.7` for the paper to cite (Emma said yes, 2026-10-29). Not pushed yet: a
   `v*` tag also triggers `publish-sdks.yml` (PyPI, npm, Maven, NuGet; at v0.4.6 those ran and
   failed), and SDK publishing is Emma-gated. Asked whether to cancel that run; her reply was
   about the cron waiting on the turn, not an answer, so the tag waits for a clear yes on the
   SDK run. Do NOT push the tag on a cron tick. Once she answers: bump `Cargo.toml` to 0.4.7,
   tag with informative notes (rename + legacy alias, SPARQL baseline bench, five-run timings),
   point paper.md line 3, METADATA's Comments and the `/arxiv/` page at v0.4.7, refresh the
   build.

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
