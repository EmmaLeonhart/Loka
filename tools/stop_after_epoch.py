"""Autonomous clean-stop for the QLoRA run at a chosen epoch.

Emma's decision (2026-05-18): stop after epoch 5. The trainer was launched
with --epochs 12 and a running process's args can't be changed, so this
watches the log and stops it *cleanly* the moment epoch N is fully done:

  1. wait for the "=== epoch N done:" line (adapter + trainer_state saved),
  2. then wait for that epoch's HF push to RESOLVE — either
     "[hf] pushed epochN" or "[hf] push FAILED" (kept local) — so epoch N
     is durable before we stop,
  3. then kill finetune.py (and any finetune_watchdog.py so it can't
     relaunch), and append a clear stop marker + "training complete." to
     the log so a future watchdog treats the run as finished, not crashed.

A safety net also fires if "ep{N+1} " appears (epoch N+1 started) — epoch N
is unambiguously complete by then.

Run in the background:  python tools/stop_after_epoch.py --epoch 5
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LOG = REPO / "training" / "finetune" / "logs" / "overnight.log"
POLL_S = 60
PUSH_GRACE_S = 1200  # after "epoch N done", wait this long for the push to resolve


def tail(path: Path, n: int = 20000) -> str:
    if not path.exists():
        return ""
    return path.read_text(encoding="utf-8", errors="replace")[-n:]


def kill_matching(substr: str) -> int:
    """Kill python processes whose command line contains `substr`. Returns
    how many were killed."""
    try:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             "$p = Get-CimInstance Win32_Process -Filter \"name='python.exe'\" | "
             f"Where-Object {{ $_.CommandLine -like '*{substr}*' }}; "
             "$p | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }; "
             "($p | Measure-Object).Count"],
            capture_output=True, text=True, timeout=60,
        )
        s = out.stdout.strip().splitlines()[-1] if out.stdout.strip() else "0"
        return int(s) if s.isdigit() else 0
    except Exception as e:  # noqa: BLE001
        print(f"[stop] kill error for {substr!r}: {e}", file=sys.stderr)
        return 0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--epoch", type=int, default=5)
    args = ap.parse_args()
    N = args.epoch
    done_marker = f"=== epoch {N} done:"
    pushed_ok = f"[hf] pushed epoch{N}"
    push_fail = "[hf] push FAILED"
    next_ep = f"ep{N + 1} "

    print(f"[stop] watching for clean end of epoch {N} "
          f"(poll {POLL_S}s). Trainer keeps running until then.",
          file=sys.stderr, flush=True)

    epoch_done_at = None
    while True:
        time.sleep(POLL_S)
        t = tail(LOG)

        if next_ep in t and epoch_done_at is None:
            # Epoch N+1 already started — epoch N is definitively complete.
            print(f"[stop] epoch {N + 1} started; epoch {N} is complete.",
                  file=sys.stderr, flush=True)
            break

        if epoch_done_at is None:
            if done_marker in t:
                epoch_done_at = time.time()
                print(f"[stop] epoch {N} done — waiting for its HF push to "
                      f"resolve (up to {PUSH_GRACE_S//60} min).",
                      file=sys.stderr, flush=True)
            continue

        # epoch N done seen; wait for push to resolve or grace timeout.
        after = t.split(done_marker, 1)[-1]
        if pushed_ok in after:
            print(f"[stop] epoch {N} pushed to HF. Stopping cleanly.",
                  file=sys.stderr, flush=True)
            break
        if push_fail in after:
            print(f"[stop] epoch {N} HF push failed but adapter is saved "
                  f"locally. Stopping cleanly.", file=sys.stderr, flush=True)
            break
        if next_ep in after:
            print(f"[stop] epoch {N + 1} started; epoch {N} durable. Stopping.",
                  file=sys.stderr, flush=True)
            break
        if time.time() - epoch_done_at > PUSH_GRACE_S:
            print(f"[stop] push grace elapsed; epoch {N} adapter is saved "
                  f"locally. Stopping.", file=sys.stderr, flush=True)
            break

    n_wd = kill_matching("finetune_watchdog.py")
    n_tr = kill_matching("finetune.py")
    msg = (f"\n=== STOPPED CLEANLY AFTER EPOCH {N} BY USER REQUEST "
           f"@ {time.strftime('%Y-%m-%d %H:%M:%S')} "
           f"(killed trainer={n_tr}, watchdog={n_wd}) ===\n"
           f"training complete.\n")
    with LOG.open("a", encoding="utf-8") as f:
        f.write(msg)
    print(f"[stop] {msg.strip()}", file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
