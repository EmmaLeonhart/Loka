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

6. **Address Gemini 3 Flash review (v1 post 2378).** Six concrete critiques in `paper/reviews/v1_post2378_review.md`. Highest-signal three for a v2 revision:
   - Cite the SutraDB v0.4.0 release URL more prominently (review called the engine "unpublished").
   - Acknowledge the heuristic-citation framing more directly as a v0 design choice (review §6.3 already does this; tighten in §3 too).
   - Add explicit "future work" framing for standard KG-completion metrics (MRR, Hits@k); justify why we're not in that regime today.
   Then `POST /api/posts/2378/revise` for v2.

7. **Repo rename SutraDB → Loka.** Top of `TODO.md` has the full checklist.

8. **World-model cascade-retraction: remove any node — real data or AI-generated — and all generated inferences that cite it disappear.** A node has two kinds of edges leaving it: ordinary data edges (`wdt:P31`, `:hasEmbedding`, etc.) and provenance back-edges from generated triples that cited it (`<<X p o>> sutra-prov:propositionInferredFrom <<source-of-X>>`). Cascade-retraction propagates **only along provenance back-edges**, recursively, regardless of whether the deleted node was real data or model-emitted. So: deleting a real-data node drops the node's own triples *and* every generated triple whose `propositionInferredFrom` chain dereferences any of those rows, transitively. Deleting a generated node does the same plus removes the node's own row. Real data → real data is *not* a dependency: ordinary edges are not derivations. (RDFS/OWL closures stay out of scope per CLAUDE.md; this is purely about provenance bookkeeping.) Engine today supports per-triple `DELETE DATA` only — no entity cascade, no RDF-star annotation cleanup, and `VectorRegistry::delete` is wired but never called from `execute_delete_data` (manual `POST /vectors/rebuild` is the only HNSW cleanup). Surface the cascade twice:
   - **MCP tool** (`retract_node` — name covers both real and generated cases). Accepts an IRI; returns count + IRIs of triples removed at each cascade depth, and a count of any HNSW tombstones flipped.
   - **Sutra Studio action**: click a node, see the dependency-tree preview (which generated rows would disappear), confirm.
   Engine-side prerequisites: (a) back-reference from inner-triple ID to annotation rows so RDF-star cleanup is O(deg) not O(N); (b) `VectorRegistry::delete` actually invoked from the delete path so HNSW tombstones go live; (c) a preview endpoint that takes a root IRI and returns the would-be-deleted set without committing. Cascade traversal must be bounded to the reserved `http://sutra.dev/provenance/` namespace — never follow a regular predicate as if it were a derivation edge.

---

## Done (2026-05-10 session)

- ✓ v6 trained on BPE tokenizer (queue.md #5 from prior session). 5 epochs, final ppl 194.98. Wall time ~2.5h with one long thermal/sleep stall in epoch 4.
- ✓ v6 pushed to HF as `EmmaLeonhart/loka` tag `v6-bpe` (the `v6` tag had been created by an earlier run before v6.pt existed). `MODEL.json` bumped to v6 with BPE tokenizer pinned alongside vocab.
- ✓ `tools/hf_snapshot.py` taught about v6.pt + BPE files (`tokenizer_bpe.json`, `vocab_bpe.json`).
- ✓ v5 vs v6 qualitative comparison on unicode-name subjects (`tools/compare_v5_v6.py` + DEVLOG entry). Findings: v6 preserves accents (v5 strips them at the regex stage), pulls v5's no-prediction holes off the floor for identifier-shaped predicates, but a per-token-floor decoder bug truncates BPE date emissions to just `"+"`. Decoder fix is the next quality lever.

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
