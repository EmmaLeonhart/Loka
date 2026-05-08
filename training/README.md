# SutraDB World-Model Training Pipeline (v0)

End-to-end pipeline that trains a small transformer to predict masked RDF triples, using SutraDB as the data source. **This is the v0 smoke-test pipeline** — proves the loop runs end-to-end on a small corpus. Scaling up the corpus and the model are separate concerns.

Architecture is documented in `planning/world-model-thesis.md`. The short version:

- **Input:** RDF triples in SutraDB. Wikidata-style data (entities with `rdfs:label`s) preferred.
- **Preprocessing:** Substitute opaque IRIs (Wikidata QIDs/PIDs) with English labels. The model sees `Douglas Adams instance of human`, not `Q42 P31 Q5`.
- **Model:** Small role-aware transformer. Each triple becomes `[CLS] s_tokens [SEP_S] p_tokens [SEP_P] o_tokens [SEP_O]`. Role embeddings carry which slot (S/P/O) each token belongs to.
- **Objective:** Mask one role at random, predict its tokens. Cross-entropy loss on masked positions.
- **Output (in v0):** A trained checkpoint that predicts plausible tokens for masked roles. **Not yet** writing inferences back to SutraDB — that's the next milestone after v0.

## Files

| File | Purpose |
|---|---|
| `preprocess.py` | Pull triples from SutraDB via SPARQL, resolve IRIs to English labels, emit a flat training file. Skips RDF-star annotations on model-generated triples. |
| `tokenizer.py` | Build a word-level vocabulary from the training file. |
| `model.py` | The role-aware transformer (PyTorch). |
| `train.py` | Masked-triple training loop with AdamW. |
| `eval.py` | Sanity-check predictions for held-out masked triples. |
| `infer_with_citations.py` | Generative-citation inference: predict new triples, write them back to SutraDB tagged `sutra:generated` with `sutra:supports` provenance edges to the cited context. |
| `requirements.txt` | Python deps. |

## Prerequisites

1. **Python 3.10+** with `pip`.
2. **SutraDB running** at `http://localhost:3030` with Wikidata data imported. Populate it first via `tools/wikidata_bfs_import.py` (see that script's usage). Smoke-test corpus = 16K triples is enough.
3. **PyTorch.** GPU recommended but CPU works for the v0 smoke test.

```bash
pip install -r training/requirements.txt
```

## End-to-end run

```bash
# 1. Make sure SutraDB is serving
sutra serve --data-dir ./sutra-data &

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
   <S>  <P>  "predicted-label" .
   << <S> <P> "predicted-label" >>  sutra:generated     "true"^^xsd:boolean .
   << <S> <P> "predicted-label" >>  sutra:generatedBy   "wikidata_v2" .
   << <S> <P> "predicted-label" >>  sutra:confidence    "0.87"^^xsd:decimal .
   << <S> <P> "predicted-label" >>  sutra:supports      << <S> <p_existing> <o_existing> >> .
   ```

The `sutra:supports` edges cite the subject's existing facts that informed the
prediction. `preprocess.py` strips these annotations (and the
`<<QUOTED_TRIPLE>>` synthetic subject) so generated triples don't pollute
future training corpora — see the TODO there about a full SPARQL-star filter
once that query support lands.

Predicted objects are emitted as plain string literals: the model output is
text. Resolving predictions to URIs via HNSW nearest-neighbor (the
`world-model-thesis.md` §5.7 commitment) is a later milestone.

## v0 limits, on purpose

- **Word-level tokenizer.** Subword/BPE comes later if labels with rare names hurt accuracy.
- **Fixed sequence length** (8 tokens per role, 28 total). Longer triples are truncated.
- **Single-language (English).** Multi-lingual training is the eventual plan but not v0.
- **Memorization is acceptable.** With 16K triples and a 5M-param model, the model will largely memorize the corpus. For v0 we want the loop to run, not generalization.
- **Write-back is now wired up.** `infer_with_citations.py` emits N-Triples-star with `sutra:generated`/`sutra:supports` provenance and posts to SutraDB. The remaining gaps: (a) URI resolution for predicted objects (HNSW decoder, world-model-thesis §5.7); (b) a full SPARQL-star filter in `preprocess.py` to exclude inner generated triples (currently only annotations are stripped).

## What this proves when it works

- The end-to-end loop runs.
- Triples can be pulled from SutraDB, label-substituted, tokenized, trained on.
- The model architecture compiles, trains, converges (loss goes down).
- `eval.py` returns sensible top-k predictions for masked roles.

That's training-ready. Everything after this — bigger corpus, bigger model, write-back, multi-language, evaluation harness — composes on top of this same loop.
