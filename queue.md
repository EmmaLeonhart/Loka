# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Sutra-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## Active

In strategic order. Top item is the current focus.

1. **Engine bug: `POST /triples` write-flush wedge.** Reproducible at ~5–6× growth in stored triples. `/health` keeps responding; writes and SPARQL hang until restart. Surfaced again on the live `--post` test (queue.md item below) — the script hangs entirely, not just slowly. Diagnose under load; fix in `sutra-proto`/`sutra-core`. Workaround in production code is automated stop-restart.

2. **Engine bug: SPARQL returns literal values in the predicate slot.** ~1% of rows on a 5M corpus. Confirmed in a separate symptom too: `<< ?s ?p ?o >> ?qp ?qv` returns `<<QUOTED_TRIPLE>>` sentinel and literal values in `?qp`. Probably an RDF-star annotation row with positions getting confused on the executor side. Repro and fix.

3. **Bigger corpus.** 27,780 entities of the 30M in `philippesaade/wikidata` is a tiny slice. `tools/wikidata_hf_import.py --max-triples 50000000` would 10× the store. Bandwidth-bound, not API-bound. Engine bug #1 will surface multiple times during the run; the auto-stop-restart loop handles it.

4. **Live `--post` end-to-end test of generative citation.** Attempted with `--max-subjects 30 --post`; script hung 7 min with no output, indicating engine bug #1 triggered during the POST phase. Re-attempt at lower scope after #1 is fixed.

5. **Fine-tuning track scaffolding.** `planning/fine-tuning-track.md` defines the parallel near-term track: Qwen 2.5 1.5B-Instruct + QLoRA on the same `triples.txt` format, sharing the `propositionInferredFrom` output schema. Build `training/finetune/`.

6. **Qualitative comparison: v5 (word) vs v6 (BPE) on unicode names.** v6 trained and pinned (final ppl 194.98, not directly comparable to v5's 84.85 because BPE has more mass per position). Run `infer_with_citations.py --bpe-tokenizer training/data/tokenizer_bpe.json` against subjects with names like `Saint-Léger`, `Wikipédia`, `Curt Meyer-Clason` and compare to v5's emissions for the same subjects. This is the actual win condition for the BPE round.

7. **Address Gemini 3 Flash review (v1 post 2378).** Six concrete critiques in `paper/reviews/v1_post2378_review.md`. Highest-signal three for a v2 revision:
   - Cite the SutraDB v0.4.0 release URL more prominently (review called the engine "unpublished").
   - Acknowledge the heuristic-citation framing more directly as a v0 design choice (review §6.3 already does this; tighten in §3 too).
   - Add explicit "future work" framing for standard KG-completion metrics (MRR, Hits@k); justify why we're not in that regime today.
   Then `POST /api/posts/2378/revise` for v2.

8. **Repo rename SutraDB → Loka.** Top of `TODO.md` has the full checklist.

9. **World-model cascade-retraction: remove a generated node and all inferences that cite it.** Today the engine has per-triple `DELETE DATA` only — no entity-level cascade, no RDF-star annotation cleanup, no HNSW tombstone flip on the delete path (rebuild required). For the world model this matters because every generated triple carries `propositionInferredFrom <<S P O>>` pointing at the context that informed it; when the user retracts a generated node X, the right semantics are: drop every triple where X is S/P/O, drop the `<<...>>`-quoted annotation rows whose inner triple involves X, and recursively retract any other generated triple whose `propositionInferredFrom` chain dereferences a now-removed triple. (RDFS/OWL inference is out of scope per CLAUDE.md, so this is *only* about model-emitted provenance chains, not symbolic entailment closures.) Expose this two ways:
   - **MCP tool** (`retract_generated_node` or similar) so an agent can do it programmatically. Accepts an IRI; returns the count + IRIs of triples removed at each cascade depth.
   - **Sutra Studio action** so a user inspecting a suspect generated triple can click "retract" and see the affected dependency tree before confirming.
   Engine-side prerequisites surfaced by the audit: (a) a back-reference from inner-triple ID to annotation rows so RDF-star cleanup is O(deg) not O(N), (b) `VectorRegistry::delete` actually called from `execute_delete_data` so the HNSW tombstone path is live, (c) optional new SPARQL+ verb or REST endpoint that takes the cascade root and returns the dependency tree before the delete commits. Cascade scope must be bounded to the reserved provenance namespace — never traverse a non-`http://sutra.dev/provenance/` predicate as a "dependency" (a regular `wdt:P31` edge is data, not derivation).

---

## Done (2026-05-10 session)

- ✓ v6 trained on BPE tokenizer (queue.md #5 from prior session). 5 epochs, final ppl 194.98. Wall time ~2.5h with one long thermal/sleep stall in epoch 4.
- ✓ v6 pushed to HF as `EmmaLeonhart/loka` tag `v6-bpe` (the `v6` tag had been created by an earlier run before v6.pt existed). `MODEL.json` bumped to v6 with BPE tokenizer pinned alongside vocab.
- ✓ `tools/hf_snapshot.py` taught about v6.pt + BPE files (`tokenizer_bpe.json`, `vocab_bpe.json`).

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
- ✓ BPE wired into `train.py` + `infer_with_citations.py` via `--bpe-tokenizer` flag (`021fd4c`).
- ✓ Pinned-model loader: `MODEL.json` + `training/loader.py`; `infer_with_citations.py` defaults pull v5 from HF on first run. README now has a "World Model (Loka)" clone-and-run section.
- ✓ Auto-sync cron `fc054cb5` — pulls, rebases, pushes, chains successor.

---

## Reference

- **`TODO.md`** — longer-horizon work (SDK publishing, Maven Central, Cypher/GQL wrappers, premium-tier, ontochronology phases-5+). Items migrate to here when ready.
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
