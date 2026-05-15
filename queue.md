# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## 🚀 Multi-version pipeline — 2026-05-13 evening pivot

After hitting Loka SPARQL OFFSET-cost (page-N is O(N) on sled — 25h projected for pass 1 alone), pivoted to a no-Loka HF-source preprocessor (`tools/preprocess_from_hf.py`). The plan now is to ship a series of normalized-wikidata snapshots and train a model on each.

**Naming convention:** *the dataset tag matches the Loka-model version trained on it.* So the corpus that trains model `v11` is tagged `v11-50k`, the corpus for `v12` is `v12-100k`, etc. The `-NNk` / `-NM` suffix is the entity-row count, the version number is the model lineage (continuing from v10).

| HF dataset tag | Entity rows | Output triples | Model trained | Status |
|---|---|---|---|---|
| `v11-50k` (also `v0.1-50k` alias) | 50,000 | 350,428 | `v11` | **✅ Fully shipped 2026-05-13** — corpus: https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata/tree/v11-50k, model: https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v11. v11 trained 3/20 epochs (loss 8.79 → 5.85 → 5.63, ppl 6577 → 347.71 → **279.12**) before CUDA OOM at epoch 4 backward at batch 32 on the 8 GB-VRAM 4070 Laptop. **All future training uses `--batch-size 16`.** DEVLOG + paper §5.9 + MODEL.json updated. |
| `v12-100k` | 100,000 | **671,817** | `v12` | **✅ corpus pushed 2026-05-13 22:42 PT** — https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata/tree/v12-100k. Model `v12` training in flight (task `bocj7flty`, batch 16, 20 epochs, log `training/logs/v12_train.log`). |
| `v13-500k` | 500,000 | **2,511,771** | `v13` | **✅ Fully shipped 2026-05-14 ~20:13 PT.** Trained 5/10 epochs; trajectory 334.76 → 242.75 → 248.29 → 258.42 → 256.04, classic Adam plateau on exclusive GPU. Epoch-2 (ppl **242.75**) promoted to canonical `v13`; per-epoch tags `v13.1`–`v13.5` all on HF. https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v13. DEVLOG + paper §5.11 + MODEL.json updated. |
| `v14-1M` | 1,000,000 | **4,021,409** | `v14` | **✅ Fully shipped 2026-05-15.** 5-epoch partial-local run: ep1 283.07 → ep2 215.45 → ep3 209.55 → **ep4 202.01 (best, SERIES BEST)** → ep5 204.70. Epoch-4 promoted to canonical `v14`; per-epoch tags `v14.1`–`v14.5` on HF. https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v14. 6–10 continuation OOM'd (concurrent pytest on the GPU — same class as the v12 LLaMA disaster); shipped epoch 4 per user call rather than re-run. Full 10-epoch run available via the donor path. DEVLOG + paper §5.12 + MODEL.json + abstract/masthead updated. |

**🎉 SERIES COMPLETE.** All four normalized-wikidata rungs trained and shipped. Headline: corpus scale is the binding lever — v11 (350k) 279.12 → v12 (672k) 250.82 → v13 (2.5M) 242.75 → **v14 (4M) 202.01**, v14 still descending where v13 plateaued. Twelve checkpoints (v3–v14) on `EmmaLeonhart/loka`; four corpus tiers on `EmmaLeonhart/normalized-wikidata`; per-epoch tags v12.*/v13.*/v14.*. Per-epoch snapshot discipline preserved every epoch across three training disruptions.

**Open follow-ups (not blocking):**
- Donor full-10-epoch v14 via `tools/contribute_v14_training.py` — expected to push below 202 on bigger hardware. Published at loka.emmaleonhart.com/contribute/.
- Clean v12 retrain (the epoch-4 best 226.86 was lost to shared-GPU contention; a clean run would land ~225).
- Propgen test (Q42 seed) on v11–v14 — deferred since v11 due to GPU fragility during shared use.
- Engine bug #2 root-cause fix in `loka-sparql` (currently filtered at preprocess).

---

