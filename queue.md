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

LLM review round of 2026-10-29 (ChatGPT Weak Reject, Claude and Mistral Weak Accept; details
and sources in `paper/reviews/llm-round-2026-10-29.md`). One fix per item, in this order:

2. §6.5 and §4.4: "the provenance cost is the same under both generators" vs the table's maxima,
   26 vs 36; say median/p90 match and the maxima differ, and check §4.4's quoted maxima (Mistral,
   Claude).
3. §6.3: say plainly that the reified join was faster (0.56 vs 0.66 ms median) and that the
   named-graph comparison is rows/bytes only (ChatGPT, Claude, Mistral).
4. §4.3: "Three model sizes at this corpus size" vs a table with two configurations (Claude).
5. §5.3: the catalog-share sentence "fell from 40 % (v6) to 0 % (v10)" should mention the rise to
   64 % at v7 (Mistral).
6. §6.2: check the units of "4,354 required removals" vs "a maximum of 4,451 triples" from one
   preview, and make them unambiguous (Claude).
7. §3.1 "for that second invariant any single layer suffices": find the test that shows it; if
   there is none, add one or soften the sentence (Mistral).
8. "Enforced at write time" (Abstract, §1, §3.1): check whether `/triples` accepts a
   client-written generated flag; if it does, scope the claim to the inference pipeline (Claude W3).
9. §6.1 vs §3.2: check whether the synthetic correctness test writes the production provenance
   predicates; if not, say so in §6.1 (Claude W6).
10. Emit threshold 0.4 (§4.4) vs ≥ 0.25 in the Appendix B.2 probe: explain or align (Mistral).
11. Minor (Claude): define "object-key" (§4.4 step 1); define confidence once; explain the 2,139
    rows between 155,324 pulled and 153,185 imported (§6.2); remove stray "Preprint." and the
    sentence "Standard." (§4.3); cite TripleProv (Wylot et al.) in §2.
12. Finish: Paper PDF CI green, then refresh `pages/arxiv/` (PDF, tarball, abstract copies).

Not queued, for Emma (bigger than a text fix; INTENT says ask before new experiment runs): an
external retraction baseline (e.g. a SPARQL `inferredFrom+` closure), repeated timing trials,
renaming `propositionInferredFrom`, and the title wording. See TODO.md.

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
