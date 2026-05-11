"""Local 12-hour training-cycle loop for Loka.

Runs forever (until Ctrl-C or --max-cycles is hit). Each cycle:

    1. git pull --rebase
    2. Determine the next version number (v8, v9, ...)
    3. If the version's checkpoint already exists locally: just ship it
       (run propgen test, write DEVLOG entry, push to HF, commit, push to repo)
    4. Otherwise: optionally fetch more data from Hugging Face (capped),
       preprocess, train from scratch, then ship.
    5. Sleep --interval (default 12 h) and repeat.

Designed to coexist with ad-hoc training runs you might kick off by hand:
the script never starts a new training pass while a checkpoint file is
still being written (file size growing), and it picks up the next free
version slot rather than overwriting.

Disk safety:
- HF import is capped at --max-triples-per-cycle (default 2,000,000).
- Per-cycle disk-free check; aborts the cycle if free space is below
  --min-free-gb (default 10 GB) before HF import or training starts.

HF auth: run `huggingface-cli login` once with a write token before starting
this loop. The script does not handle interactive auth.

Usage:
    python tools/training_cron.py

    # Custom interval and cap on cycles:
    python tools/training_cron.py --interval 21600 --max-cycles 5

    # Skip the HF data refresh — just retrain on the existing v7 corpus:
    python tools/training_cron.py --no-data-refresh

    # Stop after the first cycle (debug / dry-run flavour):
    python tools/training_cron.py --max-cycles 1
"""
from __future__ import annotations

import argparse
import io
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

REPO_ROOT = Path(__file__).resolve().parent.parent
CHECKPOINTS = REPO_ROOT / "training" / "checkpoints"
LOGS = REPO_ROOT / "training" / "logs"
DATA = REPO_ROOT / "training" / "data"
DEVLOG = REPO_ROOT / "DEVLOG.md"
PAPER = REPO_ROOT / "paper" / "paper.md"
MODEL_JSON = REPO_ROOT / "MODEL.json"


def now_utc_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def now_local_iso() -> str:
    return datetime.now().astimezone().strftime("%Y-%m-%d %H:%M %Z")


def log(msg: str) -> None:
    print(f"[{now_local_iso()}] {msg}", flush=True)


def run(cmd: list[str], cwd: Path | None = None, check: bool = True,
        capture: bool = False) -> subprocess.CompletedProcess:
    """Run a subprocess with sensible defaults. Streams stdout/stderr."""
    log(f"$ {' '.join(cmd)}")
    if capture:
        return subprocess.run(cmd, cwd=cwd or REPO_ROOT, check=check,
                              capture_output=True, text=True, encoding="utf-8")
    return subprocess.run(cmd, cwd=cwd or REPO_ROOT, check=check)


def file_is_stable(path: Path, settle_seconds: int = 10) -> bool:
    """True if `path` exists AND its size hasn't changed in `settle_seconds`."""
    if not path.exists():
        return True  # not being written; safe to start fresh
    s1 = path.stat().st_size
    time.sleep(settle_seconds)
    s2 = path.stat().st_size
    return s1 == s2


def free_gb(path: Path) -> float:
    return shutil.disk_usage(path).free / (1024 ** 3)


def latest_checkpoint_version() -> int:
    """Highest N in training/checkpoints/wikidata_vN.pt. Returns 0 if none."""
    if not CHECKPOINTS.exists():
        return 0
    versions: list[int] = []
    for p in CHECKPOINTS.glob("wikidata_v*.pt"):
        try:
            v = int(p.stem.replace("wikidata_v", ""))
            versions.append(v)
        except ValueError:
            continue
    return max(versions) if versions else 0


def loka_serving(port: int) -> bool:
    """True if a Loka instance responds on the given port."""
    try:
        import requests
        return requests.get(f"http://localhost:{port}/health", timeout=2).status_code == 200
    except Exception:
        return False


