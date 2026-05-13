# System Instability Diagnosis — Initial Triage

**Author:** Claude Code (claude-opus-4-7[1m]), session on branch `claude/diagnose-system-issues-8cjuO`.
**Written:** 2026-05-13. User reports persistent Windows instability "ever since [they] got started on this project", and is currently off-machine. This document is preliminary analysis to support the real diagnostic session once the affected machine is reachable.

The goal of this doc is to be **useful when pulled onto the other computer**: stop-the-bleeding steps first, then ranked hypotheses, then concrete diagnostic commands to run, then mitigations.

---

## TL;DR

The crash we already have written-up evidence for is **Windows kernel nonpaged-pool exhaustion** (`os error 1450`, `ERROR_NO_SYSTEM_RESOURCES`), triggered by sled 0.34's periodic flusher fsync'ing a 5 GB mmap-backed store at 2 Hz while a concurrent HTTP ingest writes the same DB and a 44.5 M-param transformer trains on the 4070. Any one of these alone is workable; all three concurrently, on a consumer Windows kernel with ~hundreds of MB of nonpaged pool, is not. Once the kernel runs out of nonpaged-pool entries the entire OS is destabilised — drivers fail to allocate I/O buffers, the disk stack stalls, the mouse can freeze, BSODs become possible. **This is the strongest single explanation for system-wide instability that started "after I got started on this project."**

A documented partial fix shipped in `c36760b` (sled config: 256 MB cache, 2 s flush, `Mode::HighThroughput`). It was verified for the *reopen-in-place* case but **not** for sustained fresh ingest at the next plateau. If v11_kickoff actually fired and the cron stack is back up, we may already be re-running the same conditions.

If you're reading this on a wobbly Windows box: **jump to "Stop the bleeding" first** before anything else.

---

## Symptoms (what the user said)

- Persistent system instability on Windows ever since starting work on this project.
- Has happened repeatedly, not a one-off.
- User had to reboot at least once for a Windows update (queue.md notes this); currently off-machine.
- "The plan that was given completely crashed my computer."

The phrase "system instability" (vs. "Loka crashed") is the key signal that this is not just a process-level bug — it's the whole machine misbehaving.

---

## Stop the bleeding (run these FIRST on the affected box)

Do these BEFORE diagnostic data-gathering. The point is to make sure nothing is going to crash the system again in the next 5 minutes.

```powershell
# 1. Disable the v11 one-shot scheduled task so it won't auto-fire on next boot.
Disable-ScheduledTask -TaskName 'loka-v11-kickoff' -ErrorAction SilentlyContinue

# 2. Kill any running cron / training / Loka / ingest processes.
Get-Process -Name 'python','python3','loka' -ErrorAction SilentlyContinue | Stop-Process -Force

# 3. Disable any Windows scheduled tasks under the Loka project tree (auto-sync, papers-CI, post-eval).
Get-ScheduledTask | Where-Object { $_.TaskName -match 'loka|Loka|post_eval|training_cron|papers-ci' } | Disable-ScheduledTask

# 4. Confirm nothing Loka-related is running.
Get-Process | Where-Object { $_.Path -match 'SutraDB|Loka' } | Select Id, ProcessName, Path
```

Once this is done the system has a chance to settle. THEN run the diagnostic block below.

---

## Working hypotheses, ranked

### H1 — Windows nonpaged-pool exhaustion driven by sled+ingest+training concurrency  *(highest confidence)*

**Why this is #1.** We already have a panic log with `Insufficient system resources exist to complete the requested service. (os error 1450)` in `training/logs/loka-restart.log`. That's the Win32 I/O manager telling sled it can't get a kernel buffer to issue `FlushFileBuffers`. DEVLOG 2026-05-12 23:52 UTC enumerates the conditions in detail.

