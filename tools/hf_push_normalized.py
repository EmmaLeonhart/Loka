"""Push the normalized-wikidata corpus to its own Hugging Face dataset repo.

Separate from `tools/hf_snapshot.py` (which pushes the Loka model + the
internal corpus) because this is an independently-useful artifact: a clean
text-form view of Wikidata that anybody training a Wikidata-grounded model
can pull, with no Loka dependency.

Default repo: `<user>/normalized-wikidata` (dataset type).

Usage:
    # Login once
    huggingface-cli login

    # Push
    python tools/hf_push_normalized.py \\
        --user EmmaLeonhart \\
        --snapshot-name 2026-05-13 \\
        --corpus training/data/triples_normalized.txt
"""
from __future__ import annotations

import argparse
import os
import sys
import time
from pathlib import Path

REPO_NAME = "normalized-wikidata"

README_TEMPLATE = """\
---
license: cc-by-sa-4.0
language:
- en
tags:
- knowledge-graph
- rdf
- wikidata
- preprocessed
- text-corpus
size_categories:
- 1M<n<100M
task_categories:
- text-generation
- feature-extraction
pretty_name: Normalized Wikidata — clean text-form triples for world-model training
---

# Normalized Wikidata

A preprocessed text-form view of Wikidata, optimised for training language
models or knowledge-graph world models. The goal is a corpus where the
*semantic content* of Wikidata triples comes through cleanly, with the
catalog-and-identifier clutter that dominates raw Wikidata by volume stripped
out.

License inherits from Wikidata: **CC-BY-SA 4.0**.

## What it is

One triple per line, tab-separated:

```
subject\\tpredicate\\tobject
```

All three positions are **English labels** — QIDs and PIDs are resolved to
their `rdfs:label@en`, either from labels already in the source dump or
fetched from the Wikidata public SPARQL endpoint as a fallback.

## What was stripped

Predicates whose Wikidata `datatype` falls into one of these classes are
**dropped entirely** — they teach the model catalog formats rather than world
knowledge, and v6 of the Loka world model demonstrated they leak format
shapes onto unrelated predicates:

- `external-id` (~10,206 properties) — Freebase ID, ISNI, GND, LCCN, Dewey, etc.
- `url` (~120 properties) — links to external sites
- `commonsMedia` (~91) — Wikimedia Commons filenames
- `math` (~36) — LaTeX formulae
- `wikibase-sense` / `-lexeme` / `-form` / `-entity-schema` (~47) — lexeme machinery
- `globe-coordinate` (~10) — `Point(lat lon)` strings
- `geo-shape` / `musical-notation` / `tabular-data` (~15) — rare, non-transferable

Predicates **kept**: `wikibase-item`, `wikibase-property`, `string`, `quantity`,
`time`, `monolingualtext`.

System-reserved provenance triples (predicates under
`http://loka.dev/provenance/`) are also dropped.

## What was normalized

- **Time** values: `+YYYY-MM-DDTHH:MM:SSZ` → `YYYY-MM-DD` (or
  `YYYY-MM-DDTHH:MM:SS` if time is non-zero). Leading `+` removed for CE
  years; `-` preserved for BCE.
- **Quantity** values: leading `+` stripped from positive numbers
  (`+1234` → `1234`).
- **Monolingualtext**: `@lang` tag stripped from the value. All languages kept;
  the model sees `Tokyo` and `東京` as plain values, not as `Tokyo@en` and
  `東京@ja`.
- **Datatype suffixes** on literals (`"value"^^<...>`): the suffix is parsed
  off so it doesn't leak into training tokens. The datatype is consulted to
  decide normalization rules and then dropped.

## What stayed

- All RDF-star inner triples that came through as plain SPO rows are kept.
- All `wikibase-item` / `wikibase-property` edges (the core knowledge graph).
- `rdfs:label` rows themselves are **excluded** from the corpus (they don't
  teach the model anything new — they just say "X is called X").

## How it was built

```
loka serve --data-dir <wikidata-store> --port 3030 &
python tools/preprocess_streaming.py \\
    --endpoint http://localhost:3030 \\
    --label-db training/data/wikidata_labels.sqlite \\
    --output training/data/triples_normalized.txt \\
    --fetch-missing-from-wikidata
```

Source: `philippesaade/wikidata` (parquet, full-dump RDF-star export),
ingested into Loka and post-processed by
[`tools/preprocess_streaming.py`](https://github.com/EmmaLeonhart/Loka/blob/main/tools/preprocess_streaming.py).

The preprocessor is **memory-flat**: streams the corpus twice over Loka's
SPARQL endpoint and keeps the label cache in SQLite, so it processes 50M
triples on a laptop without the 21 GB RAM bloat the previous one-shot version
hit.

## Snapshots

Each snapshot is tagged with the build date.

**Pulling a specific snapshot:**

```python
from huggingface_hub import hf_hub_download
path = hf_hub_download(
    repo_id="{{REPO_ID}}",
    repo_type="dataset",
    filename="triples_normalized.txt",
    revision="{{SNAPSHOT}}",
)
```

## Provenance

See [`Loka` on GitHub](https://github.com/EmmaLeonhart/Loka) for the engine,
the preprocessor source, and the paper describing the world-model training
pipeline that motivated this corpus.

## Citation

Wikidata is the upstream source. Please cite Wikidata as well as this dataset
if you use the corpus.
"""


