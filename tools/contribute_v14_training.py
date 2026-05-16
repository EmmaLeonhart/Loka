"""Train Loka v14 on a donor GPU.

The project maintainer (EmmaLeonhart) develops Loka on a laptop with an
RTX 4070 Laptop (8 GB VRAM). Training rungs through v11 (350 k triples),
v12 (672 k), and v13 (2.5 M) fit in that envelope at `--batch-size 16`,
but v14 (4.0 M triples, ~10 epochs × ~4 h each = ~40 h) is impractical on
that hardware. If you have access to a GPU with at least 8 GB VRAM and
~2 days of wall-clock to donate, this script makes the whole training
loop one command.

What it does:
  1. Authenticates with Hugging Face (uses your token, NOT Emma's).
  2. Pulls the v14-1M training corpus from
     EmmaLeonhart/normalized-wikidata (CC-BY-SA 4.0).
  3. Pulls the BPE tokenizer + vocab from EmmaLeonhart/loka@v14
     (byte-identical across all tags — stable since v6 — but pinned
     to the canonical revision for clarity).
  4. Trains 10 epochs (default) of the 44.5 M-parameter role-aware
     transformer at batch size 16, same architecture as v3 onward.
  5. After every epoch, pushes the checkpoint to YOUR Hugging Face repo
     `<your-user>/loka-v14-contribution` tagged `v14.N` for that epoch.
     If a later epoch crashes, every prior epoch is already preserved.
  6. Prints a GitHub-issue template at the end so Emma can find the
     result and mirror it to EmmaLeonhart/loka@v14.

Before you run this, please raise a GitHub issue at
https://github.com/EmmaLeonhart/Loka/issues so the project knows
someone is doing this and the work doesn't get duplicated. After it
finishes, comment on that issue with a link to your HF repo.

Usage:
    huggingface-cli login   # paste your token at the prompt
    python tools/contribute_v14_training.py --hf-user YOUR_USERNAME

Optional flags:
    --epochs 10           — change epoch count
    --batch-size 16       — change batch size (16 fits 8 GB VRAM)
    --hf-repo-name loka-v14-contribution — change the repo name on your HF account
    --skip-corpus-download — reuse a local corpus file you already have
    --dry-run             — print what would happen, then exit
"""
from __future__ import annotations

import argparse
import io
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

PROJECT_ROOT = Path(__file__).resolve().parent.parent
LOCAL_CORPUS = PROJECT_ROOT / "training" / "data" / "triples_v14.txt"
LOCAL_VOCAB = PROJECT_ROOT / "training" / "data" / "vocab_bpe.json"
LOCAL_TOKENIZER = PROJECT_ROOT / "training" / "data" / "tokenizer_bpe.json"
LOCAL_CHECKPOINT = PROJECT_ROOT / "training" / "checkpoints" / "wikidata_v14.pt"
TRAIN_LOG = PROJECT_ROOT / "training" / "logs" / "v14_contribute_train.log"
PUSHER_LOG = PROJECT_ROOT / "training" / "logs" / "v14_contribute_pusher.log"


GITHUB_ISSUE_TEMPLATE = """\
Hi! I'm contributing GPU time to train Loka v14. Going to run
`tools/contribute_v14_training.py --hf-user {hf_user}` on my hardware.

System:
- GPU: <YOUR_GPU>
- VRAM: <X GB>
- Estimated wall-clock: ~{wall_estimate}

Will update this issue with the HF link to `{hf_user}/{hf_repo_name}`
when the run is in progress / complete.
"""


def banner(msg: str) -> None:
    print(f"\n{'=' * 70}\n{msg}\n{'=' * 70}", flush=True)


def check_hf_auth() -> str | None:
    """Return the HF username we're authenticated as, or None."""
    try:
        from huggingface_hub import whoami
        info = whoami()
        return info.get("name")
    except Exception as e:
        print(f"[ERROR] HF auth check failed: {e}", file=sys.stderr)
        return None


