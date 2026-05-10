"""Pull a small Wikidata neighborhood as N-Triples, starting from a random QID.

Used by the auto-regressive propgen test as a fresh, real-world seed graph.
Designed to be small and fast (defaults: 200 entities, 30s budget) — not the
production import pipeline (that's `wikidata_bfs_import.py`).

Output is a plain N-Triples file. Load it into a fresh Loka with:

    loka import training/data/seed_<qid>.nt --data-dir ./test-data
    loka serve --data-dir ./test-data --port 3031

Usage:
    python tools/wikidata_random_seed.py
    python tools/wikidata_random_seed.py --seed Q1 --max-entities 50
    python tools/wikidata_random_seed.py --random  # ignore --seed, pick one
"""
from __future__ import annotations

import argparse
import io
import json
import random
import sys
import time
from collections import deque
from pathlib import Path
from typing import Optional

import requests

if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

WIKIDATA_SPARQL = "https://query.wikidata.org/sparql"
USER_AGENT = "Loka-RandomSeed/0.1 (https://github.com/EmmaLeonhart/Loka)"

WD_ENTITY_PREFIX = "http://www.wikidata.org/entity"
WDT_PREFIX = "http://www.wikidata.org/prop/direct"
RDFS_LABEL = "http://www.w3.org/2000/01/rdf-schema#label"

WIKIDATA_DELAY = 1.5  # polite pacing


def random_qid_via_special_random() -> Optional[str]:
    """Hit Special:Random and pull the QID from the redirect URL."""
    try:
        resp = requests.get(
            "https://www.wikidata.org/wiki/Special:Random",
            headers={"User-Agent": USER_AGENT},
            allow_redirects=True,
            timeout=15,
        )
        # Final URL ends with /wiki/Q12345 for normal entities.
        url = resp.url
        if "/wiki/Q" in url:
            tail = url.rsplit("/wiki/", 1)[-1]
            if tail.startswith("Q") and tail[1:].isdigit():
                return tail
    except Exception as e:
        print(f"  [WARN] Special:Random failed: {e}", file=sys.stderr)
    return None


def fetch_entity(qid: str) -> Optional[dict]:
    """Pull the JSON for one entity from Special:EntityData."""
    url = f"https://www.wikidata.org/wiki/Special:EntityData/{qid}.json"
    try:
        resp = requests.get(url, headers={"User-Agent": USER_AGENT}, timeout=30)
        if resp.status_code != 200:
            return None
        data = resp.json()
        return data.get("entities", {}).get(qid)
    except Exception as e:
        print(f"  [WARN] Fetching {qid}: {e}", file=sys.stderr)
        return None


