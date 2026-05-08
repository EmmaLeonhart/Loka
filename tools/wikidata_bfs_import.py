"""Wikidata BFS Import for SutraDB.

Breadth-first imports entities from Wikidata starting from a seed entity,
fetches their properties, and generates embeddings via Ollama. All data is
sent to a running SutraDB instance.

Safe to terminate at any time — each entity is committed individually.

Usage:
    python tools/wikidata_bfs_import.py [--seed Q11064932] [--max-time 3600]
"""

import argparse
import io
import json
import signal
import sys
import time
from collections import deque
from pathlib import Path
from typing import Optional

import requests

STATE_PATH = Path("wikidata_import_state.json")
SAVE_EVERY = 25  # entities

# Fix Windows Unicode
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8")

# ── Configuration ────────────────────────────────────────────────────────────

SUTRA_ENDPOINT = "http://localhost:3030"
WIKIDATA_SPARQL = "https://query.wikidata.org/sparql"
OLLAMA_ENDPOINT = "http://localhost:11434"
EMBEDDING_MODEL = "mxbai-embed-large"
EMBEDDING_DIM = 1024
VECTOR_PREDICATE = "http://sutra.dev/hasEmbedding"

USER_AGENT = "SutraDB-Import/0.1 (https://github.com/EmmaLeonhart/SutraDB)"

# Rate limits
WIKIDATA_DELAY = 1.5  # seconds between Wikidata API calls
SUTRA_DELAY = 0.05    # seconds between SutraDB calls (local, fast)

# Graceful shutdown
shutdown_requested = False

def handle_signal(signum, frame):
    global shutdown_requested
    print("\n[SIGNAL] Graceful shutdown requested. Finishing current entity...")
    shutdown_requested = True

signal.signal(signal.SIGINT, handle_signal)
signal.signal(signal.SIGTERM, handle_signal)


# ── Wikidata API ─────────────────────────────────────────────────────────────

def fetch_entity_data(qid: str) -> Optional[dict]:
    """Fetch an entity's labels, descriptions, and claims from Wikidata."""
    url = f"https://www.wikidata.org/wiki/Special:EntityData/{qid}.json"
    try:
        resp = requests.get(url, headers={"User-Agent": USER_AGENT}, timeout=30)
        if resp.status_code != 200:
            print(f"  [WARN] Wikidata returned {resp.status_code} for {qid}")
            return None
        data = resp.json()
        return data.get("entities", {}).get(qid)
    except Exception as e:
        print(f"  [ERROR] Fetching {qid}: {e}")
        return None


WDT_PREFIX = "http://www.wikidata.org/prop/direct"
WD_ENTITY_PREFIX = "http://www.wikidata.org/entity"
SUTRA_IMPORTED_FROM = "http://sutra.dev/provenance/propositionImportedFrom"
# Stable IRI for "this came from a Wikidata claim on entity Q...".
# Per-entity granularity (one importedFrom per entity URL) is enough provenance
# without inflating the corpus with one row per imported triple.
WIKIDATA_SOURCE_IRI = "http://www.wikidata.org"


def snak_to_object(snak: dict, linked_qids: list[str]) -> str | None:
    """Render a Wikidata snak as an N-Triples object token (the right-hand
    side of `<s> <p> ___ .`).

    Returns None for unsupported snak types so the caller can skip cleanly.
    Supported value types: wikibase-entityid, string, monolingualtext,
    quantity, time. globecoordinate is split into lat/long elsewhere — when
    it appears as a qualifier value we skip rather than complicate the model.

    Side-effect: appends every referenced QID to `linked_qids` so BFS keeps
    walking into qualifier-referenced entities, not just main statements.
    """
    if snak.get("snaktype") != "value":
        return None
    datavalue = snak.get("datavalue", {})
    vtype = datavalue.get("type")
    value = datavalue.get("value")

    if vtype == "wikibase-entityid":
        target_qid = value.get("id", "")
        if not target_qid.startswith("Q"):
            return None
        linked_qids.append(target_qid)
        return f"<{WD_ENTITY_PREFIX}/{target_qid}>"
    if vtype == "string":
        val = str(value).replace('"', '\\"')
        return f'"{val}"'
    if vtype == "monolingualtext":
        text = value.get("text", "").replace('"', '\\"')
        lang = value.get("language", "und")
        return f'"{text}"@{lang}'
    if vtype == "quantity":
        amount = value.get("amount", "0")
        return f'"{amount}"^^<http://www.w3.org/2001/XMLSchema#decimal>'
    if vtype == "time":
        time_val = value.get("time", "")
        return f'"{time_val}"^^<http://www.w3.org/2001/XMLSchema#dateTime>'
    # globecoordinate as a qualifier: skip (lossy but cheap; v0)
    return None