## 🚨 Old queue (above the pivot): Top of queue — ordered, do these in this sequence

The user reports persistent Windows instability "ever since [I] got started on this project," currently off the affected machine. Repo evidence (`training/logs/loka-restart.log`, DEVLOG 2026-05-12 23:52 UTC) shows a Win32 `ERROR_NO_SYSTEM_RESOURCES` panic — kernel-level resource exhaustion that destabilises the whole OS, not just Loka. Until diagnostics rule that out, treat the box as fragile.

### 1. Run the diagnostic triage on the affected box — DONE 2026-05-13

Evidence collected into `planning/system-instability-evidence-2026-05-13.md`. Verdict written to `planning/system-instability-verdict-2026-05-13.md`.

**Verdict, one paragraph:** the OS-freeze incidents are H6 (thermal / firmware-level shutdown on a thermally-constrained laptop), with H3 (GPU TDR fragility + IOMMU dGPU faults) as a co-factor. **It is a laptop**, hostname `laptop-qe4jv37b`, Ryzen 7 8845HS + Radeon 780M iGPU + RTX 4070 **Laptop** dGPU (not the desktop 4070 the previous Claude conversation assumed). Evidence: **10 Kernel-Power 41 events Mar 27 → today, zero BSODs, zero minidumps, zero WHEA, zero nvlddmkm errors** — the OS is not the layer detecting the failure; it's the firmware/EC layer. The 15-min cool-down recovery in `chats/system-instability.md` is the canonical thermal saturation signature. H1 (sled kernel pool) is real but separate — it owns the *logged* sled panic in DEVLOG, NOT the OS freezes.

### 2. Process the system-instability chat — DONE 2026-05-13

Extracted from `chats/Computer freezing during AI training run - Claude.html` → `chats/system-instability.md`. Key new evidence folded into `planning/system-instability-diagnosis.md`:

- **15-minute cool-down recovery pattern** — soft reboot didn't work, fast cold boot didn't work, leaving the box off for 15 min did. This is *not* a kernel-software-state signature. Hypothesis ranking revised: H3 (GPU TDR-stuck firmware) and H6 (thermal/PSU) promoted to co-#1 for the chat-described incident; H1 (sled kernel pool) now best explains the *logged* sled panic in DEVLOG but probably not the OS freeze.
- **Hardware confirmed**: Windows + RTX 4070 (12 GB VRAM) + CPU "comparable currentness to the 4070".
- **User's own attribution**: preprocessing pipeline. Aligns with H2 as the most likely *contributor* during the gradual-slowdown phase preceding the black screen.
- **Likely two incidents conflated**: the DEVLOG sled panic vs the chat black-screen are plausibly different failure modes. Diagnosis doc now treats them as separate.
- New diagnostic block added for H3-specific checks (Win32_ReliabilityRecords display events, driver branch Studio-vs-Game-Ready, TDR registry settings).

The extractor (`tools/extract_claude_chat.py`) had a duplication bug — each Claude turn was appending all subsequent Claude turns. Fixed in this commit; output dropped from 64 KB to 19.5 KB. Existing committed chats (`ai-bubble.md`, `world-models.md`) were not regenerated since they're already in git and the user has been working from them.

### 3. Preprocess the corpus into a clean "normalized-wikidata" dataset (NEW direction after the verdict)

The verdict made it clear that a 50M-triple raw-Wikidata training run on this laptop is thermally marginal. The fix is not just "cool the laptop better" — it's "stop training on the messy original corpus." Most of the triples in raw Wikidata aren't useful for the world-model objective anyway (external identifiers, malformed dates, opaque QID references with no labels). A normalization pass produces a **smaller, cleaner, more text-like dataset** that:
- trains faster (smaller corpus → less wall-clock GPU time → less thermal pressure)
- generalises better (no opaque QIDs littering the input)
- is genuinely useful to others doing similar work — worth a standalone HF push

**Plan, ordered:**

