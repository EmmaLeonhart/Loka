"""Post-eval cron: fires at +12 h, then every 6 h, with a 48 h hard cap.

Designed for the 2026-05-11 state where:

  - A big HF import (`tools/wikidata_hf_import.py --max-triples 100M`) is
    running in the background, populating `loka-data-cron-c1`.
  - The user wants 8 h of quiet on git pushes while the iterative-review
    workflow catches up on the rev-1 paper push.
  - After that quiet, every 6 h for up to 48 h: snapshot the growing Loka
    state, train a fresh model on it, run the propgen test, write up the
    results in `paper/paper.md`, commit, push. The paper push triggers
    `.github/workflows/papers-ci.yml` which submits to clawRxiv for an AI
    peer review on the new revision.

Each firing is one full version cycle (preprocess → train → test → paper).
Six firings over the 36 h between +12 h and +48 h produces v11 → v16.

Usage:
    python tools/post_eval_cron.py

    # Custom timing (for debugging):
    python tools/post_eval_cron.py --first-delay 60 --interval 300 --deadline 1800
"""
from __future__ import annotations

import argparse
import io
import json
import subprocess
import sys
import time
from datetime import datetime, timedelta, timezone
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

REPO_ROOT = Path(__file__).resolve().parent.parent
DATA = REPO_ROOT / "training" / "data"
LOGS = REPO_ROOT / "training" / "logs"
CHECKPOINTS = REPO_ROOT / "training" / "checkpoints"
DEVLOG = REPO_ROOT / "DEVLOG.md"
PAPER = REPO_ROOT / "paper" / "paper.md"
MODEL_JSON = REPO_ROOT / "MODEL.json"
LOKA_ENDPOINT = "http://localhost:3030"


def log(msg: str) -> None:
    print(f"[{datetime.now().astimezone().isoformat(timespec='seconds')}] {msg}", flush=True)


def run(cmd: list[str], check: bool = True, **kw) -> subprocess.CompletedProcess:
    log(f"$ {' '.join(cmd)}")
    return subprocess.run(cmd, cwd=REPO_ROOT, check=check, **kw)


def latest_checkpoint_version() -> int:
    if not CHECKPOINTS.exists():
        return 0
    versions = []
    for p in CHECKPOINTS.glob("wikidata_v*.pt"):
        try:
            versions.append(int(p.stem.replace("wikidata_v", "")))
        except ValueError:
            continue
    return max(versions) if versions else 0


def loka_triple_count() -> int | None:
    """Query the running Loka for total triple count. None if Loka not up."""
    try:
        import requests
        r = requests.post(
            f"{LOKA_ENDPOINT}/sparql",
            data="SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }",
            headers={
                "Content-Type": "application/sparql-query",
                "Accept": "application/sparql-results+json",
            },
            timeout=300,
        )
        r.raise_for_status()
        return int(r.json()["results"]["bindings"][0]["n"]["value"])
    except Exception as e:
        log(f"loka_triple_count failed: {e}")
        return None


def preprocess(version: int) -> Path:
    output = DATA / f"triples_v{version}.txt"
    run([sys.executable, "training/preprocess.py",
         "--endpoint", LOKA_ENDPOINT,
         "--output", str(output)])
    return output


def train(version: int, corpus: Path, epochs: int) -> Path:
    ckpt = CHECKPOINTS / f"wikidata_v{version}.pt"
    log_path = LOGS / f"v{version}_train.log"
    LOGS.mkdir(parents=True, exist_ok=True)
    log(f"Training v{version} ({epochs} epochs)")
    with log_path.open("w", encoding="utf-8") as fp:
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
            cwd=REPO_ROOT, stdout=fp, stderr=subprocess.STDOUT, check=True,
        )
    return ckpt


