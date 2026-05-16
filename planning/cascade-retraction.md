# World-Model Cascade-Retraction — Design Doc

**Status:** Design fixed 2026-05-16. Implementation phased; Phase 0 (the
quoted-triple reverse index) is a hard prerequisite and is not yet built.
**Source:** `queue.md` Active #8. **Owner:** see queue.

This doc exists because cascade-retraction is *destructive* and its spec lists
three deep engine prerequisites. Per the project discipline rules
(`feedback_pramana_lesson`: spec before code, no parallel half-implementations;
CLAUDE.md: never half-ship), the algorithm and its dependency chain are nailed
here before any destructive code lands.

---

## 1. What the feature is

Remove any node — real data **or** AI-generated — and every generated
inference that transitively cites it disappears with it. A node has two kinds
of out-edges:

- **Ordinary data edges** (`wdt:P31`, `:hasEmbedding`, …). These are *not*
  derivations. Real-data → real-data is **not** a dependency.
- **Provenance back-edges** from generated triples that cited it:
  `<<G>> loka-prov:propositionInferredFrom <<src>>`.

Cascade-retraction propagates **only along provenance back-edges**,
recursively, regardless of whether the deleted node is real or model-emitted:

- Deleting a real-data node → drops the node's own triples **and** every
  generated triple whose `propositionInferredFrom` chain dereferences any of
  those rows, transitively.
- Deleting a generated node → the same, plus removes the node's own row.

RDFS/OWL closures stay out of scope (CLAUDE.md). This is purely provenance
bookkeeping. Traversal is **bounded to the reserved namespace**
`http://loka.dev/provenance/` — a regular predicate is never followed as if it
were a derivation edge.

## 2. The provenance shape (authoritative)

From `training/preprocess.py` (the canonical constants) and the write-back in
`training/infer_with_citations.py`. Reserved prefix:
`http://loka.dev/provenance/`. Predicates:

| Predicate | Subject | Object | Meaning |
|---|---|---|---|
| `propositionGenerated` | `<<G>>` (quoted generated triple) | `true` | G is model-emitted |
| `propositionGeneratedBy` | `<<G>>` | `"loka-wikidata-vN"` | which model |
| `propositionConfidence` | `<<G>>` | a literal | model confidence |
| `propositionInferredFrom` | `<<G>>` | `<<src>>` (quoted source triple) | **the derivation edge** |
| `propositionImportedFrom` | `<<G>>` | source ref | import provenance |

So a generated fact `S P "X"` lands as the asserted triple `S P "X"` plus
RDF-star annotation rows whose subject is `quoted_triple_id(S,P,"X")` and, for
`propositionInferredFrom`, whose object is `quoted_triple_id(src_s,src_p,src_o)`.

## 3. The hard prerequisite: a quoted-triple reverse index (Phase 0)

`loka_core::quoted_triple_id(s,p,o)` is a **content hash**. The store keeps
**no map from a quoted-triple id back to its (s,p,o)**. This was found during
the engine-bug-#2 investigation (2026-05-16) and is logged in `queue.md` as
ingest-side **Bug A** (the proto bulk path even persists the bare
`<<QUOTED_TRIPLE>>` sentinel string for these ids).

Cascade-retraction **cannot be correct without this reverse map**:

- Given a removed row `T = (s,p,o)`, the cascade must find every generated
  `<<G>>` with `<<G>> propositionInferredFrom <<src>>` where `<<src>>`
  *dereferences to* `T`. The annotation only stores `quoted_triple_id(src)`.
  To test "does `<<src>>` reference `T`?" we must reverse
  `quoted_triple_id(src) → (src_s,src_p,src_o)` and compare. With only the
  hash, the test is impossible (hashes are one-way) — you would have to
  enumerate every triple, re-hash it, and match, which is O(N) **per cascade
  node** → O(N·depth·deg), pathological at 4 M triples.

**Phase 0 deliverable (engine, `loka-core`):** a persisted
`quoted` side-index `quoted_triple_id → (s_id, p_id, o_id)` written whenever a
quoted-triple id is minted (proto bulk path, proto INSERT DATA path, CLI
import, FFI). This *also* fixes Bug A: term resolution for a quoted-triple id
can then render a faithful `<< s p o >>` instead of the sentinel/`_:idN`.
This is the gating item; everything below assumes it exists.

## 4. Algorithm (bounded provenance cascade)

Inputs: root IRI → `root_id` (dict lookup). Store + the Phase-0 reverse index.

```
RETRACT-SET(root_id):
  removed_triples = ∅                      # set of (s,p,o)
  frontier        = ∅                      # set of (s,p,o) newly removed this round

  # 1. The node's own data rows (ordinary edges in/out of the node).
  for T in store.find_by_subject(root_id)
        ∪ store.find_by_object(root_id):
      removed_triples.add(T); frontier.add(T)

  # 2. Provenance closure — only along propositionInferredFrom back-edges,
  #    only within the reserved namespace.
  while frontier not empty:
      next_frontier = ∅
      for T in frontier:
          qid_T = quoted_triple_id(T.s, T.p, T.o)
          # generated triples that cite T as a source:
          for ann in store.find_by_predicate_object(P_inferredFrom, qid_T):
              # ann.subject is quoted_triple_id(G); reverse it (Phase 0):
              G = reverse_quoted(ann.subject)
              if G is None: continue              # not a quoted triple → skip
              # remove G's asserted row + ALL its prov annotation rows:
              if G not in removed_triples:
                  removed_triples.add(G); next_frontier.add(G)
              for a in store.find_by_subject(ann.subject):
                  if is_reserved_prov(a.predicate):  # bound to namespace
                      removed_triples.add(reify(a))
      frontier = next_frontier

  return removed_triples
```

