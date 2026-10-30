# Loka model series: debugging history (v3–v14)

Moved out of the paper's Appendix B on 2026-10-29, when the paper (`paper/paper.md`) was
reframed as a provenance-vocabulary paper and its reviewers asked for less model-debugging
history. Section references (§4.2, §5.2–5.4) point into the paper.

## History

The examples and probe results summarised in §5.2–5.4.

### B.1 Datatype leakage (§5.2)

The first corpus serialised typed literals with their datatype IRI inside the literal string. A model trained on it (v3) learned to emit those fragments: `Abbas Mirza | has works in collection | 1 http www w3 org 2001 xmlschema decimal` at confidence 0.93. Stripping datatype suffixes before tokenization (§4.2) removed the pattern; the retrained model (v4) gives `metropolitan museum of museum` for the same query. v4's perplexity is *higher* than v3's (92.5 vs 53.4), because v3 had been earning cheap loss on high-frequency datatype tokens. Lower perplexity here signalled a worse corpus, which is why we treat perplexity as a diagnostic rather than an outcome.

### B.2 Catalog noise (§5.3)

A behavioural test on the v6 model exposed a larger corpus problem. The test seeds a breadth-first, depth-3 Wikidata neighbourhood at `Q42` (183 entities, 14,586 triples), takes 30 high-PageRank source triples, generates up to 10 candidate predictions per source, and keeps those at confidence ≥ 0.25, below the inference pipeline's default of 0.4 (§4.4), so it also counts emissions the pipeline would not write. It is a small, fixed probe, used to compare versions with each other, not a benchmark.

v6 produced confident identifier-shaped hallucinations: `British Broadcasting Corporation | ISNI -> "00000000"` (0.75), `Douglas Adams | Freebase ID -> "/ m / 0 c _ _ 9"` (0.43). The identifier *shape* also leaked onto unrelated predicates: `instance of -> "+ Ġof - 00 - 03 T 00"`, a Wikidata date prefix, appeared on 15 different subjects in one run.

The cause was corpus composition. Wikidata's `external-id` datatype, used for catalog cross-references (ISNI, GND, Freebase, LCCN, ...), covers 10,206 properties, about 80 % of all Wikidata properties, and 75.7 % of the rows in the v6 training file. We rebuilt the corpus excluding `external-id` and the other datatypes whose values carry no transferable content (`url`, `commonsMedia`, `math`, lexeme/sense/form, `globe-coordinate`, `geo-shape`, `musical-notation`, `tabular-data`, `wikibase-entity-schema`): 10,525 properties dropped, 2,231 kept (`wikibase-item`, `wikibase-property`, `quantity`, `string`, `time`, `monolingualtext`). Time and quantity literals were normalised at the same time (`+2012-10-15T00:00:00Z` → `2012-10-15`, `+1234` → `1234`). The full per-datatype specification ships with the code (`training/wikidata_excluded_predicates.json`).

The same probe across the versions trained on the cleaned corpora:

| | v6 (uncleaned) | v7 | v8 | v9 | v10 |
|---|---|---|---|---|---|
| Emissions at confidence ≥ 0.25 | 52 | 14 | 47 | 35 | 60 |
| on catalog-identifier predicates | 21 (40 %) | 9 (64 %) | 7 (15 %) | 1 (3 %) | 0 (0 %) |
| on semantic predicates | 31 (60 %) | 5 (36 %) | 40 (85 %) | 34 (97 %) | 60 (100 %) |
| `instance of` date-shape leak | 15 | 0 | 0 | 0 | 0 |

The date-shape leak disappears with the rebuild (v7) and does not return. v7 emits far less, because it no longer produces confident format strings; the failure mode moves from "confidently wrong" to "declines to emit", which is the right direction for a system whose output is stored. Longer training (v8) and fresh corpus slices (v9, v10) recover emission volume while the catalog share falls to zero. The v6 → v7 comparison is controlled (same architecture, tokenizer and epochs; only the corpus changed); the later columns also change training length or corpus slice, so they show the trend without isolating its cause.

### B.3 Residual failure modes (§5.4)

The cleaned models fail in narrower, more consistent ways. Raw outputs below keep BPE artifacts visible (`Ġ` marks a word boundary; `âĢĵ` is a mis-decoded en dash):

| Subject / predicate | Output | Confidence | Failure |
|---|---|---|---|
| `Adams / different from` | `"Adams"` | 0.960 | circular: subject copied as object (v8) |
| `Joan of Arc / Commons category` | `"Joan Ġof ĠAr c Ġ( Ġ("` | 0.654 | template overfit plus BPE fragments (v8) |
| `Leonardo da Vinci / country of citizenship` | `"Polish âĢĵ"` | 0.677 | right type (nationality), wrong value (v8) |
| `– / spouse` | `"1 ."` | 0.50 | numeric-format degeneration (v10) |

Circular `different from` follows from the masked-role objective on a predicate that is mostly reflexive in the corpus. The Commons-category outputs follow the most frequent predicate template. Neither argues for restoring the excluded datatypes.
