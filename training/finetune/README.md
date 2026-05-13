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
| `prepare_jsonl.py` | Pull triples from a running Loka, build masked-triple SFT prompts, write JSONL. | implemented |
| `finetune.py` | QLoRA SFT loop on a 4-bit base model. Saves adapter + tokenizer. | stub — TODO |
| `infer.py` | Generate predictions, emit N-Triples-star matching `infer_with_citations.py`. | stub — TODO |
| `eval.py` | Held-out triple recovery + smoke coherence. | stub — TODO |

## Status (2026-05-12)

- Track approved 2026-05-08 (planning/fine-tuning-track.md).
- `prepare_jsonl.py` implemented + tested locally with a fixture file (no Loka required).
- The remaining three scripts are CLI-shape stubs so the directory contract is fixed before the first real implementation.
- **First real implementation gated on a healthy Loka.** The engine bug #1 fix (commit `c36760b`) is unverified at multi-GB scale; we won't run a real fine-tune until option B (reopen-in-place) confirms Loka stays up.

## Default choices (from planning doc, revisit after first cycle)

- **Base model:** Qwen 2.5 1.5B-Instruct (Apache 2.0, multilingual, fits 16 GB GPU).
- **Method:** QLoRA via HuggingFace `peft` + `trl.SFTTrainer`.
- **Output schema:** same `loka:generated` / `loka:inferredFrom` / `loka:confidence` triples as from-scratch.

## Why both tracks

The from-scratch transformer in `training/` is the long bet (closed-form
provenance, no pretraining leakage). The fine-tuned model is the near-term
bet (iteration speed; ships day-one with reasonable outputs). Both feed
inference back into Loka with `loka:generated` provenance, so the downstream
infrastructure is identical.

See `planning/fine-tuning-track.md` §"Why a parallel track" for the full
motivation.
