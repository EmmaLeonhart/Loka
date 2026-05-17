# Site restructure → the shared branding kit

Reconstruct the Loka site onto the canonical shared visual kit
documented + demonstrated at <https://emmaleonhart.com/branding/>.
The branding page itself names this as Loka's outstanding work:

> **Loka** — Has the great buttons (now the shared ones) + shared
> palette. Its page layout is still its own — the remaining
> reconstruction to this structure.

## Canonical kit (extracted verbatim from the branding page)

**Head / pipeline**
- `<html lang="en" data-theme="dark">`
- Inline pre-paint theme script (first in `<head>`):
  `localStorage 'theme' → documentElement[data-theme]`, dark default.
- `preconnect`×2 + Google Fonts `<link>` (Inter / Instrument Serif /
  JetBrains Mono).
- `<link rel="stylesheet" href="/identity.css">` — the ONE shared
  file (palette, `.btn`, `.theme-toggle`, `.eyebrow`, `.card`,
  `.aurora`, type). Linked, never copied.
- Page-local `<style>` only for page layout + the THREE kit
  components not yet in identity.css: `.site-nav`, `.search`,
  `.repo-widget`.

**Body skeleton**
1. `<nav class="site-nav" aria-label="Primary">` — fixed translucent
   bar (`backdrop-filter: blur(12px)`, `color-mix bg 72%`):
   `a.brand` (mono) · `.nav-spacer` · `label.search` (Material box,
   152→232px on focus) · `.nav-right` { `button#theme-toggle
   .theme-toggle` + `a.repo-widget.js-repo` }.
2. `<div class="aurora" aria-hidden="true"></div>`
3. `<div class="container">` (max 880px, z-index 2):
   - `<header class="hero">`: `svg.glyph` (cosmic orbital, `spin 60s`,
     hidden ≤720px / reduced-motion), `span.eyebrow`, `h1` (gradient
     text-fill, `.serif` italic span), `p.tagline`.
   - `<section class="section">`× N: `h2` = mono uppercase + `span.num`
     `01 —` accent + trailing gradient rule (`h2::after`). Card grids
     use `.kit` → `.card k-a|k-b|k-c|k-d` aurora-boxes, each with a
     `.k-emoji`. "Everything either has an Aurora or an emoji."
   - `<div class="sig">` — mono signature line.
4. Scripts: theme toggle; `#kit-search` live filter
   (`[data-kit]` + textContent, `.hide`); GitHub widget live facts —
   `api.github.com/repos/<O/R>` → `.rw-stars`/`.rw-forks`,
   `/releases/latest` → `.rw-version` (`.rw-version-wrap.is-empty`
   stays hidden when no release).

**`.repo-widget` markup** (the prominent Material widget — replaces
the small `.gh` pill): `span.rw-icon`(octocat) +
`span.rw-body`{ `span.rw-repo` "Owner/Repo" + `span.rw-facts`{
version·stars·forks `.rw-fact`, each svg + count span } }.

## Current Loka state (the gap)

- Homepage + `/contribute/` link `/identity.css` directly + inline
  component-var map; the other ~37 link `/style.css` (→ @import
  identity.css). `/contribute/` is intentionally self-contained.
- Nav today: `<nav class="sitenav">` text links + the small `.gh`
  pill (from `unify_site.py`). No top-bar, no search, no
  `.repo-widget`, no glyph, no numbered sections, no sig.
- `pages/identity.css` (local copy of the shared file) has palette /
  `.btn` / `.theme-toggle` / `.eyebrow` / `.card` / `.aurora` / `.gh`
  / type — NOT `.site-nav`/`.search`/`.repo-widget` (consistent with
  the branding page defining those page-locally for now).

## Plan

**Phase A — shared kit layer (one file → all pages).**
Add `.site-nav` + `.search` + `.repo-widget` + `.hero`/`.glyph`/
`.eyebrow` refinements + numbered `h2`/`.section` + `.sig`
(+ `.kit`/`.card k-*` if a page uses them) to `pages/style.css`
(the Loka shared layer over identity.css). One edit propagates to
every page that links it. Keep semantic `--green/--orange` etc.

**Phase B — converge linking.** Every page links `/style.css` so it
inherits the kit. Homepage keeps `/identity.css` direct **plus** the
kit (inline the kit there, or add `/style.css`); `/contribute/` stays
self-contained → inline the kit components there too.

**Phase C — transformer** `scripts/restructure_site.py` (idempotent
sibling of `unify_site.py`): per page, replace the old
`<nav class="sitenav|...">…</nav>` with the canonical `.site-nav`
top-bar markup (brand + search + theme-toggle + `.repo-widget`
pointed at `EmmaLeonhart/Loka`), ensure `.aurora`, append the
search-filter + repo-facts scripts, retire the `.gh` pill + back-link
helper. Re-runnable; `--check` is a no-op when done.

**Phase D — homepage as the showcase.** Hand-finish `pages/index.html`
to the full hero (cosmic glyph + eyebrow + gradient h1 + tagline) and
numbered sections, as the approved reference; mechanise the rest.

**Phase E — verify.** `restructure_site.py --check` no-op across all
pages; `unify_site.py --check` still no-op; spot-check render in both
themes; commit; note Pages-deploy verify (sister-subdomain caveat
already in queue.md).

## Decisions (resolved with Emma 2026-05-17)

1. **Search → real site-wide search.** A build step
   (`scripts/build_search_index.py`) walks `pages/**/*.html` and emits
   `pages/search.json` (per page: url, title, headings, section text,
   anchors). The `.search` bar box does client-side search-as-you-type
   with a `.search-results` dropdown that jumps to any page/section.
   Fully static, agent-operable, regenerated in CI / by the
   transformer's verify step.
2. **Hero/glyph → homepage only.** `pages/index.html` gets the full
   hero: spinning cosmic `.glyph` + `.eyebrow` + gradient `h1` +
   `.tagline`. Sub-pages get the same `.site-nav` bar + a lighter
   header (`.eyebrow` + gradient `h1`, **no** glyph). All pages:
   `.site-nav` + `.aurora` + `.sig`.
3. **Breadth:** roll out to all 39 pages this pass — homepage
   hand-finished as the showcase (Phase D), sub-pages mechanised by
   `scripts/restructure_site.py` (Phase C).

## Status — SHIPPED 2026-05-17 (commits d4d4c17 + this one)

All phases done. 38 pages on the shared kit (36 mechanised + homepage
+ /contribute/); `playground.html` kept as the full-screen IDE shell
(shared palette only, no fixed bar — would break its 100vh layout).
`build_search_index.py` → `search.json` (303 records). All three
transformers (`restructure_site.py`, `unify_site.py`,
`build_search_index.py`) are mutually idempotent `--check` no-ops;
`unify_site.py` step 6 stands down on kit pages so the two passes no
longer fight over `<nav>`.

Deferred polish (not blocking): numbered `.section` wrapping of the
homepage body; visual QA of `/contribute/`'s bespoke layout under the
kit; browser render-check in both themes; the GitHub-Pages custom-
domain deploy verify (sister-subdomain caveat in queue.md).
