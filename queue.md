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

1. **Put the arXiv ID on `/arxiv/` and in README.md once it is announced.** Submitted
   2026-10-30 20:18 PST as `submit/8274430` (cs.LG; cross-lists cs.DB, cs.AI; CC BY 4.0), status
   "processing", in moderation. Waiting on the announcement, usually one or two working days.
   arXiv emails Emma the ID, and the ID shows on her arXiv account page. Once Emma pastes it,
   or a session can see it on the account page, add the abs link to `pages/arxiv/index.html`
   and README.md. If moderation puts the paper on hold or reclassifies it, tell Emma; replying
   to moderators is hers. Unused draft `submit/8274728` expires 2026-11-06 by itself.
   **Timing:** arXiv's daily cutoff is 14:00 ET. This was submitted Friday 23:18 ET, after
   Friday's cutoff, so the earliest announcement is Monday 2026-11-02 at 20:00 ET (17:00 PST),
   unless moderation holds it. Ticks before then skip the account-page check and stay idle.

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