def entity_to_triples(qid: str, entity: dict) -> tuple[list[str], list[str], str]:
    """Convert a Wikidata entity to N-Triples lines.

    Each Wikidata claim becomes:
      - one main triple   `<entity> <wdt:Pmain> <object> .`
      - one RDF-star annotation per qualifier:
          `<< <entity> <wdt:Pmain> <object> >> <wdt:Pqualifier> <qual_obj> .`

    Wikidata's RDF dump uses `pq:` and `pr:` namespaces because it predates
    RDF-star. SutraDB has RDF-star natively, so we use the same `wdt:`
    predicate URI for both main statements and qualifiers — the
    qualifier-vs-statement distinction is structural (subject is a quoted
    triple), not lexical.

    Returns (triples, linked_qids, label_for_embedding).
    """
    wd = f"{WD_ENTITY_PREFIX}/{qid}"
    triples: list[str] = []
    linked_qids: list[str] = []

    # Labels
    label_en = ""
    labels = entity.get("labels", {})
    for lang in ["en", "ja", "de", "fr", "zh"]:
        if lang in labels:
            val = labels[lang]["value"].replace('"', '\\"')
            triples.append(f'<{wd}> <http://www.w3.org/2000/01/rdf-schema#label> "{val}"@{lang} .')
            if lang == "en":
                label_en = labels[lang]["value"]

    # Descriptions
    descriptions = entity.get("descriptions", {})
    for lang in ["en", "ja"]:
        if lang in descriptions:
            val = descriptions[lang]["value"].replace('"', '\\"')
            triples.append(f'<{wd}> <http://schema.org/description> "{val}"@{lang} .')

    # Claims → main triple + RDF-star qualifier annotations
    claims = entity.get("claims", {})
    for prop_id, claim_list in claims.items():
        for claim in claim_list:
            mainsnak = claim.get("mainsnak", {})
            if mainsnak.get("snaktype") != "value":
                continue

            # Coordinates are a special case: split into lat/long on the entity
            # itself rather than a single object. Pre-existing behaviour, kept.
            datavalue = mainsnak.get("datavalue", {})
            if datavalue.get("type") == "globecoordinate":
                value = datavalue.get("value", {})
                lat = value.get("latitude", 0)
                lon = value.get("longitude", 0)
                triples.append(f'<{wd}> <http://www.w3.org/2003/01/geo/wgs84_pos#lat> "{lat}"^^<http://www.w3.org/2001/XMLSchema#decimal> .')
                triples.append(f'<{wd}> <http://www.w3.org/2003/01/geo/wgs84_pos#long> "{lon}"^^<http://www.w3.org/2001/XMLSchema#decimal> .')
                continue

            obj = snak_to_object(mainsnak, linked_qids)
            if obj is None:
                continue
            main_pred = f"<{WDT_PREFIX}/{prop_id}>"
            main_subject = f"<{wd}>"
            triples.append(f"{main_subject} {main_pred} {obj} .")

            qt_subject = f"<< {main_subject} {main_pred} {obj} >>"

            # Provenance: this claim was imported from Wikidata's record for
            # this entity. RDF-star annotation; same shape as inferredFrom.
            triples.append(
                f'{qt_subject} <{SUTRA_IMPORTED_FROM}> '
                f'<{WIKIDATA_SOURCE_IRI}/wiki/{qid}> .'
            )

            # RDF-star qualifiers on this claim
            qualifiers = claim.get("qualifiers", {})
            for q_pid, q_snaks in qualifiers.items():
                q_pred = f"<{WDT_PREFIX}/{q_pid}>"
                for q_snak in q_snaks:
                    q_obj = snak_to_object(q_snak, linked_qids)
                    if q_obj is None:
                        continue
                    triples.append(f"{qt_subject} {q_pred} {q_obj} .")

            # Wikidata "references" — same RDF-star shape as qualifiers,
            # using Wikidata's reference predicates (P854 reference URL,
            # P248 stated in, P813 retrieved, etc.). The API treats them
            # as a separate list, but semantically they're qualifiers.
            for ref in claim.get("references", []):
                for r_pid, r_snaks in ref.get("snaks", {}).items():
                    r_pred = f"<{WDT_PREFIX}/{r_pid}>"
                    for r_snak in r_snaks:
                        r_obj = snak_to_object(r_snak, linked_qids)
                        if r_obj is None:
                            continue
                        triples.append(f"{qt_subject} {r_pred} {r_obj} .")

    # Build embedding text from label + description
    desc_en = descriptions.get("en", {}).get("value", "")
    embed_text = f"{label_en}. {desc_en}" if label_en else qid

    return triples, linked_qids, embed_text


