# Fine-Tuning Track — Planning Doc

**Status:** New parallel track, decided 2026-05-08.
**Source material:** Conversation about v0 from-scratch results being word salad
on 6.3k triples; user proposal to run fine-tuning alongside the from-scratch path.

This document captures the plan for fine-tuning an existing open-source LLM
on SutraDB's RDF-star corpus, as a near-term parallel track to the from-scratch
transformer pipeline in `training/`. Both tracks share `infer_with_citations.py`'s
generative-citation schema; only the model architecture differs.

The from-scratch track in `training/` continues unchanged. This is not a
replacement.

---

## Why a parallel track

**Empirical.** v0's 12M-param from-scratch transformer trained on 6,296
shrine-heavy triples produces token-level word salad — see commit `e1375aa`
era smoke tests. The README warned this was the v0 expectation
("memorization, not generalization"), but it also signals that the
from-scratch path needs corpora and compute orders of magnitude beyond what
we have on hand to produce coherent triples.

**Pragmatic.** A fine-tuned 1–7B-param open base model has already learned
English. It only needs to learn the RDF-star structure and our domain. A
working demo is plausibly a day's work, not weeks. That changes the cost of
the build-test-iterate loop on every other piece of the world-model
architecture (citation schema, write-back, OWL-as-template).

**Decoupled from the long bet.** The thesis (§5.9) says "small model is the
default ambition. Scale up only if the structured-data hypothesis fails
empirically." The structured-data hypothesis hasn't been falsified — we just
haven't fed it enough data. The fine-tuning track is not evidence against
from-scratch; it's a way to keep the rest of the system moving while the
from-scratch corpus grows.

## What this track preserves from the thesis

These remain non-negotiable across both tracks:

- **RDF triples in, RDF triples out** (§5.1). Fine-tune's output format is
  N-Triples-star, same as `infer_with_citations.py` emits.
- **Self-citing inference** (§5.5). Fine-tune outputs land in SutraDB with
  `sutra:generated`/`sutra:supports` provenance, identical to from-scratch.
- **Same SPARQL+ for both** (§5.6). The fine-tuned model is queried the same
  way as the database and as the from-scratch model. Federation stays
  implicit.
- **Train on human-readable labels** (§5.10). QIDs/PIDs are substituted with
  English labels at preprocessing, identical to from-scratch.
- **No SQL/MongoQL/GraphQL** (§6.7). Reaffirmed.

## What this track relaxes from the thesis

§6.6 ("fine-tuning a general model") moves from a hard rejection to a
parallel-admitted approach with documented constraints. See the "Decisions
revisited" section in `world-model-thesis.md`. The stated risks of §6.6 are
mitigated by:

- **Provenance**: every fine-tune output carries
  `sutra:baseModel "<id>"` so we can audit "was this from your training data
  or the base model's pretraining?" Honest answer: we can't fully tell, but
  we record what we know. Closed-form provenance remains a from-scratch
  property.
- **Closed-world bias**: the fine-tune is trained with masked-triple
  prediction (same objective as from-scratch), not Q&A. The base model's
  "always answer" prior is partially redirected by the structural objective.
  We accept that some bias leaks through.
- **Costs more**: yes. We accept it for the iteration speed.
- **Hallucinates more**: the citation/confidence/`sutra:generated` schema
  exposes hallucinations rather than hiding them. Caller policy decides what
  to do with low-confidence inferences.

## Default choices (subject to revision)

These are starting positions. Revise after the first train-test cycle.

### Base model

