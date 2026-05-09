"""Push a named snapshot of corpus + checkpoints to a single Hugging Face repo.

One repo, dataset type, holds everything:
  <user>/loka
    ├── corpus/triples.txt
    ├── corpus/vocab.json
    ├── corpus/generated_*.nt
    ├── sutra-data/   (optional, ~770 MB; the live store)
    └── checkpoints/wikidata_v*.pt

Each upload is a commit; --snapshot-name tags the repo with that name so a
specific revision can be pulled later via `revision="v4"`.

Auth: run `huggingface-cli login` once with a write token (Settings -> Access
Tokens on huggingface.co). After that this script just works.

Usage:
    python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v4

  Skip the 770 MB store snapshot:
    python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v4-light --no-sutra-data

  Make the repo private (requires HF Pro):
    python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v4 --private
"""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path
from typing import Optional

REPO_NAME = "loka"

# Files (repo_path, local_path)
CORPUS_FILES = [
    ("corpus/triples.txt", "training/data/triples.txt"),
    ("corpus/vocab.json", "training/data/vocab.json"),
    ("corpus/generated_v4_test.nt", "training/data/generated_v4_test.nt"),
    ("corpus/generated_v4_repcumul.nt", "training/data/generated_v4_repcumul.nt"),
]

MODEL_FILES = [
    ("checkpoints/wikidata_v3.pt", "training/checkpoints/wikidata_v3.pt"),
    ("checkpoints/wikidata_v4.pt", "training/checkpoints/wikidata_v4.pt"),
]

# Folders (repo_path, local_path)
DATA_FOLDERS = [
    ("sutra-data", "sutra-data"),
]


README_TEMPLATE = """\
---
license: apache-2.0
tags:
- knowledge-graph
- rdf-star
- wikidata
- world-model
---

# Loka — RDF-star world-model corpus and checkpoints

Snapshots of the [Loka](https://github.com/Emma-Leonhart/SutraDB) world-model
corpus + trained transformer checkpoints. Single repo so corpus and checkpoint
versions stay aligned.

## Layout

| Path | Contents |
|---|---|
| `corpus/triples.txt` | Tab-separated label-substituted triples (subject, predicate, object) used as training input. |
| `corpus/vocab.json` | Word-level vocabulary built from the training file. |
| `corpus/generated_v*.nt` | RDF-star inferences emitted by the trained model with `propositionInferredFrom` provenance. |
| `sutra-data/` | The live RDF-star store used by SutraDB. ~770 MB. Pull this and `sutra serve --data-dir sutra-data/` to query directly. |
| `checkpoints/wikidata_v*.pt` | Role-aware transformer checkpoints. v3 = pre-cleanup corpus; v4 = post-cleanup corpus. |

## Snapshots

Each meaningful checkpoint round is tagged. Pull a specific snapshot with the
`revision` parameter:

```python
from huggingface_hub import hf_hub_download
ckpt = hf_hub_download(
    repo_id="<user>/loka",
    repo_type="dataset",
    filename="checkpoints/wikidata_v4.pt",
    revision="v4",
)
```

## Provenance

Every triple under `corpus/generated_*.nt` carries RDF-star annotations:

```
<S> <P> "value" .
<<S P "value">> sutra-prov:propositionGenerated   "true"^^xsd:boolean .
<<S P "value">> sutra-prov:propositionGeneratedBy "wikidata_v4" .
<<S P "value">> sutra-prov:propositionConfidence  "0.43"^^xsd:decimal .
<<S P "value">> sutra-prov:propositionInferredFrom <<S existing_p existing_o>> .
```

`sutra-prov:` expands to `http://sutra.dev/provenance/`. Predicates under that
namespace are reserved system metadata; the model never trains on them.

## Versioning

`main` always points at the latest upload. Tagged snapshots are stable.
"""


def ensure_repo(api, repo_id: str, private: bool) -> None:
    """Create the dataset repo if it doesn't exist."""
    try:
        api.repo_info(repo_id=repo_id, repo_type="dataset")
        print(f"[OK] dataset repo exists: {repo_id}")
    except Exception:
        print(f"Creating dataset repo: {repo_id}  (private={private})")
        api.create_repo(repo_id=repo_id, repo_type="dataset", private=private, exist_ok=True)


