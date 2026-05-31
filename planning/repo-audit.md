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

  > **Update 2026-05-30:** the Flutter Studio under `loka-studio/` was deleted per
  > Emma's call (B-4 below); `loka-studio/` now holds only `electron/` (the desktop
  > shell for `web-studio/`). Correction to the table below: `electron/node_modules/`
  > was **never git-tracked** (it is gitignored — only present on disk), so the
  > "committed node_modules" line in Category A was a mis-read and is struck.

  | dir | files | what it is |
  |---|---:|---|
  | `loka-studio/` | 92→7 | **Flutter Studio DELETED**; now just `electron/` (shell for `web-studio/`) |
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

1. ~~**`loka-studio/electron/node_modules/`** — committed `node_modules`.~~ **WRONG —
   not git-tracked.** Re-checked: `git ls-files loka-studio/electron/node_modules/*`
   returns 0; the dir is gitignored and only present on disk. The Glob that surfaced
   LICENSE files was scanning the working tree, not the index. No action needed.
2. **`loka-retrieval-data-stale-20260520/`** — the 2026-05-20 vector-registry forensic
   artifact is already reduced to a 1 KB `conf` husk (the 93.7 MB sled data is gone, and
   the diagnosis is preserved in the 2026-05-20 DEVLOG entry). `git rm -r` it; the
   `loka-retrieval-data*` glob should already be gitignored for runtime dirs.
3. **`\357\200\277qp`** — a tracked file with a mojibake name (bytes `EF 80 BF` =
   U+F03F private-use + `qp`). Almost certainly an accidental commit. Read its bytes to
   confirm it carries nothing, then `git rm`.

## Category B — RESOLVED

4. **The Flutter Studio (`loka-studio/`).** ✅ **DONE 2026-05-30 — Emma chose B-i
   (delete entirely):** *"delete the Flutter Studio tree. We don't need it because
   everything is an electron."* Removed `loka-studio/{lib,windows,macos,linux,web,test}`,
   `pubspec.{yaml,lock}`, `.metadata`, `analysis_options.yaml`, `README.md`,
   `.gitignore`. Kept `loka-studio/electron/` (the desktop shell) — verified it serves
   `web-studio/` via `run-js.js` (`STUDIO_WEB_ROOT=../../web-studio`), so the running
   Studio is unaffected. Repointed `electron/server.js` default root to `web-studio/`,
   updated `main.js`/`package.json`/`README.md`, and removed the Flutter `build-studio`
   job from `.github/workflows/release.yml`.

   **Follow-up (TODO.md):** the release no longer ships a built desktop Studio. A new
   release job must package the Electron Studio into per-platform installers
   (electron-builder or equivalent), verified on a test tag, before being re-added to the
   release assets.

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

## Execution order

1. ~~B-4 Flutter deletion~~ ✅ done 2026-05-30.
2. Category A ✅ complete 2026-05-30: A-2 (`loka-retrieval-data-stale-20260520/` husk)
   removed; A-3 (mojibake `\357\200\277qp` file) removed after confirming it was a 0-byte
   empty file (git empty-blob `e69de29`). A-1 was a mis-read (struck).
3. C-5 (`loka-ffi` orphan check — now more likely orphaned since the Flutter FFI
   consumer is gone; trace remaining consumers before removing), C-6 (stale root-level
   benchmark JSONs) → fold results back here.
4. Electron Studio installer release job (see TODO.md) — needs a verified test tag.
5. C-7 (`.git` history rewrite) → TODO.md only.
