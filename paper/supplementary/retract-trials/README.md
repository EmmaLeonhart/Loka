# §6.2 retraction timings: five runs

`cargo bench -p loka-core --bench retract -- --noplot`, run five times back to back on
2026-10-29 (about 17:30–17:37 PST), release build, on the laptop described in the paper, with
no `loka serve`, ingest or training running. `runN.txt` is criterion's output for run N
(the time on its first line is the start of the whole batch, not of that run: cmd expands it
once). `runN_n<size>_estimates.json` is criterion's estimate file for that size and run.

The paper's §6.2 table takes each run's median (`median.point_estimate`, ns) and reports the
median of the five, with the min and max across runs:

| Generated triples | Per-run medians (ms) | Median | Range |
|---|---|---|---|
| 1,000 | 0.446, 0.453, 0.440, 0.465, 0.442 | 0.45 | 0.44–0.47 |
| 10,000 | 3.327, 2.567, 2.813, 3.035, 2.842 | 2.84 | 2.57–3.33 |
| 100,000 | 86.27, 104.63, 82.00, 88.70, 84.41 | 86.3 | 82.0–104.6 |
| 1,000,000 | 86.16, 85.03, 82.66, 80.52, 85.41 | 85.0 | 80.5–86.2 |
