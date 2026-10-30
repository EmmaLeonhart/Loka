# arXiv submission metadata

Step 11 of the arXiv-readiness timeline. The source package is built and verified by CI on every
paper change (`.github/workflows/paper-pdf.yml`, job artifact `paper-pdfs`, file
`loka-arxiv-source.tar.gz`): it compiles from a clean directory with pdflatex and no undefined
references. **Submitting is Emma's action.**

## Fields

- **Title:** Loka: Provenance for Retracting Model-Generated Triples in an RDF-star Store
- **Authors:** Emma Leonhart
- **Contact:** emma@topazcomputing.com
- **Primary category:** cs.AI (Artificial Intelligence)
- **Cross-list:** cs.DB (Databases)
- **Comments:** 18 pages. Code: https://github.com/EmmaLeonhart/Loka (release v0.4.7). Checkpoints and corpora: https://huggingface.co/datasets/EmmaLeonhart/loka and https://huggingface.co/datasets/EmmaLeonhart/normalized-wikidata
- **License:** CC BY 4.0 (decided 2026-10-16, see DEVLOG). It matches the rest of the release:
  open code (AGPL), CC0 source data, public checkpoints and corpora. A CC licence on arXiv can't
  be withdrawn once a version is posted, so this is the one field to re-read before clicking.

## Abstract (plain text, 1671 characters; arXiv limit 1,920)

Once model-generated statements are written into a knowledge graph next to curated data, it is hard to tell them apart, to keep them out of the next model's training data, or to remove them when a statement they depended on turns out to be wrong. We describe Loka, an RDF-star triplestore that stores model-predicted triples alongside curated ones and annotates each with RDF-star statements in a reserved namespace: the generating model, a confidence, and quoted pointers to the stored statements the prediction procedure took as input, which we call selection provenance. The namespace is checked at corpus extraction, at candidate selection and when the inference pipeline writes, and the extractor drops every triple flagged as generated, so the pipeline's own outputs never re-enter a training corpus. Because these dependencies are explicit graph edges, the store supports cascade retraction: removing a node also removes every generated triple that transitively depended on one of its statements, without following ordinary data edges. We exercise the loop with a series of small transformers trained from scratch on label-substituted Wikidata triples and use them to evaluate the provenance machinery on real data. Tested against an independent reference, retraction computes a 106k-triple removal from a 5M-row store in about 0.1 s in memory, and on a real 2M-triple Wikidata store with chained predictions it matched an independent closure on all 8,621 required removals. Selection provenance records what the procedure used, not what the model relied on, and on held-out triples the model does not beat a predicate-frequency baseline: our claims concern the provenance machinery, not the model. Code, all checkpoints and the cleaned corpora are released.

## Steps

1. Download the latest `paper-pdfs` artifact from the "Paper PDF" workflow run on `main` (or run
   `gh run download -n paper-pdfs` against that run) and take `loka-arxiv-source.tar.gz`.
2. On arxiv.org: Submit → upload the tarball as the source; arXiv compiles `paper.tex`.
3. Paste the fields and abstract above; check arXiv's compiled PDF against
   `paper-arxiv-check.pdf` from the same artifact.
