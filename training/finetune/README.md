# Loka — Fine-Tuning Track

Parallel-track training: QLoRA-fine-tune an existing open-source LLM on Loka's
RDF-star corpus, sharing the same `infer_with_citations.py` output schema as
the from-scratch transformer in `../`.

**Design doc:** `planning/fine-tuning-track.md`. Read that first.

## Scope of this directory

These four scripts compose a self-contained alternative to the from-scratch
pipeline. They do not touch `training/checkpoints/wikidata_v*.pt` or anything
else under `training/`; from-scratch artefacts stay isolated.

| File | Purpose | Status |
|---|---|---|
| `prepare_jsonl.py` | Build masked-triple SFT prompts → JSONL. Reads the normalized-wikidata TSV corpus (`--fixture`, current pipeline) or a legacy Loka endpoint (`--endpoint`). | implemented |
| `finetune.py` | QLoRA SFT loop on a 4-bit base model. Saves adapter + tokenizer. | stub — TODO |
| `infer.py` | Generate predictions, emit N-Triples-star matching `infer_with_citations.py`. | stub — TODO |
| `eval.py` | Held-out triple recovery + smoke coherence. | stub — TODO |

## Status (2026-05-16)

- Track approved 2026-05-08 (planning/fine-tuning-track.md).
- `prepare_jsonl.py` implemented; `--fixture` path is the **current-pipeline entry
  point** — point it at a normalized-wikidata corpus
  (`training/data/triples_v14.txt` or the `EmmaLeonhart/normalized-wikidata` HF
  tiers); no running Loka required. The legacy `--endpoint` (Loka-source) path is
  kept but no longer the default — v11+ has no Loka in the data path.
- The remaining three scripts are CLI-shape stubs so the directory contract is
  fixed before the first real implementation.
- **Earlier blockers cleared:** engine bug #1 (sled flusher) was verified
  reopen-in-place 2026-05-13; engine bug #2 (literal-predicate surfacing) was
  fixed engine-side 2026-05-16. The first real QLoRA run is now gated only on
  GPU availability — same donor/exclusive-GPU constraint as the rest of the
  project (GPU currently paused). It is *not* blocked on engine health.

## Default choices (from planning doc, revisit after first cycle)

- **Base model:** Qwen 2.5 1.5B-Instruct (Apache 2.0, multilingual, fits 16 GB GPU).
- **Method:** QLoRA via HuggingFace `peft` + `trl.SFTTrainer`.
- **Output schema:** same reserved provenance namespace as from-scratch —
  `http://loka.dev/provenance/` (`propositionInferredFrom`,
  `propositionGeneratedBy`, `propositionConfidence`), emitted as RDF-star
  annotations. Identical to `../infer_with_citations.py`.

## Why both tracks

The from-scratch transformer in `training/` is the long bet (closed-form
provenance, no pretraining leakage). The fine-tuned model is the near-term
bet (iteration speed; ships day-one with reasonable outputs). Both feed
inference back into Loka with `loka:generated` provenance, so the downstream
infrastructure is identical.

See `planning/fine-tuning-track.md` §"Why a parallel track" for the full
motivation.