def start_loka(port: int, data_dir: Path) -> subprocess.Popen:
    """Start `loka serve` in the background. Returns the Popen handle."""
    binary = REPO_ROOT / "target" / "release" / "loka.exe"
    if not binary.exists():
        binary = REPO_ROOT / "target" / "debug" / "loka.exe"
    if not binary.exists():
        raise FileNotFoundError(
            "loka binary not found. Run `cargo build --release -p loka-cli` first."
        )
    data_dir.mkdir(parents=True, exist_ok=True)
    log(f"Starting Loka: {binary} serve --port {port} --data-dir {data_dir}")
    return subprocess.Popen(
        [str(binary), "serve", "--port", str(port), "--data-dir", str(data_dir)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def hf_import(port: int, max_triples: int) -> None:
    """Stream from the philippesaade/wikidata HF dataset into the running Loka.

    Each cron cycle uses a fresh Loka instance with an empty data dir, so the
    persistent `wikidata_import_state.json` from prior runs (which records
    "5M triples already inserted into a now-vanished Loka") would cause the
    importer to short-circuit. Clear it for the duration of the cycle and
    restore it afterward.
    """
    log(f"HF import (max {max_triples:,} triples) into Loka on :{port}")
    # The HF import uses `wikidata_hf_import_state.json` (note `_hf_`), not
    # the legacy `wikidata_import_state.json` from the BFS importer.
    state_path = REPO_ROOT / "wikidata_hf_import_state.json"
    backup_path = REPO_ROOT / "wikidata_hf_import_state.json.cron-backup"
    if state_path.exists():
        log(f"Stashing existing state file to {backup_path.name}")
        state_path.rename(backup_path)
    env = os.environ.copy()
    env["LOKA_ENDPOINT"] = f"http://localhost:{port}"
    try:
        subprocess.run(
            [sys.executable, "tools/wikidata_hf_import.py",
             "--max-triples", str(max_triples)],
            cwd=REPO_ROOT, env=env, check=True,
        )
    finally:
        # Replace the cycle's state file with the original (or delete it if
        # there wasn't one). The cron's per-cycle state is uninteresting once
        # this cycle is done.
        cycle_state = REPO_ROOT / "wikidata_import_state.json"
        if cycle_state.exists():
            cycle_state.unlink()
        if backup_path.exists():
            backup_path.rename(state_path)
            log("Restored original state file.")


def preprocess_to_corpus(port: int, output: Path) -> None:
    log(f"Building training corpus -> {output}")
    run([sys.executable, "training/preprocess.py",
         "--endpoint", f"http://localhost:{port}",
         "--output", str(output)])


def train_version(version: int, corpus: Path, epochs: int) -> Path:
    ckpt = CHECKPOINTS / f"wikidata_v{version}.pt"
    log_path = LOGS / f"v{version}_train.log"
    LOGS.mkdir(parents=True, exist_ok=True)
    log(f"Training v{version} ({epochs} epochs) -> {ckpt}")
    with log_path.open("w", encoding="utf-8") as logfile:
        subprocess.run(
            [sys.executable, "training/train.py",
             "--data", str(corpus),
             "--vocab", "training/data/vocab_bpe.json",
             "--bpe-tokenizer", "training/data/tokenizer_bpe.json",
             "--checkpoint", str(ckpt),
             "--epochs", str(epochs),
             "--batch-size", "64",
             "--tokens-per-role", "8",
             "--d-model", "512",
             "--nhead", "8",
             "--layers", "6"],
            cwd=REPO_ROOT, stdout=logfile, stderr=subprocess.STDOUT, check=True,
        )
    return ckpt


def ensure_seed_pagerank(seed: Path) -> Path:
    """Compute (or refresh) PageRank scores over the propgen seed file.

    The propgen test uses --rank-file to bias source-triple selection toward
    structurally-important entities (see planning/pagerank-bootstrapping.md).
    Recompute whenever the rank file is missing or older than the seed —
    cheap (<1 s on a 14 k-triple seed) and self-healing if the seed gets
    re-pulled.
    """
    rank_path = seed.with_name(seed.stem.replace("seed_", "pagerank_") + ".json")
    if rank_path.exists() and rank_path.stat().st_mtime >= seed.stat().st_mtime:
        log(f"PageRank cache fresh: {rank_path.name}")
        return rank_path
    log(f"Computing PageRank over {seed.name} -> {rank_path.name}")
    run([sys.executable, "tools/compute_pagerank.py",
         "--nt-file", str(seed),
         "--output", str(rank_path)])
    return rank_path


def run_propgen_test(version: int) -> Path:
    """Run the auto-regressive propgen test against the new checkpoint."""
    ckpt = CHECKPOINTS / f"wikidata_v{version}.pt"
    seed = DATA / "seed_Q42.nt"
    if not seed.exists():
        log("Q42 seed missing — pulling fresh.")
        run([sys.executable, "tools/wikidata_random_seed.py",
             "--seed", "Q42", "--max-entities", "30",
             "--max-time", "120", "--max-depth", "2",
             "--output-dir", str(DATA)])
    rank_file = ensure_seed_pagerank(seed)
    output = DATA / f"test_propgen_Q42_v{version}.nt"
    run([sys.executable, "training/test_autoregressive_propgen.py",
         "--seed-file", str(seed),
         "--output", str(output),
         "--checkpoint", str(ckpt),
         "--bpe-tokenizer", "training/data/tokenizer_bpe.json",
         "--rank-file", str(rank_file),
         "--max-source-triples", "30",
         "--children-per-source", "10",
         "--confidence", "0.25",
         "--model-version", f"loka-wikidata-v{version}"])
    return output


def parse_train_log(log_path: Path) -> tuple[float, list[tuple[int, float, float]]]:
    """Return (final_ppl, [(epoch, loss, ppl), ...]) parsed from the train log."""
    rows: list[tuple[int, float, float]] = []
    if not log_path.exists():
        return float("nan"), rows
    for line in log_path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("epoch"):
            continue
        try:
            parts = line.split()
            ep = int(parts[1].split("/")[0])
            loss = float(parts[3])
            ppl = float(parts[5])
            rows.append((ep, loss, ppl))
        except (IndexError, ValueError):
            continue
    final_ppl = rows[-1][2] if rows else float("nan")
    return final_ppl, rows


def write_devlog_entry(version: int, train_log: Path, propgen_output: Path) -> None:
    final_ppl, rows = parse_train_log(train_log)
    rows_md = "\n".join(
        f"| {ep} | {loss:.4f} | {ppl:.2f} |" for ep, loss, ppl in rows
    )
    entry = f"""## {datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")} — v{version} trained (cron cycle)

Trained by `tools/training_cron.py` on the local 4070. Same 44.5 M-parameter
BPE architecture as v6/v7/v8.

| Epoch | Loss | Perplexity |
|---|---|---|
{rows_md}

**Final perplexity: {final_ppl:.2f}**

Auto-regressive propgen test output (Q42 seed, 30 sources, conf 0.25):
`{propgen_output.relative_to(REPO_ROOT).as_posix()}`. See the test script's
generated `_meta.json` for per-source breakdown and the asymmetric-drop
companion file for the highly-cardinal predicates filtered out of context.

Checkpoint at `training/checkpoints/wikidata_v{version}.pt`. Pushed to
Hugging Face as `EmmaLeonhart/loka@v{version}`. `MODEL.json` bumped to
pin v{version} as the default for fresh-clone inference.

---

"""
    existing = DEVLOG.read_text(encoding="utf-8")
    # Insert below the title block (after the first `---` separator).
    sentinel = "---\n\n## "
    idx = existing.find(sentinel)
    if idx == -1:
        new = existing + "\n\n" + entry
    else:
        head = existing[: idx + 4]  # keep "---\n\n"
        tail = existing[idx + 4:]
        new = head + entry + tail
    DEVLOG.write_text(new, encoding="utf-8")
    log(f"DEVLOG entry written for v{version} (final ppl {final_ppl:.2f})")


def update_model_json(version: int, revision: str | None = None) -> None:
    import json
    if not MODEL_JSON.exists():
        log("MODEL.json missing; skipping pin update")
        return
    data = json.loads(MODEL_JSON.read_text(encoding="utf-8"))
    data["name"] = f"loka-wikidata-v{version}"
    data["version"] = f"v{version}"
    data["released"] = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    data["revision"] = revision or f"v{version}"
    if "files" in data and "checkpoint" in data["files"]:
        data["files"]["checkpoint"]["in_repo"] = f"checkpoints/wikidata_v{version}.pt"
        data["files"]["checkpoint"]["local"] = (
            f"training/checkpoints/wikidata_v{version}.pt"
        )
    MODEL_JSON.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
    log(f"MODEL.json pinned to v{version}")


def push_to_hf(version: int) -> None:
    log(f"Pushing v{version} snapshot to Hugging Face")
    run([sys.executable, "tools/hf_snapshot.py",
         "--user", "EmmaLeonhart",
         "--snapshot-name", f"v{version}",
         "--no-loka-data"])


def git_commit_and_push(version: int) -> None:
    run(["git", "add", "DEVLOG.md", "MODEL.json", "paper/paper.md",
         f"training/logs/v{version}_train.log"], check=False)
    msg = f"v{version} cron cycle: trained, tested, shipped to HF\n\nGenerated by tools/training_cron.py."
    run(["git", "commit", "-m", msg], check=False)
    run(["git", "push", "origin", "main"], check=False)


def ship(version: int) -> None:
    """Run propgen test, write DEVLOG, update MODEL.json, push to HF, commit+push."""
    train_log = LOGS / f"v{version}_train.log"
    propgen_output = run_propgen_test(version)
    write_devlog_entry(version, train_log, propgen_output)
    update_model_json(version)
    push_to_hf(version)
    git_commit_and_push(version)
    log(f"v{version} shipped successfully.")


def cycle(args, cycle_idx: int) -> None:
    log(f"=== CYCLE {cycle_idx} START ===")
    run(["git", "pull", "--rebase", "origin", "main"], check=False)

    # If the latest checkpoint hasn't been shipped (no DEVLOG entry mentioning
    # it), ship it before training the next one.
    latest = latest_checkpoint_version()
    devlog_text = DEVLOG.read_text(encoding="utf-8") if DEVLOG.exists() else ""
    if latest > 0 and f"v{latest} trained" not in devlog_text and \
       f"v{latest}" not in devlog_text[:5000]:  # latest entry zone
        log(f"Latest checkpoint v{latest} appears unshipped — shipping first.")
        ckpt = CHECKPOINTS / f"wikidata_v{latest}.pt"
        if not file_is_stable(ckpt, settle_seconds=15):
            log(f"v{latest} checkpoint still being written; skipping ship this cycle.")
            return
        ship(latest)
        latest = latest_checkpoint_version()

    # Free-disk gate.
    free = free_gb(REPO_ROOT)
    log(f"Free disk: {free:.1f} GB (min {args.min_free_gb} GB required)")
    if free < args.min_free_gb:
        log("Aborting cycle: insufficient disk space.")
        return

    next_version = latest + 1
    log(f"Training v{next_version} next.")

    if args.no_data_refresh:
        corpus = DATA / "triples_v7.txt"
        if not corpus.exists():
            log("--no-data-refresh set but training/data/triples_v7.txt missing; aborting cycle.")
            return
        log(f"Reusing existing corpus: {corpus}")
    else:
        # Fresh data import into a per-cycle Loka instance.
        port = 3030 + cycle_idx  # avoid colliding with existing server
        data_dir = REPO_ROOT / f"loka-data-cron-c{cycle_idx}"
        loka_proc = start_loka(port, data_dir)
        try:
            # Wait for Loka to come up.
            for _ in range(30):
                if loka_serving(port):
                    break
                time.sleep(1)
            else:
                log("Loka failed to start within 30s; aborting cycle.")
                return
            hf_import(port, args.max_triples_per_cycle)
            corpus = DATA / f"triples_v{next_version}.txt"
            preprocess_to_corpus(port, corpus)
        finally:
            log("Stopping Loka.")
            loka_proc.terminate()
            try:
                loka_proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                loka_proc.kill()

    train_version(next_version, corpus, args.epochs)
    ship(next_version)
    log(f"=== CYCLE {cycle_idx} DONE ===")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--interval", type=int, default=12 * 3600,
                        help="Seconds between cycles (default 43200 = 12 hours)")
    parser.add_argument("--max-cycles", type=int, default=0,
                        help="Stop after this many cycles. 0 = unlimited.")
    parser.add_argument("--epochs", type=int, default=20,
                        help="Epochs per training run (default 20)")
    parser.add_argument("--max-triples-per-cycle", type=int, default=2_000_000,
                        help="HF import cap per cycle (default 2,000,000)")
    parser.add_argument("--min-free-gb", type=float, default=10.0,
                        help="Skip cycle if free disk is below this (default 10 GB)")
    parser.add_argument("--no-data-refresh", action="store_true",
                        help="Skip the HF data import; reuse the v7 corpus")
    parser.add_argument("--first-delay", type=int, default=0,
                        help="Seconds to wait before the first cycle (default 0)")
    args = parser.parse_args()

    log(f"training_cron starting. interval={args.interval}s "
        f"max_cycles={args.max_cycles or 'unlimited'} epochs={args.epochs}")
    if args.first_delay:
        log(f"Initial delay {args.first_delay}s.")
        time.sleep(args.first_delay)

    cycle_idx = 0
    while True:
        cycle_idx += 1
        try:
            cycle(args, cycle_idx)
        except KeyboardInterrupt:
            log("Interrupted by user. Exiting.")
            return
        except Exception as e:
            log(f"!! Cycle {cycle_idx} failed: {type(e).__name__}: {e}")
            log("Continuing to next cycle.")
        if args.max_cycles and cycle_idx >= args.max_cycles:
            log(f"Reached max_cycles={args.max_cycles}. Exiting.")
            return
        log(f"Sleeping {args.interval}s until next cycle.")
        time.sleep(args.interval)


if __name__ == "__main__":
    main()
