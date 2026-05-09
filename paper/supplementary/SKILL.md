---
name: loka-world-model
description: Reproduce results from the Loka paper — build the SutraDB engine, ingest the philippesaade/wikidata HF parquet stream into a 5M-triple RDF-star corpus, train the role-aware transformer (v4 baseline 16M params, v5 main 44M params), and run generative-citation inference with cumulative repetition penalty against the trained checkpoints.
allowed-tools: Bash(python *), Bash(pip *), Bash(cd *), Bash(cargo *), Bash(git *), Bash(curl *), Bash(./target/*), Bash(*sutra serve*)
---

# Loka: reproduction skill

Loka is a neuro-symbolic world model — a Rust RDF-star triplestore plus a small role-aware transformer trained on the same triples — with end-to-end generative citation expressed as RDF-star annotations.

This skill reproduces the empirical claims of the paper.

## Setup

```bash
# 1. Working directory: the repo root.
git clone https://github.com/EmmaLeonhart/SutraDB.git
cd SutraDB

# 2. Engine binary.
cargo build --release -p sutra-cli

# 3. Python deps.
pip install torch transformers
pip install -r training/requirements.txt
pip install datasets pyarrow huggingface_hub
```

## Run the engine

```bash
./target/release/sutra serve --port 3030 &
# Health check
curl http://localhost:3030/health
```

## Pull the prebuilt corpus snapshot

```bash
# Single dataset repo holds the 5M-triple store, both checkpoints, the
# tokenized corpus, and prior generated_v*.nt outputs.
git clone https://huggingface.co/datasets/EmmaLeonhart/loka /tmp/loka
cp -r /tmp/loka/sutra-data ./
cp /tmp/loka/corpus/triples.txt training/data/
cp /tmp/loka/corpus/vocab.json training/data/
cp /tmp/loka/checkpoints/wikidata_v4.pt training/checkpoints/
cp /tmp/loka/checkpoints/wikidata_v5.pt training/checkpoints/
```

## Or rebuild from scratch

```bash
# Stream the HF parquet, convert each entity to N-Triples-star with full
# qualifier and reference annotations, post to /triples.
python tools/wikidata_hf_import.py --max-triples 5000000 --batch-size 500

# Build the training corpus from the live store.
python training/preprocess.py \
    --endpoint http://localhost:3030 \
    --output training/data/triples.txt

python training/tokenizer.py \
    --input training/data/triples.txt \
    --output training/data/vocab.json \
    --max-vocab 50000

# Train v5 (44M params, 6 layers).
python training/train.py \
    --data training/data/triples.txt \
    --vocab training/data/vocab.json \
    --checkpoint training/checkpoints/wikidata_v5.pt \
    --d-model 512 --nhead 8 --layers 6 \
    --epochs 5 --batch-size 64
```

## Reproduce the prediction tables (paper §5.3)

```bash
# Same seed and penalty as the paper.
python training/infer_with_citations.py \
    --checkpoint training/checkpoints/wikidata_v5.pt \
    --vocab training/data/vocab.json \
    --endpoint http://localhost:3030 \
    --max-subjects 50 \
    --max-candidates-per-subject 5 \
    --confidence 0.4 \
    --repetition-penalty 3.0 \
    --seed 42 \
    --output training/data/generated_v5.nt

# Inspect a generated RDF-star block (paper §3.2 figure):
head -16 training/data/generated_v5.nt
```

## Reproduce the v4 baseline for comparison

```bash
python training/infer_with_citations.py \
    --checkpoint training/checkpoints/wikidata_v4.pt \
    --vocab training/data/vocab.json \
    --endpoint http://localhost:3030 \
    --max-subjects 50 \
    --max-candidates-per-subject 5 \
    --confidence 0.4 \
    --repetition-penalty 3.0 \
    --seed 42 \
    --output training/data/generated_v4.nt
```

The same seed and penalty mean the candidate (subject, predicate) pairs are identical between the two runs; differences in output are attributable to the model alone.

## Verify the reserved-namespace guard (paper §3.1)

```bash
# In the live store, every generated triple must carry a propositionGenerated
# annotation. Count via SPARQL-star:
curl -s -X POST http://localhost:3030/sparql \
    -H 'Content-Type: application/sparql-query' \
    --data 'SELECT (COUNT(*) AS ?n) WHERE {
              << ?s ?p ?o >> <http://sutra.dev/provenance/propositionGenerated> "true" .
            }'

# Equally, the corpus extractor's SPARQL-star FILTER excludes them:
grep -E 'propositionGenerated|FILTER NOT EXISTS' training/preprocess.py
```

## Engine version

Tested against SutraDB v0.4.0. Earlier versions had a `DuplicateTriple` regression on RDF-star annotation rows (fixed in commit `7143e5d`); reproduction will produce diverging results before v0.4.0.
