---
name: loka-retractable-provenance
description: Reproduce the evaluation in the Loka paper — run the cascade-retraction reference tests and latency bench in the Rust engine, and the held-out link-prediction evaluation of the v13 checkpoint against a predicate-frequency baseline and an untuned TransE baseline.
allowed-tools: Bash(python *), Bash(pip *), Bash(cd *), Bash(cargo *), Bash(git *), Bash(curl *)
---

# Loka: reproduction skill

Loka is an RDF-star triplestore that stores model-generated triples next to curated ones, each annotated in a reserved provenance namespace (generating model, confidence, and the stored statements the prediction procedure took as input), and supports cascade retraction along those dependency edges. This skill reproduces the paper's evaluation (§6). Every number in §6 comes from the commands below.

## Setup

```bash
git clone https://github.com/EmmaLeonhart/Loka.git
cd Loka
# The retraction fix described in §6.1 is in release v0.4.2 and later; use v0.4.5, which also fixes and speeds up SPARQL-star queries and batches the retraction commit.
git checkout v0.4.5

pip install torch tokenizers huggingface_hub
```

On Windows, run cargo inside a Visual Studio developer environment (`vcvars64.bat`) so that MSVC's `link.exe` is found.

## §6.1 Retraction correctness

```bash
# Randomized comparison of retract_set with an independent brute-force closure:
# 50 small graphs x 10 roots and 10 medium graphs x 20 roots.
cargo test -p loka-core --test retract_reference

# The depth-0 annotation case the reference test found:
cargo test -p loka-core depth_zero_generated_triple_takes_its_annotations
```

Both pass. To see the defect, remove the depth-0 annotation sweep in `loka-core/src/retract.rs` (the block starting `let own_rows = depth0.clone();`) and rerun: both fail, with only provenance-annotation rows missing.

## §6.2 Retraction cost

```bash
cargo bench -p loka-core --bench retract
```

Prints store rows, triples removed and max depth for each size (1k, 10k, 100k and 1M generated triples; the last builds a ~5M-row store in memory), then the criterion timings. Absolute times depend on the machine; the paper's were taken on one laptop.

## §6.2–6.3 Real-data retraction and encoding cost

```bash
# A real Wikidata neighbourhood, a fresh store, and the v13 model's predictions.
python tools/wikidata_random_seed.py --seed Q42 --max-entities 300 --max-time 300 --max-depth 2 --output-dir real
loka import real/seed_Q42.nt --data-dir real/db
loka serve --data-dir real/db --port 3037 &
python training/infer_with_citations.py --checkpoint data/model/checkpoints/wikidata_v13.pt     --vocab data/model/corpus/vocab_bpe.json --bpe-tokenizer data/model/corpus/tokenizer_bpe.json     --endpoint http://127.0.0.1:3037 --max-subjects 1000 --confidence 0.25     --model-version loka-wikidata-v13 --output real/generated_v13.nt --post --device cpu
# (data/model/corpus/vocab_bpe.json comes from EmmaLeonhart/loka at tag v13, like the checkpoint.)

python tools/retract_real_eval.py --endpoint http://127.0.0.1:3037     --seed real/seed_Q42.nt --generated real/generated_v13.nt --output retract_real.json
python tools/neighbour_evidence_stats.py --endpoint http://127.0.0.1:3037 --output neighbour.json
python tools/provenance_encodings.py real/generated_v13.nt real/enc

# Query comparison: a second store with the seed plus the reification encoding.
loka import real/seed_Q42.nt --data-dir real/db_reif
loka serve --data-dir real/db_reif --port 3038 &
curl -X POST http://127.0.0.1:3038/triples --data-binary @real/enc/reification.nt
python tools/provenance_query_compare.py --star http://127.0.0.1:3037     --reif http://127.0.0.1:3038 --seed real/seed_Q42.nt --output compare.json
```

The Wikidata pull is live, so a rerun gets the neighbourhood as it is today, not the one in the paper. The recorded outputs are in `training/logs/` (`retract_real_q42.json`, `neighbour_evidence_q42.json`, `provenance_query_compare_q42.json`).

## §6.3 Link prediction

```bash
# Corpora (prefix tiers of one stream) and the v13 checkpoint.
python - <<'EOF'
from huggingface_hub import hf_hub_download
for tag in ["v13-500k", "v14-1M"]:
    print(hf_hub_download("EmmaLeonhart/normalized-wikidata", "triples_normalized.txt",
                          repo_type="dataset", revision=tag, local_dir=f"data/{tag}"))
for f in ["checkpoints/wikidata_v13.pt", "corpus/tokenizer_bpe.json"]:
    print(hf_hub_download("EmmaLeonhart/loka", f, repo_type="dataset",
                          revision="v13", local_dir="data/model"))
EOF

# Held-out set size (expects 28,448 transductive triples):
python tools/heldout_split_check.py data/v13-500k/triples_normalized.txt data/v14-1M/triples_normalized.txt

# v13 transformer + predicate-frequency baseline (CPU, ~25 min):
python training/eval_linkpred.py \
    --train data/v13-500k/triples_normalized.txt \
    --later data/v14-1M/triples_normalized.txt \
    --checkpoint data/model/checkpoints/wikidata_v13.pt \
    --bpe-tokenizer data/model/corpus/tokenizer_bpe.json \
    --output linkpred_v13.json

# Untuned TransE baseline (PyKEEN 1.11.1, CPU, ~70 min):
pip install pykeen==1.11.1
python training/baseline_kge.py --model TransE \
    --train data/v13-500k/triples_normalized.txt \
    --later data/v14-1M/triples_normalized.txt \
    --output linkpred_v13_transe.json
```

The recorded outputs are in `training/logs/linkpred_v13.json` and `training/logs/linkpred_v13_transe.json`. The transformer and frequency-baseline numbers are deterministic; TransE depends on its seed (42) and on the PyKEEN and torch versions.

## Selection provenance cites exactly the matching statements (§3.2, §4.4)

```bash
pip install pytest
python -m pytest training/test_selection_provenance.py
```

## Reserved-namespace guard (§3.1)

```bash
grep -n "propositionGenerated\|FILTER NOT EXISTS" training/preprocess.py
grep -n "is_reserved_predicate" training/infer_with_citations.py
```

## Model series (§5, Appendix A)

All checkpoints (`v3`–`v14`) are tags of `EmmaLeonhart/loka`; the v11–v14 corpora are tags of `EmmaLeonhart/normalized-wikidata`. Perplexities in Appendix A are the training logs' exp(mean training loss) at the released epoch; we checked that the v13 checkpoint reproduces its value (about 245 on a sample, against 242.75 recorded).
