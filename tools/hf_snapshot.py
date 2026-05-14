"""Push a named snapshot of corpus + checkpoints to a single Hugging Face repo.

One repo, dataset type, holds everything:
  <user>/loka
    ├── corpus/triples.txt
    ├── corpus/vocab.json
    ├── corpus/generated_*.nt
    ├── loka-data/   (optional, ~770 MB; the live store)
    └── checkpoints/wikidata_v*.pt

Each upload is a commit; --snapshot-name tags the repo with that name so a
specific revision can be pulled later via `revision="v4"`.

Auth: run `huggingface-cli login` once with a write token (Settings -> Access
Tokens on huggingface.co). After that this script just works.

Usage:
    python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v4

  Skip the 770 MB store snapshot:
    python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v4-light --no-loka-data

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
    ("corpus/vocab_bpe.json", "training/data/vocab_bpe.json"),
    ("corpus/tokenizer_bpe.json", "training/data/tokenizer_bpe.json"),
    ("corpus/generated_v4_test.nt", "training/data/generated_v4_test.nt"),
    ("corpus/generated_v4_repcumul.nt", "training/data/generated_v4_repcumul.nt"),
]

# Auto-discovered: every wikidata_vN.pt that exists locally. The list used to
# be hardcoded; the cron loop adds v7/v8/v9/... continuously, so we glob.
def _discover_model_files() -> list[tuple[str, str]]:
    import re as _re
    from pathlib import Path as _Path
    ckpt_dir = _Path(__file__).resolve().parent.parent / "training" / "checkpoints"
    if not ckpt_dir.exists():
        return []
    matches = []
    for p in ckpt_dir.glob("wikidata_v*.pt"):
        m = _re.match(r"wikidata_v(\d+)\.pt$", p.name)
        if m:
            matches.append((int(m.group(1)), p))
    matches.sort()
    return [(f"checkpoints/{p.name}", f"training/checkpoints/{p.name}") for _, p in matches]


MODEL_FILES = _discover_model_files()

# Default folder mappings. The local path is overridable via --loka-data-path
# so the user can upload from a frozen backup directory instead of the live
# store (which is locked while loka serve is running).
DATA_FOLDERS_DEFAULT = [
    ("loka-data", "loka-data"),
]


README_TEMPLATE = """\
---
license: apache-2.0
language:
- en
- multilingual
tags:
- knowledge-graph
- rdf
- rdf-star
- sparql
- wikidata
- world-model
- neuro-symbolic
- generative-citation
size_categories:
- 100K<n<1M
task_categories:
- feature-extraction
- text-generation
pretty_name: Loka — Neuro-Symbolic World Model
---

# Loka — RDF-star world-model corpus and checkpoints

Loka is a neuro-symbolic world model: an RDF-star triplestore engine plus a small
role-aware transformer trained on the same triples, sharing a single SPARQL+
query layer. Generated triples write back into the store with
`propositionInferredFrom` citation edges that point at the curated context the
prediction was conditioned on, so every model-emitted fact is auditable,
queryable, and filterable in SPARQL.

This dataset repo holds the **corpus** (label-substituted Wikidata slice) and
the **trained transformer checkpoints** together so a corpus version and a
checkpoint version stay aligned. The engine itself lives at
<https://github.com/EmmaLeonhart/Loka>; the paper is at `paper/paper.md` in
that repo.

**Latest pinned model: `{{LATEST_VERSION}}`** (revision tag `{{LATEST_REVISION}}`,
released {{LATEST_DATE}}). See *Snapshots* below for the full version history.

## Layout

| Path | Contents |
|---|---|
| `corpus/triples.txt` | Tab-separated label-substituted triples (subject\tpredicate\tobject) used as training input. Wikidata QIDs/PIDs are substituted with English labels at preprocess time; `@lang` and `^^<datatype>` suffixes are stripped from literals. |
| `corpus/vocab_bpe.json` | 50 K-piece BPE vocabulary used by v6 and later. |
| `corpus/tokenizer_bpe.json` | `tokenizers` library JSON for the BPE tokenizer. Pass to inference / training scripts via `--bpe-tokenizer`. |
| `corpus/vocab.json` | Word-level vocabulary used by v3–v5 (kept for reproducibility). |
| `corpus/generated_v*.nt` | RDF-star inferences emitted by trained models, with `propositionInferredFrom` provenance back to their citation context. |
| `loka-data/` *(optional, ~770 MB)* | The live RDF-star Loka store used to extract the corpus. Pull this and `loka serve --data-dir loka-data/` to query directly. |
| `checkpoints/wikidata_v*.pt` | Role-aware transformer checkpoints, PyTorch `.pt` format. Same 44.5 M-parameter architecture from v5 onward (d_model 512, 6 layers, 8 heads, 50 K BPE vocab, 8 tokens per role). |

