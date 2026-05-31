# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## Completed in this session (2026-05-31)

- [x] **SDK license alignment** — All 5 SDK manifests aligned to `AGPL-3.0-or-later`.
- [x] **SDK local dry-runs** — `python -m build` and `npm pack` successful.
- [x] **Loka Studio de-bloat** — HNSW/embedding debug surface hidden behind a toggle in the Health screen.
- [x] **Recursive deletion (Retract)** — Ported the "Retract (cascade)" action to the JS Studio (`tools/browse.html`).
- [x] **Electron Studio release packaging** — Added `electron-builder` and updated `release.yml` + `loka.iss` to bundle the Studio in the Windows installer.

---

## Open engine bugs

### Engine bug #1 sustained-ingest verification (open)

Probable fix shipped in `c36760b`: explicit `sled::Config` with 256 MB cache, 2 s flush, `Mode::HighThroughput`. Reopen-in-place verified 2026-05-13 (WAL replay recovered 32,877,248 triples). Residual question: does the tuning also hold against fresh sustained ingest past 32.88 M triples? If a re-test ingest panics at the next plateau, escalate to RocksDB migration (sled 0.34 unmaintained since 2021). Not blocking under the current base+retrieval pivot.

---

## SDK publish-readiness audit — Python (PyPI) + TypeScript (npm)

Steps:

1. [x] **Current-state read.** Full findings in `planning/sdk-publish-readiness.md`.
2. [x] **Gap list:** for each SDK, the concrete blockers to a clean first publish. (2026-05-31: License mismatch fixed, PyPI docs fixed).
3. [x] **Local dry-run (no upload):** `python -m build` and `npm pack` success (2026-05-31).
4. [x] **Write findings** to `planning/sdk-publish-readiness.md`.
5. [ ] **STOP before publishing.** Surface the readiness verdict + the accounts/secrets Emma must set up.

---

## Passive follow-ups

- **Donor clean-Adam 10-epoch v14** via `tools/contribute_v14_training.py` — explicit successor experiment per paper §5.12. GPU-gated.
- **Clean v12 retrain** — epoch-4 best 226.86 lost to shared-GPU contention. GPU-gated.
- **Propgen test (Q42 seed) on v11–v14** — deferred since v11 due to GPU fragility during shared use. GPU-gated.

---

## Reference

- **`TODO.md`** — longer-horizon work.
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
- **`planning/cascade-retraction.md`** — spec for the shipped retraction system.
- **`planning/base-retrieval.md`** — spec for the shipped base+retrieval pivot.
