"""Unify every Loka website page onto the shared render pipeline.

The Loka site is hand-authored static HTML (`pages/**/*.html`). The
canonical visual identity is `/identity.css` (shared, byte-identical
across emmaleonhart.com + sister sites) pulled in via `pages/style.css`
(the Loka layer: component vars, nav, typography). The homepage was
unified by hand; this script applies the same shell to every other
page, conservatively and idempotently:

  1. Pages that link NEITHER style.css nor /identity.css get a
     `<link rel="stylesheet" href="/style.css">` (absolute → works at
     any depth on the live site), so they inherit the canonical
     palette, fonts, nav and typography.
  2. `theme-color` meta `#58a6ff` → canonical `#8b9bff`.
  3. Embedded `-apple-system` / `SFMono-Regular` font overrides →
     `var(--sans)` / `var(--mono)`.
  4. GitHub-dark chrome hexes that ignore the theme toggle → the
     matching theme-flipping token.
  5. A `<div class="aurora">` right after `<body>` (if absent).
  6. The shared `← emmaleonhart.com` back-link as the first nav
     child, a `.spacer`, and the live `.gh` GitHub repo pill as the
     last nav child + the gh-facts script (if absent).

Re-runnable: every step is guarded, so running twice is a no-op.

Usage:  python scripts/unify_site.py [--check]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

PAGES = Path(__file__).resolve().parent.parent / "pages"

FONTS_LINK = (
    '<link rel="preconnect" href="https://fonts.googleapis.com">\n'
    '    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>\n'
    '    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=Instrument+Serif:ital@0;1&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">\n'
    '    <link rel="stylesheet" href="/style.css">\n'
)

AURORA = '\n  <div class="aurora" aria-hidden="true"></div>'

BACKLINK = '\n    <a class="home" href="https://emmaleonhart.com">&larr; emmaleonhart.com</a>'

GH_PILL = (
    '\n    <span class="spacer"></span>'
    '\n    <a class="gh" href="https://github.com/EmmaLeonhart/Loka" data-gh-repo="EmmaLeonhart/Loka" aria-label="EmmaLeonhart/Loka on GitHub">'
    '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>'
    '<span class="gh-name">GitHub</span>'
    '<span class="gh-stat gh-stars" hidden title="GitHub stars"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.75.75 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25Z"/></svg><span class="gh-stars-n">&ndash;</span></span>'
    '<span class="gh-stat gh-ver" hidden title="Latest release / tag"><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1 7.775V2.75C1 1.784 1.784 1 2.75 1h5.025c.464 0 .91.184 1.238.513l6.25 6.25a1.75 1.75 0 0 1 0 2.474l-5.026 5.026a1.75 1.75 0 0 1-2.474 0l-6.25-6.25A1.75 1.75 0 0 1 1 7.775ZM2.75 2.5a.25.25 0 0 0-.25.25v5.025c0 .066.026.13.073.177l6.25 6.25a.25.25 0 0 0 .354 0l5.025-5.025a.25.25 0 0 0 0-.354l-6.25-6.25a.25.25 0 0 0-.177-.073H2.75ZM6 5a1 1 0 1 1 0 2 1 1 0 0 1 0-2Z"/></svg><span class="gh-ver-n">&ndash;</span></span>'
    '</a>'
)

GH_JS = """  <script>
  (function(){
    var el=document.querySelector('a.gh[data-gh-repo]');if(!el)return;
    var repo=el.getAttribute('data-gh-repo');
    function put(sel,val){var s=el.querySelector(sel);if(!s)return;
      var n=s.querySelector('span');if(n)n.textContent=val;s.hidden=false;}
    fetch('https://api.github.com/repos/'+repo).then(function(r){return r.ok?r.json():null;})
      .then(function(d){if(d&&typeof d.stargazers_count==='number')put('.gh-stars',d.stargazers_count.toLocaleString());}).catch(function(){});
    fetch('https://api.github.com/repos/'+repo+'/releases/latest').then(function(r){return r.ok?r.json():null;})
      .then(function(d){if(d&&d.tag_name){put('.gh-ver',d.tag_name);return;}
        return fetch('https://api.github.com/repos/'+repo+'/tags').then(function(r){return r.ok?r.json():null;})
          .then(function(t){if(t&&t.length&&t[0].name)put('.gh-ver',t[0].name);});}).catch(function(){});
  })();
  </script>
