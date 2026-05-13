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

### Additional symptoms from `chats/system-instability.md` (added 2026-05-13)

The user's recap of a recent incident (transcript in `chats/system-instability.md`, extracted 2026-05-13 from a separate Claude conversation, **9 turns**):

- "Screen goes black" — sudden display loss, not a controlled OS shutdown.
- **"Computer getting very slow and then all going black"** — gradual slowdown phase preceding the blank, ~minutes scale.
- Force-power-off was required ("force press the power button to stop it and then start it and restart it").
- **Even after the hard power-cycle the box was still "weirdly kind of frozen even when I started"** — i.e. a cold boot did not by itself recover.
- **15 minutes powered off → fully working.** "I left my computer off for 15 minutes, and then that seems to have solved the thing. It's working normally now, but the restart kind of didn't work."
- Hardware confirmed: Windows, RTX 4070 (12 GB VRAM). CPU "comparable currentness to the 4070" — presumably a modern Ryzen 7000+ or 13th-gen+ Intel.
- The user's own attribution is the **preprocessing pipeline** ("the preprocessing was the thing that caused problems"). This is informed guess, not measurement.

The 15-min-off-then-fine pattern is the single most diagnostic detail in the whole transcript. See the revised hypothesis ranking below.

### Are these the same incident as the sled panic in DEVLOG?

**Probably not.** The repo evidence (`training/logs/loka-restart.log`, DEVLOG 2026-05-12 23:52 UTC) is a *logged* sled panic with `ERROR_NO_SYSTEM_RESOURCES (os error 1450)` — i.e. the Loka process aborted in a way that left a stack trace behind. The chat describes the OS itself going dark, with no log to point at. These are plausibly two failure modes overlapping on the same workload, not one unified phenomenon:

1. **Kernel-pool exhaustion from sled** → logged panic, process death, OS probably degraded but recoverable. Fix shipped in `c36970b`.
2. **Thermal/GPU-driver wedge during sustained training+preprocess** → no log, screen blank, hard-reset needed, 15-min cool-down required for full recovery.

The diagnosis must address both — treating them as one mystery has been masking the chat-described pattern, which has a *different* signature.

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

> **Update 2026-05-13 after reading `chats/system-instability.md`:** the cool-down-recovery pattern (15 min off → working; soft reboot didn't work; cold boot alone didn't work) is **not** a kernel-software-state signature. Kernel nonpaged pool resets on every boot — H1 cannot explain "I had to wait for it to cool." So the chat-described incident is most plausibly **H3 (GPU driver wedge)** or **H6 (thermal)**, with **H2 (preprocess memory pressure)** as the most likely contributor during the gradual-slowdown phase. H1 still explains the *logged sled panic* in DEVLOG, but that appears to be a separate failure mode — see "Are these the same incident" above. The pre-revision ranking is preserved below for context; the post-chat ranking is at the top of each entry.

**Post-chat ranking summary**, highest → lowest confidence for *the chat-described incident*:
H3 (GPU TDR wedge) ≈ H6 (thermal) > H2 (preprocess RAM pressure as trigger) > H1 (sled kernel pool — owns the *logged* panic but probably not the OS freeze) > H4 (disk) > H5 (orphan crons) ≥ hardware-elsewhere.

### H1 — Windows nonpaged-pool exhaustion driven by sled+ingest+training concurrency  *(was #1 pre-chat; now best explanation of the LOGGED sled panic, but does not match cool-down recovery)*

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

### H3 — GPU driver crash during training (TDR-stuck / VIDEO_TDR_FAILURE / DPC_WATCHDOG)  *(promoted to #1 candidate for the chat-described incident)*

`train.py` runs on a 4070, batch 64, `d_model=512`, 6 layers, 44.5 M params. v10 trained 20 epochs at ~10 min/epoch. v11_kickoff plans 4 epochs at ~52 min/epoch on the bigger corpus → ~3.5 h sustained GPU load. NVIDIA driver TDR (Timeout Detection and Recovery) bugs are a recurring source of Windows BSODs, especially when other things on the box are also competing for I/O.

**Why promoted by the chat.** A TDR-stuck GPU firmware state is one of the few Windows failure modes that survives a soft reboot AND survives a fast cold boot, but is cleared by leaving the box powered off long enough for the card to fully discharge (rails settle, firmware reloads from cold). The user's pattern — soft reboot failed, immediate cold boot was "weirdly kind of frozen even when I started", 15 min off then fine — matches this exactly. Reported widely against the 4070-class consumer Ada cards under sustained CUDA load on Windows.

