# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## 🚨 Top of queue — ordered, do these in this sequence

The user reports persistent Windows instability "ever since [I] got started on this project," currently off the affected machine. Repo evidence (`training/logs/loka-restart.log`, DEVLOG 2026-05-12 23:52 UTC) shows a Win32 `ERROR_NO_SYSTEM_RESOURCES` panic — kernel-level resource exhaustion that destabilises the whole OS, not just Loka. Until diagnostics rule that out, treat the box as fragile.

### 1. Run the diagnostic triage on the affected box

Branch `claude/diagnose-system-issues-8cjuO` has the analysis. **Read `planning/system-instability-diagnosis.md` first.** Sequence:

1. Run the "Stop the bleeding" PowerShell block — disables `loka-v11-kickoff`, kills any running cron/training/Loka/ingest processes, disables every Loka-tagged scheduled task. This makes sure nothing auto-fires while you're investigating.
2. Run the "Diagnostic data to gather" block — Event Viewer (System log errors, BugCheck events, `nvlddmkm` warnings), `C:\Windows\Minidump\` listing, pool counters, free disk, stray python/loka processes, sled scratch-dir sizes. Save the output into `planning/system-instability-evidence-YYYY-MM-DD.md`.
3. Use the six ranked hypotheses (kernel nonpaged-pool, preprocess.py 21 GB OOM, GPU TDR, disk fill, orphaned crons, hardware) plus the user's answers to the five narrowing questions to collapse the tree. The most valuable single data point is whether `Minidump\` has fresh `.dmp` files — those name the bugcheck and the offending driver, which usually closes it in one step.
4. Apply mitigations in order of payoff: serialise serve/ingest/train, keep both crons disabled, tighter sled config, move sled state off C:, RocksDB migration if the same panic recurs.

**Output expected:** a verdict written into `planning/system-instability-diagnosis.md` (or a follow-up `system-instability-verdict.md`) saying which hypothesis matched and what the residual risk is, plus an updated `queue.md` entry confirming whether v11 can resume.

### 2. Process the system-instability chat (lands in a near-future commit)

The user has a chat transcript related to the instability that they'll commit into `chats/` shortly. It'll follow the same format as `chats/ai-bubble.md` / `chats/world-models.md`. When the commit lands:

- Read the new file under `chats/` (filename likely `chats/system-instability*.md` or similar).
- Extract anything the chat says that wasn't in this triage doc — additional symptoms, machine specs, what the user already tried, ruled-out hypotheses.
- Update `planning/system-instability-diagnosis.md` with the new evidence — promote or demote hypotheses as appropriate, add or remove diagnostic steps, sharpen the mitigation list. Do this in the same commit so the analysis and the source line up.
- If the chat reveals something material (e.g. "BSOD code was X", "swapped the RAM and it still happens"), call it out in the commit message — that's the diff that future sessions need to find quickly.

### 3. Start the v11 training cycle (only after step 1's verdict says it's safe)

Once diagnostics complete and the system is verified stable enough to run the workload (or the mitigations from step 1 have been applied — e.g. serialise serve/ingest/train, tighter sled config, no overlapping crons):

- Use the existing v11 restart protocol below (re-enable + fire `loka-v11-kickoff`, or run `python tools/v11_kickoff.py` directly).
- Watch live: `tail -F training/logs/v11_kickoff.log` and Event Viewer in parallel. Stop at the first kernel-pool warning, don't wait for a panic.
- Completion marker: last line of `training/logs/v11_kickoff.log` is `=== v11 kickoff DONE — v11 shipped ===` and `git log -1` shows the v11 commit.
- **Do not re-enable `training_cron.py` or `post_eval_cron.py`** as part of this — one cycle, by hand, first.

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
