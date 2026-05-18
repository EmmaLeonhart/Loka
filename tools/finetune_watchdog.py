"""Crash-watchdog for the QLoRA fine-tune run.

Why this exists: the optimizer state of the *currently running* trainer is
GPU-resident (bitsandbytes PagedAdamW8bit, CUDA-context-bound) and cannot be
extracted from outside the process — a host-RAM dump can't reach VRAM and a
reboot wipes it. So we cannot make the *current* process crash-safe
retroactively. What we CAN do: if it dies, relaunch it on the *fixed*
`finetune.py`, which from then on checkpoints `trainer_state.pt`
(optimizer + RNG) every epoch. Net effect:

  - process survives  -> optimizer stays continuous in RAM (ideal)
  - first crash        -> ONE fresh-Adam resume from the last saved adapter
                          (old-code epochs had no optimizer file); small
                          transient bump for a QLoRA-from-good-adapter
  - any later crash    -> CLEAN optimizer resume (new code wrote the state)

So the unavoidable one-time cost is paid only IF a crash happens, and never
more than once. This watchdog does not touch a healthy run.

Run (leave it running alongside the trainer):

    python tools/finetune_watchdog.py

It ensures *exactly one* trainer is alive, stops when training is complete,
and caps consecutive relaunches so a hard-failing run can't loop forever.
"""
from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LOG = REPO / "training" / "finetune" / "logs" / "overnight.log"
ADAPTERS = REPO / "training" / "finetune" / "adapters" / "qwen2.5-1.5b-loka-v1"

# Must match the launch command exactly so a relaunch resumes the same run.
TRAIN_CMD = [
    sys.executable, str(REPO / "training" / "finetune" / "finetune.py"),
    "--input", str(REPO / "training" / "finetune" / "data" / "sft_v1.jsonl"),
    "--output", str(ADAPTERS),
    "--hf-repo", "EmmaLeonhart/loka-qwen2.5-1.5b",
    "--epochs", "12", "--batch-size", "8", "--limit", "35000",
    "--log-every", "200",
]

POLL_S = 30
MAX_CONSECUTIVE_RELAUNCHES = 5  # a run that dies this many times w/o progress = give up


def trainer_running() -> bool:
    """True if a python process is executing finetune.py (Windows: match the
    command line via WMIC/CIM)."""
    try:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             "Get-CimInstance Win32_Process -Filter \"name='python.exe'\" | "
             "Where-Object { $_.CommandLine -like '*finetune.py*' } | "
             "Measure-Object | %{ $_.Count }"],
            capture_output=True, text=True, timeout=30,
        )
        return out.stdout.strip().isdigit() and int(out.stdout.strip()) > 0
    except Exception as e:  # noqa: BLE001 - never let the check kill the watchdog
        print(f"[watchdog] process check error (assuming alive): {e}",
              file=sys.stderr)
        return True


def training_complete() -> bool:
    if LOG.exists():
        tail = LOG.read_text(encoding="utf-8", errors="replace")[-4000:]
        if "training complete." in tail:
            return True
    return False


def latest_epoch() -> int:
    n = 0
    if ADAPTERS.exists():
        for d in ADAPTERS.glob("epoch*"):
            try:
                n = max(n, int(d.name[5:]))
            except ValueError:
                pass
    return n


def main() -> None:
    print(f"[watchdog] monitoring trainer (poll {POLL_S}s). "
          f"Will relaunch on the fixed finetune.py if it dies.", file=sys.stderr)
    consecutive = 0
    last_epoch_seen = latest_epoch()
    while True:
        time.sleep(POLL_S)
        if training_complete():
            print("[watchdog] training complete — exiting.", file=sys.stderr)
            return
        if trainer_running():
            consecutive = 0  # healthy
            ep = latest_epoch()
            if ep != last_epoch_seen:
                print(f"[watchdog] progress: epoch {ep} on disk.", file=sys.stderr)
                last_epoch_seen = ep
            continue

        # Trainer is gone and training is not complete -> relaunch.
        ep = latest_epoch()
        if ep != last_epoch_seen:
            consecutive = 0  # it made progress since last relaunch; reset
            last_epoch_seen = ep
        consecutive += 1
        if consecutive > MAX_CONSECUTIVE_RELAUNCHES:
            print(f"[watchdog] {consecutive} relaunches with no progress past "
                  f"epoch {ep} — stopping to avoid a crash loop. Investigate "
                  f"{LOG}.", file=sys.stderr)
            return
        print(f"[watchdog] trainer not running (last epoch on disk: {ep}). "
              f"Relaunch #{consecutive} on the fixed finetune.py "
              f"(resumes from epoch {ep}; "
              f"{'CLEAN optimizer' if (ADAPTERS / 'trainer_state.pt').exists() else 'fresh Adam — first crash only'}).",
              file=sys.stderr)
        with LOG.open("a", encoding="utf-8") as f:
            f.write(f"\n=== [watchdog] relaunch #{consecutive} "
                    f"@ {time.strftime('%Y-%m-%d %H:%M:%S')} from epoch {ep} ===\n")
            f.flush()
            subprocess.Popen(TRAIN_CMD, stdout=f, stderr=subprocess.STDOUT,
                             cwd=str(REPO))
        time.sleep(120)  # let it boot/load before the next liveness check


if __name__ == "__main__":
    main()
