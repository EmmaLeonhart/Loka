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

1. §6.2 2M-store ledger: give the pre-retraction row count (2,015,262, from
   `training/logs/retract_commit_hf2m.json`) and say what the 649 rows beyond imported +
   generated + annotations are, if the logs show it; otherwise say they are unaccounted for.
2. Abstract: "about 0.1 s" → the in-memory preview time (~85 ms), commits separate; say 8,621 is
   (entity, generated triple) pairs.
3. §6.2: one sentence tying the two `retract_set` tables together; "any SPARQL-star store" →
   "in principle; tested only on this engine".
4. §6.2: state the laptop's CPU, RAM and OS (check `planning/system-instability-verdict-2026-05-13.md`).
5. §6.3: say the reification baseline uses `prov:wasDerivedFrom` (not the `prov:used` of §2.2)
   and why, and that named graphs are compared by size only.
6. Rebuild and refresh `/arxiv/`.

Held for Emma's framing decision (2026-10-29 evening: "training isn't a big topic of this paper
... this is essentially an ontology paper"): the paper may drop training exclusion as a
contribution and shrink §5/Appendix B, which would make these two edits moot:
- Abstract and §3.1: state that exclusion holds for output written through Loka's pipeline (flagged), not for triples another tool posts unflagged; Contribution 1's "even if one guard regresses" holds for the output side only.
- Abstract, §2.4, §2.6: say selection provenance records what the selector consulted, not what the model learned in training; retraction does not remove a statement's influence on weights.
- Possible replacement for the store-size benchmarks: a Lean proof that `retract_set` does a
  bounded number of index lookups per removed triple (cost ∝ removed × log store size).

Not queued, for Emma (new experiments; INTENT says ask first): the SPARQL closure on another
RDF-star store (Jena/Oxigraph); a store-size × removal-size grid; an extractor run on the
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