def download_corpus_and_tokenizer(skip_corpus: bool) -> None:
    from huggingface_hub import hf_hub_download

    LOCAL_CORPUS.parent.mkdir(parents=True, exist_ok=True)
    LOCAL_CHECKPOINT.parent.mkdir(parents=True, exist_ok=True)

    if skip_corpus and LOCAL_CORPUS.exists():
        print(f"  reusing existing {LOCAL_CORPUS}", flush=True)
    else:
        banner("Downloading v14-1M corpus from EmmaLeonhart/normalized-wikidata")
        path = hf_hub_download(
            repo_id="EmmaLeonhart/normalized-wikidata",
            repo_type="dataset",
            filename="triples_normalized.txt",
            revision="v14-1M",
        )
        shutil.copy(path, LOCAL_CORPUS)
        size_mb = LOCAL_CORPUS.stat().st_size / 1_000_000
        print(f"  {LOCAL_CORPUS} ({size_mb:.1f} MB)", flush=True)

    banner("Downloading BPE tokenizer + vocab from EmmaLeonhart/loka@v14")
    for repo_file, local_file in [
        ("corpus/vocab_bpe.json", LOCAL_VOCAB),
        ("corpus/tokenizer_bpe.json", LOCAL_TOKENIZER),
    ]:
        if local_file.exists():
            print(f"  reusing {local_file}", flush=True)
            continue
        path = hf_hub_download(
            repo_id="EmmaLeonhart/loka",
            repo_type="dataset",
            filename=repo_file,
            revision="v14",
        )
        shutil.copy(path, local_file)
        print(f"  {local_file} ({local_file.stat().st_size:,} bytes)", flush=True)