## Architecture

All current checkpoints (v5 onward) share:

- **Role-aware masked S/P/O transformer.** Input is a concatenation of three
  fixed-length slots (subject / predicate / object), each tagged with a role
  embedding. At training time one role is masked and the model predicts the
  original tokens; at inference the object slot is masked and decoded greedily
  with a cumulative repetition penalty.
- **44.5 M parameters.** `d_model=512`, `num_layers=6`, `nhead=8`, `tokens_per_role=8`,
  `max_len=28`. Approx. 178 MB on disk per checkpoint.
- **50 K BPE vocabulary** (v6+). v5 and earlier used a word-level regex
  tokenizer; that tokenizer dropped non-ASCII characters and is preserved only
  for back-comparison.

## Datatype filtering policy

The training corpus is built from `philippesaade/wikidata` by `training/preprocess.py`
in the Loka repo. Decisions per Wikidata datatype:

**KEEP** (semantic content; ~2,231 properties):

- `wikibase-item` — entity-to-entity links (label-substituted)
- `wikibase-property` — property-to-property links
- `string` — plain string values
- `quantity` — numeric values; leading `+` stripped (`+1234` → `1234`)
- `time` — dates; leading `+` stripped, `Z` dropped, `T00:00:00` dropped when
  zero (`+2012-10-15T00:00:00Z` → `2012-10-15`; BCE keeps the `-`)
- `monolingualtext` — value with `@lang` tag stripped. **All languages kept** in
  v7+ (v6 dropped non-English).

**DROP** (catalog cross-references or specialised noise; ~10,525 properties,
about 82.5 % of all Wikidata property types):

- `external-id` (~10,206) — Freebase, ISNI, GND, LCCN, Dewey, etc. Training on
  these in v6 produced confident catalog-format hallucinations
  (`ISNI -> "00000000"`) and a leak of the format shape onto unrelated
  predicates (`instance of -> "+ Ġof - 00 - 03 T 00"`). v7 onward exclude them.
- `url` (~120), `commonsMedia` (~91), `math` (~36),
  `wikibase-sense`/`wikibase-lexeme`/`wikibase-form`/`wikibase-entity-schema`
  (~47), `globe-coordinate` (~10), `geo-shape`/`musical-notation`/`tabular-data`
  (~15) — rare or non-transferable.

Full per-datatype spec: `planning/wikidata-datatype-processing.md` in the Loka
repo. Property list pinned at `training/wikidata_excluded_predicates.json`.

## Snapshots

Each meaningful checkpoint round is tagged. Pull a specific snapshot:

```python
from huggingface_hub import hf_hub_download
ckpt = hf_hub_download(
    repo_id="EmmaLeonhart/loka",
    repo_type="dataset",
    filename="checkpoints/wikidata_v8.pt",
    revision="v8",
)
# also pull the matching tokenizer + vocab
tok = hf_hub_download(repo_id="EmmaLeonhart/loka", repo_type="dataset",
                      filename="corpus/tokenizer_bpe.json", revision="v8")
vocab = hf_hub_download(repo_id="EmmaLeonhart/loka", repo_type="dataset",
                        filename="corpus/vocab_bpe.json", revision="v8")
```