# ── Ollama Embedding ─────────────────────────────────────────────────────────

def get_embedding(text: str) -> Optional[list[float]]:
    """Get an embedding vector from Ollama."""
    try:
        resp = requests.post(
            f"{OLLAMA_ENDPOINT}/api/embeddings",
            json={"model": EMBEDDING_MODEL, "prompt": text},
            timeout=60,
        )
        if resp.status_code != 200:
            return None
        return resp.json().get("embedding")
    except Exception as e:
        print(f"  [WARN] Embedding failed: {e}")
        return None


# ── SutraDB Client ───────────────────────────────────────────────────────────

def sutra_health() -> bool:
    try:
        resp = requests.get(f"{SUTRA_ENDPOINT}/health", timeout=5)
        return resp.status_code == 200
    except:
        return False


def sutra_insert_triples(ntriples: str) -> int:
    """Insert N-Triples into SutraDB. Returns count inserted."""
    try:
        resp = requests.post(
            f"{SUTRA_ENDPOINT}/triples",
            data=ntriples.encode("utf-8"),
            headers={"Content-Type": "text/plain; charset=utf-8"},
            timeout=30,
        )
        if resp.status_code == 200:
            result = resp.json()
            return result.get("inserted", 0)
        return 0
    except Exception as e:
        print(f"  [ERROR] SutraDB insert: {e}")
        return 0


def sutra_declare_vector():
    """Declare the vector predicate (idempotent — ignores if already declared)."""
    try:
        requests.post(
            f"{SUTRA_ENDPOINT}/vectors/declare",
            json={
                "predicate": VECTOR_PREDICATE,
                "dimensions": EMBEDDING_DIM,
                "m": 16,
                "ef_construction": 200,
                "metric": "cosine",
            },
            timeout=10,
        )
    except:
        pass


def sutra_insert_vector(subject: str, vector: list[float]):
    """Insert a vector embedding for a subject."""
    try:
        requests.post(
            f"{SUTRA_ENDPOINT}/vectors",
            json={
                "predicate": VECTOR_PREDICATE,
                "subject": subject,
                "vector": vector,
            },
            timeout=30,
        )
    except Exception as e:
        print(f"  [WARN] Vector insert: {e}")


