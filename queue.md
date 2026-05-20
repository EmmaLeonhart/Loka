# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## RESULT 2026-05-18: the masked-SFT fine-tune lobotomised the model — pivot to base + prompt + retrieval

Decisive CPU probe (`tools/_ft_probe3.py`, base Qwen2.5-1.5B vs our
epoch-4 adapter, same "continue these RDF triples" prompt):
- **BASE, no fine-tune:** Tokyo → 7 clean, mostly-correct triples
  (`population | 13.9 million`, `currency | Japanese yen`, …). Capable.
- **epoch-4 adapter:** bare fragments only (`University of Zurich`;
  `UTC-09:00`, also factually wrong). Strictly WORSE than the base.

Verdict: the narrow masked-prediction SFT traded the base model's
knowledge for format-mimicry — net-negative for the actual goal. The
training run is dead (system event during epoch-5 fresh-Adam recovery;
nothing of value lost — epoch 4 ppl 2.95 banked on HF but it is the
lobotomised artifact, not the asset). DO NOT resume training.

**Validated path (zero further training):** base Qwen2.5-1.5B + Emma's
BFS+embedding retrieval assembling a relevance-ranked triple sequence +
a "continue the sequence" prompt. A format-only light adapter is a
*maybe-later*, not knowledge transfer.

**BUILT + VERIFIED END-TO-END 2026-05-19 (Slices 1–3, committed).**
3-index vectorised real-Wikidata Loka (:3031, 10.5k triples, node/name/
triple embeddings; engine change for quoted-triple vectors shipped +
fixed). `graph_retrieval.py` (BFS ∪ VECTOR_SIMILAR ∪ triple-sim,
least→most ranked, labels.json render) + `retrieval_generate.py`
(base Qwen continuation + provenance). On Q42: correct ranked context →
8 coherent on-topic triples, nationality/occupation/genre/real-works
right, birth-name/death-date hallucinated (expected 1.5B; auditable
via propositionConfidence/BaseModel). Strictly >> the lobotomised
fine-tune. **Retrieval ~95 s** (Loka per-node prefix scans ~2 s at
this scale) → double-click would be ~2–3 min: functional, not snappy.

**COMPLETE 2026-05-19.** Emma: optimise-then-wire. Done:
- Retrieval batched (VALUES per BFS level): ~95 s → **~14 s** recurring
  (one-time ~23 s MiniLM load is resident in the sidecar). Context
  cleaned: unlabeled-QID triples dropped, parser strips markdown.
- Slice 4: `tools/infer_server.py` /generate now drives base
  Qwen2.5-1.5B + the BFS+embedding retrieval (no fine-tune); startup
  pre-warms both. Verified: /health → qwen2.5-1.5b-base; POST
  /generate {Q42,:3031} → clean on-topic triples
  (occupation/nationality/genre/works correct; birthPlace
  hallucinated — expected 1.5B, auditable via
  loka:propositionBaseModel/Confidence), shape matches browse.html.

**Open engine loose-end (NOT blocking; pre-existing engine-bug-#2
family, found while verifying):** a SPARQL BGP
`<< ?s ?p ?o >> <specificPred> ?m` does **not** constrain the
predicate for quoted-triple subjects — it returns *any* annotation on
the quoted triple (verified: a query for `propositionBaseModel`
returned `retrieval/tripleEmb` f32vec rows). Affects provenance /
cascade-retraction / idx-triple filtering correctness. Belongs in the
engine-bug-#2 work, not this pivot. Recorded for follow-up.

**State now:** training dead (do not resume — it lobotomised the
model). Kept as reference (negative result, not the asset):
`finetune.py`, `finetune/infer.py`, `sft_common.py`,
`finetune_watchdog.py`, `stop_after_epoch.py`, shared
`candidate_predicates`, optimizer+RNG ckpt; adapter
`EmmaLeonhart/loka-qwen2.5-1.5b` epochs 1–4 on HF. Masked-SFT
"progression ladder" abandoned.

**ACTIVE BUILD — Emma chose FULL BFS+embedding** (not BFS-only). Plan:
`planning/base-retrieval.md`. Base Qwen2.5-1.5B (no adapter) + Emma's
retrieval + continue-the-sequence prompt; zero training. Emma's
2026-05-19 architecture note: this indexes **three vector indexes** —
node-by-id, node-by-name, and **the triple itself** (vector on the
RDF-star quoted triple) — a real change to what the DB indexes.
Validate feasibility gates BEFORE building (new CLAUDE.md rule):
- G1a: two declared vector predicates both answer `VECTOR_SIMILAR`.
- **G1b (decisive): vector on `<< s p o >>` + `VECTOR_SIMILAR` +
  faithful reverse-render** — the fragile engine-bug-#2 area; if it
  fails it IS an engine change, surface it, don't fake triple-sim.
- G2: a local offline embedding model exists (CPU).

Website: banner truth-pass done 2026-05-20 — the .run-banner was
removed from homepage and /contribute/, and the contribute lead now
puts the from-scratch v14 donor run front and centre with a short
retired-track footnote (the fine-tune is documented as retired
2026-05-18, not "active"). `contribute_finetune.py` was never built
and is moot under the pivot.

---

## ⏳ IN-FLIGHT 2026-05-20 15:30: retrieval-graph reload + persistence
## bug investigation

Emma asked to kill+restart :3031 so she could see the double-click KG
expand demo. The restart surfaced **a real engine persistence bug**:
on sled rehydrate the vector predicate registry came back wrong —
predicate ID 10711 (was `nameEmb`) resolved to an f32vec literal
string instead of its IRI, predicate 10712 (was `tripleEmb`) didn't
rehydrate at all, only 10710 (`nodeEmb`) survived intact. Triple
count went 22142 -> 12700 across the restart. The corruption is
visible in `/vectors/health` (the `predicate` field for the broken
index shows `"-0.007669 0.080905 …"^^<http://loka.dev/f32vec>` — a
vector LITERAL — instead of an IRI). Likely root cause: the predicate
IRI -> id mapping shares term-dictionary slots with vector-literal
interning, and the persisted slot for 10711/10712 got rewritten with
the first vector-literal posted under that index.

