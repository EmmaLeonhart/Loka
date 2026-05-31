# Repository bloat audit — 2026-05-30

Scope: Emma's queue item — *"we have a lot of bloated content that doesn't really
belong in the repository… there's a lot of Flutter code, but there shouldn't be…
do an audit of all the stuff in the repository that we might be able to potentially
remove and not cause issues."*

This is the **audit**, not the removal. Removal of anything in category **B** below is a
product decision for Emma; removal of category **A** is mechanically safe but is still
staged as a follow-up so each deletion can be verified against CI (not just locally).

## Measured footprint (verified 2026-05-30, `git ls-files`, HEAD `474afc9`)

- **458 tracked files total.** `.git` is 141 MB on disk (`size-pack` 138.7 MiB) — the
  pack is large, almost certainly from binaries/data committed earlier in history.
  History rewrite (BFG / `git filter-repo`) is out of scope here; this audit covers
  the *working tree*, not history.
- File counts per top-level dir (largest first):

  | dir | files | what it is |
  |---|---:|---|
  | `loka-studio/` | 92 | **Flutter Studio** (frozen) + committed `electron/node_modules/` |
  | `training/` | 60 | training pipeline (live) |
  | `pages/` | 53 | website source (live, deploys to loka.emmaleonhart.com) |
  | `sdks/` | 47 | language SDKs (NPM + Python are the ones Emma still wants) |
  | `tools/` | 45 | operational scripts (live) |
  | `paper/` | 23 | clawRxiv paper (live) |
  | `loka-core` … `loka-ffi` | ~50 | the Rust workspace crates (live) |
  | `web-studio/` | 9 | **the live JS Studio** that replaced Flutter |
  | `loka-retrieval-data-stale-20260520/` | 1 | forensic-artifact husk (data gone) |

## Category A — mechanically safe to remove (regenerable or garbage)

Staged as follow-up queue items; each is a `git rm` + (where relevant) `.gitignore`
entry, then confirm CI green.

1. **`loka-studio/electron/node_modules/`** — committed `node_modules` (LICENSE/package
   files for `responselike`, `roarr`, `semver-compare`, `serialize-error`, `type-fest`,
   `undici-types`, `universalify`, `wrappy`, `yauzl`, …). Never belongs in git; fully
   regenerable via `npm install` from `electron/package.json`. **Caveat:** confirm the
   electron wrapper still wants those deps before stripping (it does `npm install` at
   build time → safe). `git rm -r --cached loka-studio/electron/node_modules` + gitignore.
2. **`loka-retrieval-data-stale-20260520/`** — the 2026-05-20 vector-registry forensic
   artifact is already reduced to a 1 KB `conf` husk (the 93.7 MB sled data is gone, and
   the diagnosis is preserved in the 2026-05-20 DEVLOG entry). `git rm -r` it; the
   `loka-retrieval-data*` glob should already be gitignored for runtime dirs.
3. **`\357\200\277qp`** — a tracked file with a mojibake name (bytes `EF 80 BF` =
   U+F03F private-use + `qp`). Almost certainly an accidental commit. Read its bytes to
   confirm it carries nothing, then `git rm`.

## Category B — needs Emma's decision (reverses a deliberate choice)

4. **The Flutter Studio (`loka-studio/`, ~92 files minus anything shared).** DEVLOG
   2026-05-17: *"The Flutter Studio is frozen as spec + fallback, not deleted."* The JS
   Studio (`web-studio/`) is the live path. So the Flutter tree is exactly the "Flutter
   code that shouldn't be here" Emma flagged — **but it was kept on purpose.** Options:
   - **(B-i) Delete the Flutter tree entirely** — accept that `web-studio/` is the only
     Studio going forward; drop the fallback. Biggest single bloat win (~90 files).
   - **(B-ii) Keep it frozen** — status quo; revisit once `web-studio/` reaches feature
     parity (the six tabs) and is the documented default.
   - **(B-iii) Archive it out of the main tree** — move to a `legacy/` dir or a separate
     branch/tag so history is preserved without cluttering the working tree.

   *Recommendation:* B-i once `web-studio/` is confirmed to cover the tabs Emma cares
   about (Knowledge Graph, SPARQL, Triples, Health, Ontology, Playground). Until then, do
   nothing — the freeze was a considered call.

## Category C — investigate before touching

5. **`loka-ffi/` orphan check.** CLAUDE.md still documents `loka-ffi` as the single-process
   engine that Flutter loads via `dart:ffi`. If the Studio is now `web-studio/` over HTTP,
   `loka-ffi` may have no consumer. But the MCP-in-process story and `loka mcp` may still
   want it. Do not remove without tracing consumers (`Cargo.toml` workspace members, any
   `.dll`/`.so` load in `web-studio/electron/`). Tie this to the B decision.
6. **Root-level loose artifacts** — `stress_test.py`, `stress_test_report.json`,
   `storage_benchmark_results.json`, `benchmark_results.json`. Check whether the
   benchmarks CI (`.github/workflows/benchmarks.yml`) regenerates/consumes these or
   whether they're stale one-offs. Move live ones under `benchmarks/`, remove dead ones.
7. **`.git` pack size (138 MiB).** If a slimmer clone matters for the "easy to download
   and run" goal, a history rewrite to drop large historical blobs is a separate,
   higher-risk task — list it in TODO.md, do not attempt inside a work-loop tick.

## Execution order (when unblocked)

1. Category A items 1–3 (one commit each, CI-verified) — safe, immediate.
2. Resolve B-4 with Emma → execute the chosen option.
3. C-5/C-6 investigations → fold results back here.
4. C-7 (history rewrite) → TODO.md only.
