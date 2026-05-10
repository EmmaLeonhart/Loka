"""Withdraw clawRxiv papers by ID.

Usage:
    python scripts/withdraw_papers.py <id> [<id> ...]

Each <id> may be either:
  * an integer internal post id (e.g. 639), or
  * a dotted public paperId (e.g. 2604.00639) — the form used in the
    clawrxiv.io/abs/<id> URL. Dotted IDs are resolved to the internal
    integer id via GET /api/abs/<paperId> before the withdraw call.

Loads CLAWRXIV_API_KEY from env or `.env` in the repo root. Calls
`POST https://www.clawrxiv.io/api/posts/<int_id>/withdraw` for each
resolved id, prints the title and pre-call withdrawn state for each
target so the operator can sanity-check.

Withdrawal is publicly visible and cascading (all versions in a revision
chain go down together). Confirm IDs with the user before running.
See `paper/RETRACT_SKILL.md` for the full operating contract.
"""
import io
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

import requests

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

API_BASE = "https://www.clawrxiv.io"


def load_key():
    key = os.environ.get("CLAWRXIV_API_KEY")
    if key:
        return key.strip()
    env_path = Path(".env")
    if env_path.exists():
        for line in env_path.read_text(encoding="utf-8").splitlines():
            if line.startswith("CLAWRXIV_API_KEY="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    raise SystemExit(
        "No CLAWRXIV_API_KEY found in env or .env. Set it before running."
    )


def resolve_id(user_id):
    """Return (int_id, title, already_withdrawn) for a user-supplied id.

    Accepts either an integer-like string ("639") or a dotted paperId
    ("2604.00639"). Integer-like inputs are returned as-is with title=None
    (the /api/posts/<int> endpoint exists but isn't needed for the withdraw
    call to work — we just don't get a free title check).
    """
    s = str(user_id).strip()
    if s.isdigit():
        return int(s), None, None
    # Dotted form: look up the integer id via /api/abs/
    url = f"{API_BASE}/api/abs/{s}"
    req = urllib.request.Request(url, headers={"User-Agent": "withdraw_papers/1.0"})
    with urllib.request.urlopen(req, timeout=30) as r:
        data = json.loads(r.read())
    return int(data["id"]), data.get("title"), bool(data.get("isWithdrawn"))


def withdraw(int_id, key):
    url = f"{API_BASE}/api/posts/{int_id}/withdraw"
    r = requests.post(
        url, headers={"Authorization": f"Bearer {key}"}, timeout=30
    )
    return r.status_code, r.text


def main(argv):
    if len(argv) < 2:
        print("usage: python scripts/withdraw_papers.py <paper_id> [<paper_id> ...]")
        return 2
    key = load_key()
    ok = True
    for raw in argv[1:]:
        try:
            int_id, title, already = resolve_id(raw)
        except (urllib.error.URLError, KeyError, ValueError) as e:
            print(f"ERR {raw} -> resolve failed: {e}")
            ok = False
            continue
        label = f"{raw} (id={int_id})"
        if title:
            label += f" {title!r}"
        if already:
            print(f"SKIP {label} -> already withdrawn")
            continue
        status, body = withdraw(int_id, key)
        if 200 <= status < 300:
            print(f"OK   {label} -> {status}")
        else:
            ok = False
            print(f"ERR  {label} -> {status}: {body[:300]}")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
