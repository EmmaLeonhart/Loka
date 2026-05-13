# System Instability — Evidence Gathered on 2026-05-13

This is ground-truth data from the affected box, collected by Claude Code as part of queue.md step 1 ("Run the diagnostic triage on the affected box"). It complements `planning/system-instability-diagnosis.md` (the pre-evidence triage) and feeds the verdict in `planning/system-instability-verdict-2026-05-13.md`.

## Hardware (confirmed)

- **Form factor:** laptop (hostname `laptop-qe4jv37b`)
- **CPU:** AMD Ryzen 7 8845HS (Zen 4 mobile, 8 cores / 16 threads, max 3801 MHz, integrated Radeon 780M iGPU). Default cTDP ~28 W, sustained boost up to ~54 W.
- **dGPU:** NVIDIA GeForce RTX 4070 **Laptop** GPU. `AdapterRAM` reports 4.0 GB (the full WDDM-visible amount is 8 GB; AdapterRAM under-reports on dual-GPU systems). Driver 32.0.15.8183 dated 2025-11-03 — current at evidence time.
- **iGPU:** AMD Radeon 780M (driver 32.0.11020.30000, 2025-12-09). Secondary display path on the APU.
- **RAM:** 31.3 GB total. 13.2 GB free at the time of measurement.
- **Page file:** `C:\pagefile.sys`, allocated 34 GB, peak usage 20 MB (negligible — memory pressure is not the proximate cause of the freezes).
- **Disk:** C:\ has 758.6 GB free of 1148 GB used (1907 GB total). Disk-fill (H4) is OFF the table.

**The desktop-vs-laptop distinction is critical.** The chat assumed a desktop 4070 (12 GB, 200 W TGP). The actual hardware is a laptop 4070 (~8 GB, 35–115 W TGP, thermally bottlenecked). All training-load assumptions in queue.md inherit from a desktop-4070 baseline and need to be re-evaluated.

## Loka data dirs (on-disk)

```
loka-data-cron-c1                            17.59 GB
loka-data-cron-c1.v9-shipped                  0.51 GB
sutra-data                                    0.81 GB
sutra-data-backup-2026-05-09                  0.72 GB
```

`loka-data-cron-c1` has grown to **17.6 GB** (queue.md mentions "5 GB sled state" — that was an underestimate or stale). Sled fsync pressure is therefore worse than the c36970b fix was sized for.

## Process / scheduled-task state

- `loka-v11-kickoff`: `Disabled`. LastRun 1999-11-30 placeholder (never ran). LastResult 267011 (irrelevant given the never-ran state). No other Loka-tagged scheduled tasks exist.
- No `python` / `python3` / `loka` processes are running right now.
- No SutraDB/Loka processes anywhere.

The "stop the bleeding" PowerShell block is a no-op on this box at the current moment.

## Crash evidence — the smoking gun

### Kernel-Power Event 41 (unexpected shutdown — "system rebooted without cleanly shutting down")

```
2026-05-13 13:52:20
2026-05-13 13:20:39
2026-05-12 16:53:43
2026-05-01 23:15:55
2026-04-29 21:05:42
2026-04-27 02:30:07
2026-04-25 20:12:24
2026-04-20 09:58:26
2026-04-09 20:38:50
2026-03-27 15:23:34
```

**10 unexpected shutdowns over 7 weeks.** Frequency rises with the v11/big-corpus push: 2 today alone, 1 yesterday. The earliest event (2026-03-27) predates the bigger-corpus work — the instability is a longstanding hardware-level problem, but the recent workload is amplifying it.

### Boot/shutdown timeline today (2026-05-13)

```
12:30:22  SHUTDOWN
12:30:36  BOOT
12:35:07  SHUTDOWN     (5 min uptime)
12:35:18  BOOT
12:36:03  SHUTDOWN     (45 s uptime!!)
12:36:15  BOOT
13:20:32  BOOT          ← no prior SHUTDOWN = Kernel-Power 41
13:52:15  BOOT          ← no prior SHUTDOWN = Kernel-Power 41
13:52:59  SHUTDOWN
13:53:11  BOOT          (current session)
```

