# System Instability — Verdict (2026-05-13)

Based on `planning/system-instability-evidence-2026-05-13.md` and the existing diagnosis in `planning/system-instability-diagnosis.md`.

## Verdict

**H6 (thermal / firmware-level hardware shutdown on a thermally-constrained laptop) is the primary cause of the OS freezes.** **H3 (NVIDIA driver fragility on this AMD-APU + 4070-Laptop combo, exacerbated by 2 s TDR defaults and Game Ready driver branch) is a contributing factor and is the most likely explanation for the IOMMU DMA faults on the dGPU. H1 (sled kernel-pool exhaustion) is real but separate** — it owns the *logged* sled panic in `training/logs/loka-restart.log`, NOT the OS-freeze incidents.

Confidence: **high** for H6 being the OS-freeze cause. The signature is unambiguous — 10 Kernel-Power 41 events, zero BSODs, zero minidumps, zero WHEA, zero nvlddmkm errors. The failures happen below the OS layer, which is what hardware/firmware thermal cutoffs look like by definition. The user's chat-reported 15-min cool-down recovery is the canonical thermal signature.

## Why H1 is not the cause of the freezes

Kernel nonpaged-pool exhaustion makes the OS very slow and eventually causes drivers to fail allocating I/O buffers, but the OS continues to run, log events, and write minidumps when it eventually crashes. None of those happened. The 17.6 GB sled state was big enough to make pool exhaustion possible, and the fix in `c36970b` was a reasonable guard, but the LOGGED `os error 1450` panic in DEVLOG is a process-level event — the Loka process aborted with that error code, the OS itself stayed up. The OS-freeze incidents are a different failure mode.

## Why H6 is the cause

- **Form factor**: laptop with Ryzen 7 8845HS + 4070 Laptop dGPU. Laptop thermals cannot sustain `4 epochs × 52 min` at the 4070's full TGP with concurrent sled fsyncs and concurrent preprocessing. The desktop 4070 the chat-side Claude assumed (12 GB, 200 W TGP) would have had room. This one does not.
- **No OS-side crash evidence**: 0 BSODs, 0 minidumps, 0 WHEA, 0 nvlddmkm events. The OS was not the one shutting down.
- **15-min cool-down recovery** in the chat: textbook thermal-saturation pattern. Power-cycled but cold-booted twice → still wedged. Wait for components to drop below the firmware cutoff → fine. Kernel state is reset on every boot, so it isn't software.
- **Frequency correlates with workload intensity**: 10 unexpected shutdowns from 2026-03-27, accelerating to 2 today + 1 yesterday during the v11/big-corpus push.
- **IOMMU dGPU faults** (3 events 2026-05-09/10): the 4070 dGPU's DMA path on this AMD-IOMMU laptop is unstable. Not the proximate cause of freezes (IOMMU faults don't kill the OS), but the same hardware fragility that the thermal cutoff is sitting on top of.

## What this means for the v11 plan

**v11 should NOT resume on this hardware as currently planned.** A 3.5-hour sustained training run, even with the engine fix, is asking a laptop GPU to survive what desktop GPUs barely handle. The current plan was designed against assumed desktop-class thermal headroom.

### Required before v11 resumes (any single one is necessary, all three together is ideal)

1. **External cooling.** Cooling pad, elevated rear, ambient temperature reduced. Verifiable: HWiNFO64 logging during a 30-min stress test shows GPU hotspot stays under 90 °C and CPU package stays under 95 °C.
2. **Workload serialisation.** No `loka serve` + ingest + training concurrently. Run them strictly sequentially per Mitigation M1 in the diagnosis doc. The c36970b sled fix bought margin; this removes the multi-rail pressure entirely.
3. **Smaller training batch.** `--batch-size 32` instead of 64. Cuts NVIDIA driver pool pressure ~2× and reduces per-step GPU power draw. ~2× wall-clock cost.

### Required setting changes (one-time)

4. **Raise TDR timeout** from the 2 s default. A reasonable starting point for sustained CUDA on a laptop is `TdrDelay=10` (REG_DWORD, seconds, `HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers`). Requires a reboot.
5. **Switch NVIDIA driver branch to Studio** (currently Game Ready 32.0.15.8183). The Studio branch is the compute-supported path; Game Ready optimises for low-latency game framerate at the cost of long-kernel stability. Download from nvidia.com → Studio Drivers → 4070 Laptop.
6. **Check BIOS for an update.** The HAL `ACPI TAD failed` warnings + repeat IOMMU faults are firmware-level — a vendor BIOS update has a non-trivial chance of fixing the IOMMU issue and unlocking better PCIe stability for the 4070.

### Recommended, lower-priority

7. **Move the training workload off this laptop.** Cloud GPU rental (Lambda Labs, RunPod, Vast.ai) for the 3.5-h training pass would cost <$5 per cycle and eliminates the thermal risk entirely. The corpus and tokenizer already live on Hugging Face. This is the most strategically correct mitigation: a research-grade training cycle on consumer laptop hardware was always going to be marginal.

### What stays as-is

- `loka-v11-kickoff` stays Disabled.
- Both crons (`training_cron.py`, `post_eval_cron.py`) stay disabled until at least one v11 cycle completes by hand under mitigations 1–3.
- `loka-data-cron-c1` at 17.6 GB stays — the corpus is fine.

## Residual risks

- **The 4070 IOMMU faults** may persist even after thermal mitigations. If they do and they correlate with new GPU TDRs (we will be able to tell once TDR timeout is raised — TDRs will now write events), the next step is BIOS update → driver downgrade → driver Studio-branch swap. Worst case is a hardware return-to-vendor.
- **The H1 sled fix is unverified at the 50 M plateau.** Even with H6 mitigated, the previously-unresolved question "does `c36970b` hold against a fresh sustained ingest" remains open. Recommend: training only, no fresh ingest, until v11 ships.

## Next steps for queue.md

1. Mark queue step 1 (diagnostic triage) DONE.
2. Replace queue step 3 ("Start the v11 training cycle") with a precondition list: cooling + serialisation + batch-32 + Studio driver, then attempt. Add a "go / no-go check before launching" sub-step that logs HWiNFO64 at idle and verifies temps.
3. Queue step 4 (5-h post-v11 cron) remains deferred — no point designing it until v11 produces a checkpoint to analyse.
