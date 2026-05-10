"""Local fast-feedback paper review tool — bypasses GitHub Actions.

Why this exists:
    `papers-ci.yml` round-trips a paper edit → submit → review → auto-commit
    cycle in 3+ minutes per change, and serializes when concurrent. For
    directional gradient descent (test edit A vs edit B vs edit C against
    the same baseline, keep the best) that's too slow.

    This script does the same submit-and-fetch-review work locally. No git
    commit, no .post_id update, no review file written. Just a printed
    rating + summary you can use to compare candidates.

What it does:
    1. Reads paper.md (and optionally SKILL.md) from --paper-dir, OR a
       different file via --paper-md.
    2. POSTs to clawrxiv.
    3. By default treats the submission as a *candidate* (separate post,
       not part of the canonical supersede chain) by passing the
       dedup_token from the duplicate-detection response on retry. Use
       --canonical to opt into the chain (sends `supersedes` from
       paper/.post_id), but normally you don't want that — the CI does
       canonical submissions.
    4. Polls the review endpoint until ready or timeout.
    5. Prints rating + summary + cons + justification to stdout.
    6. Exits 0 on success, 1 on submission failure or hard timeout.

Required environment:
    CLAWRXIV_API_KEY — same secret used by the CI workflow. Set in your
    shell before running, e.g. via a sourced .env file.

Examples:
    # Test the current paper.md as-is (uncommitted edits OK):
    python scripts/quick_review.py

    # Test an alternate version saved out elsewhere:
    python scripts/quick_review.py --paper-md /tmp/paper-variant-A.md

    # Test a different paper directory (e.g. a fly-brain paper):
    python scripts/quick_review.py --paper-dir flybrain-paper

    # Submit canonically (rare; the CI usually does this):
    python scripts/quick_review.py --canonical

    # Tighter timeout for fast iteration (default is 90s):
    python scripts/quick_review.py --timeout 45

Workflow for directional gradient descent:

    git status                                 # baseline = HEAD
    # make edit-A in paper/paper.md
    python scripts/quick_review.py
    git checkout -- paper/paper.md             # restore baseline
    # make edit-B
    python scripts/quick_review.py
    git checkout -- paper/paper.md
    # ... etc
    # whichever variant scored best, re-apply and `git commit && git push`
    # to land it canonically through the CI.

This script reuses helper functions from paper_submit_and_fetch.py so
behavior stays consistent with what the CI does.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

# Reuse helpers from the canonical script so behavior matches the CI path.
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from paper_submit_and_fetch import (  # noqa: E402
    CLAWRXIV_BASE,
    extract_h1_title,
    fetch_review,
    read_paper,
    read_post_id,
)


def submit_with_optional_confirm(
    *,
    api_key: str,
    title: str,
    abstract: str,
    content: str,
    skill: str | None,
    tags: list[str],
    supersedes: int | None,
    confirm_duplicate: bool,
) -> dict[str, Any]:
    """POST to /api/posts. If confirm_duplicate, retry with dedup_token on 409.

    The default `submit()` in paper_submit_and_fetch.py raises on any HTTP
    error. For candidate-mode submissions we want to handle the 409
    duplicate response gracefully: clawrxiv returns a `dedup_token` we
    can echo back to confirm "yes, submit anyway as a separate post."
    """
    def post(payload: dict[str, Any]) -> tuple[int, str]:
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
                return resp.status, resp.read().decode("utf-8")
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", errors="replace") if e.fp else ""
            return e.code, body

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

    status, body = post(payload)
    if 200 <= status < 300:
        return json.loads(body)

    if status == 409 and confirm_duplicate:
        # Parse the dedup envelope and resubmit with the token to confirm.
        try:
            err = json.loads(body)
            data = err.get("data") or {}
            token = data.get("dedup_token")
            if not token:
                raise RuntimeError(
                    f"409 with no dedup_token; cannot confirm-submit. "
                    f"Body: {body[:500]}"
                )
            print(f"Got 409 (duplicate of post {data.get('duplicateId')}); "
                  f"retrying with dedup_token to force a new post...",
                  file=sys.stderr)
            payload["dedup_token"] = token
            status2, body2 = post(payload)
            if 200 <= status2 < 300:
                return json.loads(body2)
            raise RuntimeError(
                f"Confirm-retry failed: HTTP {status2}: {body2[:500]}"
            )
        except json.JSONDecodeError as e:
            raise RuntimeError(f"Could not parse 409 body: {body[:500]}") from e

    raise RuntimeError(f"clawrxiv submission failed: HTTP {status}: {body[:500]}")


def render(review: dict[str, Any]) -> str:
    """Format the review JSON into a compact, eyeball-friendly summary."""
    rating = review.get("rating") or review.get("recommendation") or "?"
    summary = review.get("summary", "(no summary)")
    pros = review.get("pros") or []
    cons = review.get("cons") or []
    justification = review.get("justification") or review.get("body") or ""
    model = review.get("model", "?")

    lines = [
        "─" * 70,
        f"  RATING: {rating}    (reviewer model: {model})",
        "─" * 70,
        "",
        "  Summary:",
        f"    {summary}",
        "",
    ]
    if pros:
        lines.append("  Pros:")
        for p in pros:
            lines.append(f"    + {p}")
        lines.append("")
    if cons:
        lines.append("  Cons:")
        for c in cons:
            lines.append(f"    - {c}")
        lines.append("")
    if justification and justification != summary:
        lines.append("  Justification:")
        lines.append(f"    {justification}")
        lines.append("")
    lines.append("─" * 70)
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Local fast-feedback paper review against clawrxiv.",
    )
    parser.add_argument(
        "--paper-dir", default="paper",
        help="Paper directory containing paper.md (and optionally SKILL.md). "
             "Default: paper",
    )
    parser.add_argument(
        "--paper-md", default=None,
        help="Path to a specific paper markdown file, overriding "
             "<paper-dir>/paper.md. SKILL.md still read from --paper-dir.",
    )
    parser.add_argument(
        "--title", default=None,
        help="Title override. Defaults to the H1 of paper.md.",
    )
    parser.add_argument(
        "--tags", default="programming-languages,vsa,embedding-spaces",
        help="Comma-separated tag list.",
    )
    parser.add_argument(
        "--canonical", action="store_true",
        help="Submit as canonical (use supersedes from paper/.post_id). "
             "Default is candidate mode: dedup_token-confirm so the "
             "submission goes to a separate post and doesn't update the "
             "canonical chain.",
    )
    parser.add_argument(
        "--timeout", type=int, default=90,
        help="Seconds to wait for the review (default 90).",
    )
    parser.add_argument(
        "--poll-seconds", type=int, default=5,
        help="Seconds between review polls (default 5).",
    )
    parser.add_argument(
        "--json-out", default=None,
        help="If set, also write the raw review JSON to this path.",
    )
    parser.add_argument(
        "--label", default="",
        help="Short variant label saved to paper/candidates.jsonl so you "
             "can tell candidate submissions apart later (e.g. "
             "'fix-demo-count', 'baseline'). Optional but strongly "
             "recommended for gradient-descent runs.",
    )
    parser.add_argument(
        "--no-track", action="store_true",
        help="Do not append the submission to paper/candidates.jsonl. "
             "Use for one-off experiments you don't want to remember.",
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

    if args.paper_md is not None:
        # Caller supplied an explicit paper.md — read that, but read
        # SKILL.md from the paper-dir so candidates can stay paired with
        # the same skill description by default.
        paper_md_path = Path(args.paper_md)
        if not paper_md_path.exists():
            print(f"ERROR: {paper_md_path} does not exist", file=sys.stderr)
            return 1
        content = paper_md_path.read_text(encoding="utf-8")
        skill_path = paper_dir / "SKILL.md"
        skill = skill_path.read_text(encoding="utf-8") if skill_path.exists() else None
        # Same abstract-extraction logic as the canonical reader.
        import re
        m = re.search(r'## Abstract\s*\n(.*?)(?=\n## [0-9])', content, re.DOTALL)
        abstract = m.group(1).strip() if m else content[:500]
    else:
        try:
            content, skill, abstract = read_paper(paper_dir)
        except FileNotFoundError as e:
            print(f"ERROR: {e}", file=sys.stderr)
            return 1

    # Title resolution: --title wins, else H1, else error.
    h1 = extract_h1_title(content)
    title = args.title or h1
    if not title:
        print("ERROR: no --title and no H1 in paper.md", file=sys.stderr)
        return 1

    tags = [t.strip() for t in args.tags.split(",") if t.strip()]
    supersedes = read_post_id(paper_dir) if args.canonical else None

    mode = "CANONICAL (supersedes chain)" if args.canonical else "CANDIDATE (separate post)"
    print(f"Mode: {mode}")
    print(f"Title: {title}")
    if supersedes:
        print(f"Superseding post: {supersedes}")
    print(f"Tags: {tags}")
    print(f"Body: {len(content)} chars from {args.paper_md or paper_dir / 'paper.md'}")
    if skill:
        print(f"Skill: {len(skill)} chars from {paper_dir / 'SKILL.md'}")
    print()

    t_submit = time.monotonic()
    try:
        result = submit_with_optional_confirm(
            api_key=api_key,
            title=title,
            abstract=abstract,
            content=content,
            skill=skill,
            tags=tags,
            supersedes=supersedes,
            confirm_duplicate=not args.canonical,
        )
    except RuntimeError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 1

    post_id = result.get("id") or result.get("postId") or result.get("post_id")
    if not post_id:
        print(f"ERROR: submission response had no post_id: {result}",
              file=sys.stderr)
        return 1

    submit_seconds = time.monotonic() - t_submit
    print(f"Submitted as post {post_id} in {submit_seconds:.1f}s. "
          f"Polling for review...")

    # Track the candidate before polling so the file gets updated even if
    # the poll times out — that way pull_all_reviews.py can still grab
    # the review later. Append-only JSON Lines is concurrent-safe across
    # parallel quick_review.py runs.
    if not args.no_track:
        candidates_path = paper_dir / "candidates.jsonl"
        entry = {
            "post_id": int(post_id),
            "submitted_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "title": title,
            "label": args.label,
            "mode": "canonical" if args.canonical else "candidate",
            "tags": tags,
        }
        with candidates_path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
        print(f"Tracked in {candidates_path}: post_id={post_id} "
              f"label={args.label or '(unlabeled)'}")
    t_poll = time.monotonic()
    review = fetch_review(
        api_key=api_key,
        post_id=int(post_id),
        poll_seconds=args.poll_seconds,
        timeout_seconds=args.timeout,
    )
    if review is None:
        print(f"\nReview not ready after {args.timeout}s. Post {post_id} "
              f"is submitted; check back later.", file=sys.stderr)
        return 0

    print(f"Review ready in {time.monotonic() - t_poll:.1f}s "
          f"(submission + review = {time.monotonic() - t_submit:.1f}s total)")
    print()
    print(render(review))

    if args.json_out:
        Path(args.json_out).write_text(
            json.dumps(review, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )
        print(f"\nRaw JSON written to {args.json_out}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