def escape_literal(s: str) -> str:
    return (
        s.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def snak_to_object(snak: dict, linked: list[str]) -> Optional[str]:
    """Render a snak as the object position of an N-Triples row.

    Pushes any QID-typed reference onto `linked` so BFS keeps walking.
    Skips snak types we don't handle simply (globecoordinate, undefined).
    """
    if snak.get("snaktype") != "value":
        return None
    dv = snak.get("datavalue", {})
    vtype = dv.get("type")
    value = dv.get("value")

    if vtype == "wikibase-entityid":
        target = value.get("id", "") if isinstance(value, dict) else ""
        if target.startswith("Q"):
            linked.append(target)
            return f"<{WD_ENTITY_PREFIX}/{target}>"
        return None
    if vtype == "string":
        return f'"{escape_literal(str(value))}"'
    if vtype == "monolingualtext":
        text = value.get("text", "") if isinstance(value, dict) else ""
        lang = value.get("language", "") if isinstance(value, dict) else ""
        if not text:
            return None
        if lang:
            return f'"{escape_literal(text)}"@{lang}'
        return f'"{escape_literal(text)}"'
    if vtype == "quantity":
        amount = value.get("amount", "") if isinstance(value, dict) else ""
        # Strip leading + for clean N-Triples decimals.
        if amount.startswith("+"):
            amount = amount[1:]
        return f'"{escape_literal(amount)}"^^<http://www.w3.org/2001/XMLSchema#decimal>'
    if vtype == "time":
        ts = value.get("time", "") if isinstance(value, dict) else ""
        if not ts:
            return None
        # Wikidata times look like "+2026-05-10T00:00:00Z" — strip the leading +.
        if ts.startswith("+"):
            ts = ts[1:]
        return f'"{escape_literal(ts)}"^^<http://www.w3.org/2001/XMLSchema#dateTime>'
    return None


def entity_to_triples(qid: str, entity: dict) -> tuple[list[str], list[str]]:
    """Convert a single entity's JSON dump into N-Triples + linked QIDs."""
    s_iri = f"<{WD_ENTITY_PREFIX}/{qid}>"
    triples: list[str] = []
    linked: list[str] = []

    # English label (if any) so the model has a prompt label downstream.
    labels = entity.get("labels", {})
    en_label = labels.get("en", {}).get("value")
    if en_label:
        triples.append(f'{s_iri} <{RDFS_LABEL}> "{escape_literal(en_label)}"@en .')

    # Mainsnaks for each property — direct (wdt:) form only. We deliberately
    # skip qualifier / reference annotations to keep the seed compact.
    for pid, statements in entity.get("claims", {}).items():
        if not pid.startswith("P"):
            continue
        p_iri = f"<{WDT_PREFIX}/{pid}>"
        for stmt in statements:
            mainsnak = stmt.get("mainsnak", {})
            obj = snak_to_object(mainsnak, linked)
            if obj is None:
                continue
            triples.append(f"{s_iri} {p_iri} {obj} .")

    return triples, linked


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", default="Q42", help="Starting Wikidata QID")
    parser.add_argument(
        "--random", action="store_true",
        help="Ignore --seed and pick a random QID via Special:Random",
    )
    parser.add_argument(
        "--max-entities", type=int, default=200,
        help="Hard cap on entities to fetch (default 200)",
    )
    parser.add_argument(
        "--max-time", type=int, default=120,
        help="Hard cap on Wikidata time in seconds (default 120)",
    )
    parser.add_argument(
        "--max-depth", type=int, default=10,
        help="BFS depth limit (default 10)",
    )
    parser.add_argument(
        "--output-dir", default="training/data",
        help="Where to write seed_<qid>.nt",
    )
    args = parser.parse_args()

    if args.random:
        picked = None
        for _ in range(5):
            picked = random_qid_via_special_random()
            if picked:
                break
        if not picked:
            print("[FATAL] Could not pick a random QID via Special:Random.", file=sys.stderr)
            sys.exit(2)
        args.seed = picked
        print(f"[RANDOM] Selected seed QID: {args.seed}", file=sys.stderr)

    output = Path(args.output_dir) / f"seed_{args.seed}.nt"
    output.parent.mkdir(parents=True, exist_ok=True)

    queue: deque[tuple[str, int]] = deque([(args.seed, 0)])
    visited: set[str] = set()
    all_triples: list[str] = []

    start = time.time()
    while queue:
        if len(visited) >= args.max_entities:
            print(f"[STOP] entity cap {args.max_entities}", file=sys.stderr)
            break
        elapsed = time.time() - start
        if elapsed > args.max_time:
            print(f"[STOP] time budget {args.max_time}s", file=sys.stderr)
            break

        qid, depth = queue.popleft()
        if qid in visited:
            continue
        if depth > args.max_depth:
            continue
        visited.add(qid)

        entity = fetch_entity(qid)
        if entity is None:
            print(f"  [SKIP] {qid} (fetch failed)", file=sys.stderr)
            continue
        time.sleep(WIKIDATA_DELAY)

        triples, linked = entity_to_triples(qid, entity)
        all_triples.extend(triples)
        print(
            f"[{len(visited):>4}/{args.max_entities}] {qid} d={depth} "
            f"+{len(triples)} triples (queue={len(queue)}, total={len(all_triples)})",
            file=sys.stderr,
        )

        for child in linked:
            if child not in visited:
                queue.append((child, depth + 1))

    output.write_text("\n".join(all_triples) + "\n", encoding="utf-8")
    summary = {
        "seed_qid": args.seed,
        "entities_fetched": len(visited),
        "triples_emitted": len(all_triples),
        "max_depth_reached": args.max_depth,
        "elapsed_seconds": round(time.time() - start, 1),
        "output_path": str(output),
    }
    summary_path = output.with_suffix(".meta.json")
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"\nWrote {len(all_triples)} triples for {len(visited)} entities", file=sys.stderr)
    print(f"  -> {output}", file=sys.stderr)
    print(f"  -> {summary_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
