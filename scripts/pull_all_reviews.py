"""Fetch every clawrxiv review we don't already have locally.

Why this exists:
    The paper is in gradient-descent phase. We submit candidates via
    `quick_review.py` (local, fast) and canonical revisions via
    `papers-ci.yml` (CI, slow). Both paths leave a trail:

      paper/candidates.jsonl   — every quick_review.py submission
      paper/.post_id           — the latest canonical post

    But individual review files only get committed back by the CI's
    "Commit review back" step on canonical runs. Candidate reviews
    submitted from a local `quick_review.py` only print to stdout —
    they're never saved to disk.

    This script closes the gap: it scans every post we know about (from
    candidates.jsonl + .post_id) and fetches reviews for any whose
    review file doesn't exist yet. Idempotent: running it twice does
    nothing the second time. Safe to run from anywhere.

What it does:
    1. Read paper/candidates.jsonl (one JSON object per line) and
       paper/.post_id to enumerate every post_id we've ever submitted.
    2. For each post_id, check whether paper/reviews/v*_post{id}_review.json
       already exists. If yes, skip. If no, GET the review.
    3. Save each new review as paper/reviews/v{N}_post{id}_review.{json,md}
       where N is one past the highest existing version number.
    4. Print a per-post status line.

    Exit code 0 on success (even if some reviews aren't ready yet —
    that's a "try again later" signal, not an error).

Required environment:
    CLAWRXIV_API_KEY — same secret used by the CI workflow.

Examples:
    # Fetch everything we don't have:
    python scripts/pull_all_reviews.py

    # Force re-fetch even for posts where we already have a file:
    python scripts/pull_all_reviews.py --refresh

    # Tighter timeout per post (default 30s — short because we're
    # iterating over many posts):
    python scripts/pull_all_reviews.py --timeout-per-post 10

This is the "catch-up" companion to quick_review.py. The CI workflow
calls it on every push so any local candidates whose reviews weren't
fetched at submit time get pulled into the repo automatically.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Iterator

# Reuse helpers.
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from paper_submit_and_fetch import (  # noqa: E402
    fetch_review,
    next_version_number,
    read_post_id,
    render_review_markdown,
    save_review,
)


def enumerate_post_ids(paper_dir: Path) -> Iterator[tuple[int, str]]:
    """Yield (post_id, source_label) for every post we know about.

    Sources, in priority order:
      - paper/.post_id (canonical)
      - paper/candidates.jsonl (each line's post_id)

    Deduplicates so the same post_id only appears once.
    """
    seen: set[int] = set()

    canonical = read_post_id(paper_dir)
    if canonical is not None:
        seen.add(canonical)
        yield canonical, "canonical"

    candidates_path = paper_dir / "candidates.jsonl"
    if candidates_path.exists():
        for line_no, raw in enumerate(
            candidates_path.read_text(encoding="utf-8").splitlines(),
            start=1,
        ):
            raw = raw.strip()
            if not raw:
                continue
            try:
                obj = json.loads(raw)
            except json.JSONDecodeError as e:
                print(f"WARNING: candidates.jsonl line {line_no} not JSON: "
                      f"{e}", file=sys.stderr)
                continue
            pid = obj.get("post_id")
            if not isinstance(pid, int):
                print(f"WARNING: candidates.jsonl line {line_no} has no "
                      f"post_id: {raw[:80]}", file=sys.stderr)
                continue
            if pid in seen:
                continue
            seen.add(pid)
            label = obj.get("label", "") or "(unlabeled)"
            yield pid, f"candidate:{label}"


def review_file_exists(reviews_dir: Path, post_id: int) -> Path | None:
    """Return the existing review JSON path for a post_id, or None."""
    if not reviews_dir.exists():
        return None
    matches = list(reviews_dir.glob(f"v*_post{post_id}_review.json"))
    return matches[0] if matches else None


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fetch any clawrxiv reviews not yet saved locally.",
    )
    parser.add_argument(
        "--paper-dir", default="paper",
        help="Paper directory (default: paper)",
    )
    parser.add_argument(
        "--refresh", action="store_true",
        help="Re-fetch even for posts where a review file already exists. "
             "Default is to skip those.",
    )
    parser.add_argument(
        "--timeout-per-post", type=int, default=30,
        help="Seconds to wait per post for a review (default 30). Short "
             "because we may be iterating across many posts; if a review "
             "isn't ready, we move on and try again next time.",
    )
    parser.add_argument(
        "--poll-seconds", type=int, default=5,
        help="Seconds between polls within a single post (default 5).",
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

    reviews_dir = paper_dir / "reviews"
    reviews_dir.mkdir(parents=True, exist_ok=True)

    posts = list(enumerate_post_ids(paper_dir))
    if not posts:
        print(f"No posts found in {paper_dir}/.post_id or "
              f"{paper_dir}/candidates.jsonl. Nothing to do.")
        return 0

    print(f"Checking reviews for {len(posts)} known post(s)...")

    fetched = 0
    skipped = 0
    not_ready = 0
    for post_id, source in posts:
        existing = review_file_exists(reviews_dir, post_id)
        if existing and not args.refresh:
            print(f"  post {post_id} ({source}): already have "
                  f"{existing.name}, skipping")
            skipped += 1
            continue

        print(f"  post {post_id} ({source}): fetching review...", end=" ",
              flush=True)
        t = time.monotonic()
        review = fetch_review(
            api_key=api_key,
            post_id=post_id,
            poll_seconds=args.poll_seconds,
            timeout_seconds=args.timeout_per_post,
        )
        if review is None:
            print(f"not ready after {args.timeout_per_post}s")
            not_ready += 1
            continue

        version = next_version_number(reviews_dir)
        json_path, md_path = save_review(
            reviews_dir=reviews_dir,
            review=review,
            version=version,
            post_id=post_id,
        )
        rating = review.get("rating") or review.get("recommendation") or "?"
        print(f"{rating}  ({time.monotonic() - t:.1f}s)  -> "
              f"{json_path.name}")
        fetched += 1

    print()
    print(f"Done: {fetched} fetched, {skipped} skipped (already had), "
          f"{not_ready} not ready yet.")
    if not_ready:
        print("Re-run later to pick up reviews that weren't ready.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