def render_readme(repo_id: str, snapshot: str) -> str:
    return (
        README_TEMPLATE
        .replace("{{REPO_ID}}", repo_id)
        .replace("{{SNAPSHOT}}", snapshot)
    )


def ensure_repo(api, repo_id: str, private: bool) -> None:
    from huggingface_hub.utils import HfHubHTTPError
    try:
        api.repo_info(repo_id=repo_id, repo_type="dataset")
        print(f"[OK] repo exists: {repo_id}")
    except HfHubHTTPError:
        print(f"[NEW] creating repo: {repo_id} (private={private})")
        api.create_repo(repo_id=repo_id, repo_type="dataset", private=private,
                        exist_ok=True)


def upload_one(api, repo_id: str, local: Path, repo_path: str) -> None:
    if not local.exists():
        print(f"  [SKIP] {local} does not exist")
        return
    size = local.stat().st_size / 1_000_000
    print(f"  uploading {local} ({size:,.1f} MB) -> {repo_path}")
    api.upload_file(
        path_or_fileobj=str(local),
        path_in_repo=repo_path,
        repo_id=repo_id,
        repo_type="dataset",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--user", default=os.environ.get("HF_USER"),
                        help="HF username or org. Defaults to HF_USER env or whoami.")
    parser.add_argument("--snapshot-name", required=True,
                        help="Tag applied after upload (e.g. 2026-05-13).")
    parser.add_argument("--private", action="store_true",
                        help="Create the repo as private (HF Pro / paid only).")
    parser.add_argument("--corpus", default="training/data/triples_normalized.txt",
                        help="Path to the normalized corpus file.")
    parser.add_argument("--label-db", default="training/data/wikidata_labels.sqlite",
                        help="Path to the SQLite label cache to include.")
    parser.add_argument("--excluded-predicates",
                        default="training/wikidata_excluded_predicates.json",
                        help="Path to the per-PID exclusion list.")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print what would be uploaded; don't actually push.")
    args = parser.parse_args()

    try:
        from huggingface_hub import HfApi, whoami
    except ImportError:
        print("[ERROR] pip install huggingface_hub", file=sys.stderr)
        sys.exit(2)

    api = HfApi()
    user = args.user
    if not user:
        try:
            user = whoami().get("name")
            print(f"[OK] using logged-in user: {user}")
        except Exception as e:
            print(f"[ERROR] no --user / HF_USER and `whoami` failed: {e}\n"
                  f"        run `huggingface-cli login` first.", file=sys.stderr)
            sys.exit(2)

    repo_id = f"{user}/{REPO_NAME}"

    if args.dry_run:
        print(f"[DRY RUN] would push to {repo_id} as snapshot '{args.snapshot_name}'")
        for path, repo_path in [
            (Path(args.corpus), "triples_normalized.txt"),
            (Path(args.label_db), "wikidata_labels.sqlite"),
            (Path(args.excluded_predicates), "wikidata_excluded_predicates.json"),
        ]:
            if path.exists():
                size = path.stat().st_size / 1_000_000
                print(f"  would upload {path} ({size:,.1f} MB) -> {repo_path}")
            else:
                print(f"  [WARN] would SKIP {path} (does not exist)")
        return

    ensure_repo(api, repo_id, private=args.private)

    print(f"\n=== Uploading README ===")
    readme = render_readme(repo_id, args.snapshot_name).encode("utf-8")
    api.upload_file(
        path_or_fileobj=readme,
        path_in_repo="README.md",
        repo_id=repo_id,
        repo_type="dataset",
    )

    print(f"\n=== Uploading corpus + cache + spec ===")
    upload_one(api, repo_id, Path(args.corpus), "triples_normalized.txt")
    upload_one(api, repo_id, Path(args.label_db), "wikidata_labels.sqlite")
    upload_one(api, repo_id, Path(args.excluded_predicates),
               "wikidata_excluded_predicates.json")

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
