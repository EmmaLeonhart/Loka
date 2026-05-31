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
| license | **`AGPL-3.0-or-later`** ✅ | **`AGPL-3.0-or-later`** ✅ |
| build | hatchling | `tsc` |
| README | present | present |
| URLs | Homepage `loka.org`, Repo | Repo |

## Blockers to a clean first publish

1. **License mismatch (all 5 SDKs) — RESOLVED 2026-05-31.** All manifests aligned to
   `AGPL-3.0-or-later` to match the project's relicense.
2. **PyPI setup is trusted-publishing — RESOLVED 2026-05-31.** Docs corrected to
   trusted-publishing instructions.
3. **npm account + `NPM_TOKEN` secret** must exist for the TS publish to run.
4. **Name availability — checked 2026-05-31:** `loka` is **available on PyPI**
   but **taken on npm** (v1.0.1, unrelated). **→ Emma's call on the npm name**
   (e.g., `@emmaleonhart/loka`).

## What is NOT a blocker (checked, fine)

- Manifest version skew (tag-driven). No `package-lock.json` needed (`npm install`).
- **Local dry-runs — SUCCESS 2026-05-31.** `python -m build` produced
  `loka-0.3.1-py3-none-any.whl`; `npm pack` produced `loka-0.1.0.tgz`. Both
  artifacts are well-formed.

## Recommended order (all gated on Emma)

1. Decide the license question (blocker #1) → if AGPL, mechanical one-line-per-file edit
   across the 5 manifests, CI-verified.
2. Fix the PyPI docs (trusted-publishing, not token) and configure the trusted publisher
   on PyPI; create the npm account + `NPM_TOKEN`.
3. ✅ Checked 2026-05-31 — PyPI `loka` available (404), npm `loka` taken (v1.0.1). Python can keep `loka`; pick a new npm name / scope for the TS SDK (blocker #4).
4. Local dry-runs (`python -m build` + `twine check`; `npm pack`) — light, can run on the
   laptop — to confirm a clean artifact before the first `v*` tag.
5. Tag → publish. (First publish is the irreversible step; needs Emma's explicit go.)