| Tag | Date | Final ppl | Corpus | Notes |
|---|---|---|---|---|
| `v3` | 2026-05-08 | 53.4 | 757 k noisy | First end-to-end run. Datatype-suffix leakage bug; emitted `xmlschema decimal http www w3 org` as memorised template. |
| `v4` | 2026-05-09 | 92.5 | 757 k cleaned | 16 M params. Datatype-suffix bug fixed. Mode collapse on common connectors (`of of of of`) addressed with decode-time cumulative repetition penalty. |
| `v5` | 2026-05-09 | 84.85 | 757 k cleaned | 44.5 M params (3 × scale-up). Bigger model picks specific entities (`halle`, `33`, `kosmos 116`) where v4 fell back to fillers. |
| `v6-bpe` | 2026-05-10 | 194.98 | 757 k cleaned | BPE tokenizer added. Same 44.5 M architecture. Final ppl not directly comparable to v5 (BPE has more tokens per role). Catalog hallucinations dominant in post-training behavioural tests — diagnosed as corpus composition. |
| `v7` | 2026-05-10 | 192.63 | 184 k v7-cleaned | Catalog datatypes dropped (~76 % of v6 corpus removed). Same architecture, 5 epochs. Tied ppl with v6 on a 4 ×-smaller corpus, but the catalog-format leak (`instance of -> "+ Ġof - 00 - 03 T 00"`) is gone. |
| `v8` | 2026-05-10 | 64.65 | 184 k v7-cleaned | Same architecture, 20 epochs from scratch on the v7 corpus. 3 × ppl improvement over v7 — the v7 corpus was undersaturated at 5 epochs. Loss still descending at epoch 20, so the next bottleneck is data scale. |
| `v9` | 2026-05-11 | 57.15 | 94 k from fresh 2 M-triple slice | First cron-cycle model. v9 propgen test: 35 emissions, 34 of them on semantic predicates (97 %) — best ratio yet. URL-prefix shape leak on `Template:* | different from` predicates is the residual failure mode. |
| `v10` | 2026-05-11 | **55.52** | 94 k from fresh 2 M-triple slice | First fully-automated cron cycle (no manual steps). 100 % semantic-predicate share on Q42 propgen — the cleanest signal of the catalog-hallucination series. Loss still descending at epoch 20. |
| `v11` | 2026-05-13 | 279.12 | **350 k** from new `normalized-wikidata` pipeline | First model on the no-Loka-in-the-loop preprocessing pipeline (`tools/preprocess_from_hf.py`). Trained 3 of 20 epochs before CUDA OOM at batch 32 on the 4070 Laptop's 8 GB VRAM — epoch-3 checkpoint is the v11 release. Future runs use batch 16. ppl looks worse than v10 because v10 ran 20 clean epochs on 94 k triples while v11 ran only 3 epochs on 350 k. **Different operating regime**, not directly comparable. Corpus tag on `EmmaLeonhart/normalized-wikidata`: `v11-50k`. |
| `v12` | 2026-05-14 | 250.82 | **672 k** from `v12-100k` normalized corpus | Second rung of the normalized-wikidata series. Training was disrupted by an unrelated LLaMA 3.1 8B experiment sharing the GPU — epochs 5–7 diverged from epoch 4's best (226.86) as Adam's momentum state corrupted under contention. Shipped at the **epoch-6 snapshot** (taken before further degradation). Even with the corruption, v12 beats v11 (250.82 vs 279.12) — the bigger, cleaner corpus shows through. Corpus tag: `v12-100k`. |

For the live latest list, see this dataset's **Files and versions** tab on
Hugging Face. Tag `main` always tracks the most recent upload.

### The normalized-wikidata pipeline (v11 onward)

v11 onward, the training corpus is built by a separate streaming preprocessor
that goes directly from `philippesaade/wikidata` parquet → text triples,
without staging into a Loka store. Corpus versions live in a parallel HF
dataset at [`EmmaLeonhart/normalized-wikidata`](https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata):

| Loka model | Corpus tag | Entity rows | Output triples |
|---|---|---|---|
| `v11` | `v11-50k` | 50 000 | 350 428 |
| `v12` | `v12-100k` | 100 000 | 671 817 |
| `v13` | `v13-500k` (in training as of 2026-05-14) | 500 000 | 2 511 771 |
| `v14` | `v14-1M` (queued) | 1 000 000 | ~7 M est. |

The series exists because **the corpus quality lever still hasn't saturated**.
v10's 55.52 ppl on a 94 k-triple corpus was the best clean-trained result;
v11–v14 are scaling the cleaned corpus up to see when the perplexity floor
re-establishes itself, and to ship a publicly-useful normalized Wikidata
dataset as a side effect.

## Generated-triple provenance

Every triple under `corpus/generated_*.nt` carries an RDF-star annotation block:

```ttl
<S> <P> "predicted-value" .
<<S P "predicted-value">> loka-prov:propositionGenerated   "true"^^xsd:boolean .
<<S P "predicted-value">> loka-prov:propositionGeneratedBy "loka-wikidata-v8" .
<<S P "predicted-value">> loka-prov:propositionConfidence  "0.67"^^xsd:decimal .
<<S P "predicted-value">> loka-prov:propositionInferredFrom <<S P_existing O_existing>> .
... (one inferredFrom edge per cited context triple)
```

`loka-prov:` expands to `http://loka.dev/provenance/`. Predicates under that
namespace are **reserved system metadata** — the model never sees them, never
proposes them as candidate predicates, and never emits them, enforced at
three layers (corpus stripping in `training/preprocess.py`, candidate
filtering in `training/infer_with_citations.py`, emit-time guard before each
primary triple is written).

