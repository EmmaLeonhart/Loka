# UNTIL — exit conditions on property-path traversal (design)

Phase 3 of `planning/large-features.md`, with Phase 2 (greedy HNSW descent) as a special case.
Written before the code, as the plan requires.

## Syntax

A triple pattern whose predicate is a `+` or `*` path may carry one trailing clause before the
pattern's terminating `.`:

```sparql
?start :broader+ ?cat UNTIL(EXISTS { ?cat a :TopCategory }) .
?start :broader+ ?cat UNTIL(STRSTARTS(STR(?cat), "http://ex.org/root")) .
?entry loka:hnswNeighbor+ ?doc GREEDY("0.1 0.2 0.3"^^loka:f32vec) .
```

- `UNTIL(expr)`: `expr` is a FILTER expression. It may use the path's object variable (bound to
  the node being visited) and any variable bound before the pattern. An existence test on the
  visited node is written as standard SPARQL, `UNTIL(EXISTS { ?cat a :TopCategory })`; it binds
  nothing outside the clause. (Decided while building: a bare triple pattern inside `UNTIL(...)`
  would need a second expression grammar, while `EXISTS` reuses FILTER's.)
- `GREEDY(vector)`: greedy descent towards `vector` (Phase 2, below). It applies only to an
  HNSW edge predicate.

Only one clause per pattern. It is an error on a pattern whose predicate is not `+`/`*`.

## Semantics of UNTIL

1. **Per-step evaluation, not a post-filter.** The condition is evaluated as each node is
   reached. A node satisfying it is **emitted and not expanded**: traversal stops along that
   branch. A node not satisfying it is **expanded and not emitted**.
2. **Results** are the nodes where the condition first held on some branch: the "nearest
   matching ancestor" query. A post-filter (`FILTER` after a `+` path) differs observably,
   because it also returns matching nodes *beyond* a first match. That difference is the test.
3. **Per-branch exit.** Stopping at a match on one branch doesn't stop other branches. The
   visited set is shared across branches from one start node, so a node reached by two branches
   is evaluated once (the first time, in traversal order).
4. **Defined traversal order.** Breadth-first by depth; within a depth, nodes in ORDER BY value
   order (the `order_key` used for ORDER BY). So "first" is deterministic and independent of
   storage order.
5. **Bounds.** The existing depth cap (50) and the query deadline still apply. `*` evaluates
   the condition on the start node first (zero-length path): if it holds there, the start node
   is the only result.

## Semantics of GREEDY (Phase 2)

HNSW's own search, expressed as a path. From the start node, look at its neighbours over the
given HNSW edge predicate, move to the one closest to `vector` **if it is closer than the
current node**, and repeat. When no neighbour is closer (**local optimum**), stop and emit that
node. Exactly one result per start node. This is the "no closer neighbour found" exit from
`TODO.md`.

**Test:** on a fixed dataset, greedy descent over `hnswNeighbor` from a start node reaches a
node whose distance to `vector` is no greater than any of its neighbours' (local optimality,
checked independently), and on a well-connected small graph it equals `index.search(k = 1)`.
The second check is stated only for the dataset used, since greedy search on a graph is not
guaranteed to find the global nearest in general.

## BEAM (beam search, ef = k)

`?entry loka:hnswNeighbor+ ?n BEAM(vector, k)`: HNSW's layer search as a path, with beam
width k. Keep a candidate queue (best first) and the k best nodes found so far. Repeatedly
expand the best unexpanded candidate; stop when it is worse than the worst of the k best (no
candidate can improve the result). Emit up to k nodes, most similar first (ties by term id).
Nodes without a vector of matching dimension are skipped. Like GREEDY, it applies only to an
HNSW edge predicate.

**Tests:**
- `BEAM(v, 1)` reaches the same node as `GREEDY(v)`: with width 1 the beam is greedy
  descent.
- On the fixed 8-node index, `BEAM(v, k)` from doc0 equals the brute-force top k. As with
  GREEDY, this is stated for this dataset only.
- Results come out in similarity order, at most k of them.

## Implementation sketch

- Parser: after the object term of a path pattern, accept `UNTIL(` *bool expr* `)` or
  `GREEDY(` *vector literal* `)`, stored on a new `Pattern::Triple` field or a new
  `Pattern::PathUntil` variant (the latter keeps every other pattern untouched).
- Executor: the `+`/`*` BFS gains (a) frontier ordering by `order_key`, (b) a per-node condition
  check that decides emit-and-stop vs expand, and (c) for GREEDY, a neighbour choice by distance
  to the query vector (the vectors come from the registry via `entity_to_vectors`).
- Existence tests inside UNTIL reuse FILTER EXISTS machinery if present, else a one-row pattern
  evaluation with the visited node bound.