def run_propgen(version: int) -> Path:
    seed = DATA / "seed_Q42.nt"
    rank = DATA / "pagerank_Q42.json"
    output = DATA / f"test_propgen_Q42_v{version}.nt"
    args = [sys.executable, "training/test_autoregressive_propgen.py",
            "--seed-file", str(seed),
            "--output", str(output),
            "--checkpoint", str(CHECKPOINTS / f"wikidata_v{version}.pt"),
            "--bpe-tokenizer", "training/data/tokenizer_bpe.json",
            "--max-source-triples", "30",
            "--children-per-source", "10",
            "--confidence", "0.25",
            "--model-version", f"loka-wikidata-v{version}"]
    if rank.exists():
        args += ["--rank-file", str(rank)]
    run(args)
    return output


def parse_train_log(log_path: Path) -> tuple[float, list[tuple[int, float, float]]]:
    rows = []
    if not log_path.exists():
        return float("nan"), rows
    for line in log_path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("epoch"):
            continue
        try:
            parts = line.split()
            rows.append((int(parts[1].split("/")[0]), float(parts[3]), float(parts[5])))
        except (IndexError, ValueError):
            continue
    return (rows[-1][2] if rows else float("nan")), rows


def append_paper_section(version: int, ppl: float, rows: list, propgen: Path,
                        triple_count: int, corpus_size: int) -> None:
    """Append a §5.X section for the new version to paper/paper.md."""
    section_num = 8 + (version - 10)  # v11 -> §5.9, v12 -> §5.10, ...
    text = PAPER.read_text(encoding="utf-8")
    sentinel = "## 6. Limitations"
    idx = text.find(sentinel)
    if idx < 0:
        log("Could not find §6 boundary in paper; skipping paper edit.")
        return
    epoch_table = "\n".join(
        f"| {ep} | {loss:.4f} | {p:.2f} |" for ep, loss, p in rows
    )
    new_section = f"""### 5.{section_num} v{version}: post-eval cron firing on growing corpus

Trained by `tools/post_eval_cron.py` at the {version - 10}-th post-eval
firing. Loka had **{triple_count:,} triples** at preprocessing time;
the resulting v{version} training file has **{corpus_size:,} triples**
after the v7 datatype filter.

| Epoch | Loss | Perplexity |
|---|---|---|
{epoch_table}

**Final perplexity: {ppl:.2f}**.

Q42 propgen output at `{propgen.relative_to(REPO_ROOT).as_posix()}` —
see the test script's generated `_meta.json` for per-source breakdown.
The post-eval cron does not stop the big HF import; subsequent firings
will see larger triple counts as the import continues.

"""
    new_text = text[:idx] + new_section + "---\n\n" + text[idx:]
    PAPER.write_text(new_text, encoding="utf-8")
    log(f"Appended §5.{section_num} for v{version} ({ppl:.2f}) to paper.md")


def append_devlog_entry(version: int, ppl: float, triple_count: int,
                        corpus_size: int) -> None:
    text = DEVLOG.read_text(encoding="utf-8")
    sentinel = "---\n\n## "
    idx = text.find(sentinel)
    entry = f"""## {datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")} — v{version} trained (post-eval cron)

Loka had {triple_count:,} triples at preprocessing time; v{version}
training file has {corpus_size:,} useful triples. Final perplexity
**{ppl:.2f}**. Same 44.5 M-parameter BPE architecture as v6-v10.

Checkpoint: `training/checkpoints/wikidata_v{version}.pt`. Pushed to
HF as `EmmaLeonhart/loka@v{version}`. `MODEL.json` bumped to pin
v{version}. Paper §5.{8 + (version - 10)} added.

---

"""
    if idx < 0:
        DEVLOG.write_text(text + "\n\n" + entry, encoding="utf-8")
    else:
        DEVLOG.write_text(text[:idx + 4] + entry + text[idx + 4:], encoding="utf-8")


def update_model_json(version: int) -> None:
    if not MODEL_JSON.exists():
        return
    data = json.loads(MODEL_JSON.read_text(encoding="utf-8"))
    data["name"] = f"loka-wikidata-v{version}"
    data["version"] = f"v{version}"
    data["released"] = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    data["revision"] = f"v{version}"
    if "files" in data and "checkpoint" in data["files"]:
        data["files"]["checkpoint"]["in_repo"] = f"checkpoints/wikidata_v{version}.pt"
        data["files"]["checkpoint"]["local"] = f"training/checkpoints/wikidata_v{version}.pt"
    MODEL_JSON.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")


