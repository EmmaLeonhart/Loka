# Large feature work — plan (Emma, 2026-10-07: "Do the large feature work")

The `TODO.md` "Future Versions" items, in dependency order. Each phase is its own queue item,
lands in its own commits, and is done only when its tests pass locally and in CI. Anything not
understood well enough to build is written up as a spec first, not guessed.

## Phase 1 — property paths traverse virtual HNSW edges

**DONE 2026-10-07** (see DEVLOG). Also fixed on the way: bound-source HNSW hops returned nothing; `+`/`*` paths repeated nodes; `a*/b+` did not parse; nested sequences could reuse an intermediate variable.

**Now:** `?s :p+ ?o` / `:p*` BFS walks `store.find_by_subject_predicate` only. The HNSW edge
predicates (`loka:hnswNeighbor`, `hnswHorizontalNeighbor`, `hnswLayerDescend`) are *virtual*:
answered by `try_evaluate_hnsw_edge_pattern` from the live index, never stored. So
`?entry loka:hnswNeighbor+ ?x` reaches nothing.
**Build:** the path BFS gets its one-step neighbours from the same source as a single triple
pattern, including virtual HNSW edges when the predicate is one of the three. Cycle detection
and the depth cap stay.
**Test:** on a small index, `?entry loka:hnswNeighbor+ ?x` from the entry point reaches every
node in its connected component, and `hnswLayerDescend*/hnswHorizontalNeighbor+` (the example
in the executor's doc comment) parses and runs.

## Phase 2 — greedy descent semantics

**DONE 2026-10-07** with Phase 3: `GREEDY(vector)` (see DEVLOG).

**Build:** a traversal mode where each step moves to the neighbour closest to a query vector
and stops at a local optimum. That is HNSW's own search expressed as a path. It depends on
Phase 3's ordered traversal and exit conditions, so Phase 2's test lands with Phase 3:
greedy descent from the entry point reaches the same nearest neighbour as `index.search(k=1)`
on a fixed dataset.

## Phase 3 — UNTIL: predicate-based exit conditions on path traversal

**DONE 2026-10-07** (see DEVLOG; tests in `loka-sparql/tests/path_until.rs`).

**Design first** (`planning/until-syntax.md`): syntax, likely `?s :p+ ?o UNTIL(<filter expr
over ?o>)`; semantics: per-step evaluation (not a post-filter), per-branch exit (one branch
stopping doesn't stop others), and defined traversal order (BFS by depth, then by term value),
so "first match" is meaningful. HNSW-specific exit: "no closer neighbour" (local optimality).
**Test:** ordered traversal with UNTIL terminates early exactly where the condition first holds
on each branch; per-step evaluation differs observably from post-filtering (fewer nodes
expanded).

## Phase 4 — cost-based planning: HNSW as an access path

**Now:** the planner's VECTOR_SIMILAR heuristic is "subject bound → graph first, else vector
first". **Build:** estimate both plans' cost (pattern cardinality from the store's
`estimate_cardinality`; HNSW cost from k and ef) and choose. Adaptive execution (reordering
mid-query) is a separate, later step; it gets a spec, not a guess.
**Test:** queries where the heuristic picks the slower plan; the cost model picks the faster
one, measured.

## Phase 5 — background maintenance cycle

**Build:** in `loka serve`, a background task that detects low usage (query rate below a
threshold for N seconds), rebuilds HNSW from current vectors into a fresh index while the old
one keeps serving, then swaps atomically. Pseudo-table rediscovery uses the same cycle.
**Test:** queries during a rebuild keep answering from the old index; after the swap, deleted
(tombstoned) vectors are gone from the graph; the swap leaves no window with no index.

## Phase 6 — pseudo-tables

**Build:** invalidation tracking (rows whose interior nodes changed are flagged stale, then
rebuilt in the maintenance cycle), and planner recognition of multi-pattern queries that match
a subgraph pseudo-table. **Test:** a stale row is never served; a matching query is answered
from the pseudo-table with identical results to the pattern path.

## Phase 7 — health: query performance metrics

**Build:** per-pattern latency percentiles and planner-decision accuracy, recorded in
`loka-sparql/src/health.rs`, exposed via `loka health --json` and `/health`. (The
Flutter-dashboard item is obsolete: Flutter Studio was removed and Studio is Electron now; the
dashboard becomes a Studio page that reads the same JSON.)
**Test:** the metrics reflect a known query workload.

## Not in scope here

Emma-gated items (SDK publishing, release checklists) and the non-integral-numbers
representation decision.
