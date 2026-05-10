# Loka World-Model Training Pipeline (v0)

End-to-end pipeline that trains a small transformer to predict masked RDF triples, using Loka as the data source. **This is the v0 smoke-test pipeline** — proves the loop runs end-to-end on a small corpus. Scaling up the corpus and the model are separate concerns.

Architecture is documented in `planning/world-model-thesis.md`. The short version:

- **Input:** RDF triples in Loka. Wikidata-style data (entities with `rdfs:label`s) preferred.
- **Preprocessing:** Substitute opaque IRIs (Wikidata QIDs/PIDs) with English labels. The model sees `Douglas Adams instance of human`, not `Q42 P31 Q5`.
- **Model:** Small role-aware transformer. Each triple becomes `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] o_tokens [SEP_O]`. Role embeddings carry which slot (S/P/O) each token belongs to.
- **Objective:** Mask one role at random, predict its tokens. Cross-entropy loss on masked positions.
- **Output (in v0):** A trained checkpoint that predicts plausible tokens for masked roles. **Not yet** writing inferences back to Loka — that's the next milestone after v0.

## Files

| File | Purpose |
|---|---|
| `preprocess.py` | Pull triples from Loka via SPARQL, resolve IRIs to English labels, emit a flat training file. Skips RDF-star annotations on model-generated triples. |
| `tokenizer.py` | Build a word-level vocabulary from the training file. |
| `model.py` | The role-aware transformer (PyTorch). |
| `train.py` | Masked-triple training loop with AdamW. |
| `eval.py` | Sanity-check predictions for held-out masked triples. |
| `infer_with_citations.py` | Generative-citation inference: predict new triples, write them back to Loka tagged `loka:generated` with `loka:inferredFrom` provenance edges to the cited context. |
| `requirements.txt` | Python deps. |

## Prerequisites

1. **Python 3.10+** with `pip`.
2. **Loka running** at `http://localhost:3030` with Wikidata data imported. Populate it first via `tools/wikidata_bfs_import.py` (see that script's usage). Smoke-test corpus = 16K triples is enough.
3. **PyTorch.** GPU recommended but CPU works for the v0 smoke test.

```bash
pip install -r training/requirements.txt
```

## End-to-end run

```bash
# 1. Make sure Loka is serving
loka serve --data-dir ./loka-data &

# 2. (One-time) Populate Wikidata data if the database is empty
python tools/wikidata_bfs_import.py --seed Q11064932 --max-time 3600

# 3. Preprocess: pull triples, substitute labels, emit training file
python training/preprocess.py \
  --endpoint http://localhost:3030 \
  --output training/data/triples.txt

# 4. Build vocabulary
python training/tokenizer.py \
  --input training/data/triples.txt \
  --output training/data/vocab.json

# 5. Train
python training/train.py \
  --data training/data/triples.txt \
  --vocab training/data/vocab.json \
  --checkpoint training/checkpoints/model.pt \
  --epochs 10

# 6. Eval
python training/eval.py \
  --checkpoint training/checkpoints/model.pt \
  --vocab training/data/vocab.json \
  --data training/data/triples.txt

# 7. Generative-citation inference: predict new triples and write them back
python training/infer_with_citations.py \
  --checkpoint training/checkpoints/model.pt \
  --vocab training/data/vocab.json \
  --endpoint http://localhost:3030 \
  --max-subjects 20 \
  --confidence 0.4 \
  --output training/data/generated.nt \
  --post                                # omit --post for a dry run
```

## Generative citation

Step 7 closes the loop. For each candidate subject, the script:

1. Finds predicates used by *graph-neighbors* (subjects that share at least one
   `(predicate, object)` statement with this one) but missing from this subject.
2. For each candidate `(S, P)`, masks the `O` slot and runs the model.
3. If the mean per-token confidence on the predicted object exceeds
   `--confidence`, emits a new triple **and** RDF-star annotations:

   ```
   <S> <P> "predicted-label" .
   << <S> <P> "predicted-label" >>  loka-prov:propositionGenerated      "true"^^xsd:boolean .
   << <S> <P> "predicted-label" >>  loka-prov:propositionGeneratedBy    "wikidata_v2" .
   << <S> <P> "predicted-label" >>  loka-prov:propositionConfidence     "0.87"^^xsd:decimal .
   << <S> <P> "predicted-label" >>  loka-prov:propositionInferredFrom   << <S> <p_existing> <o_existing> >> .
   ```

   Where `loka-prov:` expands to `http://loka.dev/provenance/`.

## Reserved provenance namespace

**Hard rule:** every predicate under `http://loka.dev/provenance/` is
system-internal. The world model:

- **never sees them** — `preprocess.py` strips every row with such a predicate
  from the training corpus (prefix match), and the SPARQL-star
  `FILTER NOT EXISTS << ?s ?p ?o >> propositionGenerated ?_g` query strips
  inner generated triples too;
- **never proposes them** as candidate predicates during inference;
- **never emits them** — `infer_with_citations.py` has a final guard before
  every primary triple is written; if the predicate is in the reserved
  namespace, it's logged and dropped.

Names are deliberately verbose (`propositionGeneratedFrom`, not
`generatedFrom`) so a human reading raw triples can recognise them at a
glance, and so collisions with real-world RDF are vanishingly unlikely. The
model can be trusted to never train on or hallucinate a triple in this
namespace.

Predicted objects are emitted as plain string literals: the model output is
text. Resolving predictions to URIs via HNSW nearest-neighbor (the
`world-model-thesis.md` §5.7 commitment) is a later milestone.

## v0 limits, on purpose

- **Word-level tokenizer.** Subword/BPE comes later if labels with rare names hurt accuracy.
- **Fixed sequence length** (8 tokens per role, 28 total). Longer triples are truncated.
- **Single-language (English).** Multi-lingual training is the eventual plan but not v0.
- **Memorization is acceptable.** With 16K triples and a 5M-param model, the model will largely memorize the corpus. For v0 we want the loop to run, not generalization.
- **Write-back is wired up.** `infer_with_citations.py` emits N-Triples-star with `loka:generated`/`loka:inferredFrom` provenance and posts to Loka. The corpus puller filters generated triples out via SPARQL-star, so the model never re-trains on its own outputs. Remaining gap: URI resolution for predicted objects (HNSW decoder, world-model-thesis §5.7) — predicted O is currently emitted as a plain literal.

## What this proves when it works

- The end-to-end loop runs.
- Triples can be pulled from Loka, label-substituted, tokenized, trained on.
- The model architecture compiles, trains, converges (loss goes down).
- `eval.py` returns sensible top-k predictions for masked roles.

That's training-ready. Everything after this — bigger corpus, bigger model, write-back, multi-language, evaluation harness — composes on top of this same loop.
