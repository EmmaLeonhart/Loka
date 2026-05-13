"""Extract a Claude conversation export (HTML) into clean markdown.

Usage: python tools/extract_claude_chat.py <input.html> <output.md>

Strategy:
  - User turns are marked with [data-testid="user-message"].
  - Claude turns are sequences of elements with class "font-claude-response",
    living between consecutive user messages.
  - Each turn's HTML is converted to markdown via html2text, with images and
    inline SVG stripped.
"""
import sys
import re
from pathlib import Path
from bs4 import BeautifulSoup, Tag
import html2text


def strip_noise(tag: Tag) -> None:
    for el in tag.find_all(["img", "svg", "script", "style", "button"]):
        el.decompose()
    for el in tag.find_all(attrs={"aria-hidden": "true"}):
        el.decompose()


def to_markdown(tag: Tag) -> str:
    strip_noise(tag)
    h = html2text.HTML2Text()
    h.body_width = 0
    h.ignore_images = True
    h.ignore_emphasis = False
    h.protect_links = True
    md = h.handle(str(tag))
    md = re.sub(r"\n{3,}", "\n\n", md).strip()
    return md


def extract(html_path: Path, md_path: Path) -> None:
    soup = BeautifulSoup(html_path.read_text(encoding="utf-8"), "html.parser")

    def is_user(t: Tag) -> bool:
        return t.get("data-testid") == "user-message"

    def is_top_claude(t: Tag) -> bool:
        if "font-claude-response" not in (t.get("class") or []):
            return False
        return not any("font-claude-response" in (p.get("class") or []) for p in t.parents)

    blocks = soup.find_all(lambda t: is_user(t) or is_top_claude(t))

    if not any(is_user(b) for b in blocks):
        raise SystemExit(f"No user messages found in {html_path}")

    out: list[str] = []
    title = html_path.stem.replace(" - Claude", "")
    out.append(f"# {title}\n")

    turn = 0
    in_claude = False
    for b in blocks:
        if is_user(b):
            turn += 1
            out.append(f"## User (turn {turn})\n")
            out.append(to_markdown(b))
            out.append("")
            in_claude = False
        else:
            if not in_claude:
                out.append(f"## Claude (turn {turn})\n")
                in_claude = True
            md = to_markdown(b)
            if md:
                out.append(md)
                out.append("")

    md_path.write_text("\n".join(out).rstrip() + "\n", encoding="utf-8")
    user_count = sum(1 for b in blocks if is_user(b))
    print(f"  -> {md_path}  ({user_count} turns, {md_path.stat().st_size:,} bytes)")


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    extract(Path(sys.argv[1]), Path(sys.argv[2]))


if __name__ == "__main__":
    main()
