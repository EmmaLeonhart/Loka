"""Compute PageRank over the entity graph in a Loka instance (or .nt file).

Writes a JSON file mapping entity IRIs to scores, sorted descending.
Optionally writes the scores back into the graph as RDF-star annotations:

    <<entity>> <http://loka.dev/pagerank> "0.0042"^^xsd:decimal .

so downstream tooling can SPARQL on rank without having to load the JSON.

Usage:
    # From a running Loka:
    python tools/compute_pagerank.py \\
        --endpoint http://localhost:3030 \\
        --output training/data/pagerank.json

    # From an N-Triples seed file:
    python tools/compute_pagerank.py \\
        --nt-file training/data/seed_Q42.nt \\
        --output training/data/pagerank_Q42.json

    # Also write the top 1000 ranks back into the running Loka:
    python tools/compute_pagerank.py \\
        --endpoint http://localhost:3030 \\
        --output training/data/pagerank.json \\
        --write-back --top-k 1000

Background: see planning/pagerank-bootstrapping.md.
"""
from __future__ import annotations

import argparse
import io
import json
import sys
import time
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

PAGERANK_PREDICATE = "http://loka.dev/pagerank"
XSD_DECIMAL = "http://www.w3.org/2001/XMLSchema#decimal"


def load_triples_from_endpoint(endpoint: str) -> list[dict]:
    """Pull every triple from a Loka SPARQL endpoint."""
    import requests
    r = requests.post(
        f"{endpoint}/sparql",
        data="SELECT ?s ?p ?o WHERE { ?s ?p ?o }",
        headers={
            "Content-Type": "application/sparql-query",
            "Accept": "application/sparql-results+json",
        },
        timeout=300,
    )
    r.raise_for_status()
    return r.json()["results"]["bindings"]


def load_triples_from_nt(path: Path) -> list[dict]:
    """Reuse the propgen test's N-Triples parser."""
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "training"))
    from test_autoregressive_propgen import load_nt_file
    return load_nt_file(path)


def build_graph(triples: list[dict]):
    """Build a directed networkx graph over URI subjects → URI objects.

    Predicates are recorded as edge attributes but don't influence rank.
    Triples with non-URI subject OR non-URI object are skipped — PageRank
    over a graph with literal sinks would just bleed mass into the literals.
    """
    import networkx as nx
    g = nx.DiGraph()
    for t in triples:
        if t["s"]["type"] != "uri" or t["o"]["type"] != "uri":
            continue
        s = t["s"]["value"]
        o = t["o"]["value"]
        p = t["p"]["value"]
        if g.has_edge(s, o):
            # Multiple edges between the same pair: count as one for rank
            # purposes, but track distinct predicates for downstream reporting.
            g[s][o].setdefault("predicates", set()).add(p)
        else:
            g.add_edge(s, o, predicates={p})
    return g


def write_back(endpoint: str, ranks: list[tuple[str, float]], top_k: int) -> int:
    """POST RDF-star pagerank annotations for the top-K entities to Loka.

    Returns the number of annotations actually inserted. Existing pagerank
    annotations are not removed — re-running this writes a new annotation
    alongside any existing one, which is the conservative thing to do.
    Filter on the latest by `propositionGeneratedBy` if you need it.
    """
    import requests
    n = 0
    body_lines: list[str] = []
    for iri, score in ranks[: top_k]:
        # Annotate the entity (not a triple) — semantically the entity's
        # PageRank score is a property *of the entity*, so a plain triple
        # is the right shape. Use the reserved provenance namespace.
        body_lines.append(
            f'<{iri}> <{PAGERANK_PREDICATE}> "{score:.8f}"^^<{XSD_DECIMAL}> .'
        )
    body = "\n".join(body_lines)
    r = requests.post(
        f"{endpoint}/triples",
        data=body.encode("utf-8"),
        headers={"Content-Type": "text/plain; charset=utf-8"},
        timeout=120,
    )
    r.raise_for_status()
    return r.json().get("inserted", 0)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    src = parser.add_mutually_exclusive_group(required=True)
    src.add_argument("--endpoint", help="Loka SPARQL endpoint")
    src.add_argument("--nt-file", help="N-Triples file path")
    parser.add_argument("--output", required=True, help="Output JSON path")
    parser.add_argument("--top-k", type=int, default=0,
                        help="If >0, keep only the top K entities. Default 0 = all.")
    parser.add_argument("--damping", type=float, default=0.85,
                        help="PageRank damping factor (default 0.85)")
    parser.add_argument("--max-iter", type=int, default=200)
    parser.add_argument("--tol", type=float, default=1e-6)
    parser.add_argument("--write-back", action="store_true",
                        help="POST loka:pagerank annotations back to --endpoint")
    args = parser.parse_args()

    if args.write_back and not args.endpoint:
        print("--write-back requires --endpoint (we can't write to a file)",
              file=sys.stderr)
        sys.exit(2)

    t0 = time.time()
    if args.endpoint:
        print(f"Pulling triples from {args.endpoint}...", file=sys.stderr)
        triples = load_triples_from_endpoint(args.endpoint)
    else:
        print(f"Loading triples from {args.nt_file}...", file=sys.stderr)
        triples = load_triples_from_nt(Path(args.nt_file))
    print(f"  got {len(triples):,} triples ({time.time() - t0:.1f}s)", file=sys.stderr)

    print("Building entity-only directed graph...", file=sys.stderr)
    g = build_graph(triples)
    print(f"  {g.number_of_nodes():,} nodes, {g.number_of_edges():,} edges",
          file=sys.stderr)
    if g.number_of_nodes() == 0:
        print("Empty graph — nothing to rank.", file=sys.stderr)
        sys.exit(1)

    print(f"Computing PageRank (damping={args.damping})...", file=sys.stderr)
    import networkx as nx
    t1 = time.time()
    scores: dict[str, float] = nx.pagerank(
        g, alpha=args.damping, max_iter=args.max_iter, tol=args.tol,
    )
    print(f"  done in {time.time() - t1:.1f}s", file=sys.stderr)

    ranked = sorted(scores.items(), key=lambda kv: -kv[1])
    if args.top_k > 0:
        ranked = ranked[: args.top_k]

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps({
            "computed_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "source": args.endpoint or args.nt_file,
            "n_nodes": g.number_of_nodes(),
            "n_edges": g.number_of_edges(),
            "damping": args.damping,
            "top_k_kept": len(ranked),
            "scores": ranked,
        }, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    print(f"Wrote {len(ranked):,} ranked entities -> {out_path}", file=sys.stderr)
    print("Top 10:", file=sys.stderr)
    for iri, score in ranked[:10]:
        print(f"  {score:.6f}  {iri}", file=sys.stderr)

    if args.write_back:
        wb_top = args.top_k if args.top_k > 0 else len(ranked)
        print(f"Writing top {wb_top} pagerank annotations back to {args.endpoint}...",
              file=sys.stderr)
        inserted = write_back(args.endpoint, ranked, wb_top)
        print(f"  inserted {inserted:,} annotation triples", file=sys.stderr)


if __name__ == "__main__":
    main()