def ensure_repo(hf_user: str, hf_repo_name: str) -> str:
    from huggingface_hub import HfApi
    from huggingface_hub.utils import HfHubHTTPError

    repo_id = f"{hf_user}/{hf_repo_name}"
    api = HfApi()
    try:
        api.repo_info(repo_id=repo_id, repo_type="dataset")
        print(f"[OK] HF repo exists: {repo_id}", flush=True)
    except HfHubHTTPError:
        print(f"[NEW] creating HF repo {repo_id}", flush=True)
        api.create_repo(repo_id=repo_id, repo_type="dataset", exist_ok=True)
    return repo_id


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hf-user", required=True,
                        help="YOUR Hugging Face username. The script pushes to <hf-user>/<hf-repo-name>.")
    parser.add_argument("--hf-repo-name", default="loka-v14-contribution",
                        help="Repo name on YOUR HF account where epoch snapshots land (default: loka-v14-contribution).")
    parser.add_argument("--epochs", type=int, default=10)
    parser.add_argument("--batch-size", type=int, default=16,
                        help="Batch size. 16 fits 8 GB VRAM at the 44.5 M-param scale; larger if you have more VRAM.")
    parser.add_argument("--skip-corpus-download", action="store_true",
                        help="Reuse training/data/triples_v14.txt if it exists.")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    banner("Loka v14 contributor training script")
    print("This will train Loka v14 on YOUR GPU, push every epoch to YOUR HF account.", flush=True)
    print(f"  Your HF account:  {args.hf_user}", flush=True)
    print(f"  Push to:          {args.hf_user}/{args.hf_repo_name}", flush=True)
    print(f"  Tag format:       v14.1, v14.2, ..., v14.{args.epochs}", flush=True)
    print(f"  Corpus:           EmmaLeonhart/normalized-wikidata@v14-1M (4,021,409 triples)", flush=True)
    print(f"  Architecture:     44.5 M params, batch {args.batch_size}, {args.epochs} epochs", flush=True)

    banner("Step 1: verify Hugging Face auth")
    me = check_hf_auth()
    if me is None:
        print("[ERROR] Not logged in. Run `huggingface-cli login` first, then re-run this.", file=sys.stderr)
        sys.exit(2)
    if me != args.hf_user:
        print(f"[WARN] You're logged in as {me!r} but --hf-user is {args.hf_user!r}. "
              f"Continuing — the push will go to {args.hf_user}/{args.hf_repo_name}.",
              flush=True)
    else:
        print(f"[OK] Logged in as {me}.", flush=True)

    if args.dry_run:
        wall = f"{args.epochs * 4} h on RTX 4090 / {args.epochs * 8} h on RTX 4070 Laptop"
        print("\n[DRY RUN] Would do:", flush=True)
        print("  1. Download corpus + tokenizer (~180 MB once)", flush=True)
        print(f"  2. Create/use HF repo {args.hf_user}/{args.hf_repo_name}", flush=True)
        print(f"  3. Run train.py for {args.epochs} epochs, batch {args.batch_size}", flush=True)
        print(f"  4. Push each epoch to HF as tag v14.N", flush=True)
        print(f"  Estimated wall: {wall}", flush=True)
        print("\n--- GitHub issue template ---", flush=True)
        print(GITHUB_ISSUE_TEMPLATE.format(
            hf_user=args.hf_user, hf_repo_name=args.hf_repo_name, wall_estimate=wall,
        ), flush=True)
        return

    banner("Step 2: download corpus + tokenizer")
    download_corpus_and_tokenizer(skip_corpus=args.skip_corpus_download)

    banner("Step 3: ensure your HF repo exists")
    repo_id = ensure_repo(args.hf_user, args.hf_repo_name)

    banner("Step 4: kick off training + epoch pusher")
    TRAIN_LOG.parent.mkdir(parents=True, exist_ok=True)

    train_cmd = [
        sys.executable, "-u", str(PROJECT_ROOT / "training" / "train.py"),
        "--data", str(LOCAL_CORPUS),
        "--vocab", str(LOCAL_VOCAB),
        "--bpe-tokenizer", str(LOCAL_TOKENIZER),
        "--checkpoint", str(LOCAL_CHECKPOINT),
        "--epochs", str(args.epochs),
        "--batch-size", str(args.batch_size),
        "--tokens-per-role", "8",
        "--d-model", "512",
        "--nhead", "8",
        "--layers", "6",
    ]
    pusher_cmd = [
        sys.executable, "-u", str(PROJECT_ROOT / "tools" / "epoch_snapshot_pusher.py"),
        "--log", str(TRAIN_LOG),
        "--checkpoint", str(LOCAL_CHECKPOINT),
        "--model", "14",
        "--hf-user", args.hf_user,
        "--hf-repo", repo_id,
        "--idle-timeout-seconds", "21600",  # 6h fallback
    ]

    print(f"Train log:   {TRAIN_LOG}", flush=True)
    print(f"Pusher log:  {PUSHER_LOG}", flush=True)
    print(f"\n  $ {' '.join(train_cmd)}", flush=True)
    print(f"\n  $ {' '.join(pusher_cmd)}", flush=True)

    # Start the pusher first (idempotent if no training output yet) so it
    # catches the very first epoch line.
    pusher = subprocess.Popen(
        pusher_cmd,
        stdout=PUSHER_LOG.open("w", encoding="utf-8"),
        stderr=subprocess.STDOUT,
    )
    trainer = subprocess.Popen(
        train_cmd,
        stdout=TRAIN_LOG.open("w", encoding="utf-8"),
        stderr=subprocess.STDOUT,
    )

    banner("Training started — tail the logs to watch progress")
    print(f"  tail -F {TRAIN_LOG}", flush=True)
    print(f"  tail -F {PUSHER_LOG}", flush=True)
    print("\nWaiting for training to finish (Ctrl-C kills both processes)...", flush=True)

    try:
        trainer.wait()
    except KeyboardInterrupt:
        print("\n[INT] killing training + pusher...", flush=True)
        trainer.terminate()
        pusher.terminate()
        sys.exit(1)

    # Give the pusher 60 s to push the final epoch's snapshot.
    print("\nTraining done. Waiting up to 60 s for the final-epoch push...", flush=True)
    for _ in range(12):
        if pusher.poll() is not None:
            break
        time.sleep(5)
    if pusher.poll() is None:
        pusher.terminate()

    banner("DONE — please comment on the GitHub issue with your HF repo link")
    wall = "see logs"
    print(GITHUB_ISSUE_TEMPLATE.format(
        hf_user=args.hf_user, hf_repo_name=args.hf_repo_name, wall_estimate=wall,
    ), flush=True)
    print(f"\nYour result is at: https://huggingface.co/datasets/{repo_id}", flush=True)
    print(f"  Tags: v14.1 through v14.{args.epochs}", flush=True)
    print(f"\nPlease open / comment on a GitHub issue at:", flush=True)
    print(f"  https://github.com/EmmaLeonhart/Loka/issues", flush=True)
    print(f"so Emma can mirror your result to EmmaLeonhart/loka@v14. Thank you!", flush=True)


if __name__ == "__main__":
    main()
