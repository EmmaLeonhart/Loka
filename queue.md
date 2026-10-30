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

1. Repeated timing trials for §6.2 (Emma, 2026-10-29: "5 runs each"). Run
   `loka-core/benches/retract.rs` five separate times at all four sizes on the renamed code,
   release build, laptop idle (no `loka serve`/training alongside). Report median and min–max
   across the five runs in the §6.2 table; replace the "one run, 1.6× faster earlier" sentence
   with what the spread shows. Keep the raw criterion outputs under `paper/supplementary/`.
2. External retraction baseline (Emma, 2026-10-29: "Yes, run it"). Time a plain SPARQL
   property-path closure (`?g loka:propositionSelectedFrom+ ?t` plus the touches-the-entity
   rule) on the same four generated stores as §6.2, five runs each, and compare with
   `retract_set`. Check the closure returns the same set. Add a column or short table to §6.2
   and a sentence to §8; drop "no external baseline" from §7.3 if it is there.
3. Retitle the paper to "Loka: Provenance for Retracting Model-Generated Triples in an RDF-star
   Store" (Emma, 2026-10-29). `paper/paper.md`, `paper/arxiv/METADATA.md`, the LaTeX build
   inputs, `pages/arxiv/index.html`, README, CLAUDE.md's focus line, TODO.md.
4. Rebuild and refresh: wait for the Paper PDF run on the new `main`, check the extracted text
   for the rename and items 1–3, and replace `pages/arxiv/`'s PDF and source tarball; update page count and
   abstract on the page and in METADATA if they changed.

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
