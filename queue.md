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

Review-round fixes (Emma, 2026-10-28: "attempt to address" the points from the ChatGPT /
Claude / Grok / Mistral reviews of the `/arxiv/` PDF). One item per commit, in order:

1. §3.1: add `propositionPredictedLabel` to the reserved-namespace table (it is written under
   the reserved prefix by `training/infer_with_citations.py:457` but missing from the "full
   namespace" list); fix the `propositionGeneratedFrom` naming example, which is not a real
   predicate; confirm the corpus filter and retraction sweep cover it.
2. §6.4: "Neither learned model beats predicate frequency" is contradicted by v13's literal
   Hits@1 (0.020 vs 0.018); state the exception.
3. §4.4: the repetition penalty divides softmax probabilities (`p[t]`), not logits — fix the
   formula; state that `propositionConfidence` is the mean of post-penalty probabilities and is
   not calibrated.
4. §6.2: per-removed-triple cost is ~0.3–0.4 µs on the two small graphs and ~0.8–0.9 µs on the
   two large ones; say so instead of resting the "follows the removed set" claim on two points
   without comment; note the byte ratio (≈1.5×) next to the row ratio in §6.3.
5. Investigate: does retracting a node leave curated RDF-star annotations (Wikidata qualifier /
   reference rows) on a deleted quoted triple? If yes: fix in `loka-core`, add a regression
   test, and describe it in §6.1/§3.4. If no: say in §3.4 why not.
6. §3.4/§7.2: state the retraction invariant explicitly (node-rooted; a shared triple is removed
   when any of its recorded derivations loses a source) as one short definition.
7. Rebuild: push, confirm the "Paper PDF" run is green, refresh `pages/arxiv/` (PDF + source
   tarball) from that run's artifacts, check `/arxiv/` serves it.
8. Ensure the three crons (:03 work-loop, :15 auto-flush, :42 status) are still running.
9. Run the status-report action once more as an end-of-burst summary.


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
