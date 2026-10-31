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
   Submit → New and fills it from `paper/arxiv/METADATA.md`: licence CC BY 4.0, primary cs.LG,
   cross-lists cs.DB and cs.AI; upload `pages/arxiv/loka-retractable-provenance-source.tar.gz` (the
   `d669ec8`+ build with the AI-assistance section, 18 pages); paste title, abstract and comments. If arXiv asks for an
   endorsement, show Emma the code and stop; requesting it is hers. Check arXiv's compiled PDF
   against `pages/arxiv/loka-retractable-provenance.pdf` (18 pages, named author). Stop before
   the final Submit and get Emma's explicit yes. After announcement, put the arXiv ID on the
   `/arxiv/` page and in README.md.
   **State 2026-10-30 ~16:25 PST:** Emma is signed in. The account already has two incomplete,
   untitled submissions, `submit/8274430` and `submit/8274728` (both expire 2026-11-06), so no
   third one was started. On `submit/8274430/start` the contact certification and the Submission
   Agreement are ticked (Emma's "Accept"), and the archive is set to cs, subject Artificial
   Intelligence (Emma then changed it to cs.LG primary, cs.DB + cs.AI cross-lists, now set to Machine Learning). The Claude Code permission check blocked Claude from clicking two radios: the
   Authorship one ("I am submitting as an author") and the CC BY licence. Emma has to click those
   two and then Continue. After that, Claude does Add Files onward. Delete `8274728` only if Emma
   says to.
   **State 2026-10-30 ~19:45 PST:** `submit/8274430` is complete up to the final Submit. Emma
   said "agree to them", so Claude ticked the certification, the Submission Agreement, authorship
   and CC BY 4.0, and set primary cs.LG. The tarball uploaded and compiled on arXiv (pdflatex,
   TeX Live 2025, 18 pages, AI-assistance section present). Title, author, abstract and
   comments come from METADATA.md, and cross-lists cs.DB and cs.AI are added. The arXiv PDF
   was previewed. **Waiting on Emma's explicit yes to click Submit.** `8274728` is untouched.

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