A cluster of crashes and reboots in ~1.5 hours, matching the user's chat description ("had to force press the power button … weirdly kind of frozen even when I started").

### NO BSODs, NO minidumps, NO WHEA, NO nvlddmkm errors

- `C:\Windows\Minidump\` does not exist on this system.
- `C:\Windows\MEMORY.DMP` does not exist.
- `Get-WinEvent` for Application log filtered to `nvlddmkm | NVIDIA` returns **zero error/warning rows**.
- `Get-WinEvent -ProviderName 'WHEA-Logger'` returns nothing in the 14-day window.
- `Win32_ReliabilityRecords` has no "display driver stopped responding" entries.

**This is the critical fingerprint.** A kernel BSOD writes a minidump. The NVIDIA driver logs an Application event on a TDR. WHEA logs hardware machine-check exceptions. **None of those happened.** The Windows OS was not the layer that detected the failure. The failure happened **below the OS** — firmware-level thermal cutoff, EC-driven power-down, or PSU/VRM protection latch. This is exactly the signature of H6 (thermal/hardware) and *not* of H1 (kernel-pool exhaustion would still let the OS write a log).

### IOMMU faults on the dGPU

```
2026-05-10 17:47:58  HAL 15  IOMMU error: Device 0x200, FaultInformation 0x4CEE0F7C, FaultReason 0x6
2026-05-10 00:28:39  HAL 15  IOMMU error: Device 0x200, FaultInformation 0x4CEE0F7C, FaultReason 0x6
2026-05-09 02:01:36  HAL 15  IOMMU error: Device 0x200, FaultInformation 0x6CEE0F7C, FaultReason 0x6
```

Device 0x200 = PCIe bus 2, function 0 = the dGPU on this laptop. Three DMA-remapping errors against the dGPU just before / during the v10/v11 workload push. **FaultReason 0x6 = IO_PAGE_FAULT on AMD IOMMU** — typically caused by the device issuing a DMA to an address the IOMMU has not been granted, OR by RAM corruption (the IOMMU sees the corrupted address and rejects it). On an AMD APU + Nvidia dGPU laptop this is a known fragile path — fixes usually involve BIOS updates, IOMMU=PT in the kernel parameters (Linux-only), or NVIDIA driver downgrade.

The IOMMU events themselves do not crash the system, but they indicate the dGPU + AMD-IOMMU interaction is unstable, which is a contributing factor.

### TDR registry — running defaults

```
TdrDelay      : (not set → default 2 s)
TdrDdiDelay   : (not set → default 5 s)
TdrLevel      : (not set → default 3 = full recovery)
TdrLimitCount : (not set → default 5)
TdrLimitTime  : (not set → default 60 s)
```

2 s is too short for sustained CUDA workloads. The Studio driver branch is the supported path for compute; we are on driver `32.0.15.8183` (Game Ready, dated 2025-11-03) — Game Ready prioritises gaming TDR responsiveness, not compute stability.

### Other findings (low-priority, recorded for completeness)

- 27× `DistributedCOM 10010` errors over 7 days — DCOM service-broker timeouts. Common on Windows during heavy CPU contention. Symptom not cause.
- 17× `TPM-WMI 1796` — known noisy event, harmless.
- 9× `HAL 20/21` "ACPI Time and Alarm Device failed (status 0xC00000BB)" — fires on every boot. ACPI TAD is not supported by this firmware; benign but evidence the BIOS has gaps.
- 5× `BTHUSB 16` — Bluetooth USB stack noise, unrelated.

## Pool counters at idle (not at-crash)

```
\Memory\Pool Nonpaged Bytes:  1031 MB
\Memory\Pool Paged Bytes:     1107 MB
```

Normal for an idle Windows 11 install with 32 GB RAM. We do not have an at-crash sample. Page-file peak of 20 MB tells us paging wasn't the issue.

## What we did NOT do, and why

The diagnosis doc's "Stop the bleeding" block would (a) disable the post_eval cron the user set up in the earlier quiet window, (b) mass-kill any python process. The auto-mode classifier blocked it as broader than the diagnostic-triage permission scope, and on closer inspection the box is already quiet (no python/loka processes, v11 task already disabled, no other Loka-tagged scheduled tasks). Nothing to disable beyond what is disabled.
