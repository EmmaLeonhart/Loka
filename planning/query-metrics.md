# Query performance metrics in health (Phase 7)

Phase 7 of `planning/large-features.md`. Written before the code.

## What to measure

- **Query latency:** wall time per query; p50 / p90 / p99 and a count.
- **Per-pattern latency:** wall time of each top-level pattern, grouped by **shape**: the
  pattern kind plus which positions are constant (`C`), bound by an earlier pattern (`B`) or
  free (`?`), e.g. `triple(?,C,C)`, `triple(B,C,?)`, `vector(?)`, `path(C,+,?)`, `filter`.
  Per shape: count, p50 / p90 / p99 latency, p50 rows out.
- **Planner estimate accuracy.** The planner orders patterns by estimated rows. That estimate
  can only be checked where it predicts something observable: a pattern evaluated with
  **none of its variables already bound** (normally the first one). There the actual row
  count is directly comparable. For those patterns, record the q-error
  `max(1, est, actual) / max(1, min(est, actual))` (≥ 1; 1 = exact), and report the count, the
  p50 / p90 q-error, and the fraction within 2×. Patterns run under a pushed-down LIMIT are
  not scored either: the limit, not the estimate, sets their row count. Patterns after a join aren't scored:
  the planner's estimate ignores runtime bindings, so a comparison there measures nothing.
- **Samples** are kept in bounded rings (the last 1024 per series), so memory stays flat
  however long the server runs. Percentiles are nearest-rank over the ring.

## Where

- `loka-sparql::health::QueryMetrics`: thread-safe recorder plus a `report()` snapshot
  (serialisable).
- The executor records into it when given one (`execute_instrumented`). With no recorder, the
  cost is one `Option` check per pattern.
- The planner exposes its row estimate (`estimate_pattern_rows`) so the executor scores
  the same number the planner used.
- `loka serve`: `GET /health/queries` returns the report. `/health` stays a plain `ok`, since
  probes depend on it.
- `loka health` (offline, no queries run) doesn't get these metrics: there's nothing to
  measure in a process that serves no queries. The Studio dashboard page reads
  `/health/queries`.

## Tests

- A known workload: N queries of known shapes through the server. Checks:
  - per-shape counts match;
  - p50 ≤ p90 ≤ p99;
  - rows-out p50 matches the known result sizes;
  - the query count matches.
- Accuracy:
  - an exactly estimated pattern (`?s ex:p ?o`, a POS count) scores q-error 1;
  - a known mis-estimate scores the computed q-error. `<s> ?p <o>` is estimated as all of
    `s`'s triples; with 10 such triples and one match, q-error = 10.
  - Join-side patterns aren't scored.
- Bounded memory: after more than 1024 queries, sample rings hold 1024.
