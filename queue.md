# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Sutra-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## Active

In strategic order. Top item is the current focus.

1. **Reframe `pages/index.html` as the Loka world-model landing.** Add a top-of-page section above "Why SutraDB?" that introduces Loka, the two-system loop, and links to `/loka/`, `/history/`, and the Hugging Face dataset. Update navbar across the site (or at minimum the homepage navbar) to include `/loka/` and `/history/` entries. Title/tagline shift: lead with "neuro-symbolic world model"; keep the database-superset pitch as a second-screen feature.

2. **Link the Hugging Face dataset prominently in `paper/paper.md`.** Add a footnote or inline reference near the abstract pointing at `https://huggingface.co/datasets/EmmaLeonhart/loka` with snapshot tags. Cite `philippesaade/wikidata` source dataset by URL in references (already done as a bare URL — confirm and tighten).

3. **Verify the clawRxiv loop end-to-end.** Post 2378 was created; trigger `pull-reviews.yml` (the missing `scripts/` files are now in place), confirm the AI peer review lands under `paper/reviews/v1_post2378_review.{json,md}`, and commit those artifacts.

4. **Engine bug: `POST /triples` write-flush wedge.** Reproducible at ~5–6× growth in stored triples. `/health` keeps responding; writes and SPARQL hang until restart. Diagnose under load; fix in `sutra-proto`/`sutra-core`. Workaround in production code is automated stop-restart.

5. **Engine bug: SPARQL returns literal values in the predicate slot.** ~1% of rows on a 5M corpus. Probably an RDF-star annotation row with positions getting confused on the executor side. Repro and fix.

6. **BPE/wordpiece tokenizer.** Word-level tokenizer chops Unicode names: "Saint-Léger" → `saint l ger`. Subword tokenization is the fix. HuggingFace `tokenizers` library is the obvious choice; drop-in replacement for `training/tokenizer.py`.

7. **Bigger corpus.** 27,780 entities of the 30M in `philippesaade/wikidata` is a tiny slice. `tools/wikidata_hf_import.py --max-triples 50000000` would 10× the store. Bandwidth-bound, not API-bound.

8. **Live `--post` end-to-end test of generative citation.** Push v5 generated triples back into the store with `propositionGenerated true` annotations. Confirm the SPARQL-star FILTER excludes them on the next `preprocess.py` run.

9. **Fine-tuning track scaffolding.** `planning/fine-tuning-track.md` defines the parallel near-term track: Qwen 2.5 1.5B-Instruct + QLoRA on the same `triples.txt` format, sharing the `propositionInferredFrom` output schema. Build `training/finetune/`.

10. **Repo rename SutraDB → Loka.** Top of `TODO.md` has the full checklist.

---

## In flight (2026-05-09 session)

- ✓ Paper draft + clawRxiv workflow stack (`cb61c94`).
- ✓ First clawRxiv submission (post 2378, paperId 2605.02378).
- ✓ Loka HF snapshots v3 / v4 / v5 uploaded with tags.
- ✓ DEVLOG comprehensive history.
- ✓ `pages/loka/` deep dive, `pages/history/` narrative.
- ✓ Auto-sync cron `fc054cb5` scheduled — pulls, rebases, pushes, then schedules its successor.

---

## Reference

- **`TODO.md`** — longer-horizon work (SDK publishing, Maven Central, Cypher/GQL wrappers, premium-tier, ontochronology phases-5+). Items migrate to here when ready.
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