1. **Audit existing preprocess.** ~~DONE 2026-05-13.~~ `training/preprocess.py` already does label substitution (English), reserved-namespace stripping, noise-datatype exclusion via `training/wikidata_excluded_predicates.json`, Wikidata-API PID fallback (cached at `training/property_label_cache.json`), and time/quantity normalization. The big problem is **it loads all triples into one Python list before processing** — that's the 21 GB RAM bloat the chat described. `tools/wikidata_hf_import.py` (stage 1) is already correctly streaming. Stage 2+3 is where the rewrite is needed.

2. **Stage 1 — lean import (keep current).** Wikidata triples land in SutraDB / `loka-data-cron-c1` with **QIDs intact**. Streaming. No in-memory accumulation. Existing `wikidata_hf_import.py` mostly fits the shape; verify it's actually streaming, not building a giant in-memory map.

3. **Stage 2 — QID/PID label resolver (built 2026-05-13).** `tools/preprocess_streaming.py` is the memory-flat replacement for `training/preprocess.py`. Two-pass design:
   - **Pass 1 ("scan")**: stream every triple from Loka, extract English `rdfs:label` rows into the SQLite cache, build the set of unique QID/PID IRIs that appear in the corpus. Memory bounded by entity count (1–10% of triple count), not triple count.
   - **Wikidata API step (optional, `--fetch-missing-from-wikidata`)**: for QIDs/PIDs the corpus mentions but doesn't have a label for, query the public Wikidata SPARQL endpoint in batches of 50, write results into the SQLite cache. Polite rate-limit (`--api-rate-seconds`).
   - **Pass 2 ("emit")**: stream every triple again, resolve via SQLite, apply the same normalization rules as the canonical `preprocess.py` (which is imported, not duplicated), write the flat tab-separated corpus.
   - Cache lives at `training/data/wikidata_labels.sqlite` and **persists across runs**. The same script can be re-run incrementally as the corpus grows.
   - **Running 2026-05-13 ~15:50 PT (restarted after fix)** — `loka serve` PID 26992 on port 3030 holds 50,002,600 triples. Background task `b213git6e` is the running preprocess. Log: `training/logs/preprocess_streaming.log`. Output: `training/data/triples_normalized.txt`. Throughput ~12 s per 100k-row page → ~100 min per pass × 2 passes = ~3.5 h total wall-clock.
   - **Critical bug discovered + fixed mid-run (commit `78e1e7e`)**: corpus-derived property labels are systematically wrong — every PID we'd resolved in the first 87 pages had the wrong label (P20 → "Belgium" not "place of death", P1412 → "English" not "languages spoken", etc.). Root cause: RDF-star annotation rows on triples being mis-surfaced by Loka's SPARQL executor (engine bug #2 again) — the property IRI gets keyed against the inner triple's object value. Entity labels are unaffected. Fix: preload `training/property_label_cache.json` as `curated`-source labels, skip corpus rdfs:label rows whose subject is a property, drop pass-2 rows where subject OR object is a property IRI. Smoke test: 2 → 744 clean rows on the same 50k-row slice. **Without this fix the normalized corpus would have been useless** — re-train was the right call.
   - Wikidata API step deferred until we see the unresolved count from this run (in-corpus rdfs:label rows + the 7,312 curated property labels should cover the majority).
   - Smoke test against the small `sutra-data` store confirmed the pipeline structurally works (commits `c516276` + `308ae90` for engine-bug-#2 + URL-object guards).

4. **Stage 3 — normalization pass (NEW Python script).**
   - Reads the corpus + the label DB.
   - Output is a text-like training format where:
     - QIDs/PIDs are replaced with their English labels (from the side table), with the original QID kept in a parallel column for traceability.
     - External identifiers (`P227` GND, `P214` VIAF, etc.) are stripped — they have no signal for the world-model objective.
     - URLs are stripped or replaced with a single `<URL>` sentinel.
     - Dates are normalized to ISO-8601 (the v7 datatype filter already does some of this).
     - Datatypes are flattened to `"value"` form — no `"value"^^xsd:integer` clutter.
   - The originals stay in SutraDB unchanged — normalization is a derived view, not a destructive rewrite.

5. **Stage 4 — push `normalized-wikidata` to Hugging Face as a standalone dataset.**
   - New HF repo: `EmmaLeonhart/normalized-wikidata` (or similar — confirm naming).
   - README explains: source = Wikidata `20YY-MM-DD` dump, normalization choices, what was stripped, why. Useful to other people training world models on Wikidata.
   - Reuse `tools/hf_snapshot.py` patterns.

6. **Stage 5 — retry v11 on the normalized corpus.** The corpus will be significantly smaller (probably 30–50% of the raw triple count after stripping external identifiers and noise). Combined with the verdict's mitigations (batch 32, serialised workload, TdrDelay=10s, possibly cloud GPU), this is the right shape for actually shipping v11.

### Old item — kept for reference: start the v11 training cycle (BLOCKED on mitigations 1–3 below; revised after the verdict)

The verdict reframes this step. The original plan (`4 epochs × 52 min`, batch 64, on this laptop) was designed against assumed desktop-4070 thermal headroom. The actual hardware cannot sustain that without firmware-level shutdown. Either the workload changes or it moves to different hardware. **Required before resuming:**

1. **External cooling.** Cooling pad / elevated rear / ambient temperature reduced. Verify via 30-min HWiNFO64 stress test: GPU hotspot < 90 °C, CPU package < 95 °C sustained.
2. **Workload serialisation.** No `loka serve` + ingest + training concurrently. Strict sequencing (per `planning/system-instability-diagnosis.md` mitigation M1).
3. **Smaller training batch.** `--batch-size 32` instead of 64. ~2× wall-clock cost; halves driver pool pressure.
4. **One-time setting changes:** raise `TdrDelay` registry value to 10 s (was empty → default 2 s); switch NVIDIA driver from Game Ready 32.0.15.8183 to the **Studio** branch. Both require a reboot.
5. **Check BIOS for an update** — HAL `ACPI TAD failed` warnings + IOMMU faults on the dGPU suggest firmware-level fixes may be pending from the vendor.

**Strongly preferred alternative**: move the training run off this laptop entirely. Lambda Labs / RunPod / Vast.ai for the 3.5-h training pass costs < $5/cycle, eliminates the thermal risk, and the corpus + tokenizer already live on Hugging Face. The training-on-laptop path was always going to be marginal; the verdict makes that explicit.

Once mitigations are in place:
- Use the v11 restart protocol below (re-enable + fire `loka-v11-kickoff`, or run `python tools/v11_kickoff.py` directly).
- Watch live: `tail -F training/logs/v11_kickoff.log` AND HWiNFO64 temps in parallel. Stop at first thermal warning OR first kernel-pool warning.
- Completion marker: last line of `training/logs/v11_kickoff.log` is `=== v11 kickoff DONE — v11 shipped ===` and `git log -1` shows the v11 commit.
- **Do not re-enable `training_cron.py` or `post_eval_cron.py`.** One cycle, by hand, first.

### 4. Set up a 5 h follow-up cron: analyse training + update paper

**Doesn't exist yet** — closest analogue is `tools/post_eval_cron.py`, which fires every 6 h for 48 h and runs the full preprocess→train→propgen→DEVLOG→paper→push pipeline. That's a different shape: a continuous-loop training driver, not a one-shot post-training analyser.

Plan for the new tool (call it `tools/post_v11_analyse.py` or similar):

- Fires once, 5 h after v11 training completes (i.e. scheduled with `Register-ScheduledTask` against the v11 completion timestamp + 5 h, OR a `--first-delay 18000` flag on the existing cron pattern).
- Reads `training/logs/v11_train.log` + `training/data/test_propgen_Q42_v11.nt` + `_meta.json`.
- Diffs against v10's numbers (final ppl, catalog vs semantic emission split, asymmetric-drop count) and writes an analysis paragraph.
- Appends a paper §5.9 update + DEVLOG entry with the comparison.
- Commits + pushes (paper push triggers `papers-ci.yml` → clawRxiv submission).
- **Single-shot** — no looping. The recurring `post_eval_cron.py` is what we have for the multi-firing pattern, and it's intentionally on hold.

Defer the implementation until step 3 has actually produced a v11 checkpoint — premature scaffolding is wasted work if v11's shape comes out different from v10's.

---

## Reference: v11 cycle restart protocol (for step 3 above)

**Date this was written:** 2026-05-13, ~11:30 PT, about 30 min before the v11 training cycle fires.

**Where we are:** the bigger-corpus ingest is **done**. `loka-data-cron-c1/` holds **50,000,521 triples**. v11 has not started training yet. A Windows scheduled task `loka-v11-kickoff` is registered to fire at noon PT 2026-05-13 — **it was Disabled before the reboot** so it wouldn't fire mid-update and leave a partially-trained checkpoint. **Re-enable or fire it manually when you're ready** (see step 4 below). The user rebooted for a Windows update; this section is so a fresh session can pick up cleanly.

### What survives the reboot vs what doesn't

| Item | Survives reboot? | Notes |
|---|---|---|
| `loka-data-cron-c1/` data dir (5+ GB sled state) | ✅ | The corpus. Don't touch it. |
| Scheduled task `loka-v11-kickoff` | ✅ | OS-level (`schtasks`), persists across reboot. Trigger: 2026-05-13 12:00:00 PT one-shot. |
| `tools/v11_kickoff.py` | ✅ | Committed to git. |
| `target/release/loka.exe` | ✅ | Pre-built binary. |
| `loka serve` process (port 3030) | ❌ | Killed by reboot. **Must be restarted before `v11_kickoff.py` runs.** |
| Background HF-importer process | N/A | Already exited cleanly at the 50M cap. |

### Restart steps (do these in order)

1. **Confirm the scheduled task is still registered.** It was Disabled pre-reboot — that's expected.
   ```powershell
   Get-ScheduledTask -TaskName 'loka-v11-kickoff' | Select State, @{n='NextRun';e={(Get-ScheduledTaskInfo $_).NextRunTime}}
   ```
   Expected: `State=Disabled, NextRun=2026-05-13 12:00:00`. If the task is **missing entirely** (update wiped it), re-register — full PowerShell block is in commit `c7f5d68`'s diff (search history for "Register-ScheduledTask -TaskName 'loka-v11-kickoff'"). Don't enable yet; do that in step 4 after Loka is back up.

2. **Restart Loka against the data dir.** Run this in a separate PowerShell window or as a background process:
   ```powershell
   target\release\loka.exe serve --data-dir loka-data-cron-c1 --port 3030
   ```
   Sled WAL replay on the 5 GB store takes ~30–60 s; `/health` returns 200 only after it finishes.

3. **Verify Loka is up and holds the corpus.** Both must be true before the scheduled task fires:
   ```powershell
   Invoke-WebRequest http://localhost:3030/health -TimeoutSec 60
   # 200 ok

   $body = "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }"
   Invoke-WebRequest http://localhost:3030/sparql -Method POST -Body $body `
     -ContentType "application/sparql-query" -TimeoutSec 1800
   # Expected: 50,000,521
   ```
   The count query takes 5–10 min on 50M triples — that's normal, not a wedge.

4. **Re-enable and either wait or fire manually.** Task is currently Disabled (see step 1).
   ```powershell
   Enable-ScheduledTask -TaskName 'loka-v11-kickoff'
   ```
   Then either:
   - **Wait** for noon (if you re-enable before 12:00 PT, it'll auto-fire at the registered trigger time and run `cmd /c python tools\v11_kickoff.py > training\logs\v11_kickoff_task.log 2>&1`).
   - **Fire now** (recommended if it's already past noon, or if you just want to go): `Start-ScheduledTask -TaskName 'loka-v11-kickoff'` (uses the registered action — logs land in `training\logs\v11_kickoff_task.log`), or run directly without the scheduler: `python tools\v11_kickoff.py`.

### What v11_kickoff.py does

(Self-contained — re-uses `training_cron.py`'s helpers via direct import.)

1. Stop any running `wikidata_hf_import.py` (no-op now — it exited at 50M).
2. Verify `/health` and triple count ≥ 33M. Aborts with non-zero exit if either fails.
3. Preprocess: SPARQL-fetch the corpus, paged at 100k rows/page, write `training/data/triples_v11.txt`. ~30–120 min at 50M scale.
4. Train v11, **4 epochs**, batch 64, ~52 min/epoch on the 4070 → ~3.5 h total. Per-epoch perplexity in `training/logs/v11_train.log`.
5. Ship: propgen test (Q42, conf 0.25, 30 sources) → DEVLOG entry → MODEL.json bump → `tools/hf_snapshot.py` push as `v11` tag on `EmmaLeonhart/loka` → `git add` paper/DEVLOG/MODEL.json/train.log → `git commit -m "v11 cron cycle: trained, tested, shipped to HF"` → `git push origin main`.

Completion marker (success): the last line of `training/logs/v11_kickoff.log` is
```
=== v11 kickoff DONE — v11 shipped ===
```
and `git log -1` shows the v11 commit.

### What to do if something is wrong

- **`v11_kickoff_task.log` empty after noon** → scheduled task didn't run. Check `Get-ScheduledTaskInfo loka-v11-kickoff` for `LastTaskResult`. Common cause: user wasn't logged in; right-click the task in Task Scheduler → Properties → Security options → "Run whether user is logged on or not".
- **kickoff fails at step 2 (triple count too low)** → Loka didn't reopen the data dir correctly. Stop the loka process, re-run step 2 above, watch `/health` until 200.
- **kickoff fails at step 4 (training)** → see `training/logs/v11_train.log` for the actual error. Most likely cause: GPU out of memory. Retry with smaller batch (edit `train_version` call site, `--batch-size 32`).
- **kickoff fails at step 5 (ship)** → checkpoint exists; just re-run `python tools/v11_kickoff.py` skipping training is not currently supported. Manual recovery: call `tc.ship(11)` from a Python REPL.

---

## Active

In strategic order. Top item is the current focus.

1. **Engine bug #1: sled flusher panic at multi-GB scale.** ~~Surfaced~~ Surfaced on 2026-05-12 as a hard panic (Win32 `ERROR_NO_SYSTEM_RESOURCES` at 32.88 M triples / 5 GB). Probable fix shipped in `c36760b`: explicit `sled::Config` with 256 MB cache, 2 s flush, `Mode::HighThroughput`. **Reopen-in-place verified 2026-05-13 01:00 UTC**: WAL replay recovered 32,877,248 triples (1,150 more than `big-pull.log` last recorded). The fix is verified for the reopen case; the residual question is whether it also holds against a fresh sustained ingest past 32.88 M. If a re-test ingest panics at the next plateau, escalate to RocksDB migration (sled 0.34 unmaintained since 2021).

2. **Engine bug #2: SPARQL returns literal values in the predicate slot.** ~1% of rows on a 5M corpus. Confirmed in a separate symptom too: `<< ?s ?p ?o >> ?qp ?qv` returns `<<QUOTED_TRIPLE>>` sentinel and literal values in `?qp`. Probably an RDF-star annotation row with positions getting confused on the executor side. Repro and fix.

3. **Bigger corpus, paused at 32.88 M triples.** `loka-data-cron-c1/` has 32,877,248 triples on disk; option B reopen verified 2026-05-13 01:00 UTC. Two remaining decisions:
   - **Resume ingest** from row 318,582 to push toward the 50 M-triple target — tests whether the sled tuning also holds under fresh sustained writes (not just reopen). Worst case: another panic at the next plateau, falls through to RocksDB migration.
   - **Stop here and use as v11's training source** — already 17× the v10 corpus (94k triples). Skips the ingest risk; v11 trains immediately.

4. **Live `--post` end-to-end test of generative citation.** Attempted with `--max-subjects 30 --post`; script hung 7 min with no output, indicating engine bug #1 triggered during the POST phase. Re-attempt at lower scope after #1's fix is verified by option B.

5. **Fine-tuning track scaffolding.** `planning/fine-tuning-track.md` defines the parallel near-term track: Qwen 2.5 1.5B-Instruct + QLoRA on the same `triples.txt` format, sharing the `propositionInferredFrom` output schema. Build `training/finetune/`.

6. **Submit paper revision v2 to clawRxiv post 2378.** Local edits done (see Done section); needs `POST /api/posts/2378/revise` to actually publish the v2. Review-flagged remaining concerns (mode collapse on connectors, anecdotal qualitative sample, "neuro-symbolic" framing) are partially addressed by the new §3.2 framing and §6.4 future-work block but would benefit from a v3 once a bigger corpus or entity-decoder lands.

7. **Repo rename Loka → Loka.** Top of `TODO.md` has the full checklist.

8. **World-model cascade-retraction: remove any node — real data or AI-generated — and all generated inferences that cite it disappear.** A node has two kinds of edges leaving it: ordinary data edges (`wdt:P31`, `:hasEmbedding`, etc.) and provenance back-edges from generated triples that cited it (`<<X p o>> loka-prov:propositionInferredFrom <<source-of-X>>`). Cascade-retraction propagates **only along provenance back-edges**, recursively, regardless of whether the deleted node was real data or model-emitted. So: deleting a real-data node drops the node's own triples *and* every generated triple whose `propositionInferredFrom` chain dereferences any of those rows, transitively. Deleting a generated node does the same plus removes the node's own row. Real data → real data is *not* a dependency: ordinary edges are not derivations. (RDFS/OWL closures stay out of scope per CLAUDE.md; this is purely about provenance bookkeeping.) Engine today supports per-triple `DELETE DATA` only — no entity cascade, no RDF-star annotation cleanup, and `VectorRegistry::delete` is wired but never called from `execute_delete_data` (manual `POST /vectors/rebuild` is the only HNSW cleanup). Surface the cascade twice:
   - **MCP tool** (`retract_node` — name covers both real and generated cases). Accepts an IRI; returns count + IRIs of triples removed at each cascade depth, and a count of any HNSW tombstones flipped.
   - **Loka Studio action**: click a node, see the dependency-tree preview (which generated rows would disappear), confirm.
   Engine-side prerequisites: (a) back-reference from inner-triple ID to annotation rows so RDF-star cleanup is O(deg) not O(N); (b) `VectorRegistry::delete` actually invoked from the delete path so HNSW tombstones go live; (c) a preview endpoint that takes a root IRI and returns the would-be-deleted set without committing. Cascade traversal must be bounded to the reserved `http://loka.dev/provenance/` namespace — never follow a regular predicate as if it were a derivation edge.

---

## Done (2026-05-10 session)

- ✓ v6 trained on BPE tokenizer (queue.md #5 from prior session). 5 epochs, final ppl 194.98. Wall time ~2.5h with one long thermal/sleep stall in epoch 4.
- ✓ v6 pushed to HF as `EmmaLeonhart/loka` tag `v6-bpe` (the `v6` tag had been created by an earlier run before v6.pt existed). `MODEL.json` bumped to v6 with BPE tokenizer pinned alongside vocab.
- ✓ `tools/hf_snapshot.py` taught about v6.pt + BPE files (`tokenizer_bpe.json`, `vocab_bpe.json`).
- ✓ v5 vs v6 qualitative comparison on unicode-name subjects (`tools/compare_v5_v6.py` + DEVLOG entry). Findings: v6 preserves accents (v5 strips them at the regex stage), pulls v5's no-prediction holes off the floor for identifier-shaped predicates, but a per-token-floor decoder bug truncates BPE date emissions to just `"+"`. Decoder fix is the next quality lever.
- ✓ Paper v2 local edits per Gemini 3 Flash review (post 2378). Three changes: (a) Loka v0.4.0 release URL cited prominently in the masthead + References, (b) §3.2 lead paragraph acknowledges heuristic citation as a v0 design choice with forward-pointer to §6.3, (c) new §6.4 "What we are *not* claiming, and why we do not report MRR / Hits@k" treating the metric gap as blocked future work gated on the entity-decoder. POST to clawRxiv held — that's queue #6 now (external publish action, separate confirmation needed).

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
