"""One-shot v11 training cycle kickoff. Fired by Windows Task Scheduler at
noon PT 2026-05-13 against the running Loka holding the bigger corpus from
the 2026-05-12 / 2026-05-13 ingest.

What this does (in order):

1. **Stop the HF ingest if it's still running.** Sends Ctrl+C-equivalent to
   any `python ... wikidata_hf_import.py` process. The importer checkpoints
   every --save-every rows, so a graceful stop here loses at most one
   checkpoint window of writes. If no ingest is running, this is a no-op.

2. **Verify Loka is reachable and has enough data.** Probes /health and the
   triple count. Aborts the cycle if Loka isn't up or has fewer than
   `--min-triples` triples (default 33M — anything past the 32.88M panic
   point means the bigger-corpus effort produced something useful).

3. **Preprocess** with `training/preprocess.py --endpoint http://localhost:3030
   --output training/data/triples_v11.txt`. Paged-SPARQL fetch from the
   running Loka.

4. **Train** v11 with `training/train.py --epochs 4` (the user's noon→4pm
   window dictates 4 epochs; see DEVLOG 2026-05-13 entry).

5. **Ship**: propgen test, DEVLOG entry, MODEL.json bump, HF push,
   git add + commit + push. All via training_cron.py's existing helpers
   (this script imports them, no duplication).

Run manually for testing:
    python tools/v11_kickoff.py --dry-run         # log what would happen
    python tools/v11_kickoff.py                   # do it

The Windows scheduled task invocation is:
    python C:\\Users\\Immanuelle\\Documents\\Github\\SutraDB\\tools\\v11_kickoff.py
"""
from __future__ import annotations

import argparse
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "tools"))

# Re-use training_cron's helpers — no duplication of the ship/test/push logic.
import training_cron as tc  # noqa: E402

LOG_PATH = REPO_ROOT / "training" / "logs" / "v11_kickoff.log"


def log(msg: str) -> None:
    line = f"[{tc.now_local_iso()}] {msg}"
    print(line, flush=True)
    LOG_PATH.parent.mkdir(parents=True, exist_ok=True)
    with LOG_PATH.open("a", encoding="utf-8") as f:
        f.write(line + "\n")


def find_ingest_pids() -> list[int]:
    """Return PIDs of running python processes whose command line includes
    `wikidata_hf_import.py`. Uses tasklist + wmic for Windows; psutil-free
    so this script has no third-party deps beyond what training_cron uses.
    """
    try:
        out = subprocess.run(
            ["wmic", "process", "where",
             "name='python.exe' or name='python3.exe'",
             "get", "ProcessId,CommandLine", "/format:csv"],
            capture_output=True, text=True, timeout=30,
        ).stdout
    except Exception as e:
        log(f"wmic query failed: {e}")
        return []
    pids: list[int] = []
    for line in out.splitlines():
        if "wikidata_hf_import" not in line:
            continue
        # CSV format: Node,CommandLine,ProcessId
        parts = line.rsplit(",", 1)
        if len(parts) != 2:
            continue
        try:
            pids.append(int(parts[1].strip()))
        except ValueError:
            continue
    return pids


def stop_ingest(grace_seconds: int = 30) -> None:
    """Send Ctrl+C-equivalent to the ingest, wait for it to exit gracefully.

    On Windows, Python's signal handling for SIGINT in a subprocess requires
    `os.kill(pid, signal.CTRL_C_EVENT)` which only works for processes in
    the same console group. Fall back to taskkill /T (tree) if needed.
    """
    pids = find_ingest_pids()
    if not pids:
        log("No wikidata_hf_import process running — nothing to stop.")
        return
    for pid in pids:
        log(f"Sending CTRL_C_EVENT to wikidata_hf_import PID {pid}")
        try:
            os.kill(pid, signal.CTRL_C_EVENT)
        except (OSError, AttributeError) as e:
            log(f"  CTRL_C_EVENT failed ({e}); falling back to taskkill")
            subprocess.run(["taskkill", "/PID", str(pid), "/F", "/T"], check=False)
    deadline = time.time() + grace_seconds
    while time.time() < deadline:
        if not find_ingest_pids():
            log("Ingest stopped.")
            return
        time.sleep(2)
    log(f"Ingest didn't stop within {grace_seconds}s; force-killing.")
    for pid in find_ingest_pids():
        subprocess.run(["taskkill", "/PID", str(pid), "/F", "/T"], check=False)


def loka_triple_count(port: int) -> int | None:
    """Query Loka for total triple count. None on failure."""
    try:
        import requests
        r = requests.post(
            f"http://localhost:{port}/sparql",
            data="SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
            headers={
                "Content-Type": "application/sparql-query",
                "Accept": "application/sparql-results+json",
            },
            timeout=1800,
        )
        r.raise_for_status()
        return int(r.json()["results"]["bindings"][0]["n"]["value"])
    except Exception as e:
        log(f"triple-count query failed: {e}")
        return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--port", type=int, default=3030,
                    help="Loka port (default 3030)")
    ap.add_argument("--epochs", type=int, default=4,
                    help="Training epochs (default 4 — fits noon→4pm window)")
    ap.add_argument("--min-triples", type=int, default=33_000_000,
                    help="Abort cycle if Loka has fewer than this many triples")
    ap.add_argument("--dry-run", action="store_true",
                    help="Log decisions but do not run preprocess/train/ship")
    args = ap.parse_args()

    log("=== v11 kickoff start ===")
    log(f"port={args.port} epochs={args.epochs} min_triples={args.min_triples:,} "
        f"dry_run={args.dry_run}")

    # 1. Stop any running ingest.
    log("Step 1: stop ingest if running")
    if not args.dry_run:
        stop_ingest()

    # 2. Verify Loka health + triple count.
    log("Step 2: verify Loka health + triple count")
    if not tc.loka_serving(args.port):
        log(f"!! Loka not reachable on port {args.port}. Aborting.")
        return 1
    count = loka_triple_count(args.port)
    if count is None:
        log("!! triple-count query failed. Aborting.")
        return 1
    log(f"Loka /health green; triple count: {count:,}")
    if count < args.min_triples:
        log(f"!! triple count {count:,} below threshold {args.min_triples:,}. "
            "Aborting.")
        return 1

    if args.dry_run:
        log("dry-run: would now preprocess → train → ship. Exiting cleanly.")
        return 0

    # 3. Preprocess.
    next_version = tc.latest_checkpoint_version() + 1
    log(f"Step 3: preprocess → training/data/triples_v{next_version}.txt")
    corpus = tc.DATA / f"triples_v{next_version}.txt"
    tc.preprocess_to_corpus(args.port, corpus)

    # 4. Train.
    log(f"Step 4: train v{next_version} ({args.epochs} epochs)")
    tc.train_version(next_version, corpus, args.epochs)

    # 5. Ship — propgen test, DEVLOG entry, MODEL.json, HF push, commit + push.
    log(f"Step 5: ship v{next_version}")
    tc.ship(next_version)

    log(f"=== v11 kickoff DONE — v{next_version} shipped ===")
    return 0


if __name__ == "__main__":
    sys.exit(main())
