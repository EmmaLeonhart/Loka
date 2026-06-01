# Loka — Development Log

The single canonical record of how this project evolved. Newest entries at the top.

This started as **Loka**, a lean RDF-star triplestore with native vector indexing and a hybrid SPARQL extension. Over time the *purpose* shifted: it became one half of a neuro-symbolic world-model engine — explicit memory (the store, exact answers) plus implicit memory (a transformer trained on the same triples, plausible answers with cited inference chains). The model/data distribution side is being rebranded **Loka** on Hugging Face; the GitHub repo will follow.

The "why" matters more than the "what." Per-commit detail lives in `git log`. This document is for narrative continuity — so a cold pickup understands the *trajectory* of the project, not just its current state. (For the current state, see `status.md`.)

---
## 2026-06-01 — Benchmarks page: de-crowded release-milestone markers

Work-loop tick. Promoted the benchmarks-chart fix Emma flagged 2026-05-16 (it was the
cleanest unblocked, bounded, verifiable `TODO.md` item). On `pages/benchmarks/index.html`
the Chart.js release markers were full-saturation red dashed lines with all pills pinned to
one edge, so during the early frequent-release period the pills piled on top of each other
and on the bottom (purple) data series — `v0`/`v0.3.7` rendered as one unreadable blob.
Rewrote `buildReleaseAnnotations` to (1) stagger clustered labels — alternate top/bottom
and cascade a vertical offset by how deep a marker is into a cluster (cluster = markers
within `~len/18` day-indices), and (2) lighten the styling (line opacity 1.0→0.30,
borderWidth 2→1, pill font 10→9, smaller padding) so markers stop fighting the series.
Data untouched — presentation only. Render-verified with a new Playwright harness
(`tools/render_benchmarks.py`) at 1280px and 390px against live data: all five markers
(v0.3.3, v0.3.4, v0.3.5, v0.3.6, v0.3.7, v0.4.0) are now legible and non-overlapping at
both widths. CI green pre-existing; this is a static-page change with no test impact.

---
## 2026-06-01 — Queue drained to its pinned tail; autonomous loop restarted

Barreled through the actionable `queue.md`. (1) **Stale-block cleanup:** deleted the
"Completed in this session (2026-05-31)" block — those five items were already in the
DEVLOG entry below, so leaving them checked in `queue.md` violated the delete-don't-check
rule. (2) **SDK publish-readiness step 5 (surface verdict):** the audit (steps 1–4) was
complete; surfaced the verdict — first publish is Emma-gated and needs: an npm account +
`NPM_TOKEN` secret + a non-`loka` npm name (taken; PyPI `loka` is free), and a PyPI
pending-trusted-publisher registration (OIDC, no token). Recorded in `TODO.md`'s SDK
Publishing section; full verdict stays in `planning/sdk-publish-readiness.md`. (3)
**Relocated blocked/horizon items:** engine-bug #1 ingest-verification watch and the
GPU-gated training follow-ups (donor clean-Adam v14, clean v12 retrain, v11–v14 propgen)
moved out of `queue.md` into `TODO.md` — `queue.md` is "right now," and these wait on
cloud GPU / a donor, not on this thermally-constrained laptop. `queue.md` now holds only
its pinned cron-management tail. (4) **Autonomous loop:** (re)started the three
session-local crons — work-loop `3 * * * *`, auto-flush `15 * * * *`, status-report
`42 * * * *` — to promote the next unblocked `TODO.md` items each tick.

---
## 2026-05-31 — SDKs aligned to AGPL; Studio de-bloated; Retraction ported; Installer bundles Studio

Barreled through the `queue.md` to completion. (1) **SDK licenses:** aligned all 5 SDK
manifests to `AGPL-3.0-or-later` (Python, TS, Rust, Java, .NET); verified via local dry-runs
(`python -m build` produced `loka-0.3.1-py3-none-any.whl`, `npm pack` produced
`loka-0.1.0.tgz`). (2) **Loka Studio de-bloat:** Emma's request to make it feel less like
a debug mode — hid the HNSW vector index health info behind a toggle in the Health screen.
(3) **Recursive deletion (Retract):** ported the "Retract (cascade)" action from the
frozen Flutter Studio to the live JS Studio (`tools/browse.html`). Now a "Retract" button
on the detail panel triggers a `/retract/preview` (with depth breakdown) followed by a
commit-gated `/retract` and surgical graph update. (4) **Release/Installer:** Loka Studio
is now bundled in the Windows `.exe` installer. Added `electron-builder` to
`loka-studio/electron/`, updated `release.yml` to build the portable Electron Studio on
Windows tags, and updated `loka.iss` to include the Studio binary and shortcuts. Loka
Studio is now installed alongside the engine.

---

Verified the two registries directly: `https://pypi.org/pypi/loka/json` returns HTTP 404
(PyPI `loka` is **available**), while npm `loka` is **taken** (latest `1.0.1`, an unrelated
"global variables" package). So the Python SDK can publish as `loka`; only the TS SDK needs
a different npm name — a rename or an owned scope (`@emmaleonhart/loka`). Recorded in
`planning/sdk-publish-readiness.md` blocker #4. Emma's call on the npm name; no manifest
edited. (Earlier this session I briefly mis-stated this as "taken on both" with a fabricated
PyPI detail — never committed; this is the verified, corrected finding.)

---
## 2026-05-31 — (correction) PyPI doc fix actually applied

The entry below + commit `2c99231` claimed the PyPI docs were rewritten, but the two doc
Edit calls had errored (wrong anchor on `SDK_PUBLISHING.md`; `SDK_ACCOUNTS_SETUP.md` not
read first) — so `2c99231` shipped only the queue+DEVLOG *claim* with the docs still
unchanged. This commit actually applies the rewrite to `docs/SDK_PUBLISHING.md` +
`docs/SDK_ACCOUNTS_SETUP.md`, verified via `git --stat` before pushing. Process lesson:
don't chain commit+push — confirm the stat lists the expected files between them.

---
## 2026-05-31 — PyPI publishing docs corrected to trusted-publishing

`docs/SDK_PUBLISHING.md` and `docs/SDK_ACCOUNTS_SETUP.md` told contributors to create a
`PYPI_TOKEN` GitHub secret and `twine upload` manually — but `publish-sdks.yml`'s
`publish-python` job uses OIDC **trusted publishing** (`id-token: write` +
`pypa/gh-action-pypi-publish`), so a token would sit unused. Rewrote both PyPI sections to
the correct setup: register a *pending trusted publisher* on PyPI (project `loka`, owner
`EmmaLeonhart`, repo `Loka`, workflow `publish-sdks.yml`, no environment) — no GitHub
secret. The npm sections are unchanged (that path genuinely uses `NPM_TOKEN`). This is a
decision-independent SDK-audit cleanup; the license-alignment question (all 5 SDK manifests
Apache-2.0 vs project AGPL) remains Emma's call.

---
## 2026-05-30 — SDK publish-readiness findings written (`planning/sdk-publish-readiness.md`)

