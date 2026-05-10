"""Submit a paper to clawRxiv and poll for its peer review.

Shared between `.github/workflows/papers-ci.yml` (automatic push-triggered
submission) and `.github/workflows/submit-papers.yml` (manual one-off
submission). Pulled out of the inline YAML script so both workflows
share one implementation, so it can be unit-tested locally, and so the
review-fetching logic isn't trapped inside a heredoc.

Reads paper content from `<paper-dir>/paper.md`, optionally reads
`<paper-dir>/SKILL.md` as supplementary material, extracts the `## Abstract`
section (falling back to the first 500 characters if there is no such
section), and POSTs to clawRxiv's `/api/posts` endpoint.

After submission succeeds, polls `/api/posts/{post_id}/review` (the
undocumented review endpoint we've been using) every `--poll-seconds`
seconds until a review is returned or `--review-timeout-seconds` elapses.
On success, writes the review JSON and a rendered Markdown copy into
`<paper-dir>/reviews/v{N}_post{post_id}_review.{json,md}` where `N` is
derived from the count of existing files in that directory plus one.

On submission success, updates `<paper-dir>/.post_id` to the new post ID
so the next push to master automatically supersedes this version.

Example (local):

    export CLAWRXIV_API_KEY=...
    python scripts/paper_submit_and_fetch.py \\
        --paper-dir loka-paper \\
        --title "Loka: Generative Citation in a Neuro-Symbolic World Model" \\
        --tags "programming-languages,vsa,embedding-spaces"

Exit code 0 on success (including successful submission with timed-out
review fetch — the submission landed, the review just isn't ready yet).
Exit code 1 on submission failure or any fatal error.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

CLAWRXIV_BASE = "https://clawrxiv.io"


def extract_h1_title(content: str) -> str | None:
    """Extract the H1 title from the paper markdown, if present."""
    match = re.match(r'^#\s+(.+)', content)
    return match.group(1).strip() if match else None


def read_paper(paper_dir: Path) -> tuple[str, str | None, str]:
    """Return (paper_content, skill_content_or_none, abstract)."""
    paper_path = paper_dir / "paper.md"
    if not paper_path.exists():
        raise FileNotFoundError(f"{paper_path} does not exist")
    content = paper_path.read_text(encoding="utf-8")

    skill_path = paper_dir / "SKILL.md"
    skill = skill_path.read_text(encoding="utf-8") if skill_path.exists() else None
    if skill is None:
        print(f"WARNING: {skill_path} does not exist — submitting without SKILL.md",
              file=sys.stderr)

    # Extract the abstract section. paper.md has:
    #     ## Abstract
    #     <paragraphs>
    #     ## <next section>
    # The regex captures the content between `## Abstract` and the next
    # H2 heading, regardless of whether that heading text starts with a
    # digit. Falls back to the first 500 characters if no abstract
    # section is found — the fly-brain paper currently has
    # "## What We Did" instead of "## Abstract", so the fallback matters.
    #
    # Note: an earlier version of this regex required the next heading
    # to start with a digit (`## [0-9]`), matching `## 1. Introduction`
    # style. When the body section numbering was stripped (paper commit
    # b3e5320, `## 1. Introduction` -> `## Introduction`), that pattern
    # stopped matching, the regex fell through to the 500-char fallback,
    # and clawRxiv reviewers flagged the abstract as "truncated mid-
    # sentence ('with th')." Match any `\n## ` now.
    match = re.search(r'## Abstract\s*\n(.*?)(?=\n## )', content, re.DOTALL)
    abstract = match.group(1).strip() if match else content[:500]
    return content, skill, abstract


def read_post_id(paper_dir: Path) -> int | None:
    """Return the previously-stored post ID for supersede, or None."""
    post_id_path = paper_dir / ".post_id"
    if not post_id_path.exists():
        return None
    raw = post_id_path.read_text(encoding="utf-8").strip()
    if not raw:
        return None
    try:
        return int(raw)
    except ValueError:
        print(f"WARNING: {post_id_path} contains non-integer value {raw!r}, "
              f"ignoring", file=sys.stderr)
        return None


def submit(
    *,
    api_key: str,
    title: str,
    abstract: str,
    content: str,
    skill: str | None,
    tags: list[str],
    supersedes: int | None,
) -> dict[str, Any]:
    """POST to /api/posts and return the parsed JSON response."""
    payload: dict[str, Any] = {
        "title": title,
        "abstract": abstract,
        "content": content,
        "tags": tags,
        "human_names": ["Emma Leonhart"],
    }
    if skill is not None:
        payload["skill_md"] = skill
    if supersedes is not None:
        payload["supersedes"] = supersedes

    data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        f"{CLAWRXIV_BASE}/api/posts",
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            body = resp.read().decode("utf-8")
    except urllib.error.HTTPError as e:
        err_body = e.read().decode("utf-8", errors="replace") if e.fp else ""
        raise RuntimeError(
            f"clawRxiv submission failed: HTTP {e.code}: {err_body}"
        ) from e
    return json.loads(body)


def fetch_review(
    *, api_key: str, post_id: int, poll_seconds: int, timeout_seconds: int,
) -> dict[str, Any] | None:
    """Poll /api/posts/{post_id}/review until a review exists or timeout.

    Returns the review JSON on success, or None if the timeout elapses
    without a review being produced. A timeout is NOT an error — the
    submission itself succeeded, the reviewer just hasn't run yet.
    """
    deadline = time.monotonic() + timeout_seconds
    url = f"{CLAWRXIV_BASE}/api/posts/{post_id}/review"
    req_headers = {
        "Authorization": f"Bearer {api_key}",
        "Accept": "application/json",
    }
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            req = urllib.request.Request(url, headers=req_headers)
            with urllib.request.urlopen(req, timeout=30) as resp:
                body = resp.read().decode("utf-8")
            parsed = json.loads(body)
            # The endpoint returns different shapes depending on state.
            # A review is "ready" if the payload has content beyond just
            # an empty envelope. Check common field names.
            if isinstance(parsed, dict) and (
                parsed.get("review")
                or parsed.get("body")
                or parsed.get("content")
                or parsed.get("rating")
            ):
                print(f"Review ready after {attempt} poll(s) "
                      f"(~{attempt * poll_seconds}s elapsed)")
                return parsed
        except urllib.error.HTTPError as e:
            # 404 or similar means "not ready yet" — normal, keep polling.
            if e.code in (404, 409, 202):
                pass
            else:
                print(f"poll attempt {attempt}: HTTP {e.code}", file=sys.stderr)
        except Exception as e:  # noqa: BLE001
            print(f"poll attempt {attempt}: {e}", file=sys.stderr)
        time.sleep(poll_seconds)
    print(f"Review not ready after {attempt} attempts "
          f"(~{timeout_seconds}s) — skipping for now",
          file=sys.stderr)
    return None


def next_version_number(reviews_dir: Path) -> int:
    """Return the next version number for a new review file."""
    if not reviews_dir.exists():
        return 1
    existing = [p for p in reviews_dir.glob("v*_post*_review.json")]
    return len(existing) + 1


def render_review_markdown(review: dict[str, Any], *, version: int, post_id: int) -> str:
    """Render the review JSON into a human-readable markdown file."""
    lines = [
        f"# Review v{version} · post {post_id}",
        "",
    ]
    rating = review.get("rating") or review.get("recommendation")
    if rating:
        lines.append(f"**Rating:** {rating}")
        lines.append("")
    body = (
        review.get("review")
        or review.get("body")
        or review.get("content")
        or json.dumps(review, indent=2)
    )
    lines.append(str(body))
    lines.append("")
    return "\n".join(lines)


def save_review(
    *, reviews_dir: Path, review: dict[str, Any], version: int, post_id: int,
) -> tuple[Path, Path]:
    reviews_dir.mkdir(parents=True, exist_ok=True)
    json_path = reviews_dir / f"v{version}_post{post_id}_review.json"
    md_path = reviews_dir / f"v{version}_post{post_id}_review.md"
    json_path.write_text(
        json.dumps(review, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    md_path.write_text(
        render_review_markdown(review, version=version, post_id=post_id),
        encoding="utf-8",
    )
    return json_path, md_path


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Submit a paper to clawRxiv and poll for its peer review.",
    )
    parser.add_argument(
        "--paper-dir", required=True,
        help="Path to paper directory containing paper.md (e.g. loka-paper)",
    )
    parser.add_argument(
        "--title", required=False, default=None,
        help="Paper title. If omitted, extracted from the H1 of paper.md. "
             "If provided, warns when it differs from the paper's H1.",
    )
    parser.add_argument(
        "--tags", required=True,
        help="Comma-separated tag list (e.g. programming-languages,vsa)",
    )
    parser.add_argument(
        "--supersedes", type=int, default=None,
        help="Post ID to supersede. Defaults to the contents of "
             "<paper-dir>/.post_id if present.",
    )
    parser.add_argument(
        "--poll-seconds", type=int, default=30,
        help="Seconds between review polls (default 30)",
    )
    parser.add_argument(
        "--review-timeout-seconds", type=int, default=600,
        help="Give up waiting for the review after this many seconds (default 600)",
    )
    parser.add_argument(
        "--no-review-wait", action="store_true",
        help="Submit and exit without polling for the review.",
    )
    args = parser.parse_args()

    api_key = os.environ.get("CLAWRXIV_API_KEY")
    if not api_key:
        print("ERROR: CLAWRXIV_API_KEY environment variable is not set",
              file=sys.stderr)
        return 1

    paper_dir = Path(args.paper_dir)
    if not paper_dir.is_dir():
        print(f"ERROR: {paper_dir} is not a directory", file=sys.stderr)
        return 1

    tags = [t.strip() for t in args.tags.split(",") if t.strip()]

    try:
        content, skill, abstract = read_paper(paper_dir)
    except FileNotFoundError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 1

    # Resolve the title: prefer the H1 from paper.md, fall back to --title.
    h1_title = extract_h1_title(content)
    if args.title is None:
        if h1_title is None:
            print("ERROR: no --title provided and no H1 found in paper.md",
                  file=sys.stderr)
            return 1
        title = h1_title
        print(f"Using title from paper.md H1: {title}")
    else:
        title = args.title
        if h1_title and h1_title != title:
            print(f"WARNING: --title does not match paper.md H1!",
                  file=sys.stderr)
            print(f"  --title:    {title}", file=sys.stderr)
            print(f"  paper H1:   {h1_title}", file=sys.stderr)
            print(f"  Using --title (override). Update the CI workflow or "
                  f"paper.md to make them consistent.", file=sys.stderr)

    supersedes = args.supersedes
    if supersedes is None:
        supersedes = read_post_id(paper_dir)
    if supersedes is not None:
        print(f"Superseding existing post {supersedes}")
    else:
        print(f"Submitting {paper_dir}/paper.md as a NEW post (no .post_id found)")

    try:
        response = submit(
            api_key=api_key,
            title=title,
            abstract=abstract,
            content=content,
            skill=skill,
            tags=tags,
            supersedes=supersedes,
        )
    except Exception as e:  # noqa: BLE001
        print(f"ERROR: {e}", file=sys.stderr)
        return 1

    print("Submission response:")
    print(json.dumps(response, indent=2, ensure_ascii=False))

    new_post_id = response.get("id") or response.get("postId")
    if not new_post_id:
        print("ERROR: submission response has no id/postId field",
              file=sys.stderr)
        return 1

    try:
        new_post_id = int(new_post_id)
    except (TypeError, ValueError):
        print(f"ERROR: response id {new_post_id!r} is not an integer",
              file=sys.stderr)
        return 1

    (paper_dir / ".post_id").write_text(str(new_post_id), encoding="utf-8")
    print(f"Wrote {paper_dir}/.post_id = {new_post_id}")

    if args.no_review_wait:
        print("Skipping review poll (--no-review-wait)")
        return 0

    reviews_dir = paper_dir / "reviews"
    version = next_version_number(reviews_dir)
    print(f"Polling for review of post {new_post_id} "
          f"(version={version}, timeout={args.review_timeout_seconds}s)...")
    review = fetch_review(
        api_key=api_key,
        post_id=new_post_id,
        poll_seconds=args.poll_seconds,
        timeout_seconds=args.review_timeout_seconds,
    )
    if review is None:
        print("No review fetched — submission still counts as success.")
        return 0

    json_path, md_path = save_review(
        reviews_dir=reviews_dir,
        review=review,
        version=version,
        post_id=new_post_id,
    )
    print(f"Wrote {json_path}")
    print(f"Wrote {md_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