Key correctness points:

- **Bound:** only `find_by_predicate_object(propositionInferredFrom, …)` is
  followed for the recursion, and only annotation rows whose predicate
  `is_reserved_prov` are swept for a removed generated node. A regular edge is
  never a derivation.
- **Termination:** `removed_triples` is monotonically growing and finite;
  `next_frontier` only contains not-yet-removed nodes → the loop terminates in
  ≤ |store| iterations, in practice ≤ provenance-DAG depth.
- **Cycles:** the `if G not in removed_triples` guard makes a provenance cycle
  safe (idempotent).
- **Real→real is not a dependency:** step 1 takes the node's own rows once;
  step 2 never follows a non-provenance predicate, so deleting a real node
  does not chase ordinary `wdt:P*` edges into the rest of the graph.

Complexity with Phase 0: O(Σ deg(removed) + |prov edges touched|). Without the
O(deg) back-reference from an inner-triple id to its annotation rows (spec
prerequisite (a)), the `find_by_subject(ann.subject)` sweep is already O(deg)
via the SPO prefix scan, so prerequisite (a) is satisfied by the existing SPO
index *once Phase 0 gives us `ann.subject`*. Prerequisite (a) is therefore
**subsumed by Phase 0** — no separate back-ref structure is needed; the SPO
prefix scan on the (now-reversible) quoted id is the back-reference.

## 5. Surfaces

### 5.1 Preview endpoint (non-destructive) — `POST /retract/preview`

Body: `{ "iri": "<...>" }`. Returns the would-be-deleted set **without
committing**:

```json
{ "root": "<iri>",
  "depth": 3,
  "triples_by_depth": [ {"depth":0,"count":N,"iris":[...]}, ... ],
  "total_triples": M,
  "hnsw_tombstones": K }
```

This is the spec's prerequisite (c). Read-only; safe to call freely.

### 5.2 MCP tool — `retract_node`

Registered in `loka-cli/src/mcp.rs` alongside `sparql_query` / `insert_triples`.

- Input: `{ "iri": string, "commit": bool=false }`.
- `commit:false` (default) → returns the §5.1 preview. **Default is dry-run**
  because the operation is destructive and outward-facing (harness rule:
  confirm hard-to-reverse actions).
- `commit:true` → runs the preview, then deletes every triple in the set via
  the existing per-triple `DELETE DATA` path, flips HNSW tombstones via
  `VectorRegistry::delete` (prerequisite (b) — currently wired but never
  invoked from the delete path; this is where it gets invoked), and returns
  the realized counts + per-depth IRIs.

Name is `retract_node` (not `delete_*`) because it covers both real and
generated roots, matching the spec.

### 5.3 Loka Studio action

Click a node → call `/retract/preview` → render the dependency tree (which
generated rows would disappear) → explicit confirm → `retract_node commit:true`.
Studio is the only place the destructive path is one click from a human, so
the preview-then-confirm gate is mandatory there.

## 6. Phased implementation plan

| Phase | Deliverable | Crate | Destructive? | Gating |
|---|---|---|---|---|
| **0** | Persisted `quoted_triple_id → (s,p,o)` reverse index; mint it on every quoted-id creation; faithful term rendering (also fixes Bug A) | `loka-core`, `loka-proto`, `loka-cli`, `loka-ffi` | no | none — do first |
| **1** | Pure `retract_set(root_id, store, qindex) -> Vec<Triple>` cascade fn + unit tests (provenance DAG, cycle, real→real isolation, namespace bound) | `loka-sparql` or `loka-core` | no | Phase 0 |
| **2** | `POST /retract/preview` read-only endpoint | `loka-proto` | no | Phase 1 |
| **3** | `retract_node` MCP tool (`commit:false` default → preview; `commit:true` → delete via existing path + `VectorRegistry::delete`) | `loka-cli` | yes (commit only) | Phase 2 |
| **4** | Studio dependency-tree preview + confirm action | `loka-studio` | yes (commit only) | Phase 3 |

Each phase is independently shippable and the destructive surface (Phases 3–4)
is gated behind the non-destructive preview (Phases 1–2) and an explicit
`commit` flag.

## 7. Why nothing destructive ships in the 2026-05-16 sweep

Phase 0 is unbuilt and is a hard correctness prerequisite (a hash cannot be
reversed). Wiring a destructive cascade on top of a store that cannot reverse
quoted-triple ids would be the exact "parallel half-implementation" the
project's discipline rules forbid, and it deletes data. The honest, correct
unit of progress this session is this design + dependency analysis, plus the
already-shipped engine-bug-#2 fix that makes the query layer honest (a
precondition for trusting any preview output). Phase 0 is now the top of the
cascade-retraction work and is recorded as such in `queue.md`.
