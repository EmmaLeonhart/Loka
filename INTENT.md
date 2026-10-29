# Loka — intent

What this project is for and the standing constraints, for the autonomous loop to re-read each
tick (cleanvibe `autonomous-loop` skill). CLAUDE.md is the full reference; this is the short form.

## Purpose

Loka is an RDF-star triplestore in Rust with native HNSW vector indexing and a SPARQL+ dialect.
Its research half stores model-generated triples next to curated ones with provenance in a
reserved namespace (`http://loka.dev/provenance/`), keeps them out of training corpora, and
supports cascade retraction.

## Current focus (Emma, 2026-10-16; still in force 2026-10-28)

The arXiv submission of `paper/paper.md` and nothing else. The queue holds only arXiv items;
an empty queue is refilled only from arXiv work (a fixable point in a new review under
`paper/reviews/`, a paper claim that no longer matches the code, a broken `/arxiv/` page or
Paper PDF build). Otherwise the tick is idle: "nothing actionable; arXiv waiting on Emma".
Only Emma does the arxiv.org steps; once she gives the arXiv ID it goes on `/arxiv/` and in
README.md.

## Standing constraints

- After any paper change: Paper PDF CI green, then refresh `pages/arxiv/` (PDF, source tarball,
  and the abstract copies in `pages/arxiv/index.html` and `paper/arxiv/METADATA.md`).
- Thermal limits on this laptop: never run `loka serve` + ingest + training together; no
  multi-hour experiment re-runs without asking Emma. v14 training is donor-only.
- Numbers in the paper are only changed when recomputed from the paper's tables or re-measured.
- Open decision for Emma (TODO.md): whether retraction should also sweep curated annotations
  of a removed triple; the paper documents current behaviour (§3.4, §7.2).
- When Emma is angry, freeze and answer from context; do not search or start anything new.