</body>"""

# GitHub-dark chrome hexes that do NOT flip with the theme toggle →
# the matching identity.css token (which does). Conservative: only the
# known chrome palette, applied case-insensitively.
HEX_TOKENS = {
    "#0d1117": "var(--bg)", "#0a0a0f": "var(--bg)", "#010409": "var(--bg)",
    "#0d0d12": "var(--bg)", "#161b22": "var(--bg-soft)", "#1c2128": "var(--bg-soft)",
    "#21262d": "var(--bg-card)", "#1a2332": "var(--bg-card)",
    "#30363d": "var(--border)", "#1e1e2a": "var(--border)", "#2d4a6f": "var(--border-hover)",
    "#21303f": "var(--border)",
    "#c9d1d9": "var(--text)", "#e6edf3": "var(--text-strong)", "#f0f6fc": "var(--text-strong)",
    "#8b949e": "var(--text-mute)", "#6e7681": "var(--text-faint)", "#7d8590": "var(--text-mute)",
    "#58a6ff": "var(--accent)", "#79c0ff": "var(--accent-bright)", "#1f6feb": "var(--accent)",
    "#388bfd": "var(--accent)",
}

FONT_FIXES = {
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif": "var(--sans)",
    "-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif": "var(--sans)",
    "'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace": "var(--mono)",
    "'SFMono-Regular', Consolas, monospace": "var(--mono)",
    "'SF Mono', SFMono-Regular, Consolas, monospace": "var(--mono)",
    "'Segoe UI', -apple-system, BlinkMacSystemFont, sans-serif": "var(--sans)",
    "'Segoe UI', system-ui, -apple-system, sans-serif": "var(--sans)",
}


def unify(html: str) -> str:
    # 2. theme-color
    html = html.replace('name="theme-color" content="#58a6ff"',
                         'name="theme-color" content="#8b9bff"')

    # 3. embedded font overrides
    for bad, good in FONT_FIXES.items():
        html = html.replace(bad, good)

    # 4. non-flipping chrome hexes → tokens (skip data: URIs / SVG favicons)
    for hexv, tok in HEX_TOKENS.items():
        html = re.sub(re.escape(hexv), tok, html, flags=re.IGNORECASE)

    # 1. stylesheet wiring for pages that link neither shared sheet
    if "style.css" not in html and "/identity.css" not in html:
        html = html.replace("</head>", f"    {FONTS_LINK}</head>", 1)

    # 5. aurora right after <body ...>
    if 'class="aurora"' not in html:
        html = re.sub(r"(<body[^>]*>)", r"\1" + AURORA, html, count=1)

    # 6. nav back-link + spacer + gh pill (+ gh-facts script)
    if "data-gh-repo" not in html:
        m = re.search(r"<nav\b[^>]*>", html)
        if m:
            nav_open_end = m.end()
            close = html.find("</nav>", nav_open_end)
            if close != -1:
                html = (html[:nav_open_end] + BACKLINK
                        + html[nav_open_end:close] + GH_PILL + html[close:])
                if "</body>" in html:
                    html = html.replace("</body>", GH_JS, 1)
    return html


def main() -> int:
    check = "--check" in sys.argv
    changed = 0
    for path in sorted(PAGES.rglob("*.html")):
        if path == PAGES / "index.html":
            continue  # already unified by hand
        before = path.read_text(encoding="utf-8")
        after = unify(before)
        rel = path.relative_to(PAGES)
        if after != before:
            changed += 1
            if check:
                print(f"would change: {rel}")
            else:
                path.write_text(after, encoding="utf-8")
                print(f"unified: {rel}")
        else:
            print(f"unchanged: {rel}")
    print(f"\n{changed} file(s) {'would change' if check else 'changed'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
