# Loka — paper supplementary

Pair with the main `paper.md`.

| File | Purpose |
|---|---|
| `README.md` | This file. |
| `REPRODUCE.md` | NeurIPS-reproducibility-checklist runbook. Step-by-step commands to rebuild the corpus, train the models, and run the inference pass that produces the paper's prediction tables. |
| `SKILL.md` | Agent-runnable shell-block version of the same. The clawRxiv AI peer review skill loads this. |
| `retract-trials/` | Raw criterion output of the five §6.2 retraction-timing runs (2026-10-29) and the per-run medians behind the §6.2 table. |

## Quick map

The runnable artifacts referenced in the paper are in the parent repository:

| Paper section | Source |
|---|---|
| §3.1 (reserved namespace, three-layer enforcement) | `training/preprocess.py` (corpus stripping + SPARQL-star FILTER), `training/infer_with_citations.py` (candidate filter + emit guard) |
| §3.2 (RDF-star reification of generative citation) | `training/infer_with_citations.py` (`predict_object`, emit block) |
| §4.1 (corpus build from philippesaade/wikidata) | `tools/wikidata_hf_import.py` |
| §4.2 (label substitution + parse_literal cleanup) | `training/preprocess.py` (`parse_literal`, `LANG_SUFFIX_RE`, `TYPED_SUFFIX_RE`) |
| §4.3 (model + training) | `training/model.py`, `training/train.py` |
| §4.4 (inference + cumulative repetition penalty) | `training/infer_with_citations.py` (`predict_object`, `--repetition-penalty`) |
| §5 (experiments) | `training/checkpoints/wikidata_v{3,4,5}.pt` and `training/data/generated_v*.nt` |

## Engine version

The paper's experiments run against Loka v0.4.0. Pre-built binaries:
https://github.com/EmmaLeonhart/Loka/releases/tag/v0.4.0

## Hugging Face mirror

Checkpoints `checkpoints/wikidata_v3.pt`–`wikidata_v14.pt` (snapshot tags exist for v5, v6,
v8–v14 and the per-epoch v13.* / v14.*; v3, v4 and v7 are files without a tag):
https://huggingface.co/datasets/EmmaLeonhart/loka