# ── Main BFS Import ─────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Wikidata BFS Import for SutraDB")
    parser.add_argument("--seed", default="Q11064932", help="Starting Wikidata QID")
    parser.add_argument("--max-time", type=int, default=3600, help="Max runtime in seconds")
    parser.add_argument("--max-entities", type=int, default=10000, help="Max entities to import")
    parser.add_argument("--no-vectors", action="store_true", help="Skip embedding generation")
    args = parser.parse_args()

    print(f"=== Wikidata BFS Import ===")
    print(f"Seed: {args.seed}")
    print(f"Max time: {args.max_time}s")
    print(f"Max entities: {args.max_entities}")
    print(f"Vectors: {'disabled' if args.no_vectors else 'enabled (mxbai-embed-large)'}")
    print()

    # Check SutraDB is running
    if not sutra_health():
        print("[ERROR] SutraDB is not running at", SUTRA_ENDPOINT)
        print("Start it with: cargo run -- serve")
        sys.exit(1)
    print("[OK] SutraDB is running")

    # Declare vector predicate
    if not args.no_vectors:
        sutra_declare_vector()
        print("[OK] Vector predicate declared")

    # BFS state — restore from prior run if present, else start fresh from seed.
    queue: deque = deque()
    visited: set[str] = set()
    if STATE_PATH.exists():
        try:
            prev = json.loads(STATE_PATH.read_text(encoding="utf-8"))
            if isinstance(prev.get("bfs_visited"), list):
                visited = set(prev["bfs_visited"])
            if isinstance(prev.get("bfs_queue"), list):
                queue = deque(prev["bfs_queue"])
            if visited or queue:
                print(f"[RESUME] {len(visited)} visited, {len(queue)} queued from prior run")
        except Exception as e:
            print(f"[WARN] State file unreadable, ignoring: {e}")
    if not queue and args.seed not in visited:
        queue.append(args.seed)
        print(f"[FRESH] Seeding from {args.seed}")

    start_time = time.time()
    total_triples = 0
    total_vectors = 0
    total_entities = 0
    errors = 0

    def save_state(final: bool = False) -> None:
        elapsed_now = time.time() - start_time
        state = {
            "seed": args.seed,
            "total_entities": total_entities,
            "total_triples": total_triples,
            "total_vectors": total_vectors,
            "errors": errors,
            "queue_remaining": len(queue),
            "elapsed_seconds": round(elapsed_now, 1),
            "visited_count": len(visited),
            "bfs_visited": sorted(visited),
            "bfs_queue": list(queue),
        }
        STATE_PATH.write_text(json.dumps(state, indent=2), encoding="utf-8")
        if final:
            print(f"\nState saved to {STATE_PATH}")

    print(f"\n--- Starting BFS from {args.seed} ---\n")

    while queue and not shutdown_requested:
        # Check time limit
        elapsed = time.time() - start_time
        if elapsed > args.max_time:
            print(f"\n[TIME] Reached {args.max_time}s limit. Stopping.")
            break

        if total_entities >= args.max_entities:
            print(f"\n[LIMIT] Reached {args.max_entities} entities. Stopping.")
            break

        qid = queue.popleft()
        if qid in visited:
            continue
        visited.add(qid)

        # Progress
        total_entities += 1
        rate = total_entities / max(elapsed, 1)
        print(f"[{total_entities:>5}] {qid} (queue={len(queue)}, triples={total_triples}, "
              f"vectors={total_vectors}, {elapsed:.0f}s, {rate:.1f} ent/s)")

        # Fetch from Wikidata
        entity = fetch_entity_data(qid)
        if entity is None:
            errors += 1
            continue
        time.sleep(WIKIDATA_DELAY)

        # Convert to triples
        triples, linked_qids, embed_text = entity_to_triples(qid, entity)

        # Insert triples into SutraDB
        if triples:
            ntriples_str = "\n".join(triples)
            inserted = sutra_insert_triples(ntriples_str)
            total_triples += inserted

        # Generate and insert embedding
        if not args.no_vectors and embed_text.strip():
            embedding = get_embedding(embed_text)
            if embedding and len(embedding) == EMBEDDING_DIM:
                wd_uri = f"http://www.wikidata.org/entity/{qid}"
                sutra_insert_vector(wd_uri, embedding)
                total_vectors += 1

        # Add linked entities to queue
        for linked in linked_qids:
            if linked not in visited:
                queue.append(linked)

        # Periodic checkpoint so Ctrl-C / crashes don't lose progress
        if total_entities % SAVE_EVERY == 0:
            save_state()

    # Summary
    elapsed = time.time() - start_time
    print(f"\n=== Import Complete ===")
    print(f"Entities processed: {total_entities}")
    print(f"Triples inserted:   {total_triples}")
    print(f"Vectors inserted:   {total_vectors}")
    print(f"Errors:             {errors}")
    print(f"Queue remaining:    {len(queue)}")
    print(f"Time:               {elapsed:.1f}s")
    print(f"Rate:               {total_entities / max(elapsed, 1):.1f} entities/s")

    save_state(final=True)


if __name__ == "__main__":
    main()
