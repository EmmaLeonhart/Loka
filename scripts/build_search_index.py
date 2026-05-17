"""Build the site-wide search index (`pages/search.json`).

Walks every content page under `pages/**/*.html`, strips chrome
(nav / footer / script / style / svg / aurora / sig), and emits one
flat record per heading section plus one page-level record. The
`.search` box in the shared `.site-nav` loads this JSON and does
client-side search-as-you-type, jumping to `url#anchor`.

Stdlib only (`html.parser`). Deterministic: pages sorted by path,
whitespace collapsed, snippets capped — so re-running on an unchanged
tree reproduces byte-identical output (idempotent, CI-safe).

Usage:  python scripts/build_search_index.py [--check]
  --check : exit 1 if pages/search.json is stale (for verify/CI),
            without writing.
"""
from __future__ import annotations

import html
import json
import re
import sys
from html.parser import HTMLParser
from pathlib import Path

PAGES = Path(__file__).resolve().parent.parent / "pages"
OUT = PAGES / "search.json"

# Whole subtrees whose text is chrome, not content.
SKIP_TAGS = {"script", "style", "nav", "footer", "svg", "head"}
# …and any element carrying one of these classes (the kit chrome).
SKIP_CLASSES = {"aurora", "sig", "site-nav", "theme-toggle",
                "search", "search-results", "repo-widget", "glyph"}
HEADINGS = {"h1", "h2", "h3"}
# HTML void elements: a start tag with no end tag — must NOT open a
# skip-stack scope, or every push goes unpopped and the whole body
# reads as "skipped".
VOID_TAGS = {"area", "base", "br", "col", "embed", "hr", "img",
             "input", "link", "meta", "param", "source", "track", "wbr"}
SNIPPET_CAP = 240
_WS = re.compile(r"\s+")


def _collapse(text: str) -> str:
    return _WS.sub(" ", html.unescape(text)).strip()


class Extractor(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.title = ""
        self._in_title = False
        # stack of bool: is this open element (or an ancestor) skipped?
        self._skip: list[bool] = []
        # sections: list of [heading, anchor, level, [text parts]]
        self.sections: list[list] = []
        self._cur: list | None = None
        self._intro: list[str] = []
        self._cap_h: str | None = None  # heading we are capturing text into
        self._cap_anchor = ""

    # --- skip-subtree bookkeeping -----------------------------------
    def _skipped(self) -> bool:
        return bool(self._skip and self._skip[-1])

    def handle_starttag(self, tag, attrs):
        ad = dict(attrs)
        if tag == "title":
            self._in_title = True
        classes = set((ad.get("class") or "").split())
        skip = (self._skipped() or tag in SKIP_TAGS
                or bool(classes & SKIP_CLASSES))
        if tag in VOID_TAGS:
            return  # no scope opened, no end tag will come
        self._skip.append(skip)
        if skip:
            return
        if tag in HEADINGS:
            self._flush()
            self._cap_h = ""
            self._cap_anchor = ad.get("id", "")
            self._cap_level = int(tag[1])

    def handle_startendtag(self, tag, attrs):
        # void / self-closing: no subtree, no text — nothing to track.
        pass

    def handle_endtag(self, tag):
        if tag == "title":
            self._in_title = False
        if tag in HEADINGS and not self._skipped() and self._cap_h is not None:
            # heading text complete → open a section for its body
            head = _collapse(self._cap_h)
            self.sections.append([head, self._cap_anchor,
                                  getattr(self, "_cap_level", 2), []])
            self._cur = self.sections[-1]
            self._cap_h = None
        if self._skip:
            self._skip.pop()

    def handle_data(self, data):
        if self._in_title:
            self.title += data
            return
        if self._skipped():
            return
        if self._cap_h is not None:           # accumulating a heading
            self._cap_h += data
        elif self._cur is not None:           # body of the current section
            self._cur[3].append(data)
        else:                                 # text before the first heading
            self._intro.append(data)

    def _flush(self) -> None:
        pass

    # --- result -----------------------------------------------------
    def records(self, url: str) -> list[dict]:
        title = _collapse(self.title) or url
        out: list[dict] = []
        intro = _collapse("".join(self._intro))
        if intro:
            out.append({"url": url, "page": title, "heading": title,
                        "anchor": "", "text": intro[:SNIPPET_CAP]})
        for head, anchor, _lvl, parts in self.sections:
            body = _collapse("".join(parts))
            if not head and not body:
                continue
            out.append({
                "url": url, "page": title,
                "heading": head or title,
                "anchor": anchor,
                "text": body[:SNIPPET_CAP],
            })
        return out


def _url_for(path: Path) -> str | None:
    rel = path.relative_to(PAGES).as_posix()
    if rel == "404.html":  # robots: noindex
        return None
    if rel == "index.html":
        return "/"
    if rel.endswith("/index.html"):
        return "/" + rel[: -len("index.html")]
    return "/" + rel


def build() -> str:
    records: list[dict] = []
    for path in sorted(PAGES.rglob("*.html")):
        url = _url_for(path)
        if url is None:
            continue
        ex = Extractor()
        ex.feed(path.read_text(encoding="utf-8"))
        records.extend(ex.records(url))
    payload = {
        "generated_by": "scripts/build_search_index.py",
        "count": len(records),
        "records": records,
    }
    return json.dumps(payload, ensure_ascii=False, indent=1) + "\n"


def main() -> int:
    check = "--check" in sys.argv
    new = build()
    old = OUT.read_text(encoding="utf-8") if OUT.exists() else None
    if new == old:
        print(f"search.json up to date ({json.loads(new)['count']} records)")
        return 0
    if check:
        print("search.json is STALE — run scripts/build_search_index.py")
        return 1
    OUT.write_text(new, encoding="utf-8")
    print(f"wrote {OUT.relative_to(PAGES.parent)} "
          f"({json.loads(new)['count']} records)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