The `propositionInferredFrom` edges are auditable like any other RDF: SPARQL
can query "every generated triple that cites a triple about X" or "remove all
v6 generations" with a single pattern match. Generated triples are flagged
out of training corpora automatically (the `FILTER NOT EXISTS << ?s ?p ?o >>
loka-prov:propositionGenerated` clause in `TRAINING_CORPUS_QUERY`).

## Versioning policy

- `main` branch on this dataset always points at the most recent upload.
- Each numbered checkpoint round (v3, v4, v5, v6-bpe, v7, v8, …) is a stable
  tag. Pull `revision="vN"` for reproducibility.
- The local 12-hour cycle loop (`tools/training_cron.py` in the Loka repo)
  produces a new tagged snapshot each cycle. README is regenerated on every
  upload so the version history table above stays current.

## License

[Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0). Source data is
Wikidata under [CC0](https://creativecommons.org/publicdomain/zero/1.0/);
the upstream `philippesaade/wikidata` snapshot is used unmodified for the
import step.

## Citation

If you use these checkpoints or the corpus, please cite:

```bibtex
@misc{loka2026,
  title = {Loka: Generative Citation in a Neuro-Symbolic World Model over RDF-Star Knowledge Graphs},
  author = {Leonhart, Emma},
  year = {2026},
  url = {https://github.com/EmmaLeonhart/Loka},
  note = {Paper at \\url{https://github.com/EmmaLeonhart/Loka/blob/main/paper/paper.md}}
}
```
"""


def render_readme() -> str:
    """Fill the version placeholders in README_TEMPLATE from MODEL.json.

    Run on every upload so the description on Hugging Face always reflects
    the latest pinned model. If MODEL.json can't be read, falls back to
    leaving the placeholders in so the failure is obvious in the rendered
    README (rather than silently injecting wrong values).
    """
    repo_root = Path(__file__).resolve().parent.parent
    model_json = repo_root / "MODEL.json"
    text = README_TEMPLATE
    if model_json.exists():
        try:
            import json as _json
            data = _json.loads(model_json.read_text(encoding="utf-8"))
            text = (
                text
                .replace("{{LATEST_VERSION}}", str(data.get("name", "unknown")))
                .replace("{{LATEST_REVISION}}", str(data.get("revision", "unknown")))
                .replace("{{LATEST_DATE}}", str(data.get("released", "unknown")))
            )
        except Exception as e:
            print(f"  [WARN] render_readme: could not parse MODEL.json: {e}")
    return text


def ensure_repo(api, repo_id: str, private: bool) -> None:
    """Create the dataset repo if it doesn't exist."""
    try:
        api.repo_info(repo_id=repo_id, repo_type="dataset")
        print(f"[OK] dataset repo exists: {repo_id}")
    except Exception:
        print(f"Creating dataset repo: {repo_id}  (private={private})")
        api.create_repo(repo_id=repo_id, repo_type="dataset", private=private, exist_ok=True)


def upload_readme(api, repo_id: str) -> None:
    """Always overwrite README.md on the dataset with the current rendered
    version. We *want* the description on Hugging Face to refresh each cycle
    so the version history table and the pinned-model line stay current —
    a stale README is worse than no README.
    """
    print("  uploading README.md (refresh)")
    rendered = render_readme().encode("utf-8")
    api.upload_file(
        path_or_fileobj=rendered,
        path_in_repo="README.md",
        repo_id=repo_id,
        repo_type="dataset",
    )


# Back-compat alias for any caller still using the old name.
maybe_upload_readme = upload_readme


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
        "--no-loka-data",
        dest="include_loka_data",
        action="store_false",
        default=True,
        help="Skip uploading the 770 MB loka-data/ folder.",
    )
    parser.add_argument(
        "--loka-data-path",
        default="loka-data",
        help="Local folder to upload as `loka-data/` in the repo. Default: "
             "the live store. Point at a backup (e.g. loka-data-backup-DATE) "
             "to avoid file-lock errors while loka serve is running.",
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
    upload_readme(api, repo_id)

    print(f"\n=== Uploading corpus + checkpoints to {repo_id} ===")
    upload_files(api, repo_id, CORPUS_FILES)
    upload_files(api, repo_id, MODEL_FILES)
    if args.include_loka_data:
        data_folders = [("loka-data", args.loka_data_path)]
        upload_folders(api, repo_id, data_folders)

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
