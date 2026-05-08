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

    user_msgs = soup.select('[data-testid="user-message"]')
    if not user_msgs:
        raise SystemExit(f"No user messages found in {html_path}")

    out: list[str] = []
    title = html_path.stem.replace(" - Claude", "")
    out.append(f"# {title}\n")

    for i, user_msg in enumerate(user_msgs):
        out.append(f"## User (turn {i + 1})\n")
        out.append(to_markdown(user_msg))
        out.append("")

        # Claude's reply: walk forward in document order until the next user message,
        # collecting any element with class "font-claude-response".
        next_user = user_msgs[i + 1] if i + 1 < len(user_msgs) else None
        claude_blocks: list[Tag] = []
        for el in user_msg.find_all_next(class_="font-claude-response"):
            if next_user is not None and (el is next_user or next_user in el.parents):
                break
            # Avoid descending into nested font-claude-response (take only top-level)
            if any("font-claude-response" in (p.get("class") or []) for p in el.parents):
                continue
            claude_blocks.append(el)

        if claude_blocks:
            out.append(f"## Claude (turn {i + 1})\n")
            for block in claude_blocks:
                md = to_markdown(block)
                if md:
                    out.append(md)
                    out.append("")

    md_path.write_text("\n".join(out).rstrip() + "\n", encoding="utf-8")
    print(f"  -> {md_path}  ({len(user_msgs)} turns, {md_path.stat().st_size:,} bytes)")


def main() -> None:
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    extract(Path(sys.argv[1]), Path(sys.argv[2]))


if __name__ == "__main__":
    main()