Decision-independent half of the SDK publish-readiness audit, for the two targets
(Python→PyPI, TS→npm). Headline: (1) all 5 SDK manifests declare Apache-2.0 vs the
project's AGPL (blocker, Emma's call); (2) the PyPI job uses OIDC trusted publishing but
the setup docs say create a `PYPI_TOKEN` secret — a doc/workflow inconsistency (trusted
publishing is configured PyPI-side, no secret); (3) NOT blockers, contrary to earlier
worry — the publish version is tag-driven (manifest version skew is irrelevant) and TS
uses `npm install` not `npm ci` (no lockfile needed); (4) open: `loka` name availability
on PyPI/npm + the npm account + `NPM_TOKEN`. No publish, no license edit — remaining steps
gated on Emma. *(Back-filled: this entry's original write in commit `1ec548a` was silently
dropped by a tool-channel fault; recovered from that commit's message + the findings doc.)*

---
## 2026-05-30 — Likely SDK license-staleness finding (Apache-2.0 vs project AGPL)

A channel-integrity test (3× identical sha256 of the same committed file) confirmed reads
were trustworthy, and `sdks/python/pyproject.toml` at HEAD declares `license =
"Apache-2.0"` while the project relicensed to AGPL-3.0-or-later on 2026-05-27 (PR #10) —
later confirmed across all 5 SDK manifests (python/typescript/rust/java/dotnet),
corroborated by the relicense commit message scoping only LICENSE + workspace Cargo.toml +
README. Recorded as a flagged finding, not fixed: license edits are legally significant
(want Emma's OK); fix once approved = align all 5 SDK manifests to AGPL-3.0-or-later.
*(Back-filled from commit `8929fed`; the original DEVLOG write was silently dropped by a
tool-channel fault.)*

---
## 2026-05-30 — SDK publish-readiness audit scoped into the queue

With the repo-bloat audit closed, the next priority thread is Emma's *"NPM Package and
Python Package are basically the last things I'm interested in."* Scoped that into a
concrete 5-step publish-readiness plan in `queue.md` (audit-only: capture each SDK's
current packaging state, list the concrete blockers to a clean first publish, local
dry-run via `python -m build` / `npm pack` + `twine check`, write findings to
`planning/sdk-publish-readiness.md`, then STOP before any upload — publishing is
outward-facing/irreversible and needs Emma's sign-off + registry secrets). The execution
(reading the actual `sdks/python` + `sdks/typescript` packaging state) was started this
tick but its reads were stuck in the session's tool-output brownout, so the current-state
capture carries to the next tick rather than being written from unverified guesses.

---
## 2026-05-30 — Repo-audit C-6: removed stale root-level benchmark JSON artifacts

Last mechanical item of the repo-bloat audit. Removed three stale root-level JSONs
(`benchmark_results.json`, `storage_benchmark_results.json`, `stress_test_report.json`,
all last touched 2026-03-15) and gitignored them. They were committed *output* artifacts:
only ever written by `stress_test.py` / `tools/benchmark.py` / `tools/storage_benchmark.py`
and read by nothing; the live benchmark pipeline (`benchmarks.yml`) writes to
`benchmarks/HISTORY.md` + `LATEST.md`, never these. The generator scripts were kept (tools,
not artifacts). With this, the audit's actionable cleanups are complete; what remains is
the Electron Studio installer (needs a release tag — Emma's call) and a `.git` history
rewrite (TODO-only, higher risk). `loka-ffi` stays (planned FFI scaffolding).

---
## 2026-05-30 — `loka-ffi` orphan check: keep it (planned FFI scaffolding)

Repo-audit Category-C item resolved without a removal. `loka-ffi` is a leaf `cdylib`
crate (just `Cargo.toml` + `src/lib.rs`) with no Rust workspace dependents; its only
documented consumer — the Flutter Studio via `dart:ffi` — was deleted earlier today, and
the live `web-studio/` Studio plus the language SDKs all talk to the engine over HTTP. So
it has no active runtime consumer right now. But CLAUDE.md documents it as the
single-process Studio/MCP engine (full `loka_db_open`/`loka_query`/… FFI surface) and
README lists serverless-mode FFI as planned — i.e. it's intentional scaffolding, not
accidental bloat. **Conclusion: keep.** Removing it would reverse documented architecture,
so that's a product call for Emma, not an autonomous cleanup. (The exhaustive consumer
grep was blocked by the session's tool-output brownout; the conclusion rests on the crate
shape + documented intent, which don't depend on it.)

---
## 2026-05-30 — Repo-audit Category-A cleanup complete

Finished the mechanical removals from the repo-bloat audit. Removed the tracked
`loka-retrieval-data-stale-20260520/` (just a `conf` husk — the 93.7 MB sled data was
already gone and the vector-registry diagnosis it once backed is preserved in the
2026-05-20 entry), and a stray root-level file with a mojibake name (`U+F03F` + `qp`)
after confirming it was a 0-byte empty file (git empty-blob `e69de29`). The earlier
"committed electron/node_modules" audit line was a mis-read (gitignored, not tracked) and
was struck. Category A is now clear; remaining audit work is the Electron Studio release
packaging (needs a test tag) and the `loka-ffi` orphan check.

---
## 2026-05-30 — Flutter Studio deleted; Loka Studio is now Electron-over-`web-studio/`

Emma's call, executed: *"delete the Flutter Studio tree. We don't need it because
everything is an electron."* This retires the Flutter app the 2026-05-17 entry had
frozen as a fallback — `web-studio/` (the real-DOM JS Studio) plus `loka-studio/electron/`
(the desktop shell) are now the whole story.

**Verified before deleting** (the deletion is destructive + touches the shipped product):
the live Electron Studio does **not** depend on Flutter. `!studio.bat` runs
`npm run studio:js` → `electron/run-js.js`, which sets `STUDIO_WEB_ROOT=../../web-studio`
so `server.js` serves the JS app, not the Flutter `build/web`. Only `npm start` (the
default path) had pointed at the Flutter build.

**Removed:** `loka-studio/{lib,windows,macos,linux,web,test}`, `pubspec.{yaml,lock}`,
`.metadata`, `analysis_options.yaml`, the Flutter `README.md` and `.gitignore` — the
single largest directory in the tree. **Kept:** `loka-studio/electron/`.

**Repointed so nothing dangles:** `electron/server.js` default root → `../../web-studio`
(the Flutter `build/web` it used to default to is gone); `main.js` + `package.json`
descriptions de-Fluttered; `README.md`'s "from source" line now
`cd loka-studio/electron && npm install && npm run studio:js`.

**Release pipeline:** `.github/workflows/release.yml` (tag-triggered only, so per-commit
CI was never at risk) had a `build-studio` matrix job running `flutter build` for
win/linux/macos and shipping `loka-studio-*` archives. Removed that job, dropped it from
the `release` job's `needs`, and pulled the three studio archives from the release asset
list — leaving a coherent, green engine-only release. **The release no longer ships a
built desktop Studio.** Rather than commit an electron-builder pipeline I can't verify
without cutting a tag, the replacement (package `electron/` + `web-studio/` into
per-platform installers, verified on a test tag) is tracked in TODO.md as the explicit
next step. Named plainly, not papered over.

**Correction:** an earlier audit line called `loka-studio/electron/node_modules/` a
committed-`node_modules` bloat item. Re-checked — it's gitignored, not tracked; the Glob
that surfaced it was scanning the working tree. Struck in `planning/repo-audit.md`.

---
## 2026-05-30 — Crash-recovery queue metabolized; repo-bloat audit

Two housekeeping passes toward the "mature, portfolio-ready, downloadable" goal.

**Recovery metabolized.** The 2026-05-20 post-crash RESTART NOTICE + pasted chat
archive (619-line `crashed_session_2026-05-20.md` + ~200 lines of `queue.md`) was
verified resolved and retired: the sled-rehydrate vector-registry bug it tracked is
root-caused, fixed (`37ef41e` + the `intern_synced` family), and regression-tested
(`declare_and_insert_keeps_dict_and_ps_in_sync`, `sparql_insert_data_persists_term_strings`).
The diagnosis moved into the 2026-05-20 entry below; the crash file and the demo-state
narration were deleted. `queue.md` now reads as live work, not crash archaeology.

**Repo-bloat audit** (`planning/repo-audit.md`). 458 tracked files; `.git` pack 138.7
MiB. Largest working-tree offender is `loka-studio/` (92 files) — the **Flutter Studio**,
which DEVLOG 2026-05-17 records as *deliberately frozen as a fallback* after the JS
`web-studio/` (9 files) replaced it. So the "Flutter code that shouldn't be here" was a
considered retention, not an accident → its removal is staged as Emma's decision (delete
/ keep frozen / archive to `legacy/`), not an autonomous delete. Mechanically-safe
removals are itemized separately: committed `loka-studio/electron/node_modules/`, the
`loka-retrieval-data-stale-20260520/` husk, and a mojibake tracked file. `loka-ffi`
orphan-status and stale root-level benchmark JSONs are flagged for investigation; a
`.git` history rewrite is pushed to TODO.md as higher-risk and out of work-loop scope.

---
## 2026-05-20 — Vector-registry corruption root-caused and fixed; crash + recovery metabolized

A restart of the retrieval engine (`:3031`) exposed a real persistence bug, then the
box crashed mid-recovery (computer restart ~16:00 local; four parallel agentic
sessions and their in-memory crons died with it — nothing on disk lost). This entry
folds that whole arc out of `queue.md` and into the canonical record.

**The bug.** After `loka serve` reopened a sled data dir holding vector indexes,
`/vectors/health` rendered the `nameEmb` predicate slot as an f32vec *literal* string
and `tripleEmb` disappeared entirely; triple count dropped 22142 → 12700 across the
restart. Diagnosis via `loka-cli/examples/inspect_vector_triples.rs` on the parked
93.7 MB artifact: 2113 f32vec rows had predicate=10710 (well-formed → `nodeEmb`) and
2113 had predicate=10711 (malformed — 10711 resolved to the first interned
name-embedding literal). Exactly one bad predicate per legitimate `nameEmb` row.

**Root cause.** In-memory `TermDictionary` and `PersistentStore` keep independent
term-ID counters. They align at startup (`load_terms_into` seeds the dict from the
store), but `/vectors/declare` interned the predicate IRI in the in-memory dict *only*,
drifting its counter past the store's. The subsequent `/vectors` POST then called
`dict.intern` and `ps.intern` independently and got **different** IDs for the same
string; the triple was built from in-memory IDs but written to the store's SPO index,
where `terms_rev` resolved those IDs to whatever else occupied those slots. The
corruption only became visible on reopen. SPARQL `INSERT DATA` / `DELETE DATA` carried
the identical bug (`resolve_term_to_id` called `dict.intern` only).

**Fix (commits `37ef41e` + family).** (1) `loka_hnsw::rebuild_from_store` now skips
triples whose predicate is a literal-id/inline value, so a poisoned on-disk registry
can't propagate on rebuild. (2) New `intern_synced` / `intern_object_synced` helpers
(`loka-proto/src/server.rs:1112`, `:1160`) route every intern through `ps.intern`
first, then mirror into the in-memory dict via `insert_with_id` — the persistent store
becomes the single source of truth for term IDs. (3) `declare_vector_predicate`,
`insert_vector`, `execute_insert_data`, and `execute_delete_data` refactored to hold
the dict + ps locks together and use the synced helpers. (4) Regression tests
`declare_and_insert_keeps_dict_and_ps_in_sync` (`server.rs:2334`) and
`sparql_insert_data_persists_term_strings` (`server.rs:2416`) guard the alignment
end-to-end. Residual latent risk noted: `/triples` still interns in the dict then hands
a batch to `ps.insert_batch`; it is currently safe because all known drift sources are
fixed, but a future handler that interns without persisting could re-introduce drift —
worth a debug-mode `dict.next_id == ps.next_id` assertion later.

**Also shipped in this arc** (per the recovered session log, now retired from the
queue): the clawRxiv paper trimmed to ≤5000 chars and posted as **post 2601 (v8)**; a
SPARQL quoted-triple predicate-filter regression test; the double-click "grow the
graph" demo wired end-to-end on Q42 against the base+retrieval sidecar; stale "live
training" website banners removed and the sitemap un-orphaned after the SutraDB→Loka
rebrand. The `:3031` demo itself is transient runtime state, not committed work — the
resurrection recipe (serve `loka-retrieval-data`, `load_retrieval_loka.py`,
`infer_server.py` on `:8092`) lives in `planning/base-retrieval.md` if it needs
standing back up.

**Still open after this:** engine bug #1 (does the `c36760b` sled tuning hold against
*fresh* sustained ingest past 32.88 M triples?) — scale-gated, not blocking under the
base+retrieval pivot.

---
## 2026-05-17 — Studio leaves Flutter: site branding kit, /browse un-orphaned, JS Studio shipped

A UI-day, three threads, one trajectory: **the graph viewer stops being a Flutter problem.**

First, the website. A botched identity-standardization pass had gutted the self-contained `/contribute/` page (an over-broad hex→token regex rewrote its own `:root` palette into circular `--bg:var(--bg)` and swapped working font stacks for undefined `var(--sans)`). Repaired it, hardened `unify_site.py` (never rewrite a custom-property *definition*; real-`<link>` shared-sheet detection), scrubbed the same dead `:root` from `benchmarks`/`playground`. Then reconstructed all 38 content pages onto the shared branding kit from emmaleonhart.com/branding/ (`scripts/restructure_site.py` — `.site-nav` bar, `.repo-widget`, hero/glyph, `.sig`; `scripts/build_search_index.py` → real site-wide search), homepage hand-finished as the showcase. Three transformers, mutually idempotent.

Then the viewer forensics. Emma remembered a *good* graph visualization that felt gone. Git history settled it: `tools/browse.html` (vis-network) was never visually degraded — only the SutraDB→Loka rebrand touched it (7 name strings) — and `/graph` was *never* a viewer (born as a Protégé Turtle export). The good viewer was **orphaned**: the engine never served it and the playground's "Graph Browser" button pointed at the Turtle dump. Fixed: shared router serves it at `GET /browse`; button repointed. (History even showed the Flutter Studio graph view was once explicitly built "toward browse.html parity" — the team already knew browse.html was the gold standard.)

Then the strategic turn. Comparing browser vs Flutter Studio, Emma's call: the JS knowledge graph is best, and Flutter→web→Electron is the easy direction. First proved Flutter-web-in-Electron works (one `dart:io` conditional-import shim — the app was otherwise 100% web-portable) — but she flagged the real issue herself: CanvasKit renders the whole UI to one `<canvas>`, so vis-network can't compose. So: **a plain HTML/JS Studio** (`web-studio/`, real DOM), `LokaClient` a 1:1 port of `loka_client.dart`, six tabs — Knowledge Graph (the `/browse` vis-network viewer in an iframe), SPARQL, Triples, Health, Ontology, Playground — built incrementally (slices 1–6, commit each), running in the browser **and** Electron. The Flutter Studio is frozen as spec + fallback, not deleted. Built autonomously via a resume cron after a usage-limit pause; full spec in `planning/js-studio.md`.

---
## 2026-05-16 — RDF-star hardened across every path; cascade-retraction ships end-to-end

Headline: **a continuous barrel (B0–B8) took RDF-star from "works on the proto
bulk path, content-hash with no reverse map" to solid across ingest /
persistence / query / export, and landed cascade-retraction — "remove a node
and every generated inference that transitively cited it" — end-to-end:
pure engine fn → preview endpoint → `/retract` + `retract_node` MCP tool →
Loka Studio action, with the destructive surface gated behind an explicit
commit flag.**

Built on the Phase-0 reverse index (same-day, below), in order, each its own
commit+push:

- **B1 — cascade Phase 1.** `loka-core/src/retract.rs`: pure
  `retract_set(root, store, dict) -> RetractSet` (depth-grouped). Bounded to
  `http://loka.dev/provenance/`, recurses only along `propositionInferredFrom`,
  cycle-safe, real→real and child→parent are not dependencies. 5 unit tests.
- **B2 — Bug B.** `loka-ffi` + `loka-cli/mcp.rs` serverless ingest used the
  non-star parser (dropped inner triples, interned the `<<QUOTED_TRIPLE>>`
  sentinel). Switched to `parse_ntriples_star_line` + `register_quoted`; FFI
  `resolve_id`/`loka_resolve` + mcp `resolve_id` now `render_term`.
- **B3 — persistence durability.** On-disk reopen round-trip test: write a
  quoted triple, drop (sled closed), reopen, `load_terms_into` → reverse map +
  faithful render survive. (Unit tests use `temporary()`, which can't reopen.)
- **B4 — export round-trips.** `resolve_term_for_turtle` (Turtle + N-Triples
  `/graph`) renders quoted ids as parseable `<< … >>`; `compact_iri` guards
  against compacting a `<<` form. Export → re-parse round-trip test.
- **B5 — SPARQL-star query coverage.** `resolve_term` already hashed a
  concrete `<< s p o >>`; locked bound + unbound + projection in with tests
  (sparql + a proto CSV-projection test).
- **B6 — cascade Phase 2.** `POST /retract/preview` — read-only, returns the
  would-be-removed set by depth + HNSW-tombstone count. Test asserts the
  store row count is unchanged.
- **B7 — cascade Phase 3.** `POST /retract {iri, commit}` (commit:false ==
  preview; commit:true deletes from in-memory + persistent store and flips
  HNSW via `VectorRegistry::delete` — the wired-but-never-called path now
  invoked) + `retract_node` MCP tool (dry-run default, serverless + server).
- **B8 — cascade Phase 4.** Loka Studio: a "Retract (cascade)" button on the
  selected-node panel → preview dialog (per-depth breakdown + HNSW count) →
  explicit confirm → destructive commit → reload. `flutter analyze` clean.

Every Rust suite green throughout (core 142, proto 14, sparql 88+, cli 14,
ffi 4), zero regressions; Studio analyzes clean. The recurring rebase against
the repo's `cargo fmt [skip ci]` cron was handled per push (one content
conflict in `server.rs`, resolved keeping the refactor). The cascade's
destructive path is opt-in everywhere: the default at the endpoint, the MCP
tool, and the Studio dialog is preview/dry-run.

---
## 2026-05-16 — Quoted-triple reverse index (cascade-retraction Phase 0); engine bug #2 Bug A fixed

Headline: **RDF-star quoted-triple ids are content hashes (`xxh3_64(s|p|o)`) and the
store had no reverse map — so a quoted-triple subject couldn't be rendered back to
`<< s p o >>` and a provenance cascade couldn't dereference a `propositionInferredFrom`
source id. Added the persisted reverse map. This unblocks cascade-retraction Phases 1–4
*and* fixes ingest-side Bug A (proto was persisting the `<<QUOTED_TRIPLE>>` sentinel
because it couldn't render the id).**

This is the engine-bug-#2 follow-through. The 2026-05-16 query-layer fix (commit
`b63af81`) made the executor refuse to emit literal predicates — honest output
regardless of how a malformed row got in. But the *root cause* was ingest-side: a
one-way content hash with no reverse map. A hash cannot be reversed, so the fix is
structural — store the mapping at mint time.

**What changed**

- `loka-core/id.rs` — `TermDictionary` gains an in-memory
  `quoted: HashMap<TermId,[TermId;3]>` plus `register_quoted` (mint+record,
  idempotent, returns the same id `quoted_triple_id` would), `resolve_quoted`,
  `insert_quoted_with_id` (hydration), and `render_term` — a recursive,
  depth-bounded renderer that turns a quoted id into faithful N-Triples-star
  `<< <s> <p> "o" >>` and is a drop-in for `resolve` on plain terms.
- `loka-core/persistent.rs` — new `quoted` sled tree (id LE 8B → s|p|o LE 24B).
  `BatchInsert` gains `quoted: Option<(TermId,TermId,TermId)>`, written **inside
  `insert_batch`'s existing multi-tree transaction** (now a 7-tuple) so the mapping
  is atomic with its rows and the wedge-fix invariant (no new per-row sled txn) is
  preserved. `register_quoted` for the non-batch path; `load_quoted_into`;
  `load_terms_into` now also hydrates the quoted map so every existing hydration
  call site gets reversal for free; `flush` covers the new tree.
- `loka-proto/server.rs` — bulk `/triples` and `INSERT DATA` mint sites call
  `register_quoted`; the annotation row persists a faithful `<< s p o >>` subject/
  object string instead of the `<<QUOTED_TRIPLE>>` sentinel (**Bug A fixed**);
  `resolve_term_to_json` (now `"type":"triple"`) and `resolve_term_for_csv` render
  via `render_term` (no more `_:idN` for quoted subjects).
- `loka-cli/main.rs` — import path registers quoted on the star path.

**Verification.** New unit tests in `loka-core` (register/resolve/render/nested/
rehydrate + a persistence round-trip through `insert_batch`) and a `loka-proto`
end-to-end test (`POST /triples` an RDF-star annotation → SPARQL it back → assert
the subject is `type:triple` with a faithful `<< … >>` value, not `_:idN`). Full
`loka-core` + `loka-proto` + `loka-sparql` suites green, zero regressions.

**Scope boundary (deliberate).** Turtle/graph **export** serializers still use
`resolve` — RDF-star Turtle export is a separate serializer concern, off the
query/cascade path. The non-star `parse_ntriples_line` in `loka-ffi`/`mcp.rs`
(Bug B) is a parser-choice bug, not the reverse map; left as a low-priority
follow-up. Design + phasing: `planning/cascade-retraction.md` §6a. Next:
cascade-retraction Phase 1 (the pure `retract_set` fn + tests).

---
## 2026-05-15 PT — v14: epoch-4 floor (ppl 202.01, series best) shipped; driving to 10 epochs under a self-resuming supervisor

Headline: **v14's epoch-4 checkpoint (ppl 202.01 — lowest in the entire v11–v14 series) is on HF as the safe floor, but the target is the full 10 epochs. A supervisor (`tools/v14_train_supervisor.py`) is driving epochs 6–10, resuming from the epoch-4 weights and auto-restarting through every GPU-contention death until epoch 10 lands.**

> **Correction to an earlier draft of this entry:** a prior version of this section described shipping at epoch 4 as a final decision. That was a misread of the user's intent — "stick with epoch 4" meant *epoch 4 is the floor, keep going to 10*, not *stop*. v14 is **not** final at epoch 4. The supervisor below carries it to 10. Epoch 4 stays on HF only as the never-lose-progress fallback.

### Per-epoch trajectory (5-epoch partial-local run)

| Epoch | Loss | Perplexity | HF tag |
|---|---|---|---|
| 1 | 5.6457 | 283.07 | `v14.1` |
| 2 | 5.3727 | 215.45 | `v14.2` |
| 3 | 5.3449 | 209.55 | `v14.3` |
| 4 | 5.3083 | **202.01** ← shipped | `v14.4` |
| 5 | 5.3216 | 204.70 | `v14.5` |

### The corpus-scale result

Clearest single finding of the whole v11→v14 arc. Holding architecture, tokenizer, batch size, and optimizer constant and only scaling the cleaned corpus:

| Model | Corpus triples | Best ppl |
|---|---|---|
| v11 | 350 428 | 279.12 |
| v12 | 671 817 | 250.82 (226.86 lost) |
| v13 | 2 511 771 | 242.75 |
| **v14** | **4 021 409** | **202.01** |

11× more clean data → 28% perplexity reduction (279 → 202), and the curve was *still descending at epoch 4* — unlike v13 (2.5M) which plateaued by epoch 2. The bigger corpus didn't just shift the floor; it extended how many epochs of useful learning the model could extract before Adam settled. Strongest evidence to date that the binding constraint on this model line is corpus scale, not architecture or training duration on this hardware.

### Why epoch 4 and not a 10-epoch run

The plan was to extend v14 to 10 epochs (it hadn't plateaued at 5). `train.py` got `--resume-from` / `--start-epoch` support + optimizer-state persistence for exactly this. First resume attempt was killed (it appended to the same log, which would have re-tripped the pusher's FINAL_RE exit — documented gotcha, now in the pusher docstring + queue.md). The clean relaunch on a fresh log then OOM'd in epoch 6's backward pass — **an unrelated `pytest sdk/sutra-compiler/tests/` run had grabbed the 8 GB laptop GPU concurrently.** Same failure class as the v12 LLaMA-contention disaster: any second CUDA process on this card during training is poison.

Decision (user call): **ship v14 at epoch 4 rather than re-run.** Epoch 4 (202.01) is already the series best and is safe on HF as both the canonical `v14` tag and the per-epoch `v14.4` tag; the full clean 10-epoch run is available via the donor path (`tools/contribute_v14_training.py`, documented at loka.emmaleonhart.com/contribute/). Re-running 16 h locally to maybe shave a few points off the already-best model, on hardware where a stray pytest run can kill it, isn't worth it — the contributor path exists precisely for this.

### Series complete

v11 → v14 all trained and shipped. Twelve model versions on `EmmaLeonhart/loka` (v3–v14), four corpus tiers on `EmmaLeonhart/normalized-wikidata` (v11-50k → v14-1M), plus per-epoch tags `v12.*` `v13.*` `v14.*`. The per-epoch snapshot discipline meant not a single epoch was lost across three training disruptions (v11 OOM, v12 contention, v14 pytest OOM). Docs consistent across GitHub, the site (loka.emmaleonhart.com), both HF READMEs, paper, DEVLOG, status.md.

---
## 2026-05-14 PT — v13 shipped at epoch-2 (ppl 242.75); v14 partial-local started

Headline: **v13 ships at the epoch-2 snapshot, ppl 242.75 on the 2,511,771-triple `v13-500k` corpus.** Trained 5 of a planned 10 epochs; trajectory plateaued in the 240–260 band after epoch 2 (classic Adam-momentum behaviour, *not* contention divergence — wall-time was stable at 112 min/epoch and `nvidia-smi` showed exclusive GPU at 74 W actual / 80 W cap, 74 °C, 83 % util). Per-epoch snapshots `v13.1` through `v13.5` are all on Hugging Face via `tools/epoch_snapshot_pusher.py`; the canonical `v13` tag is the epoch-2 checkpoint.

### Per-epoch trajectory

| Epoch | Loss | Perplexity | Wall | HF tag |
|---|---|---|---|---|
| 1 | 5.8134 | 334.76 | 6 858 s (114 min) | `v13.1` |
| 2 | 5.4920 | **242.75** ← shipped | 6 863 s (114 min) | `v13.2` |
| 3 | 5.5146 | 248.29 | 6 740 s (112 min) | `v13.3` |
| 4 | 5.5546 | 258.42 | 6 738 s (112 min) | `v13.4` |
| 5 | 5.5453 | 256.04 | 6 732 s (112 min) | `v13.5` |
| — | — | training stopped at user direction; epoch-2 promoted to canonical | — | `v13` |

### Why we shipped early

The plan was 10 epochs but the loss curve clearly plateaued. The decision rule was documented in `queue.md` before the run: "if epoch 5 stays in the 240–270 band, stop v13 and front-run partial-v14." Three reasons to make that call:

1. **It's Adam plateau, not progress.** Adam's first/second-moment estimates settled around an optimum at epoch 2; subsequent epochs are random-walking inside the basin with momentum carrying it slightly outward. Adam's *property*, not a bug.
2. **Per-epoch snapshots mean no loss.** v13.2 was already on Hugging Face the moment epoch 2 finished. The decision to stop has zero downside on result quality.
3. **v14 has 1.6× more data.** The corpus-quality lever isn't exhausted; we know it pays (v11 → v12 → v13 each got a clean ppl improvement from corpus growth). v14's 4 M triples on the same laptop is a better use of the remaining GPU-hours than 5 more epochs of v13 at-plateau.

### Comparison across the normalized-wikidata series so far

| Model | Corpus | Triples | Best epoch ppl | Shipped ppl | Epochs trained |
|---|---|---|---|---|---|
| v11 | v11-50k | 350 428 | 279.12 (epoch 3) | **279.12** | 3 (CUDA OOM at epoch 4) |
| v12 | v12-100k | 671 817 | 226.86 (epoch 4) | **250.82** (epoch 6, training corrupted) | 7 |
| v13 | v13-500k | 2 511 771 | **242.75** (epoch 2) | **242.75** | 5 |
| v14 | v14-1M | 4 021 409 | (training) | (training) | (5 partial-local + donor path) |

v13 ships slightly worse than v12's lost best (242.75 vs 226.86) and slightly better than v12's actual shipped value (242.75 vs 250.82). The corpus-quality lever is still working — but the *trained-headroom-on-this-laptop* lever has clearly bottomed out around 240-something. Contributors with bigger hardware (`--batch-size 64` on 24 GB cards, full 10 epochs without wall-clock pressure) are the obvious source of meaningfully-lower numbers; see `pages/contribute/index.html`.

### Hardware-bound power observation

`nvidia-smi` during epoch 4 showed the laptop sitting at **74 W actual / 80 W cap** with 83 % GPU util at 74 °C — power-cap-bound, not thermally throttled. Documented in CLAUDE.md and the contribute page: the v13 240–260 plateau may be an artifact of the 80 W TGP budget + batch-16 gradient noise on a single laptop GPU, not a hard data ceiling.

### Next

v14 partial-local training (5 epochs, batch 16) started immediately after v13 stopped. Same epoch_snapshot_pusher setup. Each v14 epoch will land on Hugging Face as `v14.N` regardless of whether the run completes. ETA ~16 hours.

---
## 2026-05-14 PT — Documentation sweep + v13 training in flight + per-epoch HF snapshotting

Two things this entry captures, both downstream of the v12 disaster.

### Per-epoch snapshot pattern

v12's training divergence (epochs 5–7 under shared-GPU contention) overwrote the epoch-4 best checkpoint because `train.py` saves to a single fixed path each epoch. A defensive snapshot rescued an epoch-6 result, but the *correct* best was already gone. **Fix**: `tools/epoch_snapshot_pusher.py`, a passive watcher that tails the training log, snapshots the live `.pt` file with a per-epoch suffix the moment a `epoch N/M loss … ppl …` line appears, and pushes it to HF as a tagged revision (`v13.1`, `v13.2`, …). Starts alongside training; doesn't touch the training process. Each epoch is now preserved both locally and on `EmmaLeonhart/loka` so a divergent late epoch is recoverable.

Verified working: v13 epoch 1 (ppl 334.76) snapshotted to `wikidata_v13_epoch01.pt` and uploaded to HF as tag `v13.1` while v13's epoch 2 ran.

### v13 training in flight

v13-500k corpus: 2,511,771 triples, the largest training input yet by 3.7×. Started 2026-05-14 ~10:45 PT on a now-exclusive GPU (the v12-killing LLaMA experiment exited after the first one was manually stopped, the second one we noticed running, and a third never appeared). 10 epochs at batch 16, ETA ~17 h to completion.

Epoch 1 result is a strong signal that the corpus-quality lever isn't exhausted: **ppl 334.76 at epoch 1** is dramatically better than v11's epoch 1 (6577) or v12's epoch 1 (1334). More data = stronger gradient signal per epoch.

### Documentation sweep

Brought all five public surfaces (GitHub README, loka.emmaleonhart.com homepage, loka.emmaleonhart.com/loka/, both HF dataset READMEs, paper masthead+abstract) into sync with the multi-rung v11–v14 pipeline and the two-HF-dataset structure. The history page got a new top section covering v6 → v14 (catalog cleanup, cron loop, normalized-wikidata pivot, hardware lessons). `status.md` was completely rewritten — it had been dated 2026-05-09 and was still claiming v5 as the current model.

Net effect: any AI agent or human landing on any of those surfaces now sees a consistent story about what the project is and what's currently shipping.

---
## 2026-05-14 PT — v12 trained on the v12-100k corpus, disrupted by external GPU contention

Headline: **v12 shipped at the epoch-6 snapshot, ppl 250.82, on the 671 817-triple `v12-100k` corpus — meaningfully better than v11 (ppl 279.12) despite a botched training trajectory. The training was disrupted by an unrelated LLaMA 3.1 8B experiment sharing the laptop GPU; epochs 5–7 diverged from epoch 4's best of 226.86 as Adam's momentum state corrupted under contention.**

### Trajectory

| Epoch | Loss | Perplexity | Wall |
|---|---|---|---|
| 1 | 7.1963 | 1334.50 | 5642 s (warm-up) |
| 2 | 5.8383 | 343.18 | 2309 s (38 min, clean GPU) |
| 3 | 5.4725 | 238.05 | 1929 s (32 min, clean) |
| 4 | 5.4243 | **226.86** ← best | 2445 s (41 min, clean) |
| 5 | 5.4955 | 243.59 | 10485 s (175 min, LLaMA sharing GPU) |
| 6 | 5.5247 | **250.82** ← shipped (snapshot saved before further degradation) | 11445 s (191 min) |
| 7 | 5.5521 | 257.77 | 8817 s (147 min) |
| — | — | training killed externally (exit 127) | — |

### What happened

The plan was 20 epochs on a clean GPU. Reality: ~5 hours in, a different research workflow on the same machine (`scripts/run_five_condition_experiment.py --model llama-3.1-8b`) started using the laptop's 8 GB VRAM concurrently. v12's per-epoch wall time jumped from ~37 min to ~180 min, and the loss curve started climbing instead of descending. The likely root cause is Adam's first/second-moment estimates getting corrupted by some combination of (a) CUDA stream contention slowing or reordering kernels, (b) memory fragmentation forcing pessimistic allocations, (c) thermal throttling under sustained dual-CUDA-process load.

When the LLaMA experiment was killed at user request, epoch 7 started but completed with even worse perplexity (257.77) — momentum corruption is sticky; a clean GPU mid-run doesn't immediately heal it. A second LLaMA experiment instance ("scale_8b" — a different label, different conditions) started 17 min later and ran concurrently with epoch 7. The v12 trainer was then killed externally (likely OS-level resource pressure or a kill from outside the Loka workflow) during what would have been epoch 8.

We had the foresight to snapshot the epoch-6 checkpoint before things kept getting worse (`training/checkpoints/wikidata_v12_epoch6_ppl250.pt`). That's what's pinned as v12: ppl 250.82, still better than v11.

### What this proves

- **The bigger/cleaner corpus matters.** v12 at ppl 250.82 from 6 corrupted epochs beats v11 at ppl 279.12 from 3 clean epochs — and v12's epoch 4 (ppl 226.86) is *31* points below v11's clean epoch-3 result. The normalized-wikidata pipeline produces a usefully better training input even when training itself is rough.
- **Shared-GPU compute is poison for Adam.** Future training runs on this laptop need exclusive GPU access; CUDA contention with a 8B parameter model isn't just slow, it corrupts the optimization trajectory. The hardware-laptop project memory got an addendum on this.
- **Per-epoch checkpoints saved us.** `train.py` writes a checkpoint after every epoch (same path, overwriting), and our snapshot of the epoch-6 file before the next epoch overwrote it was the difference between shipping ppl 250.82 and shipping ppl 257.77. Future ship workflow: take a snapshot every time you see a regression epoch.

### Next

- v13 (2.5M triples from the v13-500k corpus) training now in flight on the exclusive GPU. 10 epochs, batch 16, ETA ~24 h.
- v14 pass-2 corpus emit also in flight in parallel (CPU + network, different subsystem).
- Likely revisit v12 later with a clean retrain on an exclusive GPU. ETA ~12.5 h once the GPU is genuinely free.

---
## 2026-05-13 PT — v11 trained on the normalized-wikidata pipeline (no Loka in the loop)

Headline: **v11 trained on a 350,428-triple corpus produced by streaming `philippesaade/wikidata` directly through a new normalization pipeline — Loka eliminated from the training data path. Got through 3 of 20 epochs (loss 8.79 → 5.85 → 5.63, ppl 6577 → 347.71 → 279.12) before CUDA OOM at epoch-4 backward pass; the epoch-3 checkpoint is the v11 release.**

### Why the pipeline changed

Original plan was to ingest a 50 M-triple Wikidata slice into Loka, then preprocess from there. The ingest finished and reached 50,002,600 triples on `loka-data-cron-c1/`. But preprocessing via SPARQL `LIMIT/OFFSET` ran into O(offset) cost on sled — early pages took 8 s each, page 100 took 235 s, projected ~25 hours just for pass 1. Two days of pure preprocess wall-clock was untenable.

So the pipeline pivoted: a new `tools/preprocess_from_hf.py` streams `philippesaade/wikidata` straight from the HF parquet shards, builds a SQLite-backed label cache (pass 1), then streams again to emit one tab-separated `subject\tpredicate\tobject\n` line per kept claim (pass 2). No Loka in the data path — and *as a side effect*, the cleaned corpus becomes an independently-useful artifact published as `EmmaLeonhart/normalized-wikidata` on HF.

### One critical mid-run fix: corpus property labels are systematically wrong

87 pages into the original Loka-source preprocessor, an audit caught that **every property's `rdfs:label` row in the corpus was mis-keyed against the inner-triple's object value** instead of the property's actual label. Examples: P20 → "Belgium" (should be "place of death"), P1412 → "English" (should be "languages spoken, written or signed"), P3301 → "NBC" (should be "broadcast by"). Engine bug #2 (RDF-star annotation rows surfaced in the wrong slot) was producing this; entity labels were unaffected. The fix in commit `78e1e7e`: preload `training/property_label_cache.json` (7,312 curated entries) as `source='curated'`, skip pass-1 rdfs:label rows whose subject is a property, drop pass-2 rows where subject *or* object is a property IRI. **Without this catch the entire normalized corpus would have been useless** — predicates would have read like "Douglas Adams Belgium English" instead of "Douglas Adams place of death English".

### What we shipped

| Artifact | Where | Notes |
|---|---|---|
| `v11-50k` corpus (350,428 lines) | `EmmaLeonhart/normalized-wikidata` tag `v11-50k` | First normalized-wikidata release. CC-BY-SA 4.0 (inherits from Wikidata). |
| `wikidata_v11.pt` (178 MB) | `EmmaLeonhart/loka` tag `v11` *(to push)* | Epoch-3 checkpoint. Same 44.5 M-param architecture as v5+. |
| `preprocess_from_hf.py` | tools/ | Streams HF source, SQLite label cache, two-pass (or split into two processes to avoid fsspec mem accumulation). |
| `hf_push_normalized.py` | tools/ | Separate HF push targeting `EmmaLeonhart/normalized-wikidata` (not the model repo). |

### Per-epoch training trajectory

| Epoch | Loss | Perplexity | Wall |
|---|---|---|---|
| 1 | 8.7914 | 6577.15 | 1102 s |
| 2 | 5.8514 | 347.71 | 1100 s |
| 3 | 5.6316 | **279.12** | 978 s |
| 4 | — | **CUDA OOM** in backward pass | — |

Hardware lesson: the 4070 **Laptop** has 8 GB VRAM, not the 12 GB of the desktop variant. At `--batch-size 32` plus typical Adam optimizer state, gradient peak in epoch 4 pushed over the line. Future training runs (v12 / v13 / v14) must use `--batch-size 16`. Memory pinned to project memory.

### Context: this version is the start of the multi-rung pipeline

The plan after v11 is a series of corpus sizes / model versions:

| Tag | Entity rows | Output triples (est) | Model |
|---|---|---|---|
| `v11-50k` | 50,000 | 350,428 (actual) | v11 ← here |
| `v12-100k` | 100,000 | ~700k | v12 |
| `v13-500k` | 500,000 | ~3.5M | v13 |
| `v14-1M` | 1,000,000 | ~7M | v14 |

Each rung ships the corpus to HF, trains a Loka model on it, ships the model to HF, and lands as a paper §5.X update. v12-100k preprocessing is in flight as of this writing.

### What did *not* happen this cycle

- No propgen test on v11 yet. The laptop's GPU is fragile (the same OOM that killed epoch 4 makes me wary of running a sustained autoregressive inference loop right after). Defer until the v12 preprocessing finishes and the GPU is genuinely idle.
- The 50 M-triple Loka data dir (`loka-data-cron-c1/`, 17.6 GB) is still on disk but unused — keeping it as a reference snapshot in case we want to compare against Loka-source preprocessing later. Will likely be removed before the 1 M run.

---
## 2026-05-12 23:52 UTC — Engine bug #1, second incarnation: sled flusher panics on Windows at 33 M triples

Headline: **the big-corpus ingest for v11 crashed Loka, not training. v10 is intact on HF; no model lost.** sled 0.34's periodic flusher thread panicked with Win32 `ERROR_NO_SYSTEM_RESOURCES` (os error 1450) trying to fsync, at ~32.88 M triples / 5.0 GB. Same wedge class as v6–v9 (queue.md engine bug #1), but a hard panic this time instead of a hang.

### Timeline

- **2026-05-11 15:59 UTC** — v10 trained, propgen-tested, pushed to HF as `EmmaLeonhart/loka@v10`, committed + pushed (`afc7282`). End of cycle 1.
- **2026-05-11 ~22:35 UTC** — quiet window declared (no commits/pushes for 8 h; post-eval cron every 6 h for 48 h thereafter). `tools/post_eval_cron.py` started.
- **2026-05-11 06:15 UTC → 2026-05-12 12:33 UTC** — bigger-corpus ingest into a fresh `loka-data-cron-c1/` data dir, targeting queue.md item #3 (10× the existing corpus). The HF importer (`tools/wikidata_hf_import.py`) ran cleanly for ~30 hours, climbing to **318,581 entity rows / 32,876,098 triples** at 4 entities/s sustained. No wedges or stalls along the way — the application-layer batching fix from `39effbb` held.
- **2026-05-12 23:52:38 UTC** — Loka panicked:
  ```
  ERROR sled::flusher: failed to fsync from periodic flush thread:
     Insufficient system resources exist to complete the requested service. (os error 1450)
  thread 'log flusher' panicked at .../io/stdio.rs:1165:9:
  failed printing to stderr: Insufficient system resources exist to complete the requested service.
  ```
- **2026-05-12 ~23:53 UTC** — v11 preprocess attempt (`training/logs/preprocess_v11.log`) hung waiting on the dead Loka and was the downstream casualty.
- **post_eval_cron fires 2 + 3** (16:40 + 22:40 UTC) — skipped because `loka_triple_count()` timed out at the 300 s bound. The triple-count query takes 2–5 min on a quiet 33 M-triple Loka, so even with Loka healthy the count would have aborted the firing.

### Root cause

Win32 error 1450 is `ERROR_NO_SYSTEM_RESOURCES` from the OS I/O manager, returned to `FlushFileBuffers` (sled's fsync call) when the kernel runs out of resources (typically nonpaged-pool entries, file-system filter IRPs, or system-PTE pool) to issue the flush. Conditions that drove the system there:

1. **sled 0.34 defaults on a 5 GB DB**: `sled::open(path)` uses 1 GB `cache_capacity`, 500 ms `flush_every_ms`, `Mode::LowSpaceUsage`. The 2 Hz fsync on a 5 GB mmap-backed store keeps a large fraction of file-system metadata write-behind queued.
2. **Concurrent ingest**: the HF importer was POSTing batches continuously, so user-data writes interleaved with the periodic flusher's metadata fsyncs.
3. **Windows nonpaged-pool exhaustion**: each outstanding I/O request consumes a kernel pool entry; 4070-class systems have hard limits on this pool.

The v9/v10 application-layer fix (`39effbb`: one sled transaction per HTTP request, no synchronous `flush()` at request end) was necessary but not sufficient at this scale. The remaining churn comes from sled's *own* periodic flusher, which we don't control from the application.

### Fix (this commit's predecessor: `c36760b`)

`PersistentStore::open` now configures sled explicitly:

```rust
sled::Config::new()
    .path(path)
    .cache_capacity(256 * 1024 * 1024)   // 256 MB, ¼ of default
    .flush_every_ms(Some(2000))          // 2 s, ¼ of default fsync rate
    .mode(sled::Mode::HighThroughput)    // batch more before commit
    .open()
```

The durability window grows from 0.5 s to 2 s, which is fine for our workload — bulk ingest is replayable from `wikidata_hf_import_state.json`, so 2 s of unacked writes on crash is at worst 2 s of re-ingest. The smaller cache reduces memory pressure; `HighThroughput` mode trades space-amplification (extra log files that survive longer before compaction) for far less per-fsync work.

### What was lost: nothing critical

- v10 checkpoint: safe locally and on HF (`EmmaLeonhart/loka@v10`).
- v6–v9 checkpoints: safe locally and on HF.
- `loka-data-cron-c1/`: still on disk at 5.0 GB. Won't be deleted until the reopen-in-place test (queue.md option B) tells us whether sled can replay its WAL cleanly with the new config. If yes, we keep the 32.88 M triples. If no, we fall back to a full re-import (~31 h).
- `wikidata_hf_import_state.json`: intact (180 bytes). Knows the importer reached row 318,581.

### Limits of this fix + when to escalate to RocksDB

This is a **probable** fix, not a guaranteed one. We've cut sled's I/O footprint by ~4× but haven't changed its on-disk format or addressed sled 0.34's known issue of files growing without bound between manual compactions. Escalation criteria: if the same panic (or a similar Windows I/O exhaustion) recurs at the next ingest plateau, we migrate off sled 0.34 entirely. sled has been unmaintained since 2021; RocksDB is Oxigraph's choice for the same reason and is the long-standing open question in `CLAUDE.md`. This is the **next** engine task if the tuning doesn't hold.

### Side fixes in the same window

- `tools/post_eval_cron.py`: triple-count timeout 300 s → 1800 s (`b34d30d`). Counting 33 M triples on a healthy Loka takes 2–5 min; aborting the whole firing because the count is slow throws away the pipeline for nothing.
- `training/preprocess.py`: page the SPARQL fetch with LIMIT/OFFSET (`b34d30d`). The v11 preprocess attempt found that asking Loka for a 32 M-row JSON in one shot grew it to 21 GB resident accumulating the response, never sent a byte back. 100 k-row pages with a 900 s per-page bound, sled iteration is byte-order stable so unordered pagination is safe.
- `.gitignore`: add `loka-data-cron-*/` so the 5 GB scratch dirs the cron creates per cycle stay out of the repo.

### Status

- Engine fix shipped: `c36760b` ("sled: explicit config to survive multi-GB ingest on Windows").
- Persistent-store unit tests all pass (9/9) under the new config.
- Release binary rebuilt against the new config.
- **Option B verified 2026-05-13 01:00 UTC**: `loka serve --data-dir loka-data-cron-c1 --port 3030` opened the existing 5 GB sled state cleanly under the new config. `/health` returns 200, SPARQL `SELECT (COUNT(*) AS ?n)` returns **32,877,248** — 1,150 *more* than `big-pull.log`'s last recorded 32,876,098, meaning sled's WAL replay recovered every write that had reached durable storage at the moment of the panic. No data lost; the engine fix verified for the reopen case.
- Queue.md item #5 (fine-tuning scaffolding, `df8fb43`) and #6 (paper v2 publish — post 2384, supersedes 2378) both shipped in the same window.
- Next decision is the user's: resume the bigger-corpus ingest past 32.88 M triples (extending toward the original 50 M-triple target), or stop here and use this corpus as v11's training source. The probable-fix caveat in the previous section still applies — we cut sled's I/O footprint by ~4× but haven't migrated off sled 0.34; if the same panic recurs at the next plateau, RocksDB migration is queued.

---
## 2026-05-11 15:59 UTC — v10 trained (cron cycle)

Trained by `tools/training_cron.py` on the local 4070. Same 44.5 M-parameter
BPE architecture as v6/v7/v8.

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 17.6480 | 46179373.23 |
| 2 | 8.2131 | 3688.78 |
| 3 | 6.9648 | 1058.71 |
| 4 | 5.6704 | 290.15 |
| 5 | 5.3723 | 215.37 |
| 6 | 5.2357 | 187.85 |
| 7 | 5.1252 | 168.20 |
| 8 | 5.0717 | 159.44 |
| 9 | 5.0003 | 148.46 |
| 10 | 4.9168 | 136.57 |
| 11 | 4.7490 | 115.47 |
| 12 | 4.4889 | 89.03 |
| 13 | 4.3564 | 77.98 |
| 14 | 4.2933 | 73.21 |
| 15 | 4.1988 | 66.61 |
| 16 | 4.1505 | 63.47 |
| 17 | 4.1249 | 61.86 |
| 18 | 4.0934 | 59.94 |
| 19 | 4.0561 | 57.75 |
| 20 | 4.0168 | 55.52 |

**Final perplexity: 55.52**

Auto-regressive propgen test output (Q42 seed, 30 sources, conf 0.25):
`training/data/test_propgen_Q42_v10.nt`. See the test script's
generated `_meta.json` for per-source breakdown and the asymmetric-drop
companion file for the highly-cardinal predicates filtered out of context.

Checkpoint at `training/checkpoints/wikidata_v10.pt`. Pushed to
Hugging Face as `EmmaLeonhart/loka@v10`. `MODEL.json` bumped to
pin v10 as the default for fresh-clone inference.

---


## 2026-05-11 (later) — v9 trained: bigger Loka, smaller corpus, better outputs

Headline: **`/triples` wedge fixed, v9 trained to ppl 57.15 on a 94k-triple corpus, 97% of generations land on semantic predicates.** Two unexpected things at once — the engine-bug at scale is no longer a thing, and the v9 inference quality beats v8 on a *smaller* training file.

### The /triples wedge, dispatched

The wedge (paper §6.1, recurring throughout v3–v8) was caused by the `insert_triples` HTTP handler running 3-4 sled write-transactions per N-Triples line (three term-interns + one SPO/POS/OSP triple-insert) and ending every request with a synchronous `flush()`. Under sustained ingest of ~100k+ triples in one POST, sled's internal compactor couldn't keep up with the WAL accumulation, and the writer thread eventually stalled — `/health` stayed up, `/triples` timed out.

Fix in `39effbb`: `PersistentStore::insert_batch` does ONE sled multi-tree transaction across `spo / pos / osp / terms_fwd / terms_rev / meta` for the whole HTTP request, regardless of triple count. The synchronous flush is gone — sled flushes on its own periodic schedule and on Drop, which is sufficient durability for our workload. The handler collects all triples + their string forms first, then makes one `insert_batch` call.

Verified at scale on the v9 cycle: **2,000,049 triples ingested in 4003s at 500 triples/sec sustained, no timeouts**. Previous wedges hit at 90k, 174k, 1M — this run cleared all three by 20×+.

### The 94k corpus

v9's training file is *smaller* than v7's (94,202 vs 184,458 triples), despite being extracted from a 4× larger Loka data dir (2,090,640 raw triples). Why: the HF stream `philippesaade/wikidata` gives many *claims per entity* but relatively few *entities per row* — we consumed 9,647 rows for the 2M raw triples, an average of 217 triples/entity. When a triple's object is a `wikibase-item` reference to an entity whose own row hasn't been streamed yet, the preprocess pass can't resolve the label and drops the row. We lost 1,049,881 rows that way.

This is a corpus-construction issue, not a wedge issue. The fix is to either (a) stream enough rows that the label graph is mostly closed, or (b) maintain a cross-cycle label map so newly-encountered entities resolve against past cycles' labels. Out of scope for v9; planned for v10+.

### v9 training results

20 epochs from random init, same 44.5 M-param BPE architecture as v6/v7/v8, on the 94k corpus.

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 17.4379 | 37,426,431 |
| 5 | 5.3740 | 215.71 |
| 10 | 4.8977 | 134.0 |
| 15 | 4.2466 | 69.9 |
| 20 | **4.0457** | **57.15** |

Wall time 44 min on the 4070. Loss curve still descending at epoch 20 (4.09 → 4.05) — corpus not saturated despite being half the size of v7. Compare v6 (194.98) / v7 (192.63) / v8 (64.65) / **v9 (57.15)** on Q42 propgen test.

### Q42 / 30-source generation test, all four versions

| | v6 | v7 | v8 | v9 |
|---|---|---|---|---|
| Final ppl | 194.98 | 192.63 | 64.65 | **57.15** |
| Total emissions at conf ≥ 0.25 | 52 | 14 | 47 | 35 |
| Catalog-predicate emissions | 21 (40%) | 9 (64%) | 7 (15%) | **1 (3%)** |
| Semantic-predicate emissions | 31 (60%) | 5 (36%) | 40 (85%) | **34 (97%)** |
| `instance of` date-shape leak | 15 | 0 | 0 | 0 |

The catalog-leak that v6 had → 0 (v7+). Semantic-predicate share continues to climb. v9 produces almost no catalog hallucinations because the v9 corpus (after preprocessing) has even less catalog content than v7 — high label-resolution rates correlate with the entities being "core" Wikipedia-citable rather than long-tail-catalog-only.

### Selected v9 outputs

- `human | union of ->` (no emission — would have required a multi-set value, model declined)
- `Category:Children's writers | Commons category -> "Category : Ch ildren 's"` (conf 0.420) — Commons-category format right, BPE pieces visible
- `atheism | Commons category -> "at he ism Ġ( Ġ("` (conf 0.484) — same format-aware Commons template
- `male and female | Commons category -> "male Ġand Ġfemale Ġ("` (conf 0.477)
- `Template:Infobox person | different from -> "T ://"` (conf 0.511) — URL-prefix hallucination on a `wikibase-item`-typed predicate; the remaining failure pattern v9 still exhibits
- `Template:Infobox person/Wikidata | different from -> "T ://"` (conf 0.511) — same pattern, suggests model has overlearned URL formats for Template:* subjects

The `T ://` / `M ://` outputs on `different from` are the same shape-leak class as v6's `instance of -> "+ Ġof - 00 - 03 T 00"`, just in URL-format instead of date-format. The cleanup that worked for date-leaks (drop `url`/`commonsMedia` datatypes from training) should also kill URL-leaks; possibly we need a stricter filter on which string literals enter training in the first place.

### Status

- v9 checkpoint: `training/checkpoints/wikidata_v9.pt`. Pushed to HF as `EmmaLeonhart/loka@v9`.
- `MODEL.json` pinned to v9 (ppl beats v8 by ~12%).
- HF dataset README refreshed by `upload_readme()` to reflect v9 as latest.
- Wedge fix exposed to 4M+ triples cumulatively — solid evidence the per-request sled batch is the right approach.
- v10 plan: bigger corpus from a cleaner HF slice (more rows, lower triples-per-row), and look at the `Template:* | different from -> "T ://"` URL-leak pattern.

### Loose ends

- The HF state-file design also got fixed in this round (`95f56f7`): state file now lives inside the per-cycle Loka data dir instead of as a global file. The previous global-state design produced a dedup-loop on cycle restart that wasted ~30 minutes of HF stream consumption. v10+ cron cycles use the new per-data-dir state.
- Loose end from v9: the `Template:* | different from -> "T ://"` URL-shape leak on `wikibase-item`-typed predicates. Not blocking; characterised as a v10 investigation target.

---

## 2026-05-11 — v9 cron fired; no v9 results yet

**Cron:** `trig_v9_ship_pipeline` · fired ~2026-05-11T12:00Z (estimated).

**Repo state on arrival:**

- `MODEL.json` pinned to **v8** (`loka-wikidata-v8`, final ppl 64.65, 20 epochs on 184k-triple cleaned corpus).
- No `training/logs/v9_train.log`. No `training/checkpoints/wikidata_v9.pt`. No v9 section in `paper/paper.md`. DEVLOG had no 2026-05-11 v9 entry.
- `wikidata_hf_import_state.json` absent at repo root — cleared by the previous cron session (commit `074ca4c`, `training_cron: stash + clear HF state file per cycle`).
- Most recent commits before this cron were maintenance on `tools/training_cron.py` (state-file path fix, per-cycle stash logic) and editorial polish of paper §5.6 — **no new training was kicked off remotely**.

**Why v9 hasn't started:**

The previous cron session (fired ~4.5 h ago, SHA range `6dcb5cc`→`d68d5c0`) spent its cycle debugging `training_cron.py` rather than running a training pass. The script is now fixed, but it only executes on the local laptop — no GPU is available in the remote cron environment. The v9 pipeline requires the local machine to be running `tools/training_cron.py` (or an equivalent manual sequence) so it can:

1. Run `python tools/wikidata_hf_import.py --max-triples 5000000` to pull a 3–5× larger Wikidata slice.
2. Run `python training/preprocess.py` to rebuild the training file with the v7-era datatype filters.
3. Train v9 from scratch on the expanded corpus.
4. Run the Q42-seed propgen test, write DEVLOG + paper §5.7, push checkpoint to HF, commit, push.

**What to do when back at the laptop:**

```bash
# Confirm training_cron.py is not already running:
pgrep -af training_cron

# If not running, start it (handles HF import + train + ship automatically):
python tools/training_cron.py
```

Alternatively, to run the import and first training pass manually:

```bash
python tools/wikidata_hf_import.py --max-triples 5000000
python training/preprocess.py
python training/train.py  # then test + ship as per DEVLOG §v8 notes
```

**No v8 loose ends** — v8 checkpoint is on HF (`EmmaLeonhart/loka@v8`), `MODEL.json` is pinned, paper §5.6 is polished.

---

## 2026-05-10 (later still) — v8 trained: 20 epochs on cleaned corpus, ppl 64.65

Headline: **the cleaned v7 corpus had a lot more signal in it than 5 epochs surfaced.** v8 is the same 44.5M-parameter BPE architecture trained on the same 184,458-triple v7 corpus, but for 20 epochs from random init instead of 5. Final perplexity **64.65** — well below v5 (84.85), v6 (194.98) and v7 (192.63). Loss was *still descending* at epoch 20 (4.20 → 4.19 → 4.17), so this corpus is not yet saturated even at 20× passes.

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 13.0306 | 456,141.98 |
| 5 | 5.2607 | 192.63 (= v7 final) |
| 10 | 4.4257 | 83.57 (≈ v5 final) |
| 15 | 4.2540 | 70.38 |
| 20 | **4.1691** | **64.65** |

Wall time 88 min on the 4070 (matches the 4.4 min/epoch estimate). The 5 → 20 jump from 192.63 → 64.65 is a 3× perplexity improvement at no compute cost beyond more epochs — strong evidence that the v7 cleanup left a corpus the model hadn't yet exploited at 5 epochs.

### Same Q42 / 30-source generation test, v6 vs v7 vs v8

| | v6 | v7 | v8 |
|---|---|---|---|
| Final ppl | 194.98 | 192.63 | **64.65** |
| Total emissions at conf ≥ 0.25 | 52 | 14 | **47** |
| Of those: catalog-predicate | 21 (40%) | 9 (64%) | 7 (15%) |
| Of those: semantic predicate | 31 (60%) | 5 (36%) | **40 (85%)** |
| `instance of -> "+ Ġof - 00 - 03 T 00"` (date-shape leak) | 15 | 0 | 0 |

The shift is real:

- **v6** emitted lots of confident format-shaped garbage on catalog predicates and *also* leaked the catalog format onto semantic predicates (`instance of -> "+ Ġof - 00 - 03 T 00"` 15 times).
- **v7** had the catalog format un-memorised (the leak is gone) but was so undertrained on the cleaner corpus that it mostly refused to emit anything.
- **v8** keeps the catalog cleanup and the no-leak property, but with 4× more epochs the model now confidently emits semantic-predicate content. 40 of 47 emissions are on semantic predicates (vs v7's 5 of 14, vs v6's 31 of 52).

### Selected v8 outputs (raw, BPE artifacts left visible)

- `English | different from -> "English"` (conf 0.876) — circular but the predicate type is right
- `Adams | different from -> "Adams"` (conf 0.960) — same circular pattern
- `Joan of Arc | Commons category -> "Joan Ġof ĠAr c Ġ( Ġ("` (0.654) — the actual Wikipedia Commons category for Joan of Arc is "Joan of Arc"; format is right, BPE pieces visible
- `British Broadcasting Corporation | Commons category -> "British ĠBroadcasting ĠCorporation Ġ( Ġ("` (0.791)
- `myocardial infarction | Commons category -> "my ocard ial Ġin far"` (0.639)
- `Leonardo da Vinci | country of citizenship -> "Polish âĢĵ"` (0.677) — same wrong answer as v7 (should be Italian) but confidence 0.36 → 0.677
- `Leonardo da Vinci | date of birth -> "- 00 000000 - 00 - 00 T"` (0.322) — date-shape with the v7 normalisation visible (no leading `+`); content all zeros

The remaining failure modes are: (1) circular `different from` outputs (model emits the subject as the object); (2) BPE artifact leakage (`Ġ`, `âĢĵ` for em-dashes); (3) catalog predicates the seed still includes (ISNI, DiseasesDB) where the v7-trained model has no signal to draw on; (4) date and URL hallucinations on those datatypes.

### Status

- v8 checkpoint: `training/checkpoints/wikidata_v8.pt`. Pushed to HF as `EmmaLeonhart/loka@v8`.
- `MODEL.json` pinned to v8.
- Loss curve says we are not data-saturated yet at 184k triples; the next move is data scale, not more epochs. v9 plan: ~3-5× larger corpus from a fresh `tools/wikidata_hf_import.py` run with `--max-triples 5000000`.
- `tools/training_cron.py` (committed in 65781b7) is the 12h local loop that does this automatically; intended to be started after v8 ship completes.

---

## 2026-05-10 (later) — v7 trained: catalog-noise discovery + corpus cleanup

Headline: **the v6 corpus was 76% catalog cross-reference noise. After cleaning, the catalog-format hallucinations vanish.**

A post-training behavioural test (see `planning/autoregressive-propgen-test.md`) on the v6 model surfaced what looked like a model failure but was actually a corpus failure. Running auto-regressive proposition generation on a 14,586-triple Wikidata BFS-depth-3 seed (Q42 / Douglas Adams, 183 entities), v6 emitted 52 confident triples — almost all garbage:

- `British Broadcasting Corporation | ISNI -> "00000000"` (conf 0.754)
- `Joan of Arc | Library of Congress authority ID -> "n 85 - 8"` (LCCN-shaped)
- `Douglas Adams | Freebase ID -> "/ m / 0 c _ _ 9"` (Freebase format `/m/...`)
- `instance of -> "+ Ġof - 00 - 03 T 00"` on **15 different subjects** — a Wikidata date-prefix shape leaking onto an entity-typed predicate

Diagnosis: queried `wikibase:propertyType wikibase:ExternalId` directly against Wikidata. **10,206 properties** are external-identifier datatype — roughly 80% of all Wikidata property *types*. In the Q42 seed they accounted for 49.6% of triples; on the v6 training corpus, 75.7% (573,134 of 757,592 lines were dropped when re-filtered by predicate label). Half the model capacity went to learning catalog cross-reference formats.

### v7 cleanup pipeline

`training/preprocess.py` now applies a per-Wikidata-datatype keep/drop policy (full table in `planning/wikidata-datatype-processing.md`):

- **DROP** (10,525 properties, 82.5% of all property types): `external-id`, `url`, `commonsMedia`, `math`, `wikibase-sense/lexeme/form/entity-schema`, `globe-coordinate`, `geo-shape`, `musical-notation`, `tabular-data`.
- **KEEP** (2,231 properties): `wikibase-item`, `wikibase-property`, `quantity`, `string`, `time`, `monolingualtext`.

Plus value-side normalisation:
- Time: strip leading `+` (Wikidata era prefix); drop trailing `Z`; drop `T00:00:00` portion when zero. `+2012-10-15T00:00:00Z` → `2012-10-15`. BCE keeps the `-`.
- Quantity: strip leading `+`. `+1234` → `1234`.
- Monolingualtext: keep all languages (was English-only in v6); strip the `@lang` tag from values.

Exclusion lists generated by `tools/refresh_wikidata_external_id_list.py` (re-run periodically — Wikidata adds new external-ID properties continuously) and pinned to `training/wikidata_excluded_predicates.json`.

### v7 training results

Same 44.5M-parameter BPE architecture as v6, 5 epochs from random init.

| Epoch | Loss | Perplexity |
|---|---|---|
| 1 | 13.03 | 456,141 |
| 2 | 6.37 | 584 |
| 3 | 5.55 | 257 |
| 4 | 5.37 | 215 |
| 5 | **5.26** | **192.63** |

v6 final ppl was 194.98 — statistically tied. The number is not the point. Wall time on the 4070 was 22 min vs v6's 91 min, purely from the 4× corpus shrink (757k → 184k triples).

### Same Q42 / 30-source generation test, v6 vs v7

| | v6 | v7 |
|---|---|---|
| Total emissions at conf ≥ 0.25 | 52 | 14 |
| `instance of -> "+ Ġof - 00 - 03 T 00"` | 15 instances | **0** |
| `ISNI ->` confident output | `"00000000"` (0.75) | `"0 ."` (0.71) |
| `Freebase ->` confident output | `"/ m / 0 c _ _ 9"` (0.43) | below threshold |
| `country of citizenship ->` da Vinci | did not pass | `"Polish âĢĵ Ġof -"` (0.36) |

Catalog hallucinations *vanished*, not muted. The model's failure mode shifts from "confidently wrong" to "refuses to emit", which is what we want from a generative-citation system. The price is volume — emission count drops because the model no longer manufactures format-shaped strings on prompts it doesn't actually know.

### Status

- v7 checkpoint: `training/checkpoints/wikidata_v7.pt`. Not yet uploaded to HF.
- v7 corpus: `training/data/triples_v7.txt` (gitignored). Generated by re-filtering the v6 label-substituted file in-place; can be regenerated from a Loka instance via `python training/preprocess.py --output ...` (the `--keep-noise-datatypes` flag restores v6 behaviour).
- v8 (in flight): same architecture, 20 epochs from scratch on the v7 corpus, testing whether more compute closes the loss-curve gap or whether we're data-bound. ETA ~88 min.

### Pointer

Loss curve says v7 is undertrained at 5 epochs (5.36 → 5.26 still descending). At ~600 k tokens after BPE on a 44.5M-parameter model we are at 0.013 tokens/param against a Chinchilla-optimal target of ~20. Either v8's 20 epochs flatten the curve or we are bound on data — the next planned step after v8 is a much larger HF re-import (target ~5M useful training triples after filtering) and v9 from scratch.

---

## 2026-05-10 — v6 trained (BPE) and qualitative comparison vs v5

Headline: **the BPE round preserves accents and pulls v5's no-prediction holes off the floor, but a decoder bug makes v6 look worse than it is for date-shaped predicates.** v6 is the same architecture as v5 (d_model 512, 6 layers, 44M params, 5 epochs) trained on the same 757k-triple corpus, with one change: every role string is encoded by `tokenizer_bpe.json` (50K vocab) instead of the word-level regex. Final epoch-5 perplexity: 194.98. *Not directly comparable to v5's 84.85* — BPE has more tokens per role, so loss-per-position is naturally higher; the metric for v6 is qualitative.

Pushed as `EmmaLeonhart/loka@v6-bpe`. (The `v6` tag was already taken — an earlier upload run created it before v6.pt existed, so the new round uses a fresh tag rather than rewriting the existing one.) `MODEL.json` now pins the BPE tokenizer alongside the vocab so `loader.py` resolves all three pieces.

### Side-by-side on unicode-name subjects (`tools/compare_v5_v6.py`)

`predict_object` with `repetition_penalty=3.0`, `per_token_floor=0.05`, on subjects from `triples.txt` whose label contains non-ASCII characters and which have ≥5 facts. Picked candidate predicates by the same shared-object heuristic as `smoke_infer.py`. Showing representative rows from the 12-subject run.

| Subject / predicate | v5 (word) | v6 (BPE) |
|---|---|---|
| Saint-Léonard-de-Noblat / licence plate | (no pred) | "U" 0.06 (low) |
| Didier André / image | "didier andr" 0.45 | (no pred) |
| Didier André / point in time | "didier 00 01t0" 0.44 | "+" 0.99 |
| 1000 km Nürburgring / Driver Database driver ID | "1000 24 n" 0.62 | (no pred) |
| 17º Stormo Incursori / official website | "https www comu" 0.46 | "https :// www" **0.92** |
| 17º Stormo Incursori / population | (no pred) | "+" 1.00 |
| 1966–67 Cupa României / Freebase ID | "m" 1.00 | "/ m / 0 c _ _" 0.42 |
| 1970–71 DFB-Pokal / point in time | "1970 01 01t00" 0.31 | "+" 1.00 |

### What v6 actually fixed

- **Accents survive.** v5 dropped them at the regex stage: "Didier André" tokenised to `["didier", "andr"]` because `é` doesn't match the word-character class. v6's BPE keeps `é` as its own piece. So v5's emit for the image-of-Didier prediction was `"didier andr"`; v6 either emits the right thing or nothing.
- **Coverage gains on identifier-shaped predicates.** v6 produces a confident `"https :// www"` for the official website where v5 was at 0.46. Cases where v5 said "(no pred)" because the candidate predicate had zero word-vocab overlap now succeed because BPE always has *some* subword to encode through.

### What v6 looks like it broke (it didn't)

- **The `"+"` predictions on date-shaped predicates.** Wikidata serialises dates as `"+1970-01-01T00:00:00Z"`. The leading `+` is a high-frequency BPE token, and the per-token-floor `0.05` breaks decoding the moment the *next* token's probability dips. With BPE the next token is one of dozens of digit pieces and routinely sits below the floor, so we stop after `"+"`. v5 didn't have this problem because its first emitted token was `"1970"` (a word-vocab piece), which carried more of the date's mass. **This is a decoder issue, not a v6 capability issue:** the model knows the date; the heuristic stops asking. Fix is to relax `per_token_floor` for BPE or use temperature-aware multi-step decode.
- **Truncated identifiers.** `"/ m / 0 c _ _"` for a Freebase ID is valid Freebase shape. v5's `"m"` is degenerately short. v6 is closer; the `_` tokens are BPE-internal noise that needs cleaning at decode.

### What this changes

- v6 is the new pinned default for inference (`MODEL.json` rev `v6-bpe`). v5 stays around for the date-format regression cases until the decoder catches up.
- The next quality lever is the **BPE-aware decoder**: `per_token_floor` and the early-stop heuristic in `predict_object` were tuned for word-level vocab where each emitted token carries roughly one fact's worth of probability mass. BPE pieces are sub-token, so the floor needs to scale with expected token length per role. Track this as a follow-up; not a queue item yet.
- The bigger corpus (queue #3, `--max-triples 50000000`) is now the highest-leverage move. v6 tokenisation is no longer the bottleneck.

---

## 2026-05-09 (later) — v5 trained: bigger model wins

Headline: **capacity was a real bottleneck for v4.** A 3× scale-up (d_model 256→512, layers 4→6, params 16M→44M) on the same 757k-triple corpus produced both lower final perplexity *and* qualitatively cleaner predictions. With cumulative repetition penalty 3.0 at decode time, v5 + decoder produces predictions that often pick the right *specific* entity, not just the right semantic category.

### Trajectory side-by-side

| Epoch | v4 ppl (16M, 4 layers) | v5 ppl (44M, 6 layers) |
|---|---|---|
| 1 | 1150.7 | 1528.7 |
| 2 | 196.0 | 147.3 |
| 3 | 133.5 | 104.2 |
| 4 | 100.7 | 90.7 |
| **5** | **92.5** | **84.85** |

v5 starts higher in epoch 1 (more parameters, harder optimisation landscape), crosses under v4 at epoch 2, and pulls ahead from there. By epoch 4 it had already passed v4's *final* perplexity. Wall time: 91 min vs v4's 42 min — 2.2× compute, 8% better final ppl.

### Predictions, same seed (42), same penalty (3.0)

| Subject / predicate | v4 (16M) | v5 (44M) |
|---|---|---|
| canton of Romilly-sur-Seine-1 / Commons category | "canton of of sur sur" | **"canton of"** (conf 0.882) |
| Comtesse de Die / educated at | "university of of of of of of of" | **"university of halle"** (conf 0.488; she was educated in Halle) |
| Zudar / area | (didn't pass threshold) | **"33"** (conf 0.901; numeric) |
| Meeuwen-Gruitrode / locator map image | "map of comune of meeuwen province province" | "map of comune of" (conf 0.685; clean truncation) |
| Curt Meyer-Clason / Commons category | "curt meyer clason" | "curt meyer" (conf 0.825) |
| Kosmos 116 / Commons category | (didn't pass) | **"kosmos 116"** (conf 0.740) |
| Centralbahnhof / Vikidia article ID | (didn't pass cleanly) | "fr" (conf 0.798) |
| Liriodendron tulipifera / African Plant Database ID | (n/a) | "liriodendron tulipifera" (conf 0.441) |

The bigger model is doing what bigger models do — picking specific real-world tokens (`halle`, `33`, `kosmos 116`) where the smaller one had to fall back to common connectors. Provenance edges (`propositionInferredFrom`) stay attached on every emit; v5 uses the same write-back schema as v4.

### HF snapshot status

**Blocked on auth.** The leaked write token from the v4 attempt has not been rotated yet. To complete: revoke the old token at https://huggingface.co/settings/tokens, create a fresh one, then `huggingface-cli login` (paste at the prompt — token never enters chat). After that, `python tools/hf_snapshot.py --user EmmaLeonhart --snapshot-name v5 --no-loka-data` adds v5 to the existing `EmmaLeonhart/loka` repo without re-uploading the 770 MB store.

### What this changes

- v5 (`training/checkpoints/wikidata_v5.pt`, 178 MB) is the new canonical "best" model. v4 stays around for A/B comparison.
- The next quality lever is no longer "more capacity" — it's the tokenizer (BPE/wordpiece would handle "Saint-Léger" → "Saint" "-" "Léger" instead of `saint l ger`) and the corpus (27,780 entities of 30M available is still a tiny slice).
- Fine-tuning track (`planning/fine-tuning-track.md`) is still the longer-horizon parallel option.

---

## 2026-05 — The neuro-symbolic world-model pivot

### What changed in framing

The earlier framing was "lean RDF-star triplestore that handles vector queries natively." That's still true mechanically, but the *purpose* moved: the engine is now one half of a two-system composition.

- **The store** = explicit memory. Stores what is known. Returns exact answers.
- **A small transformer trained from scratch on the same triples** = implicit memory. Predicts what is plausible. Returns inferred answers with cited inference chains.

Both expose the same SPARQL+ interface. The caller doesn't pick which system answered — federation is implicit, except through provenance edges on the result. Canonical vision: `planning/world-model-thesis.md`.

Product framing: **what Ollama is to LLMs, Loka is to world models.** Pull or train a world model locally; pluggable; agent-first; honest provenance. The "agent-first" stance was already baked into Loka; the world-model layer is what makes the whole project a thing you'd want to install rather than just a database.

The thesis explicitly *rejected* fine-tuning a general LLM on RDF (§6.6) for provenance, closed-world, and hallucination reasons. That rejection was revisited mid-period (see §10.5 of the thesis) and admitted as a parallel near-term track, for empirical pragmatism: small from-scratch models on small corpora produce word salad, while a fine-tuned 1–7B base could plausibly produce coherent triples in days. Both tracks share the same `propositionInferredFrom` output schema. See `planning/fine-tuning-track.md`.

### RDF-star is THE citation mechanism

RDF-star moved from "one feature among many" to **load-bearing**. It's how every kind of citation in the system is expressed:

| Verb | Used for | Emitted by |
|---|---|---|
| `propositionInferredFrom` | model-generated triple → context that informed it | `infer_with_citations.py` |
| Wikidata `wdt:P854` / `wdt:P248` / `wdt:P813` / ... | external curated references | importers |

All use the identical `<<S P O>> verb <<source>>` shape. Wikidata's API distinguishes "qualifiers" from "references" but Loka collapses both into the same RDF-star annotation form because they're semantically the same thing.

**Reserved namespace.** Every predicate under `http://loka.dev/provenance/` is system-internal. The world model **never** sees, proposes, or emits one. Three layers of enforcement:

1. Corpus stripping (`preprocess.py`) drops every row whose predicate matches the prefix.
2. SPARQL-star `FILTER NOT EXISTS << ?s ?p ?o >> propositionGenerated ?_g` excludes inner generated triples at query time.
3. Inference (`infer_with_citations.py`) refuses to consider reserved-namespace predicates as candidates and refuses to emit one even if a downstream bug allowed it.

Names are deliberately verbose (`propositionInferredFrom`, not `inferredFrom`) so a human scanning raw triples spots them at a glance. The discipline matters: if the model ever learned the provenance predicates exist, it could hallucinate fake citation edges, undermining the auditability that is the whole point of the system.

Hallucinated *content* in citations is **not** a blocker. A fabricated citation is still an RDF-star row pointing at concrete context — auditable, filterable, often informative about what the model thinks the reasoning is. Don't add elaborate guards.

### The data layer rebuild

The corpus underwent a complete rebuild over this period.

- **BFS importer learned RDF-star qualifiers + references.** Each Wikidata claim now emits the main triple plus an RDF-star annotation per qualifier *and* per reference snak, all sharing the `<<S P O>>` quoted-triple subject. Wikidata's `pq:` and `pr:` namespaces collapse into the same `wdt:` predicate URI on the annotation row — the qualifier-vs-reference distinction is structural (subject is a quoted triple), not lexical.

- **BFS → Hugging Face parquet stream.** Wikidata's API rate-limit (1.5s per request) made BFS the bottleneck — 5M triples needed days at that rate. Switched to streaming `philippesaade/wikidata` (CC0, ~30M entities, JSON-shaped per-entity rows in parquet) via the HuggingFace `datasets` library. Local-bandwidth-bound instead of API-bound. End state: 5,055,385 triples / 1,695,402 RDF-star annotations / 27,780 entities / 770 MB on-disk store.

- **`propositionImportedFrom` dropped.** Initially every imported triple got `<<S P O>> loka:propositionImportedFrom <wikidata.org/wiki/Q...>`. For a database where every row came from Wikidata, that's redundant noise — 22,593 rows (~46% of all annotations). The actual provenance is already in Wikidata's own reference predicates.

- **Multilingual labels — every language Wikidata has.** Previously hardcoded en/ja/de/fr/zh; now iterates every language in `entity.labels` and `entity.descriptions`. The training preprocessor still filters to English, but the database keeps the multilingual richness.

- **Embeddings: gone.** The original BFS importer called Ollama (mxbai-embed-large) per entity. The world-model loop tokenizes English labels — vectors don't enter the training corpus. Stripped from the importer. The HNSW index in `loka-core` stays — that's an engine feature, not specific to import.

### Two engine bugs surfaced

- **SPARQL `?s ?p ?o` occasionally returns literal values in the predicate slot.** RDF disallows literal predicates, so this is invalid output from the executor — almost certainly RDF-star annotation rows with positions getting confused. Filtered at preprocess (drops ~1% of rows on a 5M corpus). Real engine bug; fix later.

- **`POST /triples` wedges after roughly every 5–6× growth in stored triples.** Hit at ~174k and again at ~1M during the HF ingest. `/health` keeps responding, but `/triples` and SPARQL hang indefinitely until the server is restarted. On restart, all data is intact on disk. Symptoms point at LSM compaction or persistent-index rebuild holding the write lock. Workaround: an automated stop/restart loop. Real engine bug.

A separate proto-layer bug was found and **fixed** mid-period: `POST /triples` was returning HTTP 400 for the entire batch the moment any RDF-star annotation's inner triple already existed in persistent storage. The in-memory branch already discarded `DuplicateTriple`; only the persistent branch propagated. Fixed at `server.rs:935` and `:962` so both branches handle duplicates the same way.

### Training pipeline

Versioning: v0/v1/v2 were the early smoke-test checkpoints on a 6,300-triple shrine-only corpus. v3 onward use the 5M-triple HF-derived corpus.

| Model | Architecture | Corpus | Final ppl | Notes |
|---|---|---|---|---|
| v3 | d_model 256, 4 layers, 16M params | 779k label-substituted triples | 53.4 | Pre-cleanup; misleadingly low ppl from memorising `xmlschema decimal` URI fragments |
| v4 | same | 757k cleaned triples | 92.5 | Higher ppl, *better* output. Numerical regression masks real-quality improvement |
| v5 | d_model 512, 6 layers, **44M params** | same 757k | _in flight as of writing_ | Bigger-model experiment; 3× capacity |

**Two corpus quality fixes between v3 and v4:**

1. *Strip `^^<datatype>` suffixes from typed literals.* Loka's SPARQL serialisation embeds the datatype in the value string. Without stripping, literal values like `+1966-02-18T00:00:00Z"^^<http://www.w3.org/2001/XMLSchema#dateTime>` reached the tokenizer as if the URI fragments were entity content. The model dutifully memorised them and emitted predictions like `Abbas Mirza | has works in collection | 1 http www w3 org 2001 xmlschema decimal`. After stripping: `Abbas Mirza | has works in collection | metropolitan museum of museum`. The Met genuinely holds Abbas Mirza pieces — that's real cross-entity inference, not memorised junk.

2. *Drop rows with non-URI predicates.* The Loka SPARQL quirk above produced literal values in the predicate slot, ~1% of 5M.

**Inference quality lever: cumulative repetition penalty in `infer_with_citations.py`.** The masked-prediction objective doesn't penalise the model for emitting the same token over and over, so even when the model "knows" the answer is `university of <something>`, decoding produces `university of of of of of of of`. The penalty divides each repeated token's logit by `repetition_penalty ** count` (default 3.0, cumulative — 3rd repeat divides by 27, usually drops below per-token floor and breaks the loop). Same v4 checkpoint, smarter decoder. Loops collapse to clean shorter outputs.

### Inference loop closes end-to-end

`training/infer_with_citations.py` is the generative-citation entry point. For a candidate subject:

1. Find predicates used by graph-neighbors (subjects sharing at least one (p, o) statement) but missing from this subject.
2. Mask the object slot, run the trained transformer.
3. If mean per-token confidence ≥ threshold, emit the new triple plus four kinds of RDF-star annotations:

```
<S> <P> "predicted-label" .
<<S P "predicted-label">>  loka-prov:propositionGenerated     "true"^^xsd:boolean .
<<S P "predicted-label">>  loka-prov:propositionGeneratedBy   "wikidata_v4" .
<<S P "predicted-label">>  loka-prov:propositionConfidence    "0.43"^^xsd:decimal .
<<S P "predicted-label">>  loka-prov:propositionInferredFrom  <<S existing_p existing_o>> .
   ...one row per cited context triple (default 10)
```

`--post` writes the result back into Loka. The reserved-namespace machinery ensures the model never sees its own provenance edges in subsequent training runs.

**Quality sample at v4 (50 subjects):** 32/250 candidate predictions met confidence threshold 0.4. Of those, ~⅓ are recognisably correct in shape and content, ~⅔ have the right semantic *category* but degenerate decoding ("university of of of"), a handful are wrong/garbage. The cumulative repetition penalty collapses the looping cases without losing the right-category signal.

The loop is genuinely closed: model produces predictions → predictions land in Loka tagged `propositionGenerated true` with `propositionInferredFrom` edges → preprocessor's SPARQL-star filter excludes them on the next training pass. Self-citing inference per `world-model-thesis.md` §5.5, at v0 fidelity.

### Loka — the rebrand

"Loka" is a name for the engine. The project that's emerging (engine + corpus + trained world model + inference layer) needs its own identity. **Loka** is the name on Hugging Face; the GitHub repo will be renamed to match later.

`tools/hf_snapshot.py` pushes corpus + checkpoints to a single dataset repo `<user>/loka` with each upload tagged as a snapshot revision (`v3`, `v4`, etc.). Each upload is a commit; tagged snapshots are pullable via `revision="v4"`. LFS is handled transparently by `huggingface_hub`. First push of v4 got 7 of 8 things up before the file-lock on the live `loka-data/db` blocked the folder upload — added `--loka-data-path` so future pushes can point at a frozen backup directory instead of the live store.

---

## 2026-04 — ManuForge integration testing → v0.3.7

Brief period of production-readiness fixes after testing Loka against the ManuForge SDK consumer. Surfaced a small set of real issues:

- **RDF-star query support fixes** — quoted-triple wildcards weren't matching correctly.
- **HTTP star import** — N-Triples-star payloads via `POST /triples` had edge cases on parsing.
- **CRLF line endings** — Windows clients sending CRLF-terminated N-Triples weren't being recognised.
- **Self-update asset bug** — release-pipeline self-update was downloading the wrong artifact name.
- **Import error reporting** — the response now lists per-line errors rather than failing the whole batch.

Bumped to **v0.3.7** at the end of this round. The ManuForge integration also produced `docs/AGENT_SETUP.md` for AI-agent consumers and a "limitations found in production" note that fed into the next round.

---

## 2026-03 (late) — Ontochronology + Loka Studio + FFI

After v0.2.0, two large pieces landed in parallel:

### Loka Studio (Flutter desktop/web/mobile client)

`loka-ffi` crate wraps the engine in a C-compatible shared library so non-Rust consumers (Flutter, in particular) can embed the database in-process. Studio uses `dart:ffi` to load `loka_ffi.dll`/`.so`/`.dylib` and runs the engine on a background thread sharing the same handle as the optional MCP server. Two entry points:
- `loka mcp` → MCP + database, no GUI
- Loka Studio → GUI + database + optional MCP server, all one process

Studio also auto-starts the server in serverless mode when launched, so the user never has to run `loka serve` manually. Auto-update keeps Studio in sync with the CLI version. Includes graph view (D3 then vis-network), HNSW health diagnostics, OWL/Turtle export, dark/light theme, persistent connection settings. Launch via `loka mcp --studio` or via the agent-installer's `download_studio` and `launch_studio` MCP tools.

### Ontochronology

A non-trivial extension: every triple is conceptually contained in a temporal interval, and queries can ask "what was true at time T" or "what changed between T1 and T2" without reifying every statement individually. Implementation phases:

- **Phase 1–3** — temporal literal type, predicates (`loka:assertedAt`, `loka:validFrom`, `loka:validTo`), TSPO index.
- **Phase 4a** — `AT_TIME` and `DURING` query operators.
- **Phase 4b** — `WORLD_STATE` and `TEMPORAL_DIFF`.
- Temporal-aware property path traversal.

Containment semantics use three-valued query logic (true / false / unknown). Design lives in `docs/ontochronology.md`.

### Other March-late items

- **Pseudo-tables to deep subgraph columnar indexes** — generalised the columnar shortcut so multi-hop subgraph queries can run vectorised SIMD scans where the structure repeats.
- **Cost-based query planning** — predicate pushdown, HNSW edge labelling, join-strategy selection, hash join optimization for large intermediate result sets.
- **Vector SPARQL operators** — `COSINE_SEARCH`, `EUCLID_SEARCH`, `DOTPRODUCT_SEARCH`.
- **ACID compliance** — atomic transactions, durability, isolation. `PersistentStore.clear()` and GSP DELETE durability fixes.
- **Self-update + version check + HNSW rebuild endpoint.**
- **Theory pages on loka.emmaleonhart.com** — 18+ explainer pages: HNSW-in-RDF, four-index architecture, RDF-star edges, SPARQL exit conditions, hybrid databases, traversal indexing, cost-based planning, etc.
- **Code of Ethics page** — Buddhist/Shinto-techno-animist framing, deadpan style.

---

## 2026-03 (mid) — v0.2.0 Developer Preview

A consolidating release. Headlines: query planner, agent installer, Java SDK, Loka Studio first cut. All four SDKs (Go, Rust, Java, .NET) had endpoint mismatches caught and fixed during this period. SDK publish workflow + integration test CI added.

Pseudo-tables landed in this window too: columnar indexes with zonemap pruning and vectorized scans, on top of the standard SPO/POS/OSP indexes. Designed to make multi-hop subgraph queries (the kind RDF databases are typically slow at) competitive with property-graph databases.

Released as **v0.2.0** Developer Preview on 2026-03-18.

---

## 2026-03-15 — The SPARQL completeness sweep

A single very productive day. Brought SPARQL coverage from "minimum viable" to roughly feature-complete for SPARQL 1.1 over RDF-star.

### Core engine
- **First-query cold-start fix** — replaced dense `Vec<bool>` visited list with `HashSet`, cut ~2s page-fault overhead at 200K+ HNSW nodes.
- **HNSW cross-cluster search** — multiple entry points (up to 8), score all and start from best. Fixed a long-standing bias toward the first-inserted cluster.
- **Persistence** — `PersistentStore` (sled-backed) wired to the HTTP server with write-through. In-memory stores hydrate on startup. Data survives restart.
- **Blank node support** in the N-Triples parser.
- **Query timeout** — `execute_with_timeout()` with per-pattern deadline checks and `SparqlError::Timeout`.
- **SIMD-accelerated distance functions** — AVX2/FMA + SSE fallback for `dot_product`, `squared_euclidean`, `l2_norm`.
- **HNSW rebuild from stored vector triples on startup** — vectors persist; the index is reconstructed lazily.
- **HNSW compaction** — background pass to clean tombstoned nodes, plus `/vectors/health` endpoint for diagnostics.
- **Hash join optimization** for large intermediate result sets.
- **Cardinality estimation** for cost-based planning.
- **Crash recovery** — `verify_consistency()` and `repair()` for index integrity.
- **Adjacency lists** materialized for Neo4j-speed traversal.
- **Parallel HNSW construction** via rayon.

### SPARQL completeness
- `FILTER NOT EXISTS` / `EXISTS` with sub-pattern evaluation and `LIMIT 1` push-down.
- `ASK` queries.
- `GROUP BY` + aggregates (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`, with `DISTINCT`).
- `BIND(term AS ?var)` and `VALUES ?var { ... }`.
- Boolean operators (`&&`, `||`, `!`) in `FILTER`.
- String functions (`CONTAINS`, `STRSTARTS`, `STRENDS`, `REGEX`).
- Comparison operators (`>=`, `<=`).
- Type checks (`isIRI()`, `isLiteral()`).
- `LANG()` / `LANGMATCHES()`.
- `INSERT DATA` / `DELETE DATA` (SPARQL Update).
- `CONSTRUCT` and `DESCRIBE`.
- `HAVING` clause for `GROUP BY` filtering.
- Property paths (`+`, `*`, `?`, `/`).
- Subqueries (nested `SELECT`).
- `DATATYPE()`, `STR()`, `COALESCE()`, `IF()`.
- Arithmetic in `FILTER` (`+`, `-`, `*`, `/`).
- **RDF-star quoted triple patterns** in SPARQL (`<< ?s ?p ?o >>`).

### CLI, distribution, and protocols
- `loka import` (streaming line-by-line N-Triples to sled).
- `loka export` (Turtle/N-Triples).
- `loka info` (triple/term counts).
- `loka install-agent` — agent-first installer that reasons through configuration and writes `<dbname>_loka_notes.md` with its decisions.
- Install scripts (`install.bat`, `install.sh`).
- Dockerfile (multi-stage, exposes 3030, `/data` volume).
- `GET /graph` (Turtle/N-Triples export — Protégé integration point).
- `/sparql.csv` and `/sparql.tsv` formats.
- `/sparql.xml` (SPARQL Results XML).
- Content negotiation via `Accept` header.
- Service description at `/service-description`.
- Graph Store Protocol (`GET`/`PUT`/`DELETE /graph-store`).
- Simple passcode auth (server mode, opt-in).
- Rate limiting (server mode, opt-in).
- Periodic backups (server mode, configurable hourly/daily).

### Ecosystem
- **Protégé plugin** — Java OSGi bundle: Connect/Start, Load from Loka, Save to Loka, OWL Validate.
- **MCP server** for AI-agent ↔ Loka integration. `loka mcp` runs the engine + MCP in one process.
- **Client-side OWL validation in Python SDK.**
- **owl:equivalentClass / owl:sameAs / owl:inverseOf / rdfs:subPropertyOf** support added to SDKs.
- **OWL verification query generation** — turn ontology constraints into SPARQL ASK queries.
- **Schema declaration via SPARQL `INSERT DATA`** — vector predicate dimensions, etc.
- **N-Quads parser** with named graph support.
- **Turtle parser** for bulk import.
- **RDF/XML parser** for OWL ontology imports.
- **JSON-LD parser.**
- **LangChain VectorStore integration** for Loka.
- **Jupyter `%%sparql` cell magic.**
- **Japanese label embedding script.**

### First Wikidata BFS import
On 2026-03-15: **439 entities / 16,084 triples / 439 vectors** (1024-dim mxbai-embed-large) from the Engishiki Jinmyōchō (Q11064932) BFS, 0 errors, 7,316 entities remaining in queue. Later abandoned as the corpus base in favour of the HF parquet stream (see 2026-05) — the BFS rate limit made it impractical to scale.

---

## 2026-03 (early) — Foundation + scale stress test

Project began on **2026-03-13** with the cleanvibe scaffold. Within 24 hours: architecture docs, normalised `loka-*` workspace structure, `loka-core` and `loka-hnsw` foundations. Apache 2.0 license. CI workflow. Borrowed patterns explicitly from Qdrant (HNSW: immutable `GraphLayers` for search, thread-local visited pools, per-node `RwLock` during construction) and Jena TDB2 (storage, IRI interning, sled triplestore baseline).

Subsequent days landed:
- **Sled-backed persistent triple store.**
- **SPARQL parser, query planner, executor.**
- **HTTP server + CLI** with SPARQL endpoint.
- **Vector SPARQL integration** — connecting HNSW to the query engine. Architectural decisions documented (`docs/vectorSPARQL.md`): subject-bound-before-`VECTOR_SIMILAR` runs graph first, subject-unbound runs vector search first.
- **REST endpoints** for triple insertion + N-Triples parser.
- **Serverless-by-default philosophy** + `.sdb` file extension. Single-binary, embed-or-serve.
- **Vector architecture fix** — vectors are graph objects, not standalone. Every vector insertion now creates a corresponding triple, and the graph browser doesn't try to expand vector literal nodes.
- **GitHub Pages landing page** + Open Graph meta tags + 18+ theory pages.
- **Client SDKs in six languages** — Python, Go, Rust, Java, .NET, TypeScript.
- **Browser graph debug tool** (D3 force, later vis-network).
- **1M embedding stress test** — first hard scale check. Uncovered three performance issues that all got fixed in the same window. Final stress test passed all 14 queries with zero failures.
- **HNSW edges as RDF triples** — query the index structure itself via SPARQL.
- **Mutex → RwLock** for concurrent reads.
- **Documented architectural decisions:** Oxigraph as the reference for storage/indexing patterns; RDF-star as the reification model (vs RDF 1.2); SPARQL+ as the query language with extensions for vector and exit conditions; SQL/MongoDB query interfaces *permanently rejected* (offering them would mislead AI agents into relational/document thinking).

By the end of the first 48 hours the project had: a working engine, a working SPARQL surface, a working vector layer, persistence, six SDKs, CI, a website, and a stress test passing at 1M scale. That set the pace for everything that came after.

---

## Reference: how to read this document going forward

- **Newest entries at the top.** Drop a new dated section above existing ones when something meaningful lands.
- **Narrative, not flat lists.** Per-commit detail belongs in `git log`. Devlog entries explain *why*.
- **Headlines first.** A reader skimming for "what changed in the last month" should be able to get it from the first paragraph of each section.
- **Rebrand reminder.** "Loka" still appears in code and on the website; "Loka" is the model/data distribution name, currently only on Hugging Face. The repo rename is pending.
