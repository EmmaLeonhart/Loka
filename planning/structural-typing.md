# Deterministic Structural Typing on Combinatoric Namespaces

**Status:** Design committed; v1 implementation pending.
**Date:** 2026-05-07
**Origin:** Pramana's `combinatoric_classes.py`, refined.
**Related:** `planning/symbolic-layer-and-naming.md`, `planning/world-model-thesis.md`.

---

## 1. The pattern

Class membership is a **pure function over the canonical form of an IRI**, not an asserted triple.

```turtle
# This triple is NEVER stored:
<num:1,2,0,1> rdf:type :Rational .

# Instead, the engine computes:
STRUCT_CLASS(<num:1,2,0,1>) → :Rational
STRUCT_CHAIN(<num:1,2,0,1>) → (:Rational, :Real, :Number)
```

Three pieces define a structural namespace:

1. **A parser** — parses the IRI suffix into a canonical form, rejects malformed values.
2. **A classifier** — pure function from canonical form to ordered class chain (most-specific to most-general).
3. **A v5 minter** — `uuid5(NAMESPACE, canonical_form)` so the same value always has the same identity, regardless of how it entered.

The same pattern handles vector literals (`sutra:f32vec`), numbers, dates, coordinates, durations — anything whose identity is its value.

---

## 2. Why this approach

- **Free.** No type triples stored. No `rdf:type` index pressure. Membership is computed in nanoseconds.
- **Closed-form.** No reasoner. No backtracking. No undecidability.
- **Deterministic.** Two routes to the same value collapse to the same IRI.
- **Composable with the world model.** The model never has to learn typing rules for values; that part of the symbolic layer is given.

The Pramana origin (`src/combinatoric_classes.py:38-165`) shipped this pattern in production for `num:` (Gaussian rationals). It works in the small. The risk in scaling it up is namespace proliferation; see §7.

---

## 3. Initial namespaces for v1

| Namespace | Canonical form | Class chain |
|---|---|---|
| `num:a,b,c,d` | reduced 4-tuple representing a/b + (c/d)i | GaussianRational → Rational → Real → Number (depending on values) |
| `date:YYYY-MM-DD` | ISO 8601 date | Date → TemporalPoint |
| `date:YYYY-MM-DDTHH:MM:SSZ` | ISO 8601 datetime in UTC | DateTime → TemporalPoint |
| `coord:lat,lon` | WGS84 decimal degrees, fixed precision | GeoCoordinate → SpatialPoint |
| `duration:PT30M` | ISO 8601 duration | Duration → TemporalInterval |
| `f32vec:[...]` (existing) | fixed-dim f32 array | Vector |

Each namespace ships a parser + classifier + minter as a Rust module in `sutra-core`. SPARQL+ functions exposed: `STRUCT_CLASS(?x)`, `STRUCT_CHAIN(?x)`, plus per-class predicates (`IS_INTEGER(?x)`, `IS_REAL(?x)`, `IS_PRIME(?x)`, etc.).

---

## 4. Use case: math database bootstrap

The motivating example. A math database is bootstrapped by:

1. **Established rules as code.** The `Number → Real → Rational → Integer → Whole → Natural` chain lives in `combinatoric_classes::num.rs`. No triples needed for the infinite class of rationals.
2. **Derived properties as functions.** `sign(?x)`, `decimal_approximation(?x, prec=3)`, `prime_factorization(?x)`. SPARQL+ functions, no storage.
3. **Named instances as classical IRIs.** Pi, e, golden ratio, Euler-Mascheroni get classical IRIs (`math:pi`, `math:e`, `math:phi`) with `owl:sameAs <num:...>` links to their structural twins for the rational approximations.
4. **Theorems and identities as triples.** "Pi is irrational" → `math:pi rdf:type :IrrationalNumber`. "e^(iπ) + 1 = 0" → reified RDF-star statement with citation.

A math database built this way needs zero typing triples for the infinite class of rationals. Type triples only exist for theorems, named entities, and OWL-style domain ontologies. Quoting the user: "you can use, let's say, a math database. You can bootstrap your math database with all these rules that are very well established plus individual things."

This pattern generalizes beyond math. Geographic gazetteer? Bootstrap with `coord:` plus named places. Temporal calendar? `date:` plus named events. Chemical inventory? `chem:` (formula or InChIKey) plus named compounds.

---

## 5. What it can and can't express

**Can express (covers most of OWL value-restriction territory):**

| OWL feature | Structural-typing equivalent |
|---|---|
| `subClassOf` chain | Hard-coded chain in classifier |
| `owl:Class` extension | The classifier's decision predicate |
| `owl:hasValue` / value restriction | Predicates on canonical components (`b == 1` → Integer) |
| `owl:intersectionOf` | Boolean AND in classifier |
| Derived property | Function returning a literal |
| `owl:equivalentClass` (canonicalization) | Reducer collapses equivalent forms before v5 |

**Cannot express:**

- **User-defined classes without code change.** Adding a new structural namespace requires editing Rust, not inserting triples. This is the deliberate tradeoff.
- **Cross-namespace reasoning.** Each classifier is isolated.
- **Existential restrictions** (`owl:someValuesFrom`).
- **Inverse-property and transitive closure.**
- **Cardinality restrictions.**

These limitations are fine because **structural typing is not trying to be OWL.** It is the value-space layer. OWL handles domain ontologies. The world model fills in expected triples per OWL templates. See `planning/world-model-thesis.md` §7.

---

## 6. Composition with the world model

The world model **does not learn typing for value spaces.** Asked to predict `?x rdf:type :Integer`, the model defers to `STRUCT_CHAIN(?x)`. This frees model capacity to learn what's actually hard: relations between named entities.

The model also doesn't need to predict canonical forms. If the model emits a vector that resolves (via HNSW) to `<num:3,1,0,1>`, the engine knows it's an integer 3, knows its decimal representation, knows it's prime — all without consulting the model again.

---

## 7. Risks

- **Namespace proliferation.** It is tempting to add a structural namespace for every typed literal a user wants. Resist. v1 ships exactly the namespaces in §3. New namespaces require a written justification documenting the canonical form, classifier, and use case before the namespace prefix is allocated.
- **Cross-namespace identity drift.** `num:1,1,0,1` and `date:2026-05-07` should never collide on UUID v5. Keep namespaces disjoint by including the namespace prefix in the v5 input string.
- **NFC vs. raw bytes for `str:`.** Pick one and freeze it (NFC recommended). Re-canonicalizing later breaks identity.

---

## 8. Open questions

- **Namespace UUID** for v5 minting. Pick one, freeze it.
- **NFC normalization for `str:`** — Unicode normalization form for string literals. NFC is the working default.
- **Time-zone handling for date/datetime/interval** — UTC default, offset suffixes allowed only in display. Canonical form is always UTC.
- **User-extensibility.** Pramana hardcoded the namespaces. Should SutraDB allow plugin-based namespace registration? Defer until a real use case demands it.
- **`coord:` precision and projection.** WGS84 default. Projected coordinates probably out of scope until a user asks.

---

## 9. References

- `planning/symbolic-layer-and-naming.md` — the broader symbolic-layer plan
- `planning/world-model-thesis.md` §4, §7 — how this composes with the world model
- `chats/world-models.md` — origin discussion (turn 60: "common concepts only need consistent embedding, only proper nouns need URIs")
- Pramana origin: `C:\Users\Immanuelle\Documents\Github\Pramana\src\combinatoric_classes.py:38-165`
