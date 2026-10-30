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

Second LLM review round (Emma, 2026-10-29 evening): the paper changed since this morning's
round (rename, five-run timings, SPARQL baseline, new title, v0.4.7). Each reviewer gets the
current `/arxiv/` PDF (`pages/arxiv/loka-retractable-provenance.pdf`, 18 pages, `c49db95`) and
the same prompt as `paper/reviews/llm-round-2026-10-29.md`, in Chrome via the extension. Be
persistent (Emma): retry, try another upload route (file input, drag-drop), log in or accept
terms via Emma only if truly required; a reviewer counts as unavailable only after several
different approaches fail. Record each verdict, link and main points in
`paper/reviews/llm-round-2026-10-29b.md`.

1. Review by DeepSeek (chat.deepseek.com).
2. Review by Meta AI (meta.ai).
3. Review by Gemini (gemini.google.com).
4. Review by ChatGPT (chatgpt.com, a regular chat).
5. Review by Claude (claude.ai).
6. Review by Grok (grok.com).
7. Review by Mistral (chat.mistral.ai).
8. Review by Qwen (chat.qwen.ai), extra reviewer.
9. Review by Microsoft Copilot (copilot.microsoft.com), extra reviewer.
10. Review by Perplexity (perplexity.ai), extra reviewer.
11. Summarise the round: points raised by more than one reviewer, and queue the fixable ones.

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