**Recovery in flight (background task b23rkxw6x):** stale data dir
moved aside to `loka-retrieval-data-stale-20260520/`, fresh :3031
launched on an empty `loka-retrieval-data/`, and
`python tools/load_retrieval_loka.py --endpoint http://localhost:3031`
is reloading from the source files (`graph.nt` 1.2 MB + `vectors_*.jsonl`
node 8.2 MB / name 8.2 MB / triple 30 MB). ETA a few minutes for
posting; full HNSW build then runs server-side.

**To finish the demo task (cron-resumable):**
1. Verify reload completed: `curl -s http://localhost:3031/vectors/health`
   must show 3 indexes (nodeEmb / nameEmb / tripleEmb), each ~2 100
   active_nodes for node+name and ~7 500 for triple. If it stalled
   or died, re-run `python tools/load_retrieval_loka.py
   --endpoint http://localhost:3031` (it's idempotent against an
   already-populated store — duplicate-triple errors are expected).
2. End-to-end probe: `curl -s -m 120 -X POST -H "Content-Type:
   application/json" -d '{"subject":"http://www.wikidata.org/entity/Q42",
   "endpoint":"http://localhost:3031","post":false}'
   http://localhost:8092/generate` should return ~6 generated
   triples with `model: qwen2.5-1.5b-base` in ~30-60 s.
3. The new `/browse` is live on `:3031` (binary was rebuilt
   `cargo build --release -p loka-cli` and restarted with the new
   `tools/browse.html` baked in). Emma opens
   `http://localhost:3031/browse` — endpoint field auto-fills to
   `http://localhost:3031`, infer field to `http://localhost:8092`,
   no manual tweak. Click "Run" on the default query to populate;
   double-click any IRI to expand.
4. Engine persistence bug (recorded): the vector-predicate-registry
   corruption on sled rehydrate is now the open follow-up. Likely
   lives in `loka-core/src/persistent.rs` term-dictionary slot
   assignment or in `loka-hnsw`'s predicate-id persistence on
   reopen. Belongs in the engine-bug-#2 family. Not blocking the
   demo (reload + warm process is the workaround) but should be
   fixed before any production restart story.

**Other in-session housekeeping (2026-05-20):** clawRxiv paper
resubmitted as post 2601 (the v7→v8 hop — the abstract was 6684
chars vs the 5000-char cap and had been silently failing for 5
consecutive auto-submission attempts; trimmed to 4160). New SPARQL
regression test `sparql_star_wildcard_subject_filters_by_outer_predicate`
guards the queue-claimed-but-not-reproduced quoted-triple-subject
predicate-filter bug — test passes against the current executor,
89 sparql tests green. README + status.md synced to mention the
2026-05-19 base+retrieval pivot.

---

## Website → reconstruct onto the shared branding kit

