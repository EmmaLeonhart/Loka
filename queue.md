# Loka — Work Queue

## ⭐ GO THROUGH THE QUEUE (pivot §5b, Emma 2026-07-20)

Standing top item: every cycle, actually work DOWN this queue.


**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ACTIVE — Emma, 2026-10-08 (strict order, top to bottom)

3. **Attempt every TODO.md item that is not computationally intensive** (GPU-gated items and the
   30M+ sustained-ingest check are excluded), in TODO.md order:
   a. Installer multi-model: models.toml as a list, CI pre-step generating the `.iss`
      components, mutually exclusive model components, chosen model id in
      `install-selection.toml`. No Inno Setup here: verify the generator, mark the `.iss`
      unverified until an rc build.
   b. v0.3.1 release checklist (Gradle merge, version bump, tag): check whether it's
      obsolete (current version 0.4.6); the tag itself is Emma's.
   c. Maintained distinct counts for adaptive execution: measure the insert overhead; build
      only if it's cheap and sampling shows noise.
   d. Deep pseudo-tables serving queries (`planning/deep-pseudo-table-serving.md`): store
      full column paths, exactness, freshness, chain recognition, tests.
   e. CLI health output: iterate the format from an agent's actual use.
   f. SDK publishing: re-verify readiness and dry runs; publishing itself is Emma's.
   g. Studio items (remote access, Dart FFI, embedded MCP, graph view parity, Protege):
      Flutter Studio was deleted, so map each to the Electron/web Studio or mark it obsolete,
      and attempt the ones that still apply.
   h. GQL → SPARQL transpiler, a first subset on the Cypher transpiler's template.
   i. Electron Studio desktop installers in `release.yml`: write the job. It can only be
      verified on an rc tag (Emma's call).
4. **Shut down all cron jobs.** One action after the attempts above, not a standing tail item.

---

## Reference

- **`TODO.md`** — longer-horizon work (includes the now-relocated engine-bug #1
  ingest-verification watch and the GPU-gated training follow-ups).
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
- **`planning/cascade-retraction.md`** — spec for the shipped retraction system.
- **`planning/base-retrieval.md`** — spec for the shipped base+retrieval pivot.
- **`planning/sdk-publish-readiness.md`** — SDK publish verdict (Emma-gated).
