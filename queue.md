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

Fixes from LLM review round 2 (`paper/reviews/llm-round-2026-10-29b.md`). Text-only; each
is one paper edit, no new experiment.

1. Compress §5 and Appendix B (model-debugging history) to about half a page plus the version
   table; move the full narrative to `docs/model-history.md` and link it.
2. arXiv category: primary cs.AI, cross-list cs.DB (Emma called it "an AI paper"; my call,
   easy to reverse): `paper/arxiv/METADATA.md`, `/arxiv/` page, TODO.md steps.
3. Rebuild and refresh `/arxiv/`.

Not queued, for Emma (new experiments; INTENT says ask first): a store-size × removal-size grid; an extractor run on the
153k store showing 0 generated/reserved rows; deeper chains; an over-deletion count. Also:
Copilot (Microsoft sign-in), Perplexity and HuggingChat (sign-in) were
not run; each needs Emma's yes.

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
