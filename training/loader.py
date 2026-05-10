"""Resolve model + vocab files declared in MODEL.json — auto-downloads from HF.

Why this exists: a fresh `git clone` does not include the trained checkpoint
(178 MB) or vocab. Inference scripts call `resolve_checkpoint()` /
`resolve_vocab()`; if the file is already on disk they return immediately,
otherwise they pull from the pinned Hugging Face revision and return the
cached path.

Pin file: `MODEL.json` at repo root. Bump it whenever a new model is the
"current" one. The `revision` field MUST be an immutable HF tag (not "main")
so reproducibility is preserved across uploads.

Usage:
    from training.loader import resolve_checkpoint, resolve_vocab
    ckpt = resolve_checkpoint()      # auto-downloads on first call
    vocab = resolve_vocab()
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
MODEL_JSON = REPO_ROOT / "MODEL.json"


def load_pin() -> dict:
    if not MODEL_JSON.exists():
        raise FileNotFoundError(
            f"MODEL.json not found at {MODEL_JSON}. The auto-download loader "
            "needs it to know which checkpoint is current. Re-clone the repo, "
            "or write a MODEL.json that points at a Hugging Face dataset "
            "revision (see training/loader.py docstring for the schema)."
        )
    return json.loads(MODEL_JSON.read_text(encoding="utf-8"))


def _resolve(file_key: str) -> Path:
    pin = load_pin()
    if file_key not in pin.get("files", {}):
        raise KeyError(
            f"MODEL.json has no `files.{file_key}` entry. Known keys: "
            f"{list(pin.get('files', {}).keys())}"
        )
    spec = pin["files"][file_key]
    local = REPO_ROOT / spec["local"]
    if local.exists():
        return local

    print(
        f"[loader] {spec['local']} not found locally; pulling "
        f"{spec['in_repo']} from {pin['repo_id']}@{pin['revision']} "
        f"(~{spec.get('size_mb', '?')} MB, one-time download)...",
        file=sys.stderr,
    )
    try:
        from huggingface_hub import hf_hub_download
    except ImportError:
        raise SystemExit(
            "huggingface_hub not installed. Either:\n"
            "  pip install huggingface_hub\n"
            "or download the file manually from\n"
            f"  https://huggingface.co/datasets/{pin['repo_id']}/tree/{pin['revision']}\n"
            f"and place it at {local}."
        )

    cached = hf_hub_download(
        repo_id=pin["repo_id"],
        repo_type=pin.get("repo_type", "dataset"),
        filename=spec["in_repo"],
        revision=pin["revision"],
    )
    print(f"[loader] cached at {cached}", file=sys.stderr)
    return Path(cached)


def resolve_checkpoint() -> Path:
    """Return a local path to the pinned checkpoint, downloading if needed."""
    return _resolve("checkpoint")


def resolve_vocab() -> Path:
    """Return a local path to the pinned vocab, downloading if needed."""
    return _resolve("vocab")


def info() -> dict:
    """Return the parsed pin — useful for `--model-version` defaulting."""
    return load_pin()


if __name__ == "__main__":
    pin = load_pin()
    print(f"Pinned model: {pin['name']} ({pin['version']}, released {pin['released']})")
    print(f"  repo: {pin['repo_id']}@{pin['revision']}")
    for key, spec in pin["files"].items():
        local = REPO_ROOT / spec["local"]
        status = "OK (local)" if local.exists() else "missing (will download)"
        print(f"  {key:12s} -> {spec['local']}  [{status}]")
