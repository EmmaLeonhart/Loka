# arXiv submission metadata

Step 11 of the arXiv-readiness timeline. The source package is built and verified by CI on every
paper change (`.github/workflows/paper-pdf.yml`, job artifact `paper-pdfs`, file
`loka-arxiv-source.tar.gz`): it compiles from a clean directory with pdflatex and no undefined
references. **Submitting is Emma's action.**

## Fields

- **Title:** Loka: Provenance for Retracting Model-Generated Triples in an RDF-star Store
- **Authors:** Emma Leonhart
- **Contact:** emma@topazcomputing.com
- **Primary category:** cs.LG (Machine Learning) (Emma, 2026-10-30)
- **Cross-list:** cs.DB (Databases), cs.AI (Artificial Intelligence)
- **Comments:** 18 pages. Code: https://github.com/EmmaLeonhart/Loka (release v0.4.7). Checkpoints and corpora: https://huggingface.co/datasets/EmmaLeonhart/loka and https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata
- **License:** CC BY 4.0 (decided 2026-10-16, see DEVLOG). It matches the rest of the release:
  open code (AGPL), CC0 source data, public checkpoints and corpora. A CC licence on arXiv can't
  be withdrawn once a version is posted, so this is the one field to re-read before clicking.

## Abstract (plain text, 1,737 characters; arXiv limit 1,920)

When a model writes statements into a knowledge graph next to curated data, the graph should say which statements came from a model, from which model, and which stored statements each was produced from, so that generated statements can be found, filtered, and removed when a statement they depended on is withdrawn. We describe a small provenance vocabulary for model-generated statements and its implementation in Loka, an RDF-star triplestore. Each generated triple carries RDF-star annotations in a reserved namespace: a generated flag, the generating model, a confidence, and quoted pointers to the stored statements the prediction procedure took as input, which we call selection provenance. Because these dependencies are graph edges, the store can query them with ordinary SPARQL-star, leave flagged statements out of the corpora it extracts, and retract by dependency: removing a node also removes every generated triple that transitively depended on one of its statements, without following ordinary data edges. Retraction agrees with an independent reference implementation on generated graphs, computes the set for a 106k-triple removal from a 5M-row store in about 85 ms in memory (deleting it from disk is measured separately), and on a real 2M-triple Wikidata store with chained predictions it matched an independently computed closure on all 8,621 required removals, counted as (entity, generated triple) pairs. The generated triples come from small transformers trained from scratch on Wikidata, which serve as the workload; on held-out triples they do not beat a predicate-frequency baseline. Selection provenance records what the procedure used, not what the model relied on. Code, checkpoints and corpora are released.

## Steps

1. Download the latest `paper-pdfs` artifact from the "Paper PDF" workflow run on `main` (or run
   `gh run download -n paper-pdfs` against that run) and take `loka-arxiv-source.tar.gz`.
2. On arxiv.org: Submit → upload the tarball as the source; arXiv compiles `paper.tex`.
3. Paste the fields and abstract above; check arXiv's compiled PDF against
   `paper-arxiv-check.pdf` from the same artifact.
