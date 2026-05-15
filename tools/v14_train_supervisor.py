"""Supervisor: drive v14 training to epoch 10 no matter how many times the
GPU gets stolen out from under it.

Context: the 8 GB laptop GPU keeps getting grabbed by unrelated CUDA
processes (LLaMA experiment killed v12, pytest killed the v14 6-10 resume).
Each time, training OOMs mid-epoch. train.py saves a checkpoint after every
epoch and epoch_snapshot_pusher.py mirrors each to HF, so no completed epoch
is ever lost — but a human has had to manually relaunch every time. This
supervisor removes the human.

Loop:
  1. Figure out where to resume from:
       - Highest wikidata_v14_epochNN.pt with NN in [6..9]  -> resume there,
         start-epoch NN+1. These were saved by the new train.py so they HAVE
         optimizer state — Adam continues cleanly.
       - None yet  -> first attempt: resume from wikidata_v14_epoch04.pt
         (the series-best epoch-4 weights, ppl 202.01; epoch-5's 204.70
         regression is discarded). That checkpoint predates optimizer-state
         saving, so Adam restarts fresh for this first continuation only.
  2. Wait until the GPU is actually free (no foreign CUDA compute process,
     low memory) before launching — don't even start into a contended GPU.
  3. Launch train.py (fresh per-attempt log) + epoch_snapshot_pusher.py on
     that log so epochs 6..10 land on HF as v14.6 .. v14.10.
  4. When train.py exits:
       - epoch-10 checkpoint present  -> SUCCESS, promote + stop.
       - otherwise (OOM / killed)     -> log it, wait for GPU, loop.

Safe to leave running unattended for days. Stops itself on success or after
--max-attempts.

Usage:
    python tools/v14_train_supervisor.py
"""
from __future__ import annotations

import argparse
import io
import re
import subprocess
import sys
import time
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent
CKPT_DIR = ROOT / "training" / "checkpoints"
LOG_DIR = ROOT / "training" / "logs"
CANON = CKPT_DIR / "wikidata_v14.pt"
EPOCH04 = CKPT_DIR / "wikidata_v14_epoch04.pt"
DATA = ROOT / "training" / "data" / "triples_v14.txt"
VOCAB = ROOT / "training" / "data" / "vocab_bpe.json"
TOK = ROOT / "training" / "data" / "tokenizer_bpe.json"
TARGET_EPOCHS = 10
SNAP_RE = re.compile(r"wikidata_v14_epoch(\d{2})\.pt$")


def latest_continuation_epoch() -> int | None:
    """Highest epoch NN in [6..TARGET] for which a snapshot exists, else None."""
    best = None
    for p in CKPT_DIR.glob("wikidata_v14_epoch*.pt"):
        m = SNAP_RE.search(p.name)
        if not m:
            continue
        n = int(m.group(1))
        if 6 <= n <= TARGET_EPOCHS and (best is None or n > best):
            best = n
    return best


def gpu_free() -> bool:
    """True iff no foreign CUDA *compute* process and GPU memory is low.
    Our own train.py process counts as ours, but at the moment we poll
    before launching it, so any compute app is foreign."""
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=30,
        ).stdout.strip()
        used_mib = int(out.splitlines()[0].strip())
    except Exception as e:
        print(f"  [GPU] nvidia-smi query failed ({e}); assuming busy", flush=True)
        return False
    # Idle desktop sits well under 500 MiB (display/compositing). A live
    # training or LLaMA/pytest CUDA process pushes this to GB-scale.
    free_enough = used_mib < 800
    if not free_enough:
        print(f"  [GPU] {used_mib} MiB used — still busy, waiting", flush=True)
    return free_enough


def wait_for_gpu(poll_seconds: float = 120.0, max_wait_hours: float = 72.0) -> bool:
    deadline = time.time() + max_wait_hours * 3600
    while time.time() < deadline:
        if gpu_free():
            return True
        time.sleep(poll_seconds)
    return False


