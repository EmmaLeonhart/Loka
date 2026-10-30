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

1. **Upload the paper to arXiv, actively** (Emma, 2026-10-29: "put it in the beginning of the
   queue to basically actively go through the process of uploading"). In Chrome via the
   extension: Emma signs in at arxiv.org herself (agents may not submit a password, even an
   autofilled one; the login page was left open with her username filled). Then Claude starts
   Submit → New and fills it from `paper/arxiv/METADATA.md`: licence CC BY 4.0, primary cs.AI,
   cross-list cs.DB; upload `pages/arxiv/loka-retractable-provenance-source.tar.gz` (the
   `c60a23b`+ build, 17 pages); paste title, abstract and comments. If arXiv asks for an
   endorsement, show Emma the code and stop; requesting it is hers. Check arXiv's compiled PDF
   against `pages/arxiv/loka-retractable-provenance.pdf` (17 pages, named author). Stop before
   the final Submit and get Emma's explicit yes. After announcement, put the arXiv ID on the
   `/arxiv/` page and in README.md.
   **State 2026-10-30 ~14:00 PST:** Emma is signed in. The account already has two incomplete,
   untitled submissions, `submit/8274430` and `submit/8274728` (both expire 2026-11-06), so no
   third one was started. `submit/8274430/start` is open in Chrome on the arXiv Submittal
   Agreement. Accepting it grants arXiv an irrevocable licence, so Emma has to tick it herself.
   After that, Claude fills in the rest. Delete `8274728` only if Emma says to.

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
