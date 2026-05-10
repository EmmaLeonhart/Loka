"""Stream `philippesaade/wikidata` from Hugging Face into Loka.

Each entity row from the parquet stream is converted to N-Triples-star with
full RDF-star qualifier and reference annotations on each claim. No
Wikidata API rate limit, no embeddings. Stops when --max-triples is reached
or the stream is exhausted.

Usage:
    python tools/wikidata_hf_import.py [--max-triples 5000000]

Resume: state is checkpointed every --save-every entities; on restart the
script skips that many rows from the start of the stream and continues.
"""
from __future__ import annotations

import argparse
import io
import json
import signal
import sys
import time
from pathlib import Path
from typing import Optional

import requests

# UTF-8 stdout on Windows.
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

# ── Configuration ────────────────────────────────────────────────────────────

LOKA_ENDPOINT = "http://localhost:3030"
WDT_PREFIX = "http://www.wikidata.org/prop/direct"
WD_ENTITY_PREFIX = "http://www.wikidata.org/entity"
STATE_PATH = Path("wikidata_hf_import_state.json")

# Wikidata datatypes whose `datavalue` field is a plain string, not a dict.
STRING_DATATYPES = {
    "string", "external-id", "url", "commonsMedia",
    "math", "musical-notation", "tabular-data", "geo-shape",
}


