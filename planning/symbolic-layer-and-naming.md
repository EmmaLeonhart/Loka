# SutraDB Symbolic Layer & Naming — Planning Document

**Status:** Draft, 2026-05-07
**Author:** Immanuelle (with Claude-assisted analysis)
**Decision needed:** Concrete shape of the symbolic layer + whether to rename `SutraDB` → `Pramana` (or `PramanaDB`).

---

## 1. Context

After a frank analysis of the [Pramana repo](https://github.com/Emma-Leonhart/Pramana), the conclusion is:

- Pramana the *codebase* is sunk cost. Streamlit + Fuseki + a 7 MB JSON blob is not a database. It is not worth porting.
- Pramana the *name and four ideas* are worth carrying forward.

This document captures (a) the four ideas and how they should land in SutraDB's symbolic layer, and (b) the open question of whether the database itself should be called Pramana.

The broader vision the work is in service of: a **neuro-symbolic infinite database for world models** — RDF-star storage (SutraDB has this), HNSW vector indexing (SutraDB has this), plus a symbolic layer that handles OWL class rules, provenance, stance, and epistemic grounding (this is what's missing).

---

## 2. The Four Ideas Worth Preserving

### Idea 1 — Tripartite value model with deterministic UUIDv5 structs

**The idea.** Every value in the database belongs to one of three kinds:

1. **`external-id`** — a reference to something with an authoritative ID elsewhere (Wikidata QID, ORCID, ISBN, DOI, MeSH ID, etc.). The ID is opaque to SutraDB; it just stores the string.
2. **`item`** — a SutraDB-native IRI for an entity that lives inside this graph. Minted at insert time.
3. **`struct`** — a literal value whose IRI is **deterministically derived from the canonical form of its content**, via UUIDv5. Two inserts of the same value get the same IRI. Free deduplication, no coordination required.

**Why this matters in SutraDB.** SutraDB already has `sutra:f32vec` for vector literals. The struct pattern generalises that idea: any typed literal with a canonical form gets a content-addressed IRI.

**Concrete shape.** Add to `sutra-core`:

```rust
// Existing: sutra:f32vec
// New: structured-value literal types with canonical-form interning
pub trait StructLiteral {
    fn canonical_form(&self) -> String;
    fn struct_iri(&self) -> Iri {
        Iri::from_uuid_v5(SUTRA_STRUCT_NAMESPACE, self.canonical_form())
    }
}
```

Initial struct types to ship:
- `sutra:num` — Gaussian rational `a + bi + c√n + d√(-n)` (port the four-tuple from Pramana's `combinatoric_classes.py`, simplified to start as just `Q[i]`)
- `sutra:date` — ISO 8601 with optional precision (year / month / day / instant)
- `sutra:coord` — WGS84 lat/lng pair
- `sutra:duration` — ISO 8601 duration
- `sutra:f32vec` — already exists

**Out of scope for v1.** Pramana's `LinearConstantComplex` (8-coefficient π/e basis). Cute but premature.

**Open questions.**
- UUIDv5 namespace UUID — pick one and freeze it as `SUTRA_STRUCT_NAMESPACE`.
- Should structs be visible in SPO/POS/OSP indexes as IRIs, or kept as a special literal kind? Recommend: stored as IRIs in the indexes for traversal symmetry, but the storage layer remembers they are content-addressed and can round-trip back to the canonical literal form on read.

---

### Idea 2 — Source-exempt edge classes

**The idea.** Not every triple needs provenance. Some predicates are *definitional* or *structural* and would be circular or absurd to demand citations for:

- `rdf:type`, `rdfs:subClassOf` — class membership and the class hierarchy itself
- `owl:sameAs`, `owl:differentFrom` — identity claims
- External-ID predicates (e.g. `wdt:P31`, `dbo:hasISBN`) — *the ID itself* is the citation
- The citation/stance predicates from Idea 3 — meta-edges about provenance, requiring provenance on them recurses infinitely

**Why this matters.** Without this, any "every claim must be cited" rule produces a graph where 80% of edges are bureaucratic noise about why `X rdf:type Y` is true.

**Concrete shape.** A predicate-level OWL annotation:

```turtle
sutra:sourceExempt rdf:type owl:AnnotationProperty .

rdf:type sutra:sourceExempt true .
owl:sameAs sutra:sourceExempt true .
sutra:supports sutra:sourceExempt true .
# user-extensible: any predicate the user designates as structural
```

The grounding analyzer (Idea 4) reads this annotation and skips exempt predicates when deciding whether a proposition has warrant.

**Open questions.**
- Should `sutra:sourceExempt` be a hardcoded list in the engine, or purely OWL-driven? Recommend: OWL-driven, with a default ontology shipped at `sutra:core` namespace that marks the obvious cases.

---

### Idea 3 — Stance vocabulary

**The idea.** A small, opinionated vocabulary for relationships between propositions:

| Predicate | Meaning |
|---|---|
| `sutra:supports` | Source S supports the truth of proposition P |
| `sutra:contradicts` | Source S contradicts proposition P |
| `sutra:isAbout` | Source S is about (mentions, discusses) proposition P or entity E |
| `sutra:isUncertain` | Marker for propositions the author is unsure of |
| `sutra:retracts` | A previously-asserted proposition has been withdrawn |

**Why this matters.** Provenance edges in real-world knowledge graphs are messy. PROV-O is too generic, SEPIO is too academic. Five terms cover 95% of practical needs and force consistency.

**Concrete shape.** Ship as a default ontology loaded into every `.sdb` (or `.pra`) on creation, in the `sutra:` namespace. Used with RDF-star:

```turtle
<< :paper_42 :discusses :TransformerArchitecture >> sutra:supports :paper_42 .
<< :paper_42 :discusses :TransformerArchitecture >> sutra:isUncertain true .
```

**Open questions.**
- Do we need `sutra:cites` separately, or is that just `sutra:isAbout` + the RDF-star reification? Recommend: skip `cites` for now, let users layer their own bibliographic vocabulary on top.

---

### Idea 4 — Grounding-level health metric

**The idea.** Every proposition in the graph belongs to one of these grounding levels:

- **BASE** — directly evidenced by an `external-id` source (a paper, a Wikidata QID, an observation)
- **L1..Ln** — cited *n* citation-hops back to a BASE proposition
- **ORPHAN** — no path to any BASE proposition

The distribution across these levels is a single-number health signal for "how well-grounded is this knowledge graph?"

**Why this matters.** Every existing graph DB tells you *how much* is in it. None tells you *how warranted* it is. This is the genuine epistemic insight buried in Pramana.

**Concrete shape.**

```bash
sutra health --grounding
# → BASE:    12,420 propositions (38%)
# → L1:       8,901 propositions (27%)
# → L2:       4,210 propositions (13%)
# → L3+:      1,108 propositions (3%)
# → ORPHAN:   6,191 propositions (19%) ← needs review
```

Algorithm: fixed-point traversal up to a depth bound (default 20 hops, matching Pramana's pragmatic cap), via `sutra:supports` / `sutra:isAbout` edges, treating `sutra:sourceExempt` predicates as transparent, terminating at any node with an `external-id` typed predicate.

**Out of scope for v1.** Real-time recomputation. Run on demand; cache the last result; recompute when the user asks.

**Open questions.**
- Should the depth bound be configurable per call, or fixed? Recommend: configurable, default 20.
- Should ORPHAN propositions be flagged in query results by default? Recommend: no — that's an OWL-validation-style decision, kept opt-in.

---

## 3. The Naming Question — `SutraDB` or `Pramana`?

The user has raised: *Pramana might actually be the better name for this database.*

Worth taking seriously. Here is the honest case both ways.

### 3.1 Case for renaming to Pramana

- **Semantic accuracy.** Pramāṇa (प्रमाण) means "valid means of knowledge / epistemic instrument" in Sanskrit philosophy — perception, inference, testimony, comparison. With the four ideas above, the database literally *is* an epistemic instrument: it stores claims, tracks their warrant, and reports their grounding level. The name describes the thing.
- **Sutra is generic.** "Sutra" means thread / aphorism. It evokes connectedness, which fits any graph DB. It does not fit *this* graph DB more than any other. There are also many existing products called Sutra-something, and the term is overloaded.
- **The four-idea framing demands the name.** None of the four ideas is about graph traversal *per se* — they are all about epistemic structure. Calling it `SutraDB` and then explaining "but it's about epistemology" is the wrong order.
- **`.pra` already exists.** Pramana the project uses `.pra` as a file extension. SutraDB could legitimately reclaim it for a proper binary RDF-star format (versus Pramana's JSON-blob misuse).
- **Differentiation.** "Pramana" is rare in software. "Sutra" is not.

### 3.2 Case for keeping SutraDB

- **Sunk-cost-name fallacy in reverse.** Pramana was the user's earlier failed project. Reusing the name might *feel* like dragging baggage forward, even if the new thing has no code in common.
- **Pronounceability.** "Sutra" is two syllables, immediately readable in English. "Pramāṇa" / "Pramana" is three syllables and the macron-or-no-macron ambiguity is permanent.
- **Existing investment.** Repo name, GitHub releases, Cargo crate names (`sutra-core`, `sutra-hnsw`, `sutra-sparql`, `sutra-proto`, `sutra-cli`, `sutra-ffi`), `.sdb` extension, `sutra:` IRI namespace, `sutra` CLI binary, branding in `README.md` and `CLAUDE.md`, `BENCHMARKS.md`.
- **Time.** A rename is a day of work, plus permanent confusion for early adopters.

### 3.3 Concrete rename cost catalog

If we rename, here is everything that touches:

| Surface | From | To | Cost |
|---|---|---|---|
| Repo name | `SutraDB` | `Pramana` or `PramanaDB` | Trivial (GitHub redirects). |
| Crate names | `sutra-core` etc. | `pramana-core` etc. | Trivial unless already published to crates.io. **Verify before renaming.** |
| File extension | `.sdb` | `.pra` | Trivial in code; medium for any existing `.sdb` files in the wild. **Likely none yet.** |
| CLI binary | `sutra` | `pramana` | Trivial. |
| IRI namespace | `sutra:` | `pramana:` (or keep `sutra:` for the predicate vocabulary as a sub-brand) | Affects every example and every default ontology. Medium. |
| Docs | `README.md`, `CLAUDE.md`, `docs/*.md`, `BENCHMARKS.md`, `TODO.md`, `planning/*.md` | rewrite | Half-day with `sed` + manual review. |
| Sutra Studio | the GUI | rename | Trivial. |
| External: any landing page, social, GitHub org | — | — | Depends. Likely small at this stage. |

**Realistic estimate:** one focused day if done before any v1.0 release. Significantly worse after.

### 3.4 Recommendation

**Rename to `Pramana`** (not `PramanaDB` — drop the suffix; it's clean enough), but **keep `sutra:` as the sub-brand for the SPARQL+ extension and the default predicate vocabulary**. So:

- The database is **Pramana**.
- The query language is **SPARQL+** (SPARQL 1.1 superset).
- The vector-extension predicates and default vocabulary stay in the `sutra:` namespace because that's the *thread/aphorism* metaphor that fits SPARQL traversal extensions.

This gives:
- An accurately-named database (Pramana = epistemic instrument).
- An accurately-named query extension (`sutra:` = threads through the graph).
- A natural division: Pramana the engine, Sutra the language extension.

**Trigger condition for the rename:** before the first tagged v1.0 release. After that, the cost climbs sharply.

**Don't rename if:** crates are already published to crates.io under `sutra-*` names with non-trivial download counts, OR there's a real user community calling it SutraDB.

### 3.5 crates.io status (verified 2026-05-07)

| Name | Status | Notes |
|---|---|---|
| `sutra-core`, `sutra-hnsw`, `sutra-sparql`, `sutra-proto`, `sutra-cli`, `sutra-ffi` | **all unpublished** | Migration cost on crates.io: zero. |
| `sutra` (bare) | published — unrelated | Daniel Norman's dev-environment status dashboard (v0.1.3, 43 downloads). Not in conflict; SutraDB never published under this name. |
| `pramana` (bare) | published — unrelated | Robert MacCracken's Rust statistics library: "distributions, Bayesian inference, hypothesis testing, Monte Carlo, Markov chains" (v1.2.0, 389 downloads). Mild thematic-overlap concern (Bayesian inference is epistemic) but the descriptions distinguish them clearly. The bare name is not available — workspace must use the `pramana-*` prefix. |
| `pramana-core`, `pramana-cli` | **unpublished** | Free to claim. By implication `pramana-hnsw`, `pramana-sparql`, `pramana-proto`, `pramana-ffi` are also free (verify before publishing). |

**Conclusion:** The rename is unblocked on crates.io. The full workspace would map cleanly to `pramana-*` prefix mirroring today's layout. Recommend claiming the `pramana-*` namespace by publishing placeholder v0.0.1 crates within a few days of deciding to rename, to avoid losing the namespace.

---

## 4. Implementation Order for the Symbolic Layer

Suggested rough ordering (does not yet account for the existing TODO.md / ontochronology pivot work):

1. **Idea 1 — struct literals** (`num:`, `date:`, `coord:`). Concrete, testable, immediately useful for the Wikidata BFS import. ~1 week.
2. **Idea 2 — `sutra:sourceExempt` annotation** + default ontology declaring the obvious exempt predicates. ~2 days.
3. **Idea 3 — stance vocabulary** as default ontology. Mostly schema work, no engine code. ~1 day.
4. **Idea 4 — grounding-level health metric** (`sutra health --grounding`). Depends on 2 and 3. ~3 days.
5. **Naming decision and rename** if going forward. Block before v1.0 release. ~1 day.

Total: roughly two focused weeks for the entire symbolic-layer minimum-viable surface, plus the rename window.

---

## 5. What This Document Does NOT Decide

- **Whether to rename.** The recommendation in §3.4 is a recommendation, not a commitment. Decide after sleeping on it.
- **The exact UUIDv5 namespace UUID** for struct literals.
- **Interaction with the ontochronology pivot.** TSPO indexing and temporal SPARQL+ operators are in flight; the symbolic layer needs to compose with them, not against them. To be reconciled in a separate doc.
- **MCP server surface for the symbolic layer.** Should `sutra mcp` expose `grounding_report` and `validate_owl` as tools? Probably yes, but separate decision.

---

## 6. Reference

- Frank assessment of the Pramana repo: see auto-memory `project_pramana_disposition.md`.
- Pramana repo location: `C:\Users\Immanuelle\Documents\Github\Pramana`.
- Pramana planning doc most worth reading once: `Pramana/planning/02_TECHNICAL_SPECIFICATIONS.md`.
- SutraDB current architecture: `docs/architecture.md`.
