# Loka — quick-start / status

> SutraDB engine + RDF-star world-model loop. Project is being rebranded **Loka** on Hugging Face; the GitHub repo will be renamed later.

Last touched: **2026-05-09**

---

## What this project is in one paragraph

A native RDF-star triplestore (Rust, this repo) plus a small from-scratch transformer trained on label-substituted Wikidata triples. Each model-generated triple is written back into the store as an RDF-star annotation with `propositionInferredFrom` edges to the context it was inferred from. End-to-end loop: store → corpus → train → predict → write-back-with-provenance → re-train. See `planning/world-model-thesis.md` for the canonical vision.

---

## Where we are RIGHT NOW

### Background processes
| Task | What it is | Current state |
|---|---|---|
| `bwwrjw7on` | `./target/release/sutra.exe serve --port 3030` | Live, serving 5,055,385 triples |
| `bgju84u0v` | v5 training (d_model 512, 6 layers, 44M params) | Mid epoch 1, ETA ~2 h to 5/5 |

### Stored corpus
`sutra-data/db` (~770 MB) holds 5.05M RDF-star triples + 1.69M annotations on quoted-triple subjects. Source: `philippesaade/wikidata` HF dataset, 27,780 entities, every language label, every Wikidata qualifier and reference preserved.

A cold backup lives at `sutra-data-backup-2026-05-09/` (frozen, no live writers — use this for upload sources).

### Trained checkpoints
| File | Architecture | Final ppl | Notes |
|---|---|---|---|
| `wikidata_v3.pt` | d_model 256, 4 layers, 16M params | 53.4 | Pre-cleanup corpus; has `xmlschema decimal` garbage tokens |
| `wikidata_v4.pt` | same | 92.5 | Cleaned corpus; canonical "good" model |
| `wikidata_v5.pt` | d_model 512, 6 layers, **44M params** | _in progress_ | Bigger-model experiment |

### Hugging Face mirror
`https://huggingface.co/datasets/EmmaLeonhart/loka` — corpus + checkpoints already uploaded. **`sutra-data/` folder and `v4` tag missing** (first upload was blocked by the file-lock from sutra serve; retry blocked by credential-leak guard after token was pasted in chat). To finish:

1. **Rotate the token at https://huggingface.co/settings/tokens** (assume the previous one is compromised).
2. `huggingface-cli login` (paste new token at the prompt; goes to `~/.cache/huggingface/token`, never enters chat).
3. `python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v4 --sutra-data-path sutra-data-backup-2026-05-09`

---

## The pipeline, in commands

```bash
# 1. Start the engine (already running, but for a cold start)
./target/release/sutra.exe serve --port 3030

# 2. Ingest more entities from HF (resumable; state in wikidata_hf_import_state.json)
python tools/wikidata_hf_import.py --max-triples 5000000 --batch-size 500

# 3. Build the training corpus from the live store
python training/preprocess.py --endpoint http://localhost:3030 --output training/data/triples.txt
python training/tokenizer.py --input training/data/triples.txt --output training/data/vocab.json --max-vocab 50000

# 4. Train (v4 architecture)
python training/train.py \
  --data training/data/triples.txt --vocab training/data/vocab.json \
  --checkpoint training/checkpoints/wikidata_vN.pt \
  --epochs 5 --batch-size 64

#    Bigger model (v5)
python training/train.py ... --d-model 512 --nhead 8 --layers 6

# 5. Run generative-citation inference and write RDF-star to a file
python training/infer_with_citations.py \
  --checkpoint training/checkpoints/wikidata_v4.pt \
  --vocab training/data/vocab.json \
  --endpoint http://localhost:3030 \
  --max-subjects 50 --confidence 0.4 --repetition-penalty 3.0 \
  --output training/data/generated_v4_test.nt

#    Add --post to also POST the generated triples into SutraDB.

# 6. Snapshot to Hugging Face
python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v5 \
  --sutra-data-path sutra-data-backup-2026-05-09
```

---

## Decisions to remember

