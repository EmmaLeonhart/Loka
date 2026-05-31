# SDK publish-readiness — findings (2026-05-30)

Audit of the two SDKs Emma wants published — **Python → PyPI** and **TypeScript → npm**.
Static analysis of HEAD `8929fed` (reads cross-checked after a tool-channel integrity
test passed: 3× identical sha256). **No publish performed.** Publishing is
outward-facing/irreversible and needs Emma's sign-off + registry accounts.

## How the publish pipeline actually works

`.github/workflows/publish-sdks.yml`, triggered on `push: tags: 'v*'`. Every job is
`continue-on-error: true` (one registry failing doesn't block the others). **The version
is taken from the git tag**, overwriting the manifest value at publish time — so the
manifest `version` fields (Python `dynamic`, TS `0.1.0`) do **not** block a publish; the
tag drives it.

- **Python → PyPI:** uses **OIDC trusted publishing** — `permissions: id-token: write`
  + `pypa/gh-action-pypi-publish@release/v1`, **no token env**. `python -m build` then
  the action uploads.
- **TypeScript → npm:** `npm install` → `npx tsc` → `npm publish --access public`, gated
  on `NODE_AUTH_TOKEN: secrets.NPM_TOKEN`. Uses `npm install` (not `npm ci`), so **no
  `package-lock.json` is required.**

## Per-SDK current state (HEAD)

| | Python (`sdks/python/`) | TypeScript (`sdks/typescript/`) |
|---|---|---|
| name | `loka` | `loka` |
| version | `dynamic` (set from tag) | `0.1.0` (overwritten by tag) |
| license | **`Apache-2.0`** ❌ | **`Apache-2.0`** ❌ |
| build | hatchling | `tsc` |
| README | present | present |
| URLs | Homepage `loka.org`, Repo | Repo |

## Blockers to a clean first publish

1. **License mismatch (all 5 SDKs).** Python/TS/Rust/Java/.NET manifests all declare
   `Apache-2.0`; the project is **AGPL-3.0-or-later** (relicensed 2026-05-27, PR #10,
   which by its own commit message only touched `LICENSE` + workspace `Cargo.toml` +
   README). Publishing an SDK that misdeclares its license is wrong. **→ Emma's decision:
   align all SDK manifests to AGPL (recommended — completes the relicense), or keep the
   SDKs deliberately Apache-2.0.** Until resolved, do not publish.
2. **PyPI setup is trusted-publishing, but the docs say "token".** `docs/SDK_PUBLISHING.md`
   + `docs/SDK_ACCOUNTS_SETUP.md` instruct creating a `PYPI_TOKEN` GitHub secret, but the
   workflow uses OIDC trusted publishing. The real setup is **PyPI-side**: register a
   trusted publisher for project `loka` pointing at repo `EmmaLeonhart/Loka`, workflow
   `publish-sdks.yml`. **→ fix the docs** (a `PYPI_TOKEN` secret would sit unused), and
   the trusted publisher must be configured on PyPI before the first tag.
3. **npm account + `NPM_TOKEN` secret** must exist for the TS publish to run (the step is
   gated on the token being present).
4. **Name availability — checked 2026-05-31:** `loka` is **available on PyPI**
   (`https://pypi.org/pypi/loka/json` → HTTP 404) but **taken on npm** (published, latest
   `1.0.1`, an unrelated "global variables" package). So the **Python SDK can publish as
   `loka`**; the **TS SDK needs a different npm name** — a rename, or (cleaner) an owned
   scope like `@emmaleonhart/loka`. Only `sdks/typescript/package.json` (`name`) must
   change before the first `v*` tag; `sdks/python/pyproject.toml` can keep `loka`.
   **→ Emma's call on the npm name** (and confirm she wants `loka` on PyPI).

## What is NOT a blocker (checked, fine)

- Manifest version skew (tag-driven). No `package-lock.json` needed (`npm install`).
  Metadata otherwise well-formed (keywords, classifiers, README, repo URL present).

## Recommended order (all gated on Emma)

1. Decide the license question (blocker #1) → if AGPL, mechanical one-line-per-file edit
   across the 5 manifests, CI-verified.
2. Fix the PyPI docs (trusted-publishing, not token) and configure the trusted publisher
   on PyPI; create the npm account + `NPM_TOKEN`.
3. ✅ Checked 2026-05-31 — PyPI `loka` available (404), npm `loka` taken (v1.0.1). Python can keep `loka`; pick a new npm name / scope for the TS SDK (blocker #4).
4. Local dry-runs (`python -m build` + `twine check`; `npm pack`) — light, can run on the
   laptop — to confirm a clean artifact before the first `v*` tag.
5. Tag → publish. (First publish is the irreversible step; needs Emma's explicit go.)