**Predictions if this is right:**
- Event Viewer → Application or System log shows `nvlddmkm` errors clustered around the freeze time.
- `Get-CimInstance Win32_ReliabilityRecords` lists "Display driver stopped responding and has recovered" entries.
- `C:\Windows\Minidump\` may have a `.dmp` whose bugcheck code is `0x116 VIDEO_TDR_FAILURE` or `0x117 VIDEO_TDR_TIMEOUT_DETECTED`.
- May be reproducible by holding the 4070 at sustained >95% utilisation for 30 min without any other workload — if it crashes alone, it's H3 or H6 cleanly.

**Mitigation (if confirmed):** smaller batch (32, see M6); enforce NVIDIA's recommended Pcie power-management = "Prefer maximum performance" to avoid clock thrash; consider the latest Studio driver branch (more stable than Game Ready under sustained compute).

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

### H6 — Hardware: thermals / PSU / RAM  *(promoted to co-#1 with H3 after the chat: 15 min cool-down recovery is the canonical thermal signature)*

System-wide instability that starts after a heavy workload is sometimes mis-attributed to software when it's actually a marginal RAM stick that only fails under load, or a PSU sagging when GPU + sustained disk I/O happen together, or thermal throttling reaching shutdown territory.

**Why promoted by the chat.** "Left my computer off for 15 minutes, and then that seems to have solved the thing" is, in isolation, the textbook thermal-recovery sequence: heatsoaked component (CPU, VRMs, NVMe controller, GPU power-stage MOSFETs) needs time to drop below the firmware/hardware protection cutoff. PSU sag is a similar shape — caps need time to discharge and the OCP latch resets. The gradual-slowdown phase before the black screen is also consistent with thermal throttling progressively cutting clocks until the system can't keep up.

**Predictions if this is right:**
- HWiNFO64 logged during a workload run shows CPU package > 95 °C, GPU hotspot > 95 °C, or VRM > 100 °C in the seconds before the freeze.
- `WHEA-Logger` events in Event Viewer (machine-check exceptions, especially type 19 / processor cache errors point to thermal/voltage failure).
- `mdsched` (Windows Memory Diagnostic) reports any failure — if so, that's the answer alone.
- Reproducible with FurMark or `nvidia-smi` 100% load + a CPU stress test simultaneously, with no Loka running — if it crashes there, it is not Loka's fault at all.

**Mitigation:** clean dust from heatsinks (yes, really); check case fan curves; if PSU is unknown vintage and the box has both a 4070 *and* a hungry CPU under sustained load, a PSU upgrade is cheap insurance. M3+ memtest86 pass for RAM. If thermals look fine and PSU is solid, this hypothesis demotes.

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

# Added 2026-05-13 after chat: TDR-specific checks (H3 promotion)
# Display-driver reliability records (any "Display driver stopped responding" entries):
Get-CimInstance Win32_ReliabilityRecords -ErrorAction SilentlyContinue |
  Where-Object { $_.Message -match 'display|nvlddmkm|TDR' } |
  Sort TimeGenerated -Descending | Select TimeGenerated, SourceName, Message -First 20

# Confirm the NVIDIA driver branch (Studio vs Game Ready); Studio is the supported branch for sustained compute:
Get-CimInstance Win32_VideoController | Select Name, DriverVersion, DriverDate

# Current TDR registry settings (default is 2 s timeout — short for long CUDA kernels):
Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' -ErrorAction SilentlyContinue |
  Select TdrDelay, TdrDdiDelay, TdrLevel, TdrLimitCount, TdrLimitTime
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

Updated with chat-derived answers (2026-05-13). Remaining open questions narrow H3 vs H6.

1. ~~**When the box becomes unstable, does the display blank-then-recover before it freezes?**~~ — chat answer: "the screen goes black" without mention of a recovery flash. Could still be a single-shot TDR that didn't survive; doesn't disambiguate H3 from H6.
2. ~~**Does the OS itself BSOD, or does it just hang with the mouse/keyboard frozen?**~~ — chat answer: hang, not BSOD ("getting very slow and then all going black"). Probably means H3 wedge or H6 thermal cutoff rather than a clean kernel crash with a dump. **`C:\Windows\Minidump\` is still worth checking** — if there's a `.dmp` we still get a bugcheck code.
3. **Still open: is the instability tied to ANY heavy workload, or specifically to running Loka + training?** Critical for separating H6 hardware from project-specific causes. Test: run a 30-min FurMark + Prime95 with no Loka.
4. **Still open: total system RAM and whether the page file is on the same drive as `loka-data-cron-*`.** Affects whether preprocessing memory pressure is a bigger amplifier than assumed.
5. ~~**Date the instability started — does it line up with the first big-corpus ingest (2026-05-11)?**~~ — chat doesn't pin a date; user just says "ever since I got started on this project". The 50–100M-triple workload only arrived recently, so the coincidence is plausible but not confirmed.
6. **New, from chat:** *which* process was running when the freeze happened — preprocess.py alone, training alone, both concurrently, or training + a still-active ingest? The chat is a bit ambiguous ("a big AI training run" but later "the preprocessing was the thing"). Critical for ranking H2 vs H3.
7. **New, from chat:** what driver branch is on the 4070 — Game Ready or Studio? Studio is the supported-for-compute path; Game Ready under sustained CUDA is a known TDR-stuck risk.

---

## What this doc IS and ISN'T

This is preliminary analysis from the repo only — DEVLOG, queue.md, training/logs/, and the cron source. I have not run anything on the affected machine. The hypotheses are weighted by what the repo's evidence supports; ground truth from Event Viewer / Minidump will probably collapse the tree to one or two.

This is NOT a definitive verdict. The most likely outcome of the next session is that we find one minidump or one EventID that explains 80 % of it, and this doc gets pruned hard.
