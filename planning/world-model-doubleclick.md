# Double-click a node → world model generates triples (web-studio :8091)

**Asked 2026-05-18.** In the JS Studio Knowledge Graph view, double-clicking a
node should make the trained world model generate triples for that node and add
them to the graph. Decisions taken (AskUserQuestion, 2026-05-18):

1. **Persist with provenance** — generated triples are POSTed to Loka `/triples`
   with the `http://loka.dev/provenance/` RDF-star tags, queryable + retractable
   via the existing cascade-retraction. (Not ephemeral-visual-only.)
2. **Edit the shared `/browse` viewer** (`tools/browse.html`) — the :8091
   Knowledge Graph tab is an `<iframe>` of `/browse`, so the gesture lands there
   immediately; standalone `/browse` and real `loka serve` get it too. Additive
   + gated (no-op if the sidecar is unreachable), so low risk.
3. **Shinto demo, accept rough output** — the pinned model (`loka-wikidata-v14`)
   was trained on normalized Wikidata, never on the `example.org` Shinto demo,
   so emissions will be rough/hallucinated. Acceptable: the point is to see the
   mechanism; hallucinations are auditable via the provenance tags and
   filterable. (Memory: `feedback_hallucinated_citations_ok`.)

## Constraint that drives the architecture

There is **no on-demand inference surface**. Model inference is only the
standalone batch script `training/infer_with_citations.py`, which loads a
178 MB checkpoint fresh (~10 s) each run. A double-click cannot shell out to
that per click. So: a **long-lived sidecar** that loads the checkpoint once and
stays resident, exposing HTTP `/generate`.

`huggingface-cli login` is **not** required — `EmmaLeonhart/loka` is a public
dataset repo; `loader.resolve_checkpoint()` downloads anonymously. First sidecar
start downloads ckpt+vocab+tokenizer (~180 MB). torch 2.10+cu128 present, CUDA
available; sidecar defaults to **CPU** anyway (44.5 M-param model, a handful of
forward passes per node — sub-second to a few seconds on CPU, and the laptop GPU
fragility docs say don't add concurrent GPU load without need).

## No parallel implementation (memory: `feedback_pramana_lesson`)

The per-subject generation algorithm lives once. Refactor
`infer_with_citations.py` to extract:

- `load_model(checkpoint, vocab, device, bpe_tokenizer) -> (model, vocab,
  inv_vocab, tokens_per_role, encode_fn, model_version)` — was inline in
  `main()` (lines ~297–326).
- `build_inference_state(triples, property_cache) -> (labels, subj_facts,
  pred_usage, n_reserved_skipped)` — was inline (lines ~332–366).
- `generate_for_subject(model, s_uri, *, labels, subj_facts, pred_usage, vocab,
  inv_vocab, tokens_per_role, device, model_version, confidence,
  repetition_penalty, max_candidates_per_subject, max_citations, encode_fn)
  -> (out_lines, log)` — the per-subject body (lines ~379–468).

`main()` is rewritten to call all three; its observable behaviour (CLI, output
file, `--post`) is unchanged. The sidecar imports the same three functions.

## Pieces

1. **`training/infer_with_citations.py`** — the extract-function refactor above.
2. **`tools/infer_server.py`** — stdlib `http.server` (no new deps), model
   loaded once at startup:
   - `GET /health` → `{ok, model, device}`
   - `POST /generate` `{subject, endpoint, confidence?, max_candidates?,
     post?}` → `fetch_all_triples(endpoint)` → `build_inference_state` →
     `generate_for_subject(subject)` → if `post`, POST N-Triples to
     `endpoint/triples` → return `{subject, generated:[{s,p,o,confidence}],
     nt, inserted, errors}`.
   - CORS `*` + OPTIONS preflight (browser at :8091 / iframe calls cross-origin).
   - `--port 8092 --device cpu --endpoint http://localhost:3030`.
   - Re-fetch + rebuild state per request (73-triple demo = trivial; and
     `fetch_all_triples` already SPARQL-star-filters out prior generations, so
     double-clicking repeatedly doesn't feed the model its own output).
3. **`tools/browse.html`** — `network.on('doubleClick')` now calls the sidecar
   instead of the detail panel:
   - Header gains a "Model:" endpoint field (`localStorage`-persisted, default
     `http://localhost:8092`) + a status dot, mirroring the `#endpoint` pattern.
   - Double-click → "✨ asking world model…" → POST `/generate`
     `{subject:nid, endpoint:EP(), confidence:0.2, post:true}`.
   - Returned triples added via `addNode`/`addEdge` styled distinctly
     (amber dashed edge, ✨-prefixed object node, title = confidence +
     "model-generated"); new legend item; `gstatus` shows "+N generated".
   - Sidecar unreachable → friendly inline message naming `!infer.bat`.
   - Detail panel still reachable from the left results list (`selectRow`);
     header hint updated to "Click: expand · Double-click: ✨ generate".
4. **`!infer.bat`** + docs (`web-studio/README.md`, this file, `queue.md`).

## Verify

Start sidecar (`tools/infer_server.py --port 8092`), playground_server :3030,
node static :8091. Double-click a Shinto node in the Knowledge Graph tab →
`/generate` returns ≥0 triples, they POST to :3030 with provenance, and appear
amber-dashed in the graph. `SELECT` for `loka.dev/provenance/propositionGenerated`
on :3030 shows the persisted annotations. Rough output is expected and fine.
