# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Sutra-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## Active

In strategic order. Top item is the current focus.

1. **Engine bug: `POST /triples` write-flush wedge.** Reproducible at ~5–6× growth in stored triples. `/health` keeps responding; writes and SPARQL hang until restart. Surfaced again on the live `--post` test (queue.md item below) — the script hangs entirely, not just slowly. Diagnose under load; fix in `sutra-proto`/`sutra-core`. Workaround in production code is automated stop-restart.

2. **Engine bug: SPARQL returns literal values in the predicate slot.** ~1% of rows on a 5M corpus. Confirmed in a separate symptom too: `<< ?s ?p ?o >> ?qp ?qv` returns `<<QUOTED_TRIPLE>>` sentinel and literal values in `?qp`. Probably an RDF-star annotation row with positions getting confused on the executor side. Repro and fix.

3. **Wire BPE tokenizer into train.py.** `training/tokenizer_bpe.py` exists (`8252f13`); the swap into `train.py` and `infer_with_citations.py` is the next mechanical step. Future v6 training run uses the BPE vocab.

4. **Bigger corpus.** 27,780 entities of the 30M in `philippesaade/wikidata` is a tiny slice. `tools/wikidata_hf_import.py --max-triples 50000000` would 10× the store. Bandwidth-bound, not API-bound. Engine bug #1 will surface multiple times during the run; the auto-stop-restart loop handles it.

5. **Live `--post` end-to-end test of generative citation.** Attempted with `--max-subjects 30 --post`; script hung 7 min with no output, indicating engine bug #1 triggered during the POST phase. Re-attempt at lower scope after #1 is fixed.

6. **Train v6 with BPE tokenizer.** d_model 512, 6 layers, 5 epochs, but on the BPE-vocabbed corpus. Compare prediction quality on the unicode-name cases (`Saint-Léger`, `Wikipédia`).

7. **Fine-tuning track scaffolding.** `planning/fine-tuning-track.md` defines the parallel near-term track: Qwen 2.5 1.5B-Instruct + QLoRA on the same `triples.txt` format, sharing the `propositionInferredFrom` output schema. Build `training/finetune/`.

8. **Address Gemini 3 Flash review (v1 post 2378).** Six concrete critiques in `paper/reviews/v1_post2378_review.md`. Highest-signal three for a v2 revision:
   - Cite the SutraDB v0.4.0 release URL more prominently (review called the engine "unpublished").
   - Acknowledge the heuristic-citation framing more directly as a v0 design choice (review §6.3 already does this; tighten in §3 too).
   - Add explicit "future work" framing for standard KG-completion metrics (MRR, Hits@k); justify why we're not in that regime today.
   Then `POST /api/posts/2378/revise` for v2.

9. **Repo rename SutraDB → Loka.** Top of `TODO.md` has the full checklist.

---

## Done (2026-05-09 session)

- ✓ Paper draft + clawRxiv workflow stack (`cb61c94`).
- ✓ First clawRxiv submission (post 2378, paperId 2605.02378).
- ✓ Loka HF snapshots v3 / v4 / v5 uploaded with tags.
- ✓ DEVLOG comprehensive history.
- ✓ `pages/loka/` deep dive, `pages/history/` narrative.
- ✓ Homepage reframe — Loka world-model lead, HF + /loka + /history nav (`7871ce7`).
- ✓ HF link prominent in paper (`7871ce7`).
- ✓ clawRxiv loop verified end-to-end — Gemini 3 Flash review v1 committed (`d50fad2`).
- ✓ BPE tokenizer (`8252f13`) — `Saint-Léger` → `['Saint', '-', 'Lé', 'ger']`.
- ✓ Auto-sync cron `fc054cb5` — pulls, rebases, pushes, chains successor.

---

## Reference

- **`TODO.md`** — longer-horizon work (SDK publishing, Maven Central, Cypher/GQL wrappers, premium-tier, ontochronology phases-5+). Items migrate to here when ready.
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
