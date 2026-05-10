# Reproducing the paper results

NeurIPS reproducibility-checklist runnable-code map. Pair with `SKILL.md` (the agent-runnable shell-block version) and the live source under the parent repository.

## Working directory

All commands assume the Loka repository root as cwd. (`git clone https://github.com/EmmaLeonhart/Loka.git && cd Loka`.)

## Quick start

```bash
# Build the engine binary.
cargo build --release -p loka-cli

# Start the store. Ports 3030 by default.
./target/release/loka serve --port 3030 &

# Pull the corpus snapshot from Hugging Face. ~150 MB without the live store; ~940 MB with.
git clone https://huggingface.co/datasets/EmmaLeonhart/loka
cp loka/loka-data/* loka-data/    # if you want the prebuilt 5M-triple store
                                     # otherwise re-ingest from scratch (next section)

# Python deps
pip install -r training/requirements.txt
pip install datasets pyarrow         # for HF parquet streaming
```

## Re-ingesting the corpus (if not pulling the prebuilt store)

```bash
# Stream philippesaade/wikidata, convert to N-Triples-star, post to /triples.
# Default target 5,000,000 triples. Resumable via wikidata_hf_import_state.json.
python tools/wikidata_hf_import.py --max-triples 5000000 --batch-size 500
```

Expected: ~1 hour wall-clock to 5M triples on a typical home connection. Two known engine bugs may surface at scale (see §6.1 of the paper); both recover by stop-restart of `loka serve` with no data loss.

## Building the training corpus

```bash
# Pull all triples, substitute QIDs/PIDs with English labels.
# Strips reserved-provenance predicates (paper §3.1) and applies parse_literal
# fixes (paper §4.2). Writes ~750k tab-separated lines.
python training/preprocess.py \
    --endpoint http://localhost:3030 \
    --output training/data/triples.txt

# Build word-level vocab (50k cap).
python training/tokenizer.py \
    --input training/data/triples.txt \
    --output training/data/vocab.json \
    --max-vocab 50000
```

## Training

| Model | Command |
|---|---|
| v4 (16M params, 4 layers, baseline) | `python training/train.py --data training/data/triples.txt --vocab training/data/vocab.json --checkpoint training/checkpoints/wikidata_v4.pt --epochs 5 --batch-size 64` |
| v5 (44M params, 6 layers, main) | `python training/train.py ... --checkpoint training/checkpoints/wikidata_v5.pt --d-model 512 --nhead 8 --layers 6 --epochs 5 --batch-size 64` |

Wall time on an RTX 4070 Laptop (8 GB VRAM): ~42 min for v4, ~91 min for v5.

## Inference (paper §5.3 prediction tables)

```bash
# Same seed (42), same penalty (3.0), 50 subjects.
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
```

Add `--post` to write the generated triples back into the live store with `propositionGenerated true` annotations and `propositionInferredFrom` citation edges.

## Paper claim → command map

| Paper claim | Reproduction |
|---|---|
| §3.1 reserved namespace; three-layer guard | Inspect `training/preprocess.py:RESERVED_PROVENANCE_PREFIX` and `training/infer_with_citations.py:is_reserved_predicate` |
| §3.2 RDF-star generative citation block | Run inference command above; `head -16 training/data/generated_v5.nt` shows one full block (1 main triple + 13 annotation rows) |
| §4.1 5M-triple corpus from HF parquet | `tools/wikidata_hf_import.py --max-triples 5000000`; final state in `wikidata_hf_import_state.json` |
| §4.2 datatype-suffix cleanup | Compare `parse_literal` returns on `'+1966-...^^<dateTime>'` before vs after `TYPED_SUFFIX_RE` |
| §5.1 v3→v4 corpus regression | `training/checkpoints/wikidata_v3.pt` (pre-cleanup) and `wikidata_v4.pt` (post-cleanup); inference outputs comparable |
| §5.2 v4 vs v5 trajectory | Train both, compare `epoch N/5  loss X  ppl Y` lines from `train.py` |
| §5.3 v4 vs v5 prediction comparison | Run inference twice with `--checkpoint` set to v4 then v5, same `--seed 42 --repetition-penalty 3.0` |
| §6.1 engine bugs | The wedge surfaces in the BFS or HF import logs; the SPARQL literal-as-predicate quirk surfaces in `preprocess.py`'s `skipped_nonuri_pred` count |

## Known limitations of this reproduction

- Random initialization is seeded but PyTorch CUDA non-determinism may cause epoch-loss numbers to differ by ±0.05 between hardware platforms.
- The HF dataset (`philippesaade/wikidata`) is a frozen 2024-09-18 snapshot; if the dataset is updated, an exact byte-for-byte corpus rebuild may not be reproducible.
- The two engine bugs (§6.1) are stochastic with respect to ingest size; the paper's results were produced with the proto-layer DuplicateTriple fix applied (commit `7143e5d` and later).
