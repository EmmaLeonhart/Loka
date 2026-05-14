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
- world-model
size_categories:
- 1M<n<10M
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

This dataset is the input to a corresponding series of Loka world-model
checkpoints at [`EmmaLeonhart/loka`](https://huggingface.co/datasets/EmmaLeonhart/loka).
Each snapshot here is named to match the Loka model trained on it — e.g.
the `v11-50k` snapshot is the corpus the `v11` Loka model was trained on,
`v12-100k` corresponds to `v12`, and so on.

## Snapshots

| Tag | Entity rows | Output triples | File size | Trained Loka model |
|---|---|---|---|---|
| `v11-50k` (alias `v0.1-50k`) | 50,000 | **350,428** | 14.7 MB | [`EmmaLeonhart/loka@v11`](https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v11) |
| `v12-100k` | 100,000 | **671,817** | 28.4 MB | [`EmmaLeonhart/loka@v12`](https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v12) |
| `v13-500k` | 500,000 | **2,511,771** | 109 MB | (training in progress 2026-05-14) |
| `v14-1M` | 1,000,000 | **4,021,409** | 176 MB | (training queued behind v13) |

All four corpus tiers are shipped as of 2026-05-14. The latest pushed tag is
`{{SNAPSHOT}}`. The total file-size sum across all four tiers is ~330 MB; pulling
just the largest gives you the deepest training signal.

**Pulling a specific snapshot:**

```python
from huggingface_hub import hf_hub_download
path = hf_hub_download(
    repo_id="{{REPO_ID}}",
    repo_type="dataset",
    filename="triples_normalized.txt",
    revision="v11-50k",  # or v12-100k, v13-500k, ...
)
```

Each snapshot is **strictly larger than the previous** — same first-N rows from
the same upstream stream, just with N raised. The SQLite label cache at
`wikidata_labels.sqlite` also grows monotonically across snapshots (~7,300
curated property labels preloaded, plus all entity labels seen in the slice).

## What it is

One triple per line, tab-separated:

```
subject\\tpredicate\\tobject
```

All three positions are **English labels** — QIDs and PIDs are resolved to
their `rdfs:label@en`. Entity labels come from the entity's own row in the
source dump; **property labels come from a curated cache** of 7,312 manually-
resolved Wikidata properties, never from corpus `rdfs:label` rows on
properties (those are corrupted by an upstream RDF-star executor bug — see
"Known issues with raw Wikidata" below).

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

In addition, object-level guards drop:

- URL-shaped values (`http://`, `https://`, `ftp://`, `irc://`, `mailto:`) that
  slipped through with non-catalog predicates
- Long digit-only strings (8+ digits — GND/VIAF/ISNI shape) and DOIs
  (`10.NNNN/...`) in the object position
- Rows where the subject *or* object is itself a property IRI
  (`wdt:P\\d+`) — these are RDF-star annotation rows surfacing in the wrong
  slot, never legitimate
- System-reserved provenance triples (predicates under
  `http://loka.dev/provenance/`)

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

## Known issues with raw Wikidata that this corpus addresses

1. **Catalog / identifier explosion.** ~82 % of Wikidata's property types by
   count are external identifiers, URLs, or other non-semantic catalog refs.
   Training on them teaches the model catalog formats rather than world
   knowledge. We strip them by datatype.
2. **Property `rdfs:label` corruption when materialised through some RDF-star
   executors.** A `<<S P O>> rdfs:label "..."@en` annotation row, depending
   on the executor, can surface as `wdt:Pnnn rdfs:label "object-value"@en`
   — i.e. the property gets keyed against the inner triple's object value
   instead of its real label. Entity labels are unaffected. We work around
   this by sourcing property labels from a curated cache and never from
   in-corpus `rdfs:label` rows on properties.
3. **Datatype suffix leakage.** `"2012-10-15T00:00:00Z"^^<...dateTime>` if
   processed naively leaks tokens like `xmlschema`, `dateTime` etc. into the
   training corpus. We strip these.
4. **Mixed-language values.** Wikidata's `monolingualtext` includes all
   languages; we keep them but strip the `@lang` tag so values like `Tokyo`
   and `東京` are plain strings.

## How it was built

The current preprocessor streams `philippesaade/wikidata` directly from
Hugging Face, with a SQLite label cache that persists across runs:

```bash
python tools/preprocess_from_hf.py \\
    --max-rows 100000 \\          # entity-row count, sets the size tier
    --label-db training/data/wikidata_labels.sqlite \\
    --output training/data/normalized/normalized_wikidata_v12_100k.txt
```

Two passes over the dataset:
- **Pass 1** scans every row to extract English `labels.en.value` into the
  SQLite cache (constant memory regardless of corpus size).
- **Pass 2** streams again to emit the tab-separated text corpus, using the
  cache for label lookups, applying the noise-datatype filter, normalising
  time/quantity values, and dropping engine-bug-#2 RDF-star fallout at the
  s/o level.

Source code: [`tools/preprocess_from_hf.py`](https://github.com/EmmaLeonhart/Loka/blob/main/tools/preprocess_from_hf.py),
[`tools/hf_push_normalized.py`](https://github.com/EmmaLeonhart/Loka/blob/main/tools/hf_push_normalized.py).

An earlier two-pass version that fetched from a Loka `.sdb` over SPARQL
(`tools/preprocess_streaming.py`) hit O(offset) cost at multi-million-triple
scale; the HF-direct version sidesteps that by streaming the upstream parquet.

## Provenance

See [`Loka` on GitHub](https://github.com/EmmaLeonhart/Loka) for the engine,
the preprocessor source, the trained model checkpoints, and the paper
describing the world-model training pipeline that motivated this corpus.

The Loka model series on Hugging Face:
[`EmmaLeonhart/loka`](https://huggingface.co/datasets/EmmaLeonhart/loka).

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