def push_to_hf(version: int) -> None:
    run([sys.executable, "tools/hf_snapshot.py",
         "--user", "EmmaLeonhart",
         "--snapshot-name", f"v{version}",
         "--no-loka-data"])


def commit_and_push(version: int) -> None:
    run(["git", "add", "DEVLOG.md", "MODEL.json", "paper/paper.md",
         f"training/logs/v{version}_train.log"], check=False)
    msg = (
        f"v{version} (post-eval cron): trained, tested, paper §5.{8 + (version - 10)}, "
        f"shipped to HF\n\nGenerated by tools/post_eval_cron.py."
    )
    run(["git", "commit", "-m", msg], check=False)
    run(["git", "push", "origin", "main"], check=False)


def fire(epochs: int) -> None:
    """One firing: snapshot Loka, train next version, test, paper, push."""
    triple_count = loka_triple_count()
    if triple_count is None:
        log("Loka not reachable on :3030 — skipping firing.")
        return
    log(f"Loka has {triple_count:,} triples at firing time.")
    version = latest_checkpoint_version() + 1
    log(f"Training v{version}.")
    corpus = preprocess(version)
    corpus_size = sum(1 for _ in corpus.open(encoding="utf-8"))
    log(f"v{version} corpus: {corpus_size:,} triples.")
    ckpt = train(version, corpus, epochs)
    ppl, rows = parse_train_log(LOGS / f"v{version}_train.log")
    log(f"v{version} final ppl: {ppl:.2f}")
    propgen = run_propgen(version)
    push_to_hf(version)
    update_model_json(version)
    append_devlog_entry(version, ppl, triple_count, corpus_size)
    append_paper_section(version, ppl, rows, propgen, triple_count, corpus_size)
    commit_and_push(version)
    log(f"v{version} fired complete.")


def sleep_until(t: datetime) -> None:
    while True:
        now = datetime.now(timezone.utc)
        delta = (t - now).total_seconds()
        if delta <= 0:
            return
        log(f"sleeping {delta:.0f}s until {t.isoformat(timespec='seconds')}")
        time.sleep(min(delta, 600))  # wake every 10 min to be responsive to Ctrl-C


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--first-delay", type=int, default=12 * 3600,
                        help="Seconds before the first firing (default 43200 = 12 h)")
    parser.add_argument("--interval", type=int, default=6 * 3600,
                        help="Seconds between firings (default 21600 = 6 h)")
    parser.add_argument("--deadline", type=int, default=48 * 3600,
                        help="Total runtime cap (default 172800 = 48 h)")
    parser.add_argument("--epochs", type=int, default=20,
                        help="Epochs per training run (default 20)")
    args = parser.parse_args()

    start = datetime.now(timezone.utc)
    end = start + timedelta(seconds=args.deadline)
    next_fire = start + timedelta(seconds=args.first_delay)

    log(f"post_eval_cron starting at {start.isoformat(timespec='seconds')}")
    log(f"  first firing: {next_fire.isoformat(timespec='seconds')}")
    log(f"  interval: {args.interval}s ({args.interval / 3600:.1f}h)")
    log(f"  hard end:  {end.isoformat(timespec='seconds')}")

    firing_idx = 0
    while datetime.now(timezone.utc) < end:
        sleep_until(min(next_fire, end))
        if datetime.now(timezone.utc) >= end:
            break
        firing_idx += 1
        log(f"=== firing {firing_idx} ===")
        try:
            fire(args.epochs)
        except KeyboardInterrupt:
            log("interrupted; exiting")
            return
        except Exception as e:
            log(f"!! firing {firing_idx} failed: {type(e).__name__}: {e}")
        next_fire += timedelta(seconds=args.interval)
    log("deadline reached; exiting")


if __name__ == "__main__":
    main()