**What can put a Windows box into this state, all simultaneously on this project:**
- `loka serve` with sled 0.34's default `Mode::LowSpaceUsage`, 500 ms flush, 1 GB cache on a 5 GB mmap-backed store → fsync storms.
- `tools/wikidata_hf_import.py` POSTing batches into the same Loka at 4 ent/s = ~400 triples/s → user-data writes interleaved with flusher fsyncs.
- `train.py` on the 4070 → NVIDIA driver allocates large amounts of nonpaged pool for DMA buffers, especially with batch 64 and `d_model=512`.
- The HNSW index code path (per `loka-hnsw` crate) holds large in-memory structures + Arc<Mutex>-style locks. On the FFI thread, this also touches nonpaged pool.

The c36970b fix reduces sled's footprint by ~4× but does not change the fundamentals. If v11_kickoff fired and started another sustained ingest past 33 M triples, we may be back in the same regime.

**Predictions if this is right:**
- Event Viewer → System log shows `EventID 2019` ("nonpaged pool"), `Disk` driver timeouts, or `Ntfs` warnings near the crash time.
- `poolmon` (if installed via WDK) shows tags like `MmSt`, `File`, `Ntfs` allocating heavily and not freeing.
- The crash is more likely when both ingest AND training are running, less likely when only one is.

### H2 — Pagefile thrashing from `preprocess.py` ballooning to 21 GB resident  *(high confidence, separate failure mode)*

DEVLOG 2026-05-12 23:52 UTC documents: "the v11 preprocess attempt found that asking Loka for a 32 M-row JSON in one shot grew it to 21 GB resident accumulating the response, never sent a byte back." That alone, on a typical 32 GB box, drives the page file hard, makes the rest of the OS feel unresponsive, and can hang explorer.exe and the shell.

The pagination fix landed in `b34d30d`, but: (a) it's only one of several callers; (b) `loka_triple_count` in `post_eval_cron.py` still runs a `SELECT (COUNT(*) AS ?n)` which scans the entire SPO index and can take 2–5 minutes at 33 M triples — during which Loka is pegged and the cron's `requests` call holds open sockets.

**Predictions:** RAM usage spikes, page-file grows multi-GB, mouse stutter / window-redraw stalls, then either recovery (slow) or hard freeze.

### H3 — GPU driver crash during training (DPC_WATCHDOG / VIDEO_TDR_FAILURE)  *(medium confidence)*

`train.py` runs on a 4070, batch 64, `d_model=512`, 6 layers, 44.5 M params. v10 trained 20 epochs at ~10 min/epoch. v11_kickoff plans 4 epochs at ~52 min/epoch on the bigger corpus → ~3.5 h sustained GPU load. NVIDIA driver TDR (Timeout Detection and Recovery) bugs are a recurring source of Windows BSODs, especially when other things on the box are also competing for I/O.

**Predictions:** BSOD with `VIDEO_TDR_FAILURE` or `DPC_WATCHDOG_VIOLATION`. Event Viewer shows `nvlddmkm` warnings/errors at crash time. Display blanks then recovers (TDR) seconds before the freeze.

### H4 — Disk fill / sled compaction-amplification  *(medium confidence)*

`Mode::HighThroughput` trades space-amplification for fewer fsyncs. The 5 GB sled state grows over time before manual compaction. The cron creates per-cycle data dirs (`loka-data-cron-c1`, `c2`, ...) at ~5 GB each; the `.gitignore` excludes them from git but they pile up on disk. If the user is running on a smaller SSD partition, repeated cycles fill the drive, and Windows behaviour at <5 % free disk is famously terrible.

**Predictions:** `Get-PSDrive C` shows free GB < 10 ; `ls -Force C:\Users\Immanuelle\Documents\Github\SutraDB\loka-data-cron-*` shows multiple multi-GB directories.

### H5 — Mode collapse of background processes; orphaned crons from prior sessions  *(medium confidence, easy to check)*

We have at least four concurrent loops that can run on the box:
- `tools/training_cron.py` — 12 h cycle, forever.
- `tools/post_eval_cron.py` — 6 h cycle for 48 h. **Still ticking as of 2026-05-13** based on `post_eval_cron.log`; it's been retrying triple-count against a dead Loka in a loop.
- `loka-v11-kickoff` scheduled task — one-shot at noon PT 2026-05-13.
- The "schedule" skill's remote crons that edit the paper.