- **`http://sutra.dev/provenance/` is a reserved namespace.** The world model never sees, proposes, or emits predicates under it. Enforced at three layers in `preprocess.py` and `infer_with_citations.py`. See `feedback_provenance_namespace_reserved` memory.
- **RDF-star is THE citation mechanism.** `propositionInferredFrom` (model output), `propositionImportedFrom` (ingest, currently disabled — was redundant for "all from Wikidata"), `propositionGenerated`/`propositionGeneratedBy`/`propositionConfidence` for metadata.
- **Hallucinated citations are not a blocker.** They're auditable RDF-star rows; filterable like any other generated triple. Don't add elaborate guards.
- **Fine-tuning a base model is admitted as a parallel near-term track** (planning/world-model-thesis.md §10.5). Default = Qwen 2.5 1.5B-Instruct + QLoRA. Not started yet.

---

## What's good and what's broken

Working:
- 5M-triple RDF-star corpus with full qualifiers/references on every claim
- Training loop converges cleanly (v3 final ppl 53, v4 final ppl 92 on cleaner data)
- Inference produces real-shape predictions ("metropolitan museum" for art-collection, "https www" for reference-URL, "fr" for Vikidia article ID, etc.)
- RDF-star write-back schema is solid: every generated triple has `propositionInferredFrom` edges to ~10 context triples
- Cumulative repetition penalty kills the "of of of of" loops at decode time

Known issues:
- **SutraDB SPARQL occasionally returns literal values in the predicate position.** Filtered out at preprocess; a real engine bug to fix later. ~1% of rows.
- **SutraDB `/triples` endpoint wedges after ~1M write-flush cycles.** Recoverable by restart (data persists). Hit it twice during the 5M ingest. Real engine bug.
- **Word-level tokenizer chops unicode names.** "Saint-Léger" → `saint l ger`. BPE/wordpiece would fix.
- **Mode collapse on common connector tokens.** Even with rep penalty, "of/and" still win when the model has thin entity-content knowledge for a given (S, P) pair. Bigger model (v5) and bigger corpus are the levers.

---

## Open levers in priority order

1. **See if v5 (bigger model) materially helps.** In flight — auto-checks scheduled.
2. **BPE/wordpiece tokenizer.** Replaces the lossy word-level regex; lets the model handle unicode names and hyphenated entities.
3. **Bigger corpus.** Stream more entities from HF (`--max-triples 20000000`). 27k of 30M entities is a tiny slice.
4. **Fine-tuning track.** `planning/fine-tuning-track.md` has the spec — Qwen 2.5 1.5B-Instruct + QLoRA against the same triples.txt format. Not coded yet.
5. **`--post` inference back into SutraDB.** Loop-closing experiment: model generates triples → land in store → retrain on the bigger corpus that includes its own (filtered) outputs.
6. **Engine bug fixes.** SPARQL literal-as-predicate quirk; periodic write-flush wedge.

---

## Files you'll touch most

```
tools/wikidata_hf_import.py     — ingest from philippesaade/wikidata parquet
tools/hf_snapshot.py             — push corpus + ckpts to EmmaLeonhart/loka
training/preprocess.py           — pull live store -> tab-separated triples.txt
training/tokenizer.py            — word-level vocab
training/train.py                — train role-aware transformer
training/infer_with_citations.py — predict + emit N-Triples-star with provenance
training/eval.py                 — top-k accuracy on held-out triples
planning/world-model-thesis.md   — canonical vision; READ FIRST when in doubt
planning/fine-tuning-track.md    — Qwen + QLoRA parallel track plan
```

---

## Memory pointers

The `~/.claude/projects/.../memory/` directory has durable cross-conversation context including:

- `feedback_provenance_namespace_reserved.md` — the reserved-namespace rule
- `feedback_hallucinated_citations_ok.md` — don't be defensive about invented citations
- `project_finetuning_parallel_track.md` — fine-tune is admitted alongside from-scratch
- `project_ollama_for_world_models.md` — the product-vision framing
- `feedback_pramana_lesson.md` — discipline > ambition; spec before code