**Default: Qwen 2.5 1.5B-Instruct.** Small enough to QLoRA on a 16 GB GPU,
big enough to have learned English structure, Apache 2.0 license, multilingual
(matters for §3.1's "multilingual training is the plan" position).

Alternatives:

- **Phi-3.5-mini-instruct** (3.8B) — stronger English priors, smaller train cost,
  MIT license, English-only.
- **Llama-3.2-3B-Instruct** — slightly larger context window, Meta's community
  license has restrictions worth re-reading before commercial use.
- **Mistral-7B-Instruct** or **Llama-3.1-8B-Instruct** — stronger results,
  larger train cost, may need rented GPU.

Pick the smallest model that produces coherent triples at our corpus size.
Don't default to 7B+ unless 1.5B clearly fails.

### Training data format

**Default: masked-triple prediction over JSONL.** Each row:

```json
{"context": [["Tokyo", "instance of", "capital city"], ["Tokyo", "country", "Japan"]],
 "target": ["Tokyo", "population", "?"],
 "answer": "13929286"}
```

Fed to the model as a structured prompt:

```
Context:
- Tokyo | instance of | capital city
- Tokyo | country | Japan

Predict the object of: Tokyo | population | ?
```

Train the model to emit the answer. This is the same masked-S/P/O objective
as the from-scratch model, in chat-template form. Qualifier rows
(`<<S P O>> P_q O_q`) are flattened into the context as
`subject(P→O) | qualifier_pred | qualifier_value` so the model sees
qualifier-context but isn't forced to learn RDF-star syntax.

### Method

**QLoRA** (4-bit quantized base, LoRA adapters) via HuggingFace `peft` +
`trl.SFTTrainer`. Adapters are 50–500 MB instead of multi-GB; can be swapped
without re-downloading the base. Aligns with the "Ollama for world models"
pluggability framing.

### Code location

`training/finetune/` — separate subdirectory so the from-scratch pipeline in
`training/` stays uncluttered. Suggested files:

| File | Purpose |
|---|---|
| `prepare_jsonl.py` | Pull triples (qualifier-aware via SPARQL-star), serialize to JSONL prompts. |
| `finetune.py` | QLoRA SFT loop. Saves adapter + tokenizer. |
| `infer.py` | Generate predictions, emit N-Triples-star with `sutra:generated` etc. — same schema as the from-scratch `infer_with_citations.py`. |
| `eval.py` | Held-out triple recovery rate; sanity-check coherence. |

### Evaluation

Same evaluation surface as from-scratch:

- **Held-out triple recovery**: mask one slot per triple, top-1 / top-5 match.
- **Smoke coherence**: hand-pick 10 entities, generate 5 candidates each,
  flag obvious nonsense.
- **Citation usefulness**: spot-check whether `sutra:supports` edges actually
  motivate the prediction, or whether they're decorative.

## Interaction with the corpus

The current SutraDB corpus has 11,921 triples but no RDF-star qualifiers —
the BFS importer dropped them until commit `045f673`. To get a
qualifier-rich corpus, re-walk the BFS from the seed:

```bash
rm wikidata_import_state.json
python tools/wikidata_bfs_import.py --seed Q11064932 --max-time 18000
```

SutraDB's `POST /triples` errors silently on duplicate main triples (the
proto layer records and continues — `server.rs:989`). Re-walk adds qualifier
rows to existing entities without a data wipe.

If you'd rather start clean: delete `sutra-data/` first.

## Open questions

- **Multilingual scope.** Train on English-labeled triples first, or include
  Japanese/Chinese where Wikidata has them? The from-scratch track defaults to
  English; this track inherits the same default but the base model can handle
  more.
- **Prompt vs continuation format.** Chat-template (Qwen/Llama instruct) vs
  base-model continuation. Default chat-template; revisit if it bottlenecks.
- **Adapter sharing.** If we fine-tune Qwen 1.5B on Japanese shrine triples
  and another adapter on biomedical RDF, can we mix them at inference? PEFT
  supports adapter composition; useful for the Ollama-pull framing.
- **When to retrain.** Generated triples are excluded from the corpus
  (preprocess.py SPARQL-star filter). When the corpus grows enough to retrain,
  is that days, weeks, or as-needed? Decide based on observed drift.

## Status

Track approved 2026-05-08. Code not yet written. Next step: confirm base-model
choice, then build `training/finetune/prepare_jsonl.py` and `finetune.py`.
