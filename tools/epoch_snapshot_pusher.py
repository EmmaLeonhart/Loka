"""Watch a training log; on each per-epoch line, snapshot + push to HF.

train.py saves a checkpoint after every epoch by overwriting the same .pt
file. That means if a later epoch diverges (the v12 disaster) or the run
is killed externally, all earlier epochs are lost. This watcher fixes
that: as soon as a new "epoch N/M loss ... ppl ..." line appears in the
log, copy the current .pt to a versioned path and push it to HF tagged
`v{model}.{epoch}` so each epoch is preserved both locally and remotely.

Doesn't touch the training process; pure log tail + filesystem copy +
network upload. Safe to run alongside training.

Usage:
    python tools/epoch_snapshot_pusher.py \\
        --log training/logs/v13_train.log \\
        --checkpoint training/checkpoints/wikidata_v13.pt \\
        --model 13 \\
        --hf-user EmmaLeonhart \\
        --hf-repo EmmaLeonhart/loka

Stops when the training log indicates final save ("Saved checkpoint to")
OR when the log file hasn't grown for --idle-timeout-seconds (default 1
hour — covers epoch-1 warmup plus a margin).
"""
from __future__ import annotations

import argparse
import io
import re
import shutil
import sys
import time
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

EPOCH_RE = re.compile(
    r"^epoch\s+(?P<epoch>\d+)/(?P<total>\d+)\s+loss\s+(?P<loss>[0-9.]+)\s+ppl\s+(?P<ppl>[0-9.]+)"
)
FINAL_RE = re.compile(r"Saved checkpoint to")


def push_to_hf(api, repo_id: str, local_path: Path, repo_path: str, tag: str) -> None:
    print(f"  [HF] uploading {local_path.name} -> {repo_id}:{repo_path}", flush=True)
    api.upload_file(
        path_or_fileobj=str(local_path),
        path_in_repo=repo_path,
        repo_id=repo_id,
        repo_type="dataset",
        commit_message=f"epoch snapshot: {tag}",
    )
    print(f"  [HF] tagging {tag}", flush=True)
    api.create_tag(
        repo_id=repo_id,
        repo_type="dataset",
        tag=tag,
        exist_ok=True,
    )
    print(f"  [HF] done -> https://huggingface.co/datasets/{repo_id}/tree/{tag}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", required=True, help="Path to the training log to watch.")
    parser.add_argument("--checkpoint", required=True, help="Path to the .pt file train.py overwrites each epoch.")
    parser.add_argument("--model", required=True, help="Model version number (e.g. 13 for v13).")
    parser.add_argument("--hf-user", required=True, help="HF user/org owning the repo.")
    parser.add_argument("--hf-repo", default="EmmaLeonhart/loka", help="Full repo id (default EmmaLeonhart/loka).")
    parser.add_argument("--poll-seconds", type=float, default=10.0)
    parser.add_argument("--idle-timeout-seconds", type=float, default=3600.0,
                        help="Exit if the log hasn't grown in this many seconds. Default 1 h.")
    parser.add_argument("--no-hf", action="store_true",
                        help="Only do local snapshots; skip HF upload (for testing).")
    args = parser.parse_args()

    log_path = Path(args.log)
    ckpt_path = Path(args.checkpoint)
    model = args.model

    if not args.no_hf:
        from huggingface_hub import HfApi
        api = HfApi()
    else:
        api = None

    print(f"Watching {log_path} for epoch completions on model v{model}.", flush=True)
    print(f"Snapshotting {ckpt_path} on each epoch.", flush=True)
    if not args.no_hf:
        print(f"Pushing to {args.hf_repo} as tags v{model}.N", flush=True)

    seen_epochs: set[int] = set()
    last_size = 0
    last_growth = time.time()

    # Read existing log from the start (in case training already produced
    # some epoch lines before we started).
    pos = 0
    while True:
        if not log_path.exists():
            time.sleep(args.poll_seconds)
            continue

        size = log_path.stat().st_size
        if size > last_size:
            last_growth = time.time()
            last_size = size

        with log_path.open("r", encoding="utf-8", errors="replace") as f:
            f.seek(pos)
            for line in f:
                # Final-save line means training is done.
                if FINAL_RE.search(line):
                    print(f"[DONE] training emitted final-save line; exiting watcher.", flush=True)
                    return
                m = EPOCH_RE.match(line)
                if not m:
                    continue
                epoch = int(m.group("epoch"))
                if epoch in seen_epochs:
                    continue
                seen_epochs.add(epoch)
                loss = m.group("loss")
                ppl = m.group("ppl")
                print(f"[EPOCH {epoch}] loss {loss} ppl {ppl} — snapshotting", flush=True)

                # Snapshot the .pt locally with the epoch suffix.
                snap = ckpt_path.with_name(f"{ckpt_path.stem}_epoch{epoch:02d}.pt")
                if not ckpt_path.exists():
                    print(f"  [WARN] checkpoint {ckpt_path} not yet on disk; skipping snapshot of epoch {epoch}", flush=True)
                    continue
                shutil.copy2(ckpt_path, snap)
                size_mb = snap.stat().st_size / 1_000_000
                print(f"  local snapshot: {snap} ({size_mb:.1f} MB)", flush=True)

                if api is not None:
                    tag = f"v{model}.{epoch}"
                    try:
                        push_to_hf(api, args.hf_repo, snap,
                                   f"checkpoints/{snap.name}", tag)
                    except Exception as e:
                        print(f"  [WARN] HF push failed for epoch {epoch}: {e!r}", flush=True)

            pos = f.tell()

        if time.time() - last_growth > args.idle_timeout_seconds:
            print(f"[TIMEOUT] log idle for {args.idle_timeout_seconds:.0f}s; exiting.", flush=True)
            return

        time.sleep(args.poll_seconds)


if __name__ == "__main__":
    main()