Multiple of these started in different sessions; nothing inventoried what's actually running. A reboot only clears the user-foreground ones — the Task Scheduler entries survive.

**Predictions:** Running `Get-ScheduledTask | ? State -ne Disabled` shows more Loka-tagged tasks than expected; multiple `python.exe` instances visible in Task Manager between reboots.

### H6 — Hardware (RAM, PSU, thermals)  *(low confidence, but cheap to check)*

System-wide instability that starts after a heavy workload is sometimes mis-attributed to software when it's actually a marginal RAM stick that only fails under load, or a PSU sagging when GPU + sustained disk I/O happen together, or thermal throttling reaching shutdown territory.

**Predictions:** WHEA-Logger events in Event Viewer; `mdsched` (Windows Memory Diagnostic) reports failures; HWiNFO64 logs show CPU package temp > 95 °C or VR temp > 100 °C during training.

---

## Diagnostic data to gather (on the affected box, in order)

Copy the outputs into `planning/system-instability-evidence-YYYY-MM-DD.md` (or wherever) so this analysis can be revised against ground truth.

```powershell
# Did anything in the OS log out a crash?
# Look back 7 days at System log Errors and Critical.
Get-WinEvent -LogName System -MaxEvents 5000 |
  Where-Object { $_.LevelDisplayName -in 'Error','Critical' -and $_.TimeCreated -gt (Get-Date).AddDays(-7) } |
  Group-Object Id, ProviderName | Sort-Object Count -Descending | Select Count, Name -First 30

# Specifically — BugCheck events (BSODs):
Get-WinEvent -FilterHashtable @{LogName='System'; Id=1001; ProviderName='Microsoft-Windows-WER-SystemErrorReporting'} -MaxEvents 20 -ErrorAction SilentlyContinue
Get-WinEvent -FilterHashtable @{LogName='System'; Id=1001; ProviderName='Microsoft-Windows-WER-SystemErrorReporting'} -MaxEvents 20 -ErrorAction SilentlyContinue | Select TimeCreated, Message
Get-WinEvent -FilterHashtable @{LogName='System'; ProviderName='Microsoft-Windows-Kernel-Power'} -MaxEvents 30 | Select TimeCreated, Id, Message

# Nonpaged pool? PoolMon needs WDK; fallback is the perf counter:
Get-Counter '\Memory\Pool Nonpaged Bytes' -SampleInterval 1 -MaxSamples 10
Get-Counter '\Memory\Pool Paged Bytes' -SampleInterval 1 -MaxSamples 10

# Disk free + sled scratch dirs:
Get-PSDrive C
Get-ChildItem -Force C:\Users\Immanuelle\Documents\Github\SutraDB | Where-Object PSIsContainer | ForEach-Object {
  $size = (Get-ChildItem $_.FullName -Recurse -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum / 1GB
  "{0,-50} {1:N2} GB" -f $_.Name, $size
}

# What scheduled tasks are still armed?
Get-ScheduledTask | Where-Object State -ne 'Disabled' | Where-Object TaskName -match 'loka|Loka|post_eval|training|cron' | Select TaskName, State, @{n='NextRun'; e={(Get-ScheduledTaskInfo $_).NextRunTime}}

# Stray python processes?
Get-Process python, python3, loka -ErrorAction SilentlyContinue | Select Id, ProcessName, StartTime, WorkingSet64, Path

# GPU driver state + crash dump pointers:
Get-WinEvent -LogName Application -MaxEvents 500 -ErrorAction SilentlyContinue |
  Where-Object { $_.ProviderName -match 'nvlddmkm|NVIDIA' -and $_.LevelDisplayName -in 'Error','Warning','Critical' } |
  Select TimeCreated, Id, Message -First 20
dir C:\Windows\Minidump\ -ErrorAction SilentlyContinue | Sort LastWriteTime -Descending | Select -First 10
dir C:\Windows\MEMORY.DMP -ErrorAction SilentlyContinue | Select Name, Length, LastWriteTime
```

