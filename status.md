# Loka — quick-start / status

> Loka engine (RDF-star triplestore) + a small role-aware transformer trained on the same triples. Now in its multi-rung normalized-wikidata phase (v11 → v14). Models live at [`EmmaLeonhart/loka`](https://huggingface.co/datasets/EmmaLeonhart/loka); the training corpora live at [`EmmaLeonhart/normalized-wikidata`](https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata).

Last touched: **2026-05-16**

---

## What this project is in one paragraph

A neuro-symbolic world model. **Loka** is the engine (Rust, this repo): a lean RDF-star triplestore with native HNSW vector indexing and a SPARQL+ query layer. **Loka** is also the project as a whole: the engine plus a 44.5 M-parameter role-aware transformer trained on label-substituted RDF-star Wikidata triples. The transformer predicts missing slot values in `<<S P O>>` triples and its predictions land back in the store as RDF-star annotations with `propositionInferredFrom` provenance edges. End-to-end loop: ingest → preprocess → train → predict → write-back-with-provenance. Canonical vision: `planning/world-model-thesis.md`. Per-version detail: `DEVLOG.md`.

---

## Where we are RIGHT NOW (2026-05-16)

### Background processes in flight

**None.** v11–v14 are all shipped-final; the supervisor, trainer, and epoch-snapshot pusher were cleanly stopped. The GPU is intentionally idle (released for the maintainer's other use). No training, ingest, or `loka serve` running. The code-only barrel that ran while the GPU is paused is **complete**: engine bug #2 fully closed (query-layer invariant + ingest-side quoted-triple reverse index + ffi/mcp parser fix), RDF-star solid across ingest/persistence/query/export, fine-tuning track scaffolded, and **cascade-retraction shipped end-to-end** (pure `retract_set` → `/retract/preview` → `/retract` + `retract_node` MCP tool → Loka Studio action; destructive path opt-in). All Rust suites green; Studio analyzes clean. See `queue.md` (BARREL COMPLETE) and DEVLOG 2026-05-16.

### Current pinned model

`MODEL.json` → `loka-wikidata-v14` (epoch-4 checkpoint, ppl **202.01** — series best, released 2026-05-15). Pulls from `EmmaLeonhart/loka@v14`.

### Multi-rung pipeline status — series complete

| HF corpus tag | Entity rows | Output triples | Model | Status |
|---|---|---|---|---|
| `v11-50k` | 50 000 | 350 428 | `v11` | ✅ shipped — ppl 279.12 (3 epochs; batch-32 OOM) |
| `v12-100k` | 100 000 | 671 817 | `v12` | ✅ shipped — ppl 250.82 (epoch-6 snapshot; epoch-4 best 226.86 lost to contention) |
| `v13-500k` | 500 000 | 2 511 771 | `v13` | ✅ shipped — ppl 242.75 (epoch-2 canonical; plateaued by epoch 2) |
| `v14-1M` | 1 000 000 | 4 021 409 | `v14` | ✅ shipped — ppl **202.01** (epoch-4 canonical, **series best**) |

Headline: 11× more clean data → **28 % perplexity reduction** (279.12 → 202.01), architecture/tokenizer/config held fixed. Per-epoch tags `v12.*` `v13.*` `v14.*` all on HF.

### Corpus + cache state on disk

- `loka-data-cron-c1/` — 17.6 GB, 50,002,600 ingested raw triples. **Currently unused** (the v11+ pipeline streams from HF directly). Keep as a reference snapshot.
- `training/data/wikidata_labels.sqlite` — ~25 MB, 900 065 entity labels + 7 312 curated property labels. Grows with each pass-1 run.
- `training/data/normalized/` — per-tier output files: `normalized_wikidata_v11_50k.txt` (14.7 MB), `v12_100k.txt` (28.4 MB), `v13_500k.txt` (109 MB), `v14_1M.txt` (~165 MB).
- `training/checkpoints/wikidata_v11.pt` … `wikidata_v14.pt` plus per-epoch snapshot `.pt` files.

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
- ~~**Engine bug #2: Loka SPARQL occasionally returns literal values in the predicate position.**~~ **RESOLVED 2026-05-16** — query-layer invariant + ingest-side quoted-triple reverse index + ffi/mcp parser fix. RDF-star round-trips losslessly across ingest/persistence/query/export.
- **The v11+ Loka data dir (`loka-data-cron-c1/`, 17.6 GB) is no longer in the training data path.** Kept on disk for reference.
- **Mode collapse on common connector tokens.** Even with rep penalty, "of/and" still win when (S, P) coverage is thin. The v11→v14 corpus-scale series narrowed this (best ppl 279.12 → 202.01) but did not eliminate it; a clean 10-epoch v14 on bigger hardware is the next lever.
- **v12 shipped at the epoch-6 snapshot** (ppl 250.82) rather than the epoch-4 best (226.86) — lost to an unrelated LLaMA 3.1 8B experiment sharing the GPU. A clean v12 retrain (~225 expected) remains a GPU-blocked queue item.

---

## Open levers in priority order

_The code-only barrel is complete (engine bug #2, RDF-star hardening, fine-tuning scaffold, cascade-retraction — all shipped 2026-05-16). Remaining levers are GPU-blocked._

1. ✅ **DONE — Engine bug #2 fully closed** (query-layer + ingest-side reverse index + ffi/mcp parser).
2. ✅ **DONE — Fine-tuning track scaffolded** (`training/finetune/`, current-pipeline aligned).
3. ✅ **DONE — World-model cascade-retraction shipped end-to-end** (`retract_set` → `/retract/preview` → `/retract` + `retract_node` MCP tool → Studio; destructive opt-in). Spec: `planning/cascade-retraction.md`.
4. **Donor clean-Adam 10-epoch v14** (`tools/contribute_v14_training.py`) — the explicit successor experiment per paper §5.12, expected to push below 202.01. Passive: waits for a contributor.
5. **Retrain v12 cleanly** — exclusive-GPU time (~12 h) to get the trajectory v12 *should* have had at ppl ~225.
6. **Propgen-test v11/v12/v13/v14 on the standard Q42 seed** — same evaluation as v6–v10; deferred from v11 onward because of GPU fragility. Catalog-predicate share expected to remain 0 % (cleaning is dataset-side).

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
DEVLOG.md                         — narrative history (v3 → v14, latest first)
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
