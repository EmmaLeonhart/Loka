# Loka — quick-start / status

> Loka engine (RDF-star triplestore) + a small role-aware transformer trained on the same triples. Now in its multi-rung normalized-wikidata phase (v11 → v14). Models live at [`EmmaLeonhart/loka`](https://huggingface.co/datasets/EmmaLeonhart/loka); the training corpora live at [`EmmaLeonhart/normalized-wikidata`](https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata).

Last touched: **2026-05-14**

---

## What this project is in one paragraph

A neuro-symbolic world model. **Loka** is the engine (Rust, this repo): a lean RDF-star triplestore with native HNSW vector indexing and a SPARQL+ query layer. **Loka** is also the project as a whole: the engine plus a 44.5 M-parameter role-aware transformer trained on label-substituted RDF-star Wikidata triples. The transformer predicts missing slot values in `<<S P O>>` triples and its predictions land back in the store as RDF-star annotations with `propositionInferredFrom` provenance edges. End-to-end loop: ingest → preprocess → train → predict → write-back-with-provenance. Canonical vision: `planning/world-model-thesis.md`. Per-version detail: `DEVLOG.md`.

---

## Where we are RIGHT NOW (2026-05-14)

### Background processes in flight

| Task | What it is | State |
|---|---|---|
| `b6n9v2spd` | `training/train.py` v13 — 10 epochs, batch 16, on 2,511,771-triple `v13-500k` corpus | Epoch 1 done (ppl 334.76, 114 min). 9 epochs to go, ETA ~17 h |
| `b60mi28cf` | `tools/preprocess_from_hf.py --max-rows 1000000 --skip-scan` — v14-1M pass-2 emit | In flight, ~25 rows/s, several hours remaining |
| `bv6patdms` | `tools/epoch_snapshot_pusher.py` watching v13_train.log | Snapshotted + pushed `wikidata_v13_epoch01.pt` as HF tag `v13.1`; will fire on each subsequent epoch |

### Current pinned model

`MODEL.json` → `loka-wikidata-v12` (epoch-6 snapshot, ppl 250.82, released 2026-05-14). Pulls from `EmmaLeonhart/loka@v12`.

### Multi-rung pipeline status

| HF corpus tag | Entity rows | Output triples | Model | Status |
|---|---|---|---|---|
| `v11-50k` | 50 000 | 350 428 | `v11` | ✅ shipped — ppl 279.12 |
| `v12-100k` | 100 000 | 671 817 | `v12` | ✅ shipped — ppl 250.82 (epoch 6 snapshot) |
| `v13-500k` | 500 000 | 2 511 771 | `v13` | corpus ✅, model training (epoch 1/10 done) |
| `v14-1M` | 1 000 000 | ~7 M (est) | `v14` | pass-1 ✅, pass-2 in flight, training queued |

### Corpus + cache state on disk

- `loka-data-cron-c1/` — 17.6 GB, 50,002,600 ingested raw triples. **Currently unused** (the v11+ pipeline streams from HF directly). Keep as a reference snapshot.
- `training/data/wikidata_labels.sqlite` — ~25 MB, 900 065 entity labels + 7 312 curated property labels. Grows with each pass-1 run.
- `training/data/normalized/` — per-tier output files: `normalized_wikidata_v11_50k.txt` (14.7 MB), `v12_100k.txt` (28.4 MB), `v13_500k.txt` (109 MB), `v14_1M.txt` (in flight).
- `training/checkpoints/wikidata_v11.pt`, `wikidata_v12.pt`, `wikidata_v12_epoch6_ppl250.pt` (snapshot), `wikidata_v13.pt`, `wikidata_v13_epoch01.pt`.

---

## The pipeline, in commands

```bash
# 1. Stream HF Wikidata into a normalized text corpus (no Loka in the loop, scales linearly)
python tools/preprocess_from_hf.py \
    --max-rows 1000000 \
    --label-db training/data/wikidata_labels.sqlite \
    --output training/data/normalized/normalized_wikidata_v14_1M.txt \
    --skip-emit   # pass 1 only; emit pass runs as a separate process

python tools/preprocess_from_hf.py \
    --max-rows 1000000 \
    --label-db training/data/wikidata_labels.sqlite \
    --output training/data/normalized/normalized_wikidata_v14_1M.txt \
    --skip-scan   # emit pass

# 2. Push corpus to HF
python tools/hf_push_normalized.py --user EmmaLeonhart \
    --snapshot-name v14-1M \
    --corpus training/data/normalized/normalized_wikidata_v14_1M.txt

# 3. Train
python training/train.py \
    --data training/data/triples_v14.txt \
    --vocab training/data/vocab_bpe.json \
    --bpe-tokenizer training/data/tokenizer_bpe.json \
    --checkpoint training/checkpoints/wikidata_v14.pt \
    --epochs 10 --batch-size 16 \
    --d-model 512 --nhead 8 --layers 6 --tokens-per-role 8

# 4. Run the per-epoch snapshot pusher in parallel (saves every epoch as a tag)
python tools/epoch_snapshot_pusher.py \
    --log training/logs/v14_train.log \
    --checkpoint training/checkpoints/wikidata_v14.pt \
    --model 14 --hf-user EmmaLeonhart

# 5. Push the final model
python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v14 --no-loka-data

# 6. (optional) Run generative-citation inference
python training/infer_with_citations.py \
    --bpe-tokenizer training/data/tokenizer_bpe.json \
    --max-subjects 50 --confidence 0.4 --repetition-penalty 3.0
# Add --post to write predictions back into a running Loka store.
```

---

## Decisions to remember

- **`http://loka.dev/provenance/` is a reserved namespace.** The world model never sees, proposes, or emits predicates under it. Enforced at three layers in `preprocess.py`, `preprocess_from_hf.py`, and `infer_with_citations.py`.
- **RDF-star is THE citation mechanism.** `propositionInferredFrom`, `propositionGenerated`, `propositionGeneratedBy`, `propositionConfidence`.
- **Hallucinated citations are not a blocker.** They're auditable RDF-star rows; filterable like any other generated triple.
- **Property labels from corpus `rdfs:label` rows are corrupt** (engine bug #2 fallout in RDF-star executors). Always source property labels from `training/property_label_cache.json` (7 312 curated entries) + Wikidata API fallback. The streaming preprocessor enforces this.
- **`--batch-size 16` for all training on this laptop.** Batch 32 OOMs the 4070 Laptop's 8 GB VRAM at the 44.5 M-param scale.
- **Don't run two CUDA processes concurrently on the same GPU during training.** Adam's momentum corrupts under shared-GPU contention (the v12 trajectory loss).
- **Snapshot each epoch as a separate HF tag** (`v{N}.{epoch}`) via `tools/epoch_snapshot_pusher.py`. The v12 disaster lost the best epoch because `train.py` overwrites the same path each epoch.

---

## What's good and what's broken

Working:
- Two-HF-dataset pipeline (`EmmaLeonhart/loka` for models, `EmmaLeonhart/normalized-wikidata` for corpus) — both with proper READMEs documenting the multi-rung series.
- Streaming preprocessor with SQLite label cache, retry logic on HF network drops, two-process split to avoid fsspec memory accumulation.
- Per-epoch snapshot pusher — every training epoch becomes its own HF tag, so a divergent late epoch can't lose the early ones.
- Hardware-aware training params pinned in `CLAUDE.md` + project memory (batch 16, exclusive GPU).

Known issues:
- **Engine bug #2: Loka SPARQL occasionally returns literal values or entity IRIs in the predicate position.** Filtered at preprocess. Real engine bug to fix later.
- **The v11+ Loka data dir (`loka-data-cron-c1/`, 17.6 GB) is no longer in the training data path.** Kept on disk for reference.
- **Mode collapse on common connector tokens.** Even with rep penalty, "of/and" still win when (S, P) coverage is thin. v11–v14 are pushing on corpus scale to address this.
- **v12 training was disrupted mid-run** by an unrelated LLaMA 3.1 8B experiment sharing the GPU. Shipped at the epoch-6 snapshot rather than the epoch-4 best. A clean v12 retrain is on the queue for a future cycle.

---

## Open levers in priority order

1. **Let v13 finish (10 epochs)** — currently 1/10 done. Will produce the first clean run on the v13-500k corpus.
2. **Ship v13 + train v14** — same recipe on the 1M-row v14 corpus once v14 pass-2 completes.
3. **Retrain v12 cleanly** — when there's exclusive-GPU time (~12 h), to get the trajectory v12 *should* have had at ppl ~225.
4. **Propgen-test v11/v12/v13/v14 on the standard Q42 seed** — same evaluation as v6–v10; deferred from v11 onward because of GPU fragility during shared use. Catalog-predicate share is expected to remain at 0 % since cleaning is dataset-side.
5. **Engine bug #2 root-cause fix.** Currently filtered at preprocess; should be fixed in `loka-sparql` so the database is honest.
6. **Fine-tuning track** (`planning/fine-tuning-track.md`) — Qwen 2.5 1.5B-Instruct + QLoRA on the same `triples_v{N}.txt` format. Scaffolded but not run yet.

---

## Files you'll touch most

```
tools/preprocess_from_hf.py       — stream HF Wikidata → normalized text corpus
tools/hf_push_normalized.py       — push corpus to EmmaLeonhart/normalized-wikidata
tools/hf_snapshot.py              — push model to EmmaLeonhart/loka
tools/epoch_snapshot_pusher.py    — per-epoch checkpoint snapshot + HF push
training/preprocess.py            — original Loka-source preprocess (legacy, still works)
training/train.py                 — train role-aware transformer (saves per-epoch)
training/infer_with_citations.py  — predict + emit N-Triples-star with provenance
training/property_label_cache.json — 7 312 curated Wikidata property labels (authoritative)
training/wikidata_excluded_predicates.json — per-PID exclusion list (noise datatypes)
queue.md                          — live in-flight work + tag pipeline state
DEVLOG.md                         — narrative history (v3 → v12, latest first)
```

---

## Memory pointers

`~/.claude/projects/.../memory/`:

- `project_hardware_laptop_not_desktop.md` — the box is a 4070 Laptop, not desktop
- `project_training_batch_size_laptop_gpu.md` — use batch 16, not 32
- `project_corpus_property_labels_corrupt.md` — never trust corpus rdfs:label rows on properties
- `feedback_provenance_namespace_reserved.md` — `http://loka.dev/provenance/` is system-only
- `feedback_hallucinated_citations_ok.md` — don't be defensive about invented citations
- `project_finetuning_parallel_track.md` — fine-tune is admitted alongside from-scratch
- `project_ollama_for_world_models.md` — the product-vision framing
- `feedback_pramana_lesson.md` — discipline > ambition; spec before code
