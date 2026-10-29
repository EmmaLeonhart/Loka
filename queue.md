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

Emma, 2026-10-28, in this order (cleanvibe already cloned to `../cleanvibe`, v2.0.6):

1. Paper §1 / §3.2: say in the contributions and §3.2 that `propositionInferredFrom` records
   candidate-proposal dependencies, not value-generation justification (Gemini).
2. Paper §3.4: say what statement-level retraction would need (index of statement-level citations,
   cite every consulted statement of each neighbour) (Gemini).
3. Paper §5: condense the v3–v10 training narrative in the main text to a summary and move the
   debugging detail to an appendix (Gemini: model story distracts from the systems contribution).
4. Finish: push, confirm Paper PDF green, refresh `/arxiv/` (PDF, tarball, abstract copies),
   confirm the papers-ci clawRxiv submission ran, and record the new clawRxiv review when it lands.


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
