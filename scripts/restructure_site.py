"""Reconstruct every Loka page onto the shared branding kit.

Sibling of `unify_site.py` (which put pages on identity.css). This one
swaps each page's bespoke `<nav>` for the canonical `.site-nav` top
bar — brand · nav-links · search · [theme-toggle · prominent
.repo-widget] — drops the old back-link / `.gh` pill / dead gh-facts
script, adds the `.sig` line, and wires the site-wide search + repo
widget scripts. The kit CSS itself lives once in `pages/style.css`
(Phase A); this only rewrites markup.

Scope: every page that links `/style.css` EXCEPT `index.html` (the
hand-finished showcase, Phase D). `contribute/index.html` is picked
up once Phase B converges it onto `/style.css`.

Idempotent: a page already carrying `class="site-nav"` skips the nav
rebuild; `.sig` and the kit script are guarded; the dead gh-facts
block is removed if still present. `--check` reports drift only.

Usage:  python scripts/restructure_site.py [--check]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

PAGES = Path(__file__).resolve().parent.parent / "pages"

LINKS_STYLE = re.compile(r'<link\b[^>]*\bhref\s*=\s*["\'][^"\']*?/style\.css["\']',
                         re.IGNORECASE)

# Canonical top-bar nav links (absolute → identical at any page depth).
NAV_LINKS = [
    ("/loka/", "World Model"),
    ("/history/", "History"),
    ("/theory/", "Theory"),
    ("/benchmarks/", "Benchmarks"),
    ("/sparql/", "SPARQL+"),
    ("/codeofethics/", "Code of Ethics"),
    ("/contribute/", "Contribute GPU"),
]

# The canonical Material light/dark toggle, verbatim (every page ships
# an identical one today; we re-place THE one inside the bar).
TOGGLE = (
    '<button id="theme-toggle" class="theme-toggle" type="button"'
    ' aria-label="Toggle light and dark theme" title="Toggle light / dark">'
    '<svg class="icon-sun" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">'
    '<path d="M12,7A5,5 0 0,1 17,12A5,5 0 0,1 12,17A5,5 0 0,1 7,12A5,5 0 0,1 12,7M12,9A3,3 0 0,0 9,12A3,3 0 0,0 12,15A3,3 0 0,0 15,12A3,3 0 0,0 12,9M12,2L14.39,5.42C13.65,5.15 12.84,5 12,5C11.16,5 10.35,5.15 9.61,5.42L12,2M3.34,7L7.5,6.65C6.9,7.16 6.36,7.78 5.94,8.5C5.5,9.24 5.25,10 5.11,10.79L3.34,7M3.36,17L5.12,13.23C5.26,14 5.53,14.78 5.95,15.5C6.37,16.24 6.91,16.86 7.5,17.37L3.36,17M20.65,7L18.88,10.79C18.74,10 18.47,9.23 18.05,8.5C17.63,7.78 17.1,7.15 16.5,6.64L20.65,7M20.64,17L16.5,17.36C17.09,16.85 17.62,16.22 18.04,15.5C18.46,14.77 18.73,14 18.87,13.21L20.64,17M12,22L9.59,18.56C10.33,18.83 11.14,19 12,19C12.82,19 13.63,18.86 14.37,18.59L12,22Z"></path></svg>'
    '<svg class="icon-moon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">'
    '<path d="M17.75,4.09L15.22,6.03L16.13,9.09L13.5,7.28L10.87,9.09L11.78,6.03L9.25,4.09L12.44,4L13.5,1L14.56,4L17.75,4.09M21.25,11L19.61,12.25L20.2,14.23L18.5,13.06L16.8,14.23L17.39,12.25L15.75,11L17.81,10.95L18.5,9L19.19,10.95L21.25,11M18.97,15.95C19.8,15.87 20.69,17.05 20.16,17.8C19.84,18.25 19.5,18.67 19.08,19.07C15.17,23 8.84,23 4.94,19.07C1.03,15.17 1.03,8.83 4.94,4.93C5.34,4.53 5.76,4.17 6.21,3.85C6.96,3.32 8.14,4.21 8.06,5.04C7.79,7.9 8.75,10.87 10.95,13.06C13.14,15.26 16.1,16.22 18.97,15.95M17.33,17.97C14.5,17.81 11.7,16.64 9.53,14.5C7.36,12.31 6.2,9.5 6.04,6.68C3.23,9.82 3.34,14.4 6.35,17.41C9.37,20.43 14,20.54 17.33,17.97Z"></path></svg>'
    '</button>'
)

OCTOCAT = (
    '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 '
    '3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53'
    '-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 '
    '1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95'
    ' 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 '
    '2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56'
    '.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93'
    '-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"></path></svg>'
)
SEARCH_SVG = (
    '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9.5,3A6.5,6.5 0 0,1 16,9.5'
    'C16,11.11 15.41,12.59 14.44,13.73L14.71,14H15.5L20.5,19L19,20.5L14,15.5V14.71'
    'L13.73,14.44C12.59,15.41 11.11,16 9.5,16A6.5,6.5 0 0,1 3,9.5A6.5,6.5 0 0,1 9.5,3'
    'M9.5,5C7,5 5,7 5,9.5C5,12 7,14 9.5,14C12,14 14,12 14,9.5C14,7 12,5 9.5,5Z">'
    '</path></svg>'
)
TAG_SVG = ('<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5.5,7A1.5,1.5 0 0,1 '
           '4,5.5A1.5,1.5 0 0,1 5.5,4A1.5,1.5 0 0,1 7,5.5A1.5,1.5 0 0,1 5.5,7M21.41,'
           '11.58L12.41,2.58C12.05,2.22 11.55,2 11,2H4C2.89,2 2,2.89 2,4V11C2,11.55 '
           '2.22,12.05 2.59,12.41L11.58,21.41C11.95,21.78 12.45,22 13,22C13.55,22 '
           '14.05,21.78 14.41,21.41L21.41,14.41C21.78,14.05 22,13.55 22,13C22,12.44 '
           '21.77,11.94 21.41,11.58Z"/></svg>')
STAR_SVG = ('<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12,17.27L18.18,21'
            'L16.54,13.97L22,9.24L14.81,8.62L12,2L9.19,8.62L2,9.24L7.45,13.97L5.82,21'
            'L12,17.27Z"/></svg>')
FORK_SVG = ('<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6,2A3,3 0 0,1 9,5'
            'C9,6.28 8.19,7.38 7,7.82V11H11V7.82C9.81,7.38 9,6.28 9,5A3,3 0 0,1 12,2'
            'A3,3 0 0,1 15,5C15,6.28 14.19,7.38 13,7.82V11H17A2,2 0 0,1 19,13V16.18'
            'C20.19,16.62 21,17.72 21,19A3,3 0 0,1 18,22A3,3 0 0,1 15,19C15,17.72 '
            '15.81,16.62 17,16.18V13H7V16.18C8.19,16.62 9,17.72 9,19A3,3 0 0,1 6,22'
            'A3,3 0 0,1 3,19C3,17.72 3.81,16.62 5,16.18V13A2,2 0 0,1 7,11"/></svg>')

REPO_WIDGET = (
    '<a class="repo-widget js-repo" href="https://github.com/EmmaLeonhart/Loka"'
    ' aria-label="EmmaLeonhart/Loka on GitHub">'
    f'<span class="rw-icon">{OCTOCAT}</span>'
    '<span class="rw-body">'
    '<span class="rw-repo">EmmaLeonhart/Loka</span>'
    '<span class="rw-facts">'
    f'<span class="rw-fact rw-version-wrap is-empty">{TAG_SVG}<span class="rw-version"></span></span>'
    f'<span class="rw-fact">{STAR_SVG}<span class="rw-stars">&middot;</span></span>'
    f'<span class="rw-fact">{FORK_SVG}<span class="rw-forks">&middot;</span></span>'
    '</span></span></a>'
)

SIG = ('<div class="sig">loka.emmaleonhart.com &middot; built on the shared kit '
       '&mdash; <a href="https://emmaleonhart.com/branding/">branding</a></div>')

KIT_SCRIPT = '''<script data-kit="scripts">
(function(){
  // ---- live repo widget (graceful on rate-limit) -------------------
  function setAll(s,v){document.querySelectorAll(s).forEach(function(e){e.textContent=v;});}
  fetch('https://api.github.com/repos/EmmaLeonhart/Loka').then(function(r){return r.ok?r.json():null;})
    .then(function(d){if(!d)return;
      if(typeof d.stargazers_count==='number')setAll('.rw-stars',d.stargazers_count);
      if(typeof d.forks_count==='number')setAll('.rw-forks',d.forks_count);}).catch(function(){});
  fetch('https://api.github.com/repos/EmmaLeonhart/Loka/releases/latest').then(function(r){return r.ok?r.json():null;})
    .then(function(d){if(d&&d.tag_name){setAll('.rw-version',d.tag_name);
      document.querySelectorAll('.rw-version-wrap').forEach(function(e){e.classList.remove('is-empty');});}}).catch(function(){});

  // ---- site-wide search (build_search_index.py -> /search.json) ----
  var box=document.getElementById('kit-search'), panel=document.getElementById('kit-results');
  if(!box||!panel)return;
  var idx=null, rows=[], sel=-1;
  function esc(s){return s.replace(/[&<>"]/g,function(c){return({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'})[c];});}
  function load(){ if(idx)return Promise.resolve(idx);
    return fetch('/search.json').then(function(r){return r.ok?r.json():null;})
      .then(function(d){idx=(d&&d.records)||[];return idx;}).catch(function(){idx=[];return idx;}); }
  function close(){panel.classList.remove('open');panel.innerHTML='';rows=[];sel=-1;}
  function go(r){window.location.href=r.url+(r.anchor?('#'+r.anchor):'');}
  function render(q){
    var terms=q.toLowerCase().split(/\\s+/).filter(Boolean);
    var scored=[];
    idx.forEach(function(r){
      var hay=(r.page+' '+r.heading+' '+r.text).toLowerCase(), sc=0, ok=true;
      terms.forEach(function(t){ var h=r.heading.toLowerCase().indexOf(t), x=hay.indexOf(t);
        if(x===-1){ok=false;return;} sc+= h!==-1?3:1; });
      if(ok)scored.push([sc,r]);
    });
    scored.sort(function(a,b){return b[0]-a[0];});
    rows=scored.slice(0,8).map(function(p){return p[1];}); sel=-1;
    if(!rows.length){panel.innerHTML='<div class="sr-empty">No matches for &ldquo;'+esc(q)+'&rdquo;</div>';panel.classList.add('open');return;}
    panel.innerHTML=rows.map(function(r,i){
      return '<a class="sr-item" data-i="'+i+'" href="'+r.url+(r.anchor?('#'+r.anchor):'')+'">'
        +'<div class="sr-title">'+esc(r.heading)+'</div>'
        +'<div class="sr-ctx">'+esc(r.text.slice(0,150))+'</div>'
        +'<div class="sr-crumb">'+esc(r.page)+' &middot; '+esc(r.url)+'</div></a>';
    }).join('');
    panel.classList.add('open');
  }
  function mark(){panel.querySelectorAll('.sr-item').forEach(function(e,i){e.classList.toggle('active',i===sel);});}
  box.addEventListener('input',function(){var q=box.value.trim();
    if(q.length<2){close();return;} load().then(function(){render(q);});});
  box.addEventListener('keydown',function(e){
    if(!rows.length){if(e.key==='Escape')box.blur();return;}
    if(e.key==='ArrowDown'){e.preventDefault();sel=(sel+1)%rows.length;mark();}
    else if(e.key==='ArrowUp'){e.preventDefault();sel=(sel-1+rows.length)%rows.length;mark();}
    else if(e.key==='Enter'){e.preventDefault();go(rows[sel<0?0:sel]);}
    else if(e.key==='Escape'){close();box.blur();}
  });
  document.addEventListener('click',function(e){ if(!e.target.closest('.search'))close(); });
})();
</script>'''


def _url_for(path: Path) -> str:
    rel = path.relative_to(PAGES).as_posix()
    if rel == "index.html":
        return "/"
    if rel.endswith("/index.html"):
        return "/" + rel[: -len("index.html")]
    return "/" + rel


def _site_nav(active_url: str) -> str:
    links = []
    for href, label in NAV_LINKS:
        cls = ' class="active"' if href == active_url else ""
        links.append(f'<a{cls} href="{href}">{label}</a>')
    return (
        '<nav class="site-nav" aria-label="Primary">'
        '<a class="brand" href="/"><span>Loka</span></a>'
        f'<div class="nav-links">{"".join(links)}</div>'
        '<div class="nav-spacer"></div>'
        '<div class="search" title="Search Loka">'
        f'{SEARCH_SVG}'
        '<input id="kit-search" type="search" placeholder="Search Loka"'
        ' aria-label="Search Loka" autocomplete="off">'
        '<div class="search-results" id="kit-results" role="listbox"></div>'
        '</div>'
        '<div class="nav-right">'
        f'{TOGGLE}{REPO_WIDGET}'
        '</div>'
        '</nav>'
    )


def restructure(html: str, url: str) -> str:
    # Ensure the .aurora backdrop sits right after <body> (defensive;
    # unify already added it on every page).
    if 'class="aurora"' not in html:
        html = re.sub(r"(<body[^>]*>)", r"\1\n  "
                      '<div class="aurora" aria-hidden="true"></div>',
                      html, count=1)

    if 'class="site-nav"' not in html:
        # Drop the standalone fixed-corner toggle — the bar owns the
        # one canonical #theme-toggle now.
        html = re.sub(r'<button id="theme-toggle".*?</button>', "",
                      html, count=1, flags=re.S)
        # Replace the first <nav>…</nav> wholesale with the kit bar.
        html = re.sub(r"<nav\b[^>]*>.*?</nav>", lambda _m: _site_nav(url),
                      html, count=1, flags=re.S)

    # Remove the dead unify gh-facts script (targets a.gh[data-gh-repo],
    # which the kit bar no longer has).
    html = re.sub(
        r"<script>(?:(?!</script>).)*?a\.gh\[data-gh-repo\](?:(?!</script>).)*?</script>\s*",
        "", html, flags=re.S)

    # The .sig line, just before <footer> (or before </body>).
    if 'class="sig"' not in html:
        if re.search(r"<footer\b", html):
            html = re.sub(r"(\s*<footer\b)", "\n  " + SIG + r"\1",
                          html, count=1)
        else:
            html = html.replace("</body>", "  " + SIG + "\n</body>", 1)

    # The kit script (search + repo facts), once, before </body>.
    if 'data-kit="scripts"' not in html:
        html = html.replace("</body>", "  " + KIT_SCRIPT + "\n</body>", 1)

    return html


def main() -> int:
    check = "--check" in sys.argv
    changed = 0
    for path in sorted(PAGES.rglob("*.html")):
        rel = path.relative_to(PAGES)
        if rel.as_posix() in ("index.html", "playground.html"):
            # index.html = hand-finished showcase (Phase D).
            # playground.html = full-screen SPARQL IDE: no <nav> by
            # design, 100vh flex layout the fixed bar would break. It
            # still gets the shared palette via /style.css.
            continue
        before = path.read_text(encoding="utf-8")
        if not LINKS_STYLE.search(before):
            print(f"skip (no /style.css): {rel}")
            continue
        after = restructure(before, _url_for(path))
        if after != before:
            changed += 1
            if check:
                print(f"would change: {rel}")
            else:
                path.write_text(after, encoding="utf-8")
                print(f"restructured: {rel}")
        else:
            print(f"unchanged: {rel}")
    print(f"\n{changed} file(s) {'would change' if check else 'changed'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