def maybe_upload_readme(api, repo_id: str) -> None:
    """If the repo has no README.md yet, upload the template."""
    try:
        files = api.list_repo_files(repo_id=repo_id, repo_type="dataset")
    except Exception:
        files = []
    if "README.md" not in files:
        print("  uploading README.md")
        api.upload_file(
            path_or_fileobj=README_TEMPLATE.encode("utf-8"),
            path_in_repo="README.md",
            repo_id=repo_id,
            repo_type="dataset",
        )


def upload_files(api, repo_id: str, files: list[tuple[str, str]]) -> None:
    for repo_path, local_path in files:
        if not Path(local_path).exists():
            print(f"  [SKIP] {local_path} not found")
            continue
        size_mb = Path(local_path).stat().st_size / 1_000_000
        print(f"  uploading {local_path} ({size_mb:,.1f} MB) -> {repo_path}")
        api.upload_file(
            path_or_fileobj=local_path,
            path_in_repo=repo_path,
            repo_id=repo_id,
            repo_type="dataset",
        )


def upload_folders(api, repo_id: str, folders: list[tuple[str, str]]) -> None:
    for repo_path, local_path in folders:
        local = Path(local_path)
        if not local.exists():
            print(f"  [SKIP] {local_path} not found")
            continue
        size_mb = sum(f.stat().st_size for f in local.rglob("*") if f.is_file()) / 1_000_000
        print(f"  uploading folder {local_path}/ ({size_mb:,.1f} MB) -> {repo_path}/")
        api.upload_folder(
            folder_path=local_path,
            path_in_repo=repo_path,
            repo_id=repo_id,
            repo_type="dataset",
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--user",
        default=os.environ.get("HF_USER"),
        help="HF username/org owning the repo. Or set HF_USER env var. "
             "If unset, will try `huggingface-cli whoami`.",
    )
    parser.add_argument(
        "--snapshot-name",
        required=True,
        help="Tag applied after upload (e.g. v4, v4-clean).",
    )
    parser.add_argument(
        "--private",
        action="store_true",
        default=False,
        help="Create the repo as private (HF Pro / paid plan required).",
    )
    parser.add_argument(
        "--no-sutra-data",
        dest="include_sutra_data",
        action="store_false",
        default=True,
        help="Skip uploading the 770 MB sutra-data/ folder.",
    )
    args = parser.parse_args()

    try:
        from huggingface_hub import HfApi, whoami
    except ImportError:
        print("[ERROR] pip install huggingface_hub", file=sys.stderr)
        sys.exit(2)

    api = HfApi()

    user: Optional[str] = args.user
    if not user:
        try:
            user = whoami().get("name")
            print(f"[OK] using logged-in user: {user}")
        except Exception as e:
            print(
                f"[ERROR] no --user / HF_USER and `whoami` failed: {e}\n"
                f"        run `huggingface-cli login` first.",
                file=sys.stderr,
            )
            sys.exit(2)

    repo_id = f"{user}/{REPO_NAME}"
    ensure_repo(api, repo_id, private=args.private)
    maybe_upload_readme(api, repo_id)

    print(f"\n=== Uploading corpus + checkpoints to {repo_id} ===")
    upload_files(api, repo_id, CORPUS_FILES)
    upload_files(api, repo_id, MODEL_FILES)
    if args.include_sutra_data:
        upload_folders(api, repo_id, DATA_FOLDERS)

    print(f"\n=== Tagging '{args.snapshot_name}' ===")
    api.create_tag(
        repo_id=repo_id,
        repo_type="dataset",
        tag=args.snapshot_name,
        exist_ok=True,
    )

    print(f"\nDone.")
    print(f"  Snapshot: https://huggingface.co/datasets/{repo_id}/tree/{args.snapshot_name}")
    print(f"  Latest:   https://huggingface.co/datasets/{repo_id}")


if __name__ == "__main__":
    main()