def _escape(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ").replace("\r", " ").replace("\t", " ")


def hf_snak_to_object(snak) -> Optional[str]:
    """Render a philippesaade/wikidata snak as an N-Triples object token.

    Schema differs from the raw Wikidata API: there is no `snaktype` field,
    `datavalue` IS the value (not wrapped in `{type, value}`), and the
    datatype hint lives in `datatype` rather than inside datavalue. Each
    branch maps to the standard RDF representation Wikidata uses elsewhere.
    """
    if not isinstance(snak, dict):
        return None
    datatype = snak.get("datatype")
    datavalue = snak.get("datavalue")
    if datavalue is None:
        return None

    if datatype in ("wikibase-item", "wikibase-property", "wikibase-lexeme",
                    "wikibase-form", "wikibase-sense"):
        if not isinstance(datavalue, dict):
            return None
        target_id = datavalue.get("id", "")
        if not isinstance(target_id, str) or not target_id:
            return None
        return f"<{WD_ENTITY_PREFIX}/{target_id}>"

    if datatype in STRING_DATATYPES:
        if not isinstance(datavalue, str):
            return None
        return f'"{_escape(datavalue)}"'

    if datatype == "monolingualtext":
        if not isinstance(datavalue, dict):
            return None
        text = str(datavalue.get("text", ""))
        lang = str(datavalue.get("language", "und"))
        return f'"{_escape(text)}"@{lang}'

    if datatype == "time":
        if not isinstance(datavalue, dict):
            return None
        time_val = str(datavalue.get("time", ""))
        return f'"{_escape(time_val)}"^^<http://www.w3.org/2001/XMLSchema#dateTime>'

    if datatype == "quantity":
        if not isinstance(datavalue, dict):
            return None
        amount = str(datavalue.get("amount", "0"))
        return f'"{_escape(amount)}"^^<http://www.w3.org/2001/XMLSchema#decimal>'

    # globe-coordinate: skip in qualifier/reference position; main statements
    # split into separate lat/long triples in entity_to_triples.
    return None

# Graceful shutdown
shutdown_requested = False


def handle_signal(signum, frame):
    global shutdown_requested
    print("\n[SIGNAL] Graceful shutdown requested. Finishing current batch...")
    shutdown_requested = True


signal.signal(signal.SIGINT, handle_signal)
signal.signal(signal.SIGTERM, handle_signal)


# ── Parsing helpers ──────────────────────────────────────────────────────────

def maybe_json(value) -> dict:
    """Some columns are JSON strings, others may already be dicts. Be flexible."""
    if not value:
        return {}
    if isinstance(value, dict):
        return value
    try:
        return json.loads(value)
    except (TypeError, json.JSONDecodeError):
        return {}


def entity_to_triples(row: dict, skip_deprecated: bool = True) -> list[str]:
    """Convert one philippesaade/wikidata row to N-Triples-star lines.

    Each main-statement claim emits its primary triple plus RDF-star
    annotations: qualifier rows (one per qualifier snak) and reference rows
    (one per reference snak), all sharing the same `<<S P O>>` quoted-triple
    subject. Deprecated-rank claims are skipped by default.
    """
    qid = row.get("id", "")
    if not qid or not qid[0] in ("Q", "P"):
        return []

    wd = f"{WD_ENTITY_PREFIX}/{qid}"
    triples: list[str] = []

    # Labels — every language Wikidata has for this entity.
    for lang, lab in maybe_json(row.get("labels")).items():
        if not isinstance(lab, dict):
            continue
        val = str(lab.get("value", "")).replace('"', '\\"').replace("\n", " ")
        if val:
            triples.append(
                f'<{wd}> <http://www.w3.org/2000/01/rdf-schema#label> "{val}"@{lang} .'
            )

    # Descriptions — every language too.
    for lang, desc in maybe_json(row.get("descriptions")).items():
        if not isinstance(desc, dict):
            continue
        val = str(desc.get("value", "")).replace('"', '\\"').replace("\n", " ")
        if val:
            triples.append(
                f'<{wd}> <http://schema.org/description> "{val}"@{lang} .'
            )

    # Claims → main triple + RDF-star qualifiers + RDF-star references.
    for prop_id, claim_list in maybe_json(row.get("claims")).items():
        if not isinstance(claim_list, list):
            continue
        for claim in claim_list:
            if not isinstance(claim, dict):
                continue
            rank = claim.get("rank", "normal")
            if skip_deprecated and rank == "deprecated":
                continue
            mainsnak = claim.get("mainsnak", {})
            if not isinstance(mainsnak, dict):
                continue

            # globe-coordinate as a main statement: split into wgs84 lat/long
            # on the entity directly. Keeps the spatial pair attached without
            # wrestling with how to serialise a coordinate object.
            if mainsnak.get("datatype") == "globe-coordinate":
                dv = mainsnak.get("datavalue") or {}
                if isinstance(dv, dict):
                    lat = dv.get("latitude")
                    lon = dv.get("longitude")
                    if lat is not None and lon is not None:
                        triples.append(
                            f'<{wd}> <http://www.w3.org/2003/01/geo/wgs84_pos#lat> '
                            f'"{lat}"^^<http://www.w3.org/2001/XMLSchema#decimal> .'
                        )
                        triples.append(
                            f'<{wd}> <http://www.w3.org/2003/01/geo/wgs84_pos#long> '
                            f'"{lon}"^^<http://www.w3.org/2001/XMLSchema#decimal> .'
                        )
                continue

            obj = hf_snak_to_object(mainsnak)
            if obj is None:
                continue
            main_pred = f"<{WDT_PREFIX}/{prop_id}>"
            main_subject = f"<{wd}>"
            triples.append(f"{main_subject} {main_pred} {obj} .")
            qt = f"<< {main_subject} {main_pred} {obj} >>"

            for q_pid, q_snaks in (claim.get("qualifiers") or {}).items():
                if not isinstance(q_snaks, list):
                    continue
                for q_snak in q_snaks:
                    q_obj = hf_snak_to_object(q_snak)
                    if q_obj is None:
                        continue
                    triples.append(f"{qt} <{WDT_PREFIX}/{q_pid}> {q_obj} .")

            # References in this dataset are shaped [{Pxxx: [snak, ...]}, ...]
            # — no `snaks` wrapper, predicates are the dict keys directly.
            for ref in claim.get("references", []) or []:
                if not isinstance(ref, dict):
                    continue
                for r_pid, r_snaks in ref.items():
                    if not isinstance(r_snaks, list):
                        continue
                    for r_snak in r_snaks:
                        r_obj = hf_snak_to_object(r_snak)
                        if r_obj is None:
                            continue
                        triples.append(f"{qt} <{WDT_PREFIX}/{r_pid}> {r_obj} .")

    return triples


# ── Loka I/O ──────────────────────────────────────────────────────────────

def loka_health() -> bool:
    try:
        return requests.get(f"{LOKA_ENDPOINT}/health", timeout=5).status_code == 200
    except Exception:
        return False


def loka_insert_triples(ntriples: str) -> tuple[int, int]:
    """POST a batch. Returns (inserted, error_count). Errors are mostly
    duplicates from re-runs and don't matter."""
    try:
        resp = requests.post(
            f"{LOKA_ENDPOINT}/triples",
            data=ntriples.encode("utf-8"),
            headers={"Content-Type": "text/plain; charset=utf-8"},
            timeout=120,
        )
        if resp.status_code == 200:
            j = resp.json()
            return j.get("inserted", 0), len(j.get("errors", []))
        print(f"  [WARN] non-200: {resp.status_code} {resp.text[:120]}", flush=True)
        return 0, 0
    except Exception as e:
        print(f"  [ERROR] insert: {e}", flush=True)
        return 0, 0


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset", default="philippesaade/wikidata")
    parser.add_argument("--split", default="train")
    parser.add_argument(
        "--max-triples",
        type=int,
        default=5_000_000,
        help="Stop after this many triples have been inserted (default 5M).",
    )
    parser.add_argument(
        "--max-rows",
        type=int,
        default=10_000_000,
        help="Hard cap on rows consumed (default ~all).",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=1000,
        help="Number of triple lines per POST /triples call.",
    )
    parser.add_argument("--save-every", type=int, default=200, help="Checkpoint cadence (rows).")
    parser.add_argument(
        "--keep-deprecated",
        action="store_true",
        help="Include rank=deprecated claims (default: skip them).",
    )
    args = parser.parse_args()

    if not loka_health():
        print(f"[ERROR] Loka not running at {LOKA_ENDPOINT}", flush=True)
        print("Start it with: ./target/release/loka.exe serve --port 3030", flush=True)
        sys.exit(1)
    print("[OK] Loka is running", flush=True)

    # Restore state so re-runs don't redo work.
    rows_to_skip = 0
    starting_total_triples = 0
    if STATE_PATH.exists():
        try:
            prev = json.loads(STATE_PATH.read_text(encoding="utf-8"))
            rows_to_skip = int(prev.get("rows_consumed", 0))
            starting_total_triples = int(prev.get("total_triples", 0))
            print(
                f"[RESUME] Skipping first {rows_to_skip:,} rows; "
                f"resuming from {starting_total_triples:,} triples already inserted",
                flush=True,
            )
        except Exception as e:
            print(f"[WARN] state file unreadable, starting fresh: {e}", flush=True)

    print(f"Loading dataset {args.dataset!r} (split={args.split}, streaming)...", flush=True)
    from datasets import load_dataset

    ds = load_dataset(args.dataset, streaming=True, split=args.split)
    iter_ds = iter(ds)

    # Skip already-consumed rows.
    if rows_to_skip:
        print(f"Skipping {rows_to_skip:,} rows from previous state...", flush=True)
        for i in range(rows_to_skip):
            try:
                next(iter_ds)
            except StopIteration:
                print("  stream exhausted before reaching skip point", flush=True)
                return
            if i and i % 5000 == 0:
                print(f"  ... skipped {i:,}", flush=True)

    rows_consumed = rows_to_skip
    total_triples = starting_total_triples
    total_errors = 0
    triples_buffer: list[str] = []
    start_time = time.time()
    last_log = start_time

    def save_state() -> None:
        STATE_PATH.write_text(
            json.dumps(
                {
                    "dataset": args.dataset,
                    "split": args.split,
                    "rows_consumed": rows_consumed,
                    "total_triples": total_triples,
                    "total_errors": total_errors,
                    "elapsed_seconds": round(time.time() - start_time, 1),
                },
                indent=2,
            ),
            encoding="utf-8",
        )

    def flush_buffer() -> None:
        nonlocal total_triples, total_errors
        if not triples_buffer:
            return
        body = "\n".join(triples_buffer)
        ins, errs = loka_insert_triples(body)
        total_triples += ins
        total_errors += errs
        triples_buffer.clear()

    print(f"\n--- Streaming entities; target {args.max_triples:,} triples ---\n", flush=True)

    for row in iter_ds:
        if shutdown_requested:
            break
        if total_triples >= args.max_triples:
            print(f"\n[TARGET] Reached {args.max_triples:,} triples. Stopping.", flush=True)
            break
        if rows_consumed >= args.max_rows:
            print(f"\n[LIMIT] Reached {args.max_rows:,} rows. Stopping.", flush=True)
            break

        rows_consumed += 1
        triples = entity_to_triples(row, skip_deprecated=not args.keep_deprecated)
        triples_buffer.extend(triples)

        if len(triples_buffer) >= args.batch_size:
            flush_buffer()

        now = time.time()
        if now - last_log >= 5.0:
            elapsed = now - start_time
            rate_t = (total_triples - starting_total_triples) / max(elapsed, 1)
            rate_r = (rows_consumed - rows_to_skip) / max(elapsed, 1)
            print(
                f"  rows={rows_consumed:,}  triples={total_triples:,}  "
                f"({rate_r:.0f} ent/s, {rate_t:,.0f} triples/s, errors={total_errors})",
                flush=True,
            )
            last_log = now

        if rows_consumed % args.save_every == 0:
            flush_buffer()
            save_state()

    # Final flush + checkpoint.
    flush_buffer()
    save_state()

    elapsed = time.time() - start_time
    print(f"\n=== Import {'paused' if shutdown_requested else 'complete'} ===", flush=True)
    print(f"Rows consumed:       {rows_consumed:,}", flush=True)
    print(f"Triples inserted:    {total_triples:,}", flush=True)
    print(f"Per-line errors:     {total_errors:,}  (mostly duplicates on resume)", flush=True)
    print(f"Time:                {elapsed:.0f}s", flush=True)
    print(f"State saved to {STATE_PATH}", flush=True)


if __name__ == "__main__":
    main()
