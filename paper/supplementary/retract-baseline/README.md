# §6.2 external baseline: SPARQL-star closure vs `retract_set`

`cargo bench -p loka-sparql --bench retract_baseline -- --noplot`, run five times back to back
on 2026-10-29 (about 17:42–17:52 PST), release build, with no `loka serve`, ingest or training
running. Each run first checks that the SPARQL-star result equals `retract_set`'s set (the
`equal true` lines in `runN.txt`), then times both in the same process.
`runN_<bench>_n<size>_estimates.json` is criterion's estimate file.

Per-run medians (ms) and the median of the five:

| Size | `retract_set` | median | SPARQL-star closure | median |
|---|---|---|---|---|
| 1,000 | 0.373, 0.365, 0.406, 0.376, 0.360 | 0.373 | 3.793, 3.696, 3.733, 3.997, 3.657 | 3.733 |
| 10,000 | 2.359, 2.300, 2.451, 2.328, 2.373 | 2.359 | 29.80, 20.69, 20.79, 23.82, 20.54 | 20.79 |
| 100,000 | 73.13, 68.82, 71.51, 68.65, 71.56 | 71.51 | 450.6, 477.5, 467.3, 483.3, 469.4 | 469.4 |
| 1,000,000 | 95.14, 71.89, 70.21, 71.26, 70.63 | 71.26 | 414.5, 443.5, 429.4, 430.7, 421.6 | 429.4 |