The single most valuable piece of data is whether `C:\Windows\Minidump\` has fresh `.dmp` files near the crash times — those identify the bugcheck code and the offending driver, which usually solves the whole question in one step.

---

## Mitigations to try, ranked by ease/payoff

### M1 — Don't run ingest + training + serve concurrently  *(easiest, highest payoff)*

The c36970b sled fix is a probable fix, not a guaranteed one. Until we know it holds at the 50 M plateau, run the three workloads sequentially:

1. `loka serve` + ingest only → finish the ingest cap, stop the importer.
2. Stop `loka serve`. Restart it for query-only.
3. Preprocess → corpus file written. Stop `loka serve`.
4. Training only (GPU + small Python footprint, no Loka at all).
5. Restart `loka serve` for inference / `--post` writeback.

This costs wall-clock time. It buys back kernel headroom that the c36970b fix can't reclaim alone.

### M2 — Disable both crons until the engine is verified

`training_cron.py` and `post_eval_cron.py` exist to keep the cycle going unattended. They were great for v6-v10 when the corpus was small. At 33-50 M triples the autonomous loop is the failure amplifier: a failing cycle just retries on the next firing. **Disable both until at least one full v11 cycle completes by hand.** This is `Disable-ScheduledTask` for whichever launches them.

### M3 — Cap sled cache and flush rate even more aggressively for the next ingest test

Current (c36970b): 256 MB cache, 2 s flush, `Mode::HighThroughput`.
Try: 128 MB cache, 5 s flush, `HighThroughput`. The downside is bigger durability window (5 s of writes lost on crash) but at this scale we have application-layer replay (`wikidata_hf_import_state.json`) anyway.

### M4 — Move sled state off the system drive

If `loka-data-cron-c1/` is on `C:\`, system-pool pressure is harder to recover from because Windows itself competes for the same disk's I/O queue. A secondary SSD for the sled state isolates the I/O storm.

### M5 — Bring forward the RocksDB migration

sled 0.34 has been unmaintained since 2021. RocksDB has battle-tested behaviour on Windows at TB scale. The c36970b fix was always a tactical patch; the strategic answer in `CLAUDE.md`'s open questions has been "RocksDB" from day one. If the same panic recurs at the next ingest plateau (queue.md item #1 explicitly names this as the escalation criterion), this is the work that pays off.

### M6 — Reduce GPU pressure: smaller batch, fewer parallel workers

If H3 is contributing, batch 32 instead of 64 cuts NVIDIA driver pool pressure roughly in half. Same `d_model`/`layers`, ~2× the wall-clock training time. Cheap insurance while we figure out which hypothesis dominates.

---

## What I'd want the user to answer when they get back

(These narrow the hypothesis tree; the user can fill them in as a comment on this file or just answer in chat.)

1. **When the box becomes unstable, does the display blank-then-recover before it freezes?** (yes → H3 GPU; no → H1/H2 kernel/memory.)
2. **Does the OS itself BSOD, or does it just hang with the mouse/keyboard frozen?** (BSOD → check `C:\Windows\Minidump\` ; hang → H1 nonpaged-pool or H6 hardware.)
3. **Is the instability tied to ANY heavy workload, or specifically to running Loka + training?** (any heavy workload → H6 hardware; only this project → H1/H2/H3.)
4. **Total system RAM and whether the page file is on the same drive as `loka-data-cron-*`.** (If both on `C:` with limited free space → H4 amplifier.)
5. **Date the instability started — does it line up with the first big-corpus ingest (2026-05-11) or earlier?** Earlier instability suggests H6 hardware; coincident with the bigger corpus suggests H1/H2.

---

## What this doc IS and ISN'T

This is preliminary analysis from the repo only — DEVLOG, queue.md, training/logs/, and the cron source. I have not run anything on the affected machine. The hypotheses are weighted by what the repo's evidence supports; ground truth from Event Viewer / Minidump will probably collapse the tree to one or two.

This is NOT a definitive verdict. The most likely outcome of the next session is that we find one minidump or one EventID that explains 80 % of it, and this doc gets pruned hard.