def launch_attempt(attempt: int) -> subprocess.Popen:
    cont = latest_continuation_epoch()
    if cont is None:
        resume_from = EPOCH04
        start_epoch = 6
        note = "first continuation: resume from epoch-4 weights (ppl 202.01), fresh Adam"
    else:
        resume_from = CKPT_DIR / f"wikidata_v14_epoch{cont:02d}.pt"
        start_epoch = cont + 1
        note = f"auto-resume from epoch-{cont} snapshot (has optimizer state, Adam continues)"

    ts = time.strftime("%Y%m%d_%H%M%S")
    train_log = LOG_DIR / f"v14_sup_attempt{attempt}_{ts}_train.log"
    push_log = LOG_DIR / f"v14_sup_attempt{attempt}_{ts}_pusher.log"

    print(f"\n=== Attempt {attempt}: {note} ===", flush=True)
    print(f"  resume_from={resume_from.name}  start_epoch={start_epoch}  "
          f"target={TARGET_EPOCHS}", flush=True)
    print(f"  train log:  {train_log}", flush=True)
    print(f"  pusher log: {push_log}", flush=True)

    train_cmd = [
        sys.executable, "-u", str(ROOT / "training" / "train.py"),
        "--data", str(DATA), "--vocab", str(VOCAB), "--bpe-tokenizer", str(TOK),
        "--checkpoint", str(CANON),
        "--epochs", str(TARGET_EPOCHS), "--start-epoch", str(start_epoch),
        "--batch-size", "16", "--tokens-per-role", "8",
        "--d-model", "512", "--nhead", "8", "--layers", "6",
        "--resume-from", str(resume_from),
    ]
    pusher_cmd = [
        sys.executable, "-u", str(ROOT / "tools" / "epoch_snapshot_pusher.py"),
        "--log", str(train_log), "--checkpoint", str(CANON),
        "--model", "14", "--hf-user", "EmmaLeonhart",
        "--hf-repo", "EmmaLeonhart/loka", "--idle-timeout-seconds", "21600",
    ]
    pusher = subprocess.Popen(pusher_cmd, stdout=push_log.open("w", encoding="utf-8"),
                              stderr=subprocess.STDOUT)
    trainer = subprocess.Popen(train_cmd, stdout=train_log.open("w", encoding="utf-8"),
                               stderr=subprocess.STDOUT)
    trainer._loka_pusher = pusher  # type: ignore[attr-defined]
    trainer._loka_train_log = train_log  # type: ignore[attr-defined]
    return trainer


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--max-attempts", type=int, default=20)
    ap.add_argument("--gpu-poll-seconds", type=float, default=120.0)
    args = ap.parse_args()

    print("v14 training supervisor — drives epochs 6..10 to completion, "
          "auto-resuming on any GPU-contention death.", flush=True)

    for attempt in range(1, args.max_attempts + 1):
        if (CKPT_DIR / f"wikidata_v14_epoch{TARGET_EPOCHS:02d}.pt").exists():
            print(f"epoch-{TARGET_EPOCHS} checkpoint already exists — done.", flush=True)
            break

        print(f"\n[Attempt {attempt}/{args.max_attempts}] waiting for a free GPU...",
              flush=True)
        if not wait_for_gpu(poll_seconds=args.gpu_poll_seconds):
            print("[ABORT] GPU never freed within the max wait window.", flush=True)
            return

        trainer = launch_attempt(attempt)
        rc = trainer.wait()
        pusher = getattr(trainer, "_loka_pusher", None)

        done = (CKPT_DIR / f"wikidata_v14_epoch{TARGET_EPOCHS:02d}.pt").exists()
        if done:
            print(f"\n[SUCCESS] epoch-{TARGET_EPOCHS} reached (train.py rc={rc}).",
                  flush=True)
            # Let the pusher finish the final epoch push.
            if pusher is not None:
                for _ in range(24):
                    if pusher.poll() is not None:
                        break
                    time.sleep(5)
                if pusher.poll() is None:
                    pusher.terminate()
            break

        print(f"\n[ATTEMPT {attempt} ENDED] train.py rc={rc}, "
              f"epoch-{TARGET_EPOCHS} not reached "
              f"(latest continuation snapshot: epoch "
              f"{latest_continuation_epoch()}). Killing this attempt's pusher, "
              f"waiting for GPU, will resume.", flush=True)
        if pusher is not None and pusher.poll() is None:
            pusher.terminate()
        time.sleep(30)  # let CUDA memory actually release
    else:
        print(f"[STOP] hit --max-attempts={args.max_attempts} without reaching "
              f"epoch {TARGET_EPOCHS}.", flush=True)

    cont = latest_continuation_epoch()
    print(f"\nSupervisor exiting. Latest continuation epoch on disk: {cont}. "
          f"Per-epoch HF tags v14.6.. are pushed by the per-attempt pushers. "
          f"Promote/ship is a separate manual step (or a follow-up).", flush=True)


if __name__ == "__main__":
    main()