Rebuild the Loka site onto the canonical shared visual kit
(<https://emmaleonhart.com/branding/>): fixed translucent `.site-nav`
top-bar (brand · `.search` · `.theme-toggle` · prominent
`.repo-widget`), `.aurora`, hero (cosmic `.glyph` + `.eyebrow` +
gradient `h1` + `.tagline`), numbered `.section`/`h2`, `.sig`, and
the search-filter + repo-facts scripts. Full analysis + phased plan:
`planning/site-restructure-branding-kit.md`.

- [x] **A** — kit components in `pages/style.css` (`.site-nav`,
      `.search` + results dropdown, `.repo-widget`, hero/glyph/eyebrow,
      numbered `.section`/`h2`, `.sig`).
- [x] **Search** — `scripts/build_search_index.py` → `pages/search.json`
      (302 records / 38 pages); the bar box does client-side
      search-as-you-type with a jump-to-section dropdown.
- [x] **C** — `scripts/restructure_site.py` applied to all 36 content
      pages: old `<nav>` → `.site-nav` bar (brand · nav-links ·
      search · toggle · `.repo-widget`→EmmaLeonhart/Loka), `.sig`,
      kit script; `.gh` pill/back-link/dead gh-facts retired.
      `unify_site.py` step 6 now stands down on kit pages (no more
      two-transformer fight). `playground.html` excluded (full-screen
      IDE, no nav by design). All three `--check` runs are no-ops.
- [x] **B** — homepage + `/contribute/` converged onto `/style.css`
      (kit chrome applies; bespoke inline layout preserved; body
      padding-top + scroll-padding added so content clears the fixed
      bar).
- [x] **D** — `pages/index.html`: canonical `.site-nav` bar + `.sig`
      + kit scripts via the transformer; hero gains the spinning
      cosmic `.glyph` + `.eyebrow`. (Deep body section-numbering into
      `.section`/numbered-h2 is optional polish, deferred.)
- [x] **E** — all three `--check` no-ops; structural sweep clean
      across all 38 kit pages + playground; search.json 303 records.

Pages-deploy verify done 2026-05-20: homepage + /contribute/ both
load on https://loka.emmaleonhart.com after the truth-pass push;
sitemap.xml repaired in the same pass (orphaned /theory/sutradb/
fixed to /theory/loka/, missing /loka/ /history/ /contribute/
/benchmarks/ added). Remaining: render-check both themes in a real
browser — WebFetch can confirm the HTML loaded but can't render JS
or test the theme-toggle visually. Deferred polish: numbered
`.section` wrapping of the homepage body; full visual QA of
`/contribute/`'s bespoke layout under the shared kit.

Predecessor (DONE — `e97f933`, `18cbfbe`, `bdc6b80`): all 39 pages on
identity.css via style.css; `/contribute/` repaired self-contained;
`unify_site.py` hardened; circular `:root` scrubbed everywhere.

---

## Data-viewer 3-way comparison — LIVE, awaiting Emma's verdict

Environment is **up** (2026-05-17). Three viewers on ONE database
(built-in Shinto demo, 73 triples) for Emma to judge old-vs-new and
decide whether to "go back to" the old vis-network viewer.

**Running processes (do NOT kill on session resume — Emma is using
them):**
- `playground_server.exe` PID ~39852 — `target/debug/examples/
  playground_server.exe`, serves Shinto KG on `http://localhost:3030`
  (`/`, `/sparql`, `/graph-store`; CORS `*`). Re-launch: that exe, or
  `cargo run --example playground_server -p loka-proto`.
- `loka_studio.exe` PID ~40952 — Flutter Windows desktop, HTTP-only,
  `LOKA_ENDPOINT=http://localhost:3030`. Re-launch: `PATH+=/c/Users/
  Immanuelle/flutter/bin`, `LOKA_ENDPOINT=...`, `cd loka-studio &&
  flutter run -d windows`. (No loka-ffi build needed.)
- Browser: `tools/browse.html` (vis-network, the "good" old viewer,
  endpoint field defaults :3030) + `http://localhost:3030/`
  (playground IDE — confirmed: no graph viz, "Graph Browser" button
  only dumps `/graph` Turtle).

**Finding:** the "new browser" surface never had a graph
visualization; `browse.html` (vis-network) vs Loka Studio (Flutter
canvas) is the real old-vs-new question.

**Resolved 2026-05-17 — old viewer un-orphaned.** Git forensics:
`browse.html` was never visually degraded (only the `366e056`
SutraDB→Loka rebrand touched it: 7 name strings, zero viz code);
`/graph` was *never* a viewer (born as Turtle export, commit
`9d88b13`). The good viewer was simply orphaned — not served by the
engine, and the playground's "Graph Browser" button pointed at
`/graph` (Turtle dump). Fix shipped:
- `loka-proto/src/server.rs`: shared `router()` now serves the
  vis-network viewer at `GET /browse` (covers `loka serve` AND the
  playground_server example).
- `pages/playground.html`: "Graph Browser" button → `/browse`.
- `!serve.bat` worked all along — it just needed `loka.exe`; built
  via `cargo build --release -p loka-cli` (`loka 0.4.0`).
Verified live: `http://localhost:3030/browse` = 22 673 B vis-network
viewer on the 73-triple Shinto demo; button repointed; loka.exe runs.

Optional follow-ups (Emma's call): make `/browse` (or playground)
the default landing; embed the graph view inside the playground IDE;
bring browse.html's strengths into Loka Studio's Flutter canvas.

### Move Loka Studio out of Flutter → web + Electron (2026-05-17)

Direction: Flutter→web→Electron is the easy port (vs Electron→Flutter).
Keep HNSW viz. Studio is ~100% web-portable — only ONE web blocker:
`loka-studio/lib/main.dart` `import 'dart:io' show Platform` +
`Platform.environment['LOKA_ENDPOINT']`. Node 22/npm present, Flutter
web scaffolding (`loka-studio/web/`) present.

SHIPPED 2026-05-17 — Studio now has a Flutter→web→Electron path:
- `loka-studio/lib/env_endpoint{,_io}.dart` + conditional import in
  `main.dart` — only web blocker (`dart:io Platform`) removed; desktop
  still reads `LOKA_ENDPOINT`, no regression. Whole `lib/` was
  otherwise 100% web-portable (no FFI/File/Process).
- `flutter build web --release` → `loka-studio/build/web` (gitignored).
- `loka-studio/electron/` — `server.js` (static, correct wasm MIME,
  SPA fallback, EADDRINUSE-tolerant so the browser tab + Electron
  share one :8090 server), `main.js` (Electron shell), `package.json`
  (electron ^33). `node_modules/` gitignored.
- Live now: full Studio UI in the browser (`http://localhost:8090`)
  AND in an Electron window, both on the :3030 Shinto demo.

### NEXT (paused 2026-05-17, usage limits) → JS/HTML Studio

Decision: Flutter-web carries CanvasKit interop limits. Build a
plain HTML/JS Studio (real DOM), **port the Dart screens to JS**,
**vis-network `/browse` as THE graph** (no Flutter canvas graph).
Flutter Studio frozen as spec+fallback. Incremental, commit per
slice. **Full self-contained plan: `planning/js-studio.md`** (read
it first — chat context is gone in the resuming session).

Emma's refinement: tabbed app; existing engine HTML surfaces
(`/browse`, playground `/`) are tabs; "JS knowledge graph is best".

- [x] **Slice 1** — `web-studio/` shell (index.html + style.css +
      app.js): tabbed UI, endpoint state + conn dot + theme toggle,
      JS `LokaClient` (1:1 port of `loka_client.dart`). Graph tab =
      iframe `/browse` (vis-network), Playground tab = iframe `/`.
      `electron/server.js` ROOT now overridable via `STUDIO_WEB_ROOT`
      (Flutter path unchanged). Live + verified `http://localhost:8091`
      (`STUDIO_WEB_ROOT=web-studio STUDIO_WEB_PORT=8091 node
      loka-studio/electron/server.js`).
- [x] **Slice 2** — `web-studio/screens/sparql.js`: editor +
      examples + Ctrl/Cmd-Enter, type-coloured results table via
      `LokaClient.query`. Verified vs :3030.
- [x] **Slice 3-5** — `screens/triples.js` (paged SELECT, prev/next,
      page size), `health.js` (reachable + triple count + type dist +
      HNSW `/vectors/health` cards), `ontology.js` (`/graph` Turtle/
      N-Triples view + download). All served + endpoints verified vs
      :3030 (Turtle export, 4 types, /vectors/health 200).
- [x] **Slice 6** — `electron/run-js.js` + `studio:js` npm script +
      root `!studio.bat` launch Electron pointed at `web-studio`
      (STUDIO_WEB_ROOT + :8091); plain `npm start` still loads the
      frozen Flutter build. `web-studio/README.md` written. Verified:
      Electron window up on :8091 serving the JS Studio.

**JS Studio = SHIPPED end to end** (commits 3d10fc5, 5bbe3b4,
b572321, + slice 6). Five tabs (Knowledge Graph `/browse` + SPARQL +
Triples + Health + Ontology) in a **left sidebar** (not a top
nav), browser **and** Electron. Flutter Studio left frozen as
spec/fallback.

Emma's 2026-05-17 refinement applied: the Playground tab (an iframe
of the engine's `:3030/` SPARQL IDE) was dropped as redundant bloat —
the SPARQL tab already covers it — and the `playground_server`
example no longer serves an HTML page at `/` (real `loka serve`
never did). The studio at `:8091` is the single integrated UI; tabs
are a side rail.

Running locally: playground_server :3030 (Shinto demo, 73 triples;
SPARQL/API only, no `/` page), JS Studio static server :8091,
Electron. Full plan/spec: `planning/js-studio.md`. Optional
follow-ups: repo-README mention; deeper per-screen parity with the
Dart screens; auth/passcode field.

## 🚀 Multi-version pipeline — 2026-05-13 evening pivot

After hitting Loka SPARQL OFFSET-cost (page-N is O(N) on sled — 25h projected for pass 1 alone), pivoted to a no-Loka HF-source preprocessor (`tools/preprocess_from_hf.py`). The plan now is to ship a series of normalized-wikidata snapshots and train a model on each.

**Naming convention:** *the dataset tag matches the Loka-model version trained on it.* So the corpus that trains model `v11` is tagged `v11-50k`, the corpus for `v12` is `v12-100k`, etc. The `-NNk` / `-NM` suffix is the entity-row count, the version number is the model lineage (continuing from v10).

| HF dataset tag | Entity rows | Output triples | Model trained | Status |
|---|---|---|---|---|
| `v11-50k` (also `v0.1-50k` alias) | 50,000 | 350,428 | `v11` | **✅ Fully shipped 2026-05-13** — corpus: https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata/tree/v11-50k, model: https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v11. v11 trained 3/20 epochs (loss 8.79 → 5.85 → 5.63, ppl 6577 → 347.71 → **279.12**) before CUDA OOM at epoch 4 backward at batch 32 on the 8 GB-VRAM 4070 Laptop. **All future training uses `--batch-size 16`.** DEVLOG + paper §5.9 + MODEL.json updated. |
| `v12-100k` | 100,000 | **671,817** | `v12` | **✅ corpus pushed 2026-05-13 22:42 PT** — https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata/tree/v12-100k. Model `v12` training in flight (task `bocj7flty`, batch 16, 20 epochs, log `training/logs/v12_train.log`). |
| `v13-500k` | 500,000 | **2,511,771** | `v13` | **✅ Fully shipped 2026-05-14 ~20:13 PT.** Trained 5/10 epochs; trajectory 334.76 → 242.75 → 248.29 → 258.42 → 256.04, classic Adam plateau on exclusive GPU. Epoch-2 (ppl **242.75**) promoted to canonical `v13`; per-epoch tags `v13.1`–`v13.5` all on HF. https://huggingface.co/datasets/EmmaLeonhart/loka/tree/v13. DEVLOG + paper §5.11 + MODEL.json updated. |
| `v14-1M` | 1,000,000 | **4,021,409** | `v14` | **✅ Finalized at epoch-4 (ppl 202.01) — 2026-05-15.** Trajectory: ep1 283.07 → ep2 215.45 → ep3 209.55 → **ep4 202.01 (best)** → ep5 204.70. A bounded continuation experiment (commit `a4c586e` supervisor, log `training/logs/v14_sup_attempt1_20260515_160405_train.log`) resumed from epoch-4 weights with fresh Adam (the epoch-4 checkpoint predates optimizer-state saving) and ran epochs 6–7: **ep6 206.12, ep7 209.87** — same fresh-Adam-after-good-weights signature as v13 §5.11, drifting *outward* not inward. Experiment concluded — **~202 is the practical floor for a from-scratch fit on this corpus and this 8 GB power-capped laptop GPU.** Going materially below is the contributor-donor 10-epoch clean-Adam path, not a resume-with-fresh-optimizer path. Per-epoch tags `v14.1`…`v14.7` all on HF; canonical `v14` = epoch-4. Paper §5.12 + abstract + §5.10 closer all updated to the final framing (commits `1573af6`, `bba9043`). GPU intentionally idle (released for Minecraft); supervisor + trainer + pusher cleanly stopped. |

**Series status:** v11/v12/v13/v14 **all shipped-final.** Corpus-scale headline result: v11 350k→279.12, v12 672k→250.82, v13 2.5M→242.75, v14 4M→**202.01** — a 28% best-perplexity drop from corpus-scale alone, architecture/tokenizer/config held fixed across the series.

**Open follow-ups (not blocking, ordered by leverage):**
- **Donor clean-Adam 10-epoch v14** via `tools/contribute_v14_training.py` — the explicit successor experiment per paper §5.12. Expected to push below 202 on bigger hardware running a single clean optimizer for the full 10 epochs (rather than a fresh-Adam resume). Published at loka.emmaleonhart.com/contribute/. Passive: waits for a contributor.
- **Clean v12 retrain** — the epoch-4 best 226.86 was lost to shared-GPU contention; a clean run would land ~225. Needs GPU.
- **Propgen test (Q42 seed) on v11–v14** — deferred since v11 due to GPU fragility during shared use. Paper §5.12 already notes the v14 propgen is a contributor-path follow-up. Needs GPU.
- **Engine bug #2 root-cause fix in `loka-sparql`** — RDF-star annotation rows project literal values into the predicate slot on ~1% of rows; currently masked at preprocess (`tools/preprocess_streaming.py` drops rows where subject or object is a property IRI). Code-only work, no GPU dependency — the only follow-up actionable while training is paused.

---

## 🚨 Old queue (above the pivot): Top of queue — ordered, do these in this sequence

The user reports persistent Windows instability "ever since [I] got started on this project," currently off the affected machine. Repo evidence (`training/logs/loka-restart.log`, DEVLOG 2026-05-12 23:52 UTC) shows a Win32 `ERROR_NO_SYSTEM_RESOURCES` panic — kernel-level resource exhaustion that destabilises the whole OS, not just Loka. Until diagnostics rule that out, treat the box as fragile.

### 1. Run the diagnostic triage on the affected box — DONE 2026-05-13

Evidence collected into `planning/system-instability-evidence-2026-05-13.md`. Verdict written to `planning/system-instability-verdict-2026-05-13.md`.

**Verdict, one paragraph:** the OS-freeze incidents are H6 (thermal / firmware-level shutdown on a thermally-constrained laptop), with H3 (GPU TDR fragility + IOMMU dGPU faults) as a co-factor. **It is a laptop**, hostname `laptop-qe4jv37b`, Ryzen 7 8845HS + Radeon 780M iGPU + RTX 4070 **Laptop** dGPU (not the desktop 4070 the previous Claude conversation assumed). Evidence: **10 Kernel-Power 41 events Mar 27 → today, zero BSODs, zero minidumps, zero WHEA, zero nvlddmkm errors** — the OS is not the layer detecting the failure; it's the firmware/EC layer. The 15-min cool-down recovery in `chats/system-instability.md` is the canonical thermal saturation signature. H1 (sled kernel pool) is real but separate — it owns the *logged* sled panic in DEVLOG, NOT the OS freezes.

### 2. Process the system-instability chat — DONE 2026-05-13

Extracted from `chats/Computer freezing during AI training run - Claude.html` → `chats/system-instability.md`. Key new evidence folded into `planning/system-instability-diagnosis.md`:

- **15-minute cool-down recovery pattern** — soft reboot didn't work, fast cold boot didn't work, leaving the box off for 15 min did. This is *not* a kernel-software-state signature. Hypothesis ranking revised: H3 (GPU TDR-stuck firmware) and H6 (thermal/PSU) promoted to co-#1 for the chat-described incident; H1 (sled kernel pool) now best explains the *logged* sled panic in DEVLOG but probably not the OS freeze.
- **Hardware confirmed**: Windows + RTX 4070 (12 GB VRAM) + CPU "comparable currentness to the 4070".
- **User's own attribution**: preprocessing pipeline. Aligns with H2 as the most likely *contributor* during the gradual-slowdown phase preceding the black screen.
- **Likely two incidents conflated**: the DEVLOG sled panic vs the chat black-screen are plausibly different failure modes. Diagnosis doc now treats them as separate.
- New diagnostic block added for H3-specific checks (Win32_ReliabilityRecords display events, driver branch Studio-vs-Game-Ready, TDR registry settings).

The extractor (`tools/extract_claude_chat.py`) had a duplication bug — each Claude turn was appending all subsequent Claude turns. Fixed in this commit; output dropped from 64 KB to 19.5 KB. Existing committed chats (`ai-bubble.md`, `world-models.md`) were not regenerated since they're already in git and the user has been working from them.

### 3. Preprocess the corpus into a clean "normalized-wikidata" dataset (NEW direction after the verdict)

The verdict made it clear that a 50M-triple raw-Wikidata training run on this laptop is thermally marginal. The fix is not just "cool the laptop better" — it's "stop training on the messy original corpus." Most of the triples in raw Wikidata aren't useful for the world-model objective anyway (external identifiers, malformed dates, opaque QID references with no labels). A normalization pass produces a **smaller, cleaner, more text-like dataset** that:
- trains faster (smaller corpus → less wall-clock GPU time → less thermal pressure)
- generalises better (no opaque QIDs littering the input)
- is genuinely useful to others doing similar work — worth a standalone HF push

**Plan, ordered:**

1. **Audit existing preprocess.** ~~DONE 2026-05-13.~~ `training/preprocess.py` already does label substitution (English), reserved-namespace stripping, noise-datatype exclusion via `training/wikidata_excluded_predicates.json`, Wikidata-API PID fallback (cached at `training/property_label_cache.json`), and time/quantity normalization. The big problem is **it loads all triples into one Python list before processing** — that's the 21 GB RAM bloat the chat described. `tools/wikidata_hf_import.py` (stage 1) is already correctly streaming. Stage 2+3 is where the rewrite is needed.

2. **Stage 1 — lean import (keep current).** Wikidata triples land in SutraDB / `loka-data-cron-c1` with **QIDs intact**. Streaming. No in-memory accumulation. Existing `wikidata_hf_import.py` mostly fits the shape; verify it's actually streaming, not building a giant in-memory map.

3. **Stage 2 — QID/PID label resolver (built 2026-05-13).** `tools/preprocess_streaming.py` is the memory-flat replacement for `training/preprocess.py`. Two-pass design:
   - **Pass 1 ("scan")**: stream every triple from Loka, extract English `rdfs:label` rows into the SQLite cache, build the set of unique QID/PID IRIs that appear in the corpus. Memory bounded by entity count (1–10% of triple count), not triple count.
   - **Wikidata API step (optional, `--fetch-missing-from-wikidata`)**: for QIDs/PIDs the corpus mentions but doesn't have a label for, query the public Wikidata SPARQL endpoint in batches of 50, write results into the SQLite cache. Polite rate-limit (`--api-rate-seconds`).
   - **Pass 2 ("emit")**: stream every triple again, resolve via SQLite, apply the same normalization rules as the canonical `preprocess.py` (which is imported, not duplicated), write the flat tab-separated corpus.
   - Cache lives at `training/data/wikidata_labels.sqlite` and **persists across runs**. The same script can be re-run incrementally as the corpus grows.
   - **Running 2026-05-13 ~15:50 PT (restarted after fix)** — `loka serve` PID 26992 on port 3030 holds 50,002,600 triples. Background task `b213git6e` is the running preprocess. Log: `training/logs/preprocess_streaming.log`. Output: `training/data/triples_normalized.txt`. Throughput ~12 s per 100k-row page → ~100 min per pass × 2 passes = ~3.5 h total wall-clock.
   - **Critical bug discovered + fixed mid-run (commit `78e1e7e`)**: corpus-derived property labels are systematically wrong — every PID we'd resolved in the first 87 pages had the wrong label (P20 → "Belgium" not "place of death", P1412 → "English" not "languages spoken", etc.). Root cause: RDF-star annotation rows on triples being mis-surfaced by Loka's SPARQL executor (engine bug #2 again) — the property IRI gets keyed against the inner triple's object value. Entity labels are unaffected. Fix: preload `training/property_label_cache.json` as `curated`-source labels, skip corpus rdfs:label rows whose subject is a property, drop pass-2 rows where subject OR object is a property IRI. Smoke test: 2 → 744 clean rows on the same 50k-row slice. **Without this fix the normalized corpus would have been useless** — re-train was the right call.
   - Wikidata API step deferred until we see the unresolved count from this run (in-corpus rdfs:label rows + the 7,312 curated property labels should cover the majority).
   - Smoke test against the small `sutra-data` store confirmed the pipeline structurally works (commits `c516276` + `308ae90` for engine-bug-#2 + URL-object guards).

4. **Stage 3 — normalization pass (NEW Python script).**
   - Reads the corpus + the label DB.
   - Output is a text-like training format where:
     - QIDs/PIDs are replaced with their English labels (from the side table), with the original QID kept in a parallel column for traceability.
     - External identifiers (`P227` GND, `P214` VIAF, etc.) are stripped — they have no signal for the world-model objective.
     - URLs are stripped or replaced with a single `<URL>` sentinel.
     - Dates are normalized to ISO-8601 (the v7 datatype filter already does some of this).
     - Datatypes are flattened to `"value"` form — no `"value"^^xsd:integer` clutter.
   - The originals stay in SutraDB unchanged — normalization is a derived view, not a destructive rewrite.

5. **Stage 4 — push `normalized-wikidata` to Hugging Face as a standalone dataset.**
   - New HF repo: `EmmaLeonhart/normalized-wikidata` (or similar — confirm naming).
   - README explains: source = Wikidata `20YY-MM-DD` dump, normalization choices, what was stripped, why. Useful to other people training world models on Wikidata.
   - Reuse `tools/hf_snapshot.py` patterns.

6. **Stage 5 — retry v11 on the normalized corpus.** The corpus will be significantly smaller (probably 30–50% of the raw triple count after stripping external identifiers and noise). Combined with the verdict's mitigations (batch 32, serialised workload, TdrDelay=10s, possibly cloud GPU), this is the right shape for actually shipping v11.

### Old item — kept for reference: start the v11 training cycle (BLOCKED on mitigations 1–3 below; revised after the verdict)

The verdict reframes this step. The original plan (`4 epochs × 52 min`, batch 64, on this laptop) was designed against assumed desktop-4070 thermal headroom. The actual hardware cannot sustain that without firmware-level shutdown. Either the workload changes or it moves to different hardware. **Required before resuming:**

1. **External cooling.** Cooling pad / elevated rear / ambient temperature reduced. Verify via 30-min HWiNFO64 stress test: GPU hotspot < 90 °C, CPU package < 95 °C sustained.
2. **Workload serialisation.** No `loka serve` + ingest + training concurrently. Strict sequencing (per `planning/system-instability-diagnosis.md` mitigation M1).
3. **Smaller training batch.** `--batch-size 32` instead of 64. ~2× wall-clock cost; halves driver pool pressure.
4. **One-time setting changes:** raise `TdrDelay` registry value to 10 s (was empty → default 2 s); switch NVIDIA driver from Game Ready 32.0.15.8183 to the **Studio** branch. Both require a reboot.
5. **Check BIOS for an update** — HAL `ACPI TAD failed` warnings + IOMMU faults on the dGPU suggest firmware-level fixes may be pending from the vendor.

**Strongly preferred alternative**: move the training run off this laptop entirely. Lambda Labs / RunPod / Vast.ai for the 3.5-h training pass costs < $5/cycle, eliminates the thermal risk, and the corpus + tokenizer already live on Hugging Face. The training-on-laptop path was always going to be marginal; the verdict makes that explicit.

Once mitigations are in place:
- Use the v11 restart protocol below (re-enable + fire `loka-v11-kickoff`, or run `python tools/v11_kickoff.py` directly).
- Watch live: `tail -F training/logs/v11_kickoff.log` AND HWiNFO64 temps in parallel. Stop at first thermal warning OR first kernel-pool warning.
- Completion marker: last line of `training/logs/v11_kickoff.log` is `=== v11 kickoff DONE — v11 shipped ===` and `git log -1` shows the v11 commit.
- **Do not re-enable `training_cron.py` or `post_eval_cron.py`.** One cycle, by hand, first.

### 4. Set up a 5 h follow-up cron: analyse training + update paper

**Doesn't exist yet** — closest analogue is `tools/post_eval_cron.py`, which fires every 6 h for 48 h and runs the full preprocess→train→propgen→DEVLOG→paper→push pipeline. That's a different shape: a continuous-loop training driver, not a one-shot post-training analyser.

Plan for the new tool (call it `tools/post_v11_analyse.py` or similar):

- Fires once, 5 h after v11 training completes (i.e. scheduled with `Register-ScheduledTask` against the v11 completion timestamp + 5 h, OR a `--first-delay 18000` flag on the existing cron pattern).
- Reads `training/logs/v11_train.log` + `training/data/test_propgen_Q42_v11.nt` + `_meta.json`.
- Diffs against v10's numbers (final ppl, catalog vs semantic emission split, asymmetric-drop count) and writes an analysis paragraph.
- Appends a paper §5.9 update + DEVLOG entry with the comparison.
- Commits + pushes (paper push triggers `papers-ci.yml` → clawRxiv submission).
- **Single-shot** — no looping. The recurring `post_eval_cron.py` is what we have for the multi-firing pattern, and it's intentionally on hold.

Defer the implementation until step 3 has actually produced a v11 checkpoint — premature scaffolding is wasted work if v11's shape comes out different from v10's.

---

## Reference: v11 cycle restart protocol (for step 3 above)

**Date this was written:** 2026-05-13, ~11:30 PT, about 30 min before the v11 training cycle fires.

**Where we are:** the bigger-corpus ingest is **done**. `loka-data-cron-c1/` holds **50,000,521 triples**. v11 has not started training yet. A Windows scheduled task `loka-v11-kickoff` is registered to fire at noon PT 2026-05-13 — **it was Disabled before the reboot** so it wouldn't fire mid-update and leave a partially-trained checkpoint. **Re-enable or fire it manually when you're ready** (see step 4 below). The user rebooted for a Windows update; this section is so a fresh session can pick up cleanly.

### What survives the reboot vs what doesn't

| Item | Survives reboot? | Notes |
|---|---|---|
| `loka-data-cron-c1/` data dir (5+ GB sled state) | ✅ | The corpus. Don't touch it. |
| Scheduled task `loka-v11-kickoff` | ✅ | OS-level (`schtasks`), persists across reboot. Trigger: 2026-05-13 12:00:00 PT one-shot. |
| `tools/v11_kickoff.py` | ✅ | Committed to git. |
| `target/release/loka.exe` | ✅ | Pre-built binary. |
| `loka serve` process (port 3030) | ❌ | Killed by reboot. **Must be restarted before `v11_kickoff.py` runs.** |
| Background HF-importer process | N/A | Already exited cleanly at the 50M cap. |

### Restart steps (do these in order)

1. **Confirm the scheduled task is still registered.** It was Disabled pre-reboot — that's expected.
   ```powershell
   Get-ScheduledTask -TaskName 'loka-v11-kickoff' | Select State, @{n='NextRun';e={(Get-ScheduledTaskInfo $_).NextRunTime}}
   ```
   Expected: `State=Disabled, NextRun=2026-05-13 12:00:00`. If the task is **missing entirely** (update wiped it), re-register — full PowerShell block is in commit `c7f5d68`'s diff (search history for "Register-ScheduledTask -TaskName 'loka-v11-kickoff'"). Don't enable yet; do that in step 4 after Loka is back up.

2. **Restart Loka against the data dir.** Run this in a separate PowerShell window or as a background process:
   ```powershell
   target\release\loka.exe serve --data-dir loka-data-cron-c1 --port 3030
   ```
   Sled WAL replay on the 5 GB store takes ~30–60 s; `/health` returns 200 only after it finishes.

3. **Verify Loka is up and holds the corpus.** Both must be true before the scheduled task fires:
   ```powershell
   Invoke-WebRequest http://localhost:3030/health -TimeoutSec 60
   # 200 ok

   $body = "SELECT (COUNT(*) AS ?n) WHERE { ?s ?p ?o }"
   Invoke-WebRequest http://localhost:3030/sparql -Method POST -Body $body `
     -ContentType "application/sparql-query" -TimeoutSec 1800
   # Expected: 50,000,521
   ```
   The count query takes 5–10 min on 50M triples — that's normal, not a wedge.

4. **Re-enable and either wait or fire manually.** Task is currently Disabled (see step 1).
   ```powershell
   Enable-ScheduledTask -TaskName 'loka-v11-kickoff'
   ```
   Then either:
   - **Wait** for noon (if you re-enable before 12:00 PT, it'll auto-fire at the registered trigger time and run `cmd /c python tools\v11_kickoff.py > training\logs\v11_kickoff_task.log 2>&1`).
   - **Fire now** (recommended if it's already past noon, or if you just want to go): `Start-ScheduledTask -TaskName 'loka-v11-kickoff'` (uses the registered action — logs land in `training\logs\v11_kickoff_task.log`), or run directly without the scheduler: `python tools\v11_kickoff.py`.

### What v11_kickoff.py does

(Self-contained — re-uses `training_cron.py`'s helpers via direct import.)

1. Stop any running `wikidata_hf_import.py` (no-op now — it exited at 50M).
2. Verify `/health` and triple count ≥ 33M. Aborts with non-zero exit if either fails.
3. Preprocess: SPARQL-fetch the corpus, paged at 100k rows/page, write `training/data/triples_v11.txt`. ~30–120 min at 50M scale.
4. Train v11, **4 epochs**, batch 64, ~52 min/epoch on the 4070 → ~3.5 h total. Per-epoch perplexity in `training/logs/v11_train.log`.
5. Ship: propgen test (Q42, conf 0.25, 30 sources) → DEVLOG entry → MODEL.json bump → `tools/hf_snapshot.py` push as `v11` tag on `EmmaLeonhart/loka` → `git add` paper/DEVLOG/MODEL.json/train.log → `git commit -m "v11 cron cycle: trained, tested, shipped to HF"` → `git push origin main`.

Completion marker (success): the last line of `training/logs/v11_kickoff.log` is
```
=== v11 kickoff DONE — v11 shipped ===
```
and `git log -1` shows the v11 commit.

### What to do if something is wrong

- **`v11_kickoff_task.log` empty after noon** → scheduled task didn't run. Check `Get-ScheduledTaskInfo loka-v11-kickoff` for `LastTaskResult`. Common cause: user wasn't logged in; right-click the task in Task Scheduler → Properties → Security options → "Run whether user is logged on or not".
- **kickoff fails at step 2 (triple count too low)** → Loka didn't reopen the data dir correctly. Stop the loka process, re-run step 2 above, watch `/health` until 200.
- **kickoff fails at step 4 (training)** → see `training/logs/v11_train.log` for the actual error. Most likely cause: GPU out of memory. Retry with smaller batch (edit `train_version` call site, `--batch-size 32`).
- **kickoff fails at step 5 (ship)** → checkpoint exists; just re-run `python tools/v11_kickoff.py` skipping training is not currently supported. Manual recovery: call `tc.ship(11)` from a Python REPL.

---

## Active

In strategic order. Top item is the current focus.

1. **Engine bug #1: sled flusher panic at multi-GB scale.** ~~Surfaced~~ Surfaced on 2026-05-12 as a hard panic (Win32 `ERROR_NO_SYSTEM_RESOURCES` at 32.88 M triples / 5 GB). Probable fix shipped in `c36760b`: explicit `sled::Config` with 256 MB cache, 2 s flush, `Mode::HighThroughput`. **Reopen-in-place verified 2026-05-13 01:00 UTC**: WAL replay recovered 32,877,248 triples (1,150 more than `big-pull.log` last recorded). The fix is verified for the reopen case; the residual question is whether it also holds against a fresh sustained ingest past 32.88 M. If a re-test ingest panics at the next plateau, escalate to RocksDB migration (sled 0.34 unmaintained since 2021).

2. ✅ **DONE — Engine bug #2 fully closed 2026-05-16** (query-layer invariant + ingest-side quoted-triple reverse index + ffi/mcp parser fix; Bug A + Bug B both fixed). See the BARREL section above + DEVLOG 2026-05-16.

3. **Bigger corpus, paused at 32.88 M triples.** `loka-data-cron-c1/` has 32,877,248 triples on disk; option B reopen verified 2026-05-13 01:00 UTC. Two remaining decisions:
   - **Resume ingest** from row 318,582 to push toward the 50 M-triple target — tests whether the sled tuning also holds under fresh sustained writes (not just reopen). Worst case: another panic at the next plateau, falls through to RocksDB migration.
   - **Stop here and use as v11's training source** — already 17× the v10 corpus (94k triples). Skips the ingest risk; v11 trains immediately.

4. **Live `--post` end-to-end test of generative citation.** Attempted with `--max-subjects 30 --post`; script hung 7 min with no output, indicating engine bug #1 triggered during the POST phase. Re-attempt at lower scope after #1's fix is verified by option B.

5. ⚰️ **RETIRED — Fine-tuning track**. Scaffold shipped (`training/finetune/`, `df8fb43`); QLoRA epochs 1-4 ran 2026-05-16 → 2026-05-18; epoch-4 adapter pushed to HF. Decisive probe 2026-05-18 (`tools/_ft_probe3.py`) showed the masked-SFT lobotomised the base model — strictly worse than untouched Qwen. **DO NOT RESUME.** Adapter kept on HF as negative-result reference only. Replaced by the base + BFS+embedding retrieval pivot (see top of file).

6. **Submit paper revision v2 to clawRxiv post 2378.** Local edits done (see Done section); needs `POST /api/posts/2378/revise` to actually publish the v2. Review-flagged remaining concerns (mode collapse on connectors, anecdotal qualitative sample, "neuro-symbolic" framing) are partially addressed by the new §3.2 framing and §6.4 future-work block but would benefit from a v3 once a bigger corpus or entity-decoder lands.

7. ✅ **DONE — Repo rebrand SutraDB → Loka** (commit `366e056`, 2026-05-10). All crates renamed (sutra-* → loka-*), SDKs renamed, FFI symbols renamed, IRI namespace migrated, env vars LOKA_*, pages/theory/sutradb → pages/theory/loka, GitHub repo is `EmmaLeonhart/Loka`. The orphaned `Loka → Loka` block in TODO.md was deleted 2026-05-20.

8. ✅ **DONE — World-model cascade-retraction shipped end-to-end 2026-05-16.** All 5 phases:
   reverse index → pure `retract_set` → `POST /retract/preview` → `POST /retract` +
   `retract_node` MCP tool → Loka Studio "Retract (cascade)" action. Provenance-bounded,
   cycle-safe, real→real not a dependency; destructive path opt-in everywhere. Spec:
   `planning/cascade-retraction.md`; full arc in DEVLOG 2026-05-16. Docs synced across
   paper §6.1/§6.3, website (history + /loka/ + homepage card), README, status.md,
   CLAUDE.md, architecture.md.

---

## Done (2026-05-10 session)

- ✓ v6 trained on BPE tokenizer (queue.md #5 from prior session). 5 epochs, final ppl 194.98. Wall time ~2.5h with one long thermal/sleep stall in epoch 4.
- ✓ v6 pushed to HF as `EmmaLeonhart/loka` tag `v6-bpe` (the `v6` tag had been created by an earlier run before v6.pt existed). `MODEL.json` bumped to v6 with BPE tokenizer pinned alongside vocab.
- ✓ `tools/hf_snapshot.py` taught about v6.pt + BPE files (`tokenizer_bpe.json`, `vocab_bpe.json`).
- ✓ v5 vs v6 qualitative comparison on unicode-name subjects (`tools/compare_v5_v6.py` + DEVLOG entry). Findings: v6 preserves accents (v5 strips them at the regex stage), pulls v5's no-prediction holes off the floor for identifier-shaped predicates, but a per-token-floor decoder bug truncates BPE date emissions to just `"+"`. Decoder fix is the next quality lever.
- ✓ Paper v2 local edits per Gemini 3 Flash review (post 2378). Three changes: (a) Loka v0.4.0 release URL cited prominently in the masthead + References, (b) §3.2 lead paragraph acknowledges heuristic citation as a v0 design choice with forward-pointer to §6.3, (c) new §6.4 "What we are *not* claiming, and why we do not report MRR / Hits@k" treating the metric gap as blocked future work gated on the entity-decoder. POST to clawRxiv held — that's queue #6 now (external publish action, separate confirmation needed).

## Done (2026-05-09 session)

- ✓ Paper draft + clawRxiv workflow stack (`cb61c94`).
- ✓ First clawRxiv submission (post 2378, paperId 2605.02378).
- ✓ Loka HF snapshots v3 / v4 / v5 uploaded with tags.
- ✓ DEVLOG comprehensive history.
- ✓ `pages/loka/` deep dive, `pages/history/` narrative.
- ✓ Homepage reframe — Loka world-model lead, HF + /loka + /history nav (`7871ce7`).
- ✓ HF link prominent in paper (`7871ce7`).
- ✓ clawRxiv loop verified end-to-end — Gemini 3 Flash review v1 committed (`d50fad2`).
- ✓ BPE tokenizer (`8252f13`) — `Saint-Léger` → `['Saint', '-', 'Lé', 'ger']`.
- ✓ BPE wired into `train.py` + `infer_with_citations.py` via `--bpe-tokenizer` flag (`021fd4c`).
- ✓ Pinned-model loader: `MODEL.json` + `training/loader.py`; `infer_with_citations.py` defaults pull v5 from HF on first run. README now has a "World Model (Loka)" clone-and-run section.
- ✓ Auto-sync cron `fc054cb5` — pulls, rebases, pushes, chains successor.

---

## Reference

- **`TODO.md`** — longer-horizon work (SDK publishing, Maven Central, Cypher/GQL wrappers, premium-tier, ontochronology phases-5+). Items migrate to here when ready.
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
