# Loka — Ontochronology

> Ontochronology: the study and modeling of what exists at which times.
> Time is not metadata on triples. Time is a structural axis of the database.
> Draft v0.1

---

## 1. What Is Ontochronology?

Ontochronology is the formal modeling of entity existence, state, and relationships across time in a queryable knowledge graph. It answers the question: **what was the complete state of the world at time T?**

The term combines *ontology* (what exists) with *chronology* (when it exists). Unlike temporal knowledge graphs that treat time as a qualifier hanging off triples, an ontochronological database treats time as a **first-class axis of the graph topology** — triples are organized along time, not merely annotated with it.

This distinction determines what you can query efficiently:

| Approach | Query | Cost |
|---|---|---|
| **Time as qualifier** | "Was Napoleon emperor at 1810?" | Point lookup on triple, then filter by time qualifier |
| **Time as structural axis** | "What was the complete world state at 1810?" | Single range scan on time-primary index |
| **Time as qualifier** | "What changed between 1804 and 1814?" | Full graph scan, filter every triple's qualifiers |
| **Time as structural axis** | "What changed between 1804 and 1814?" | Two range scans, compute diff |

The second approach makes temporal queries graph-native rather than post-hoc filters. This is the approach Loka takes.

---

## 2. Why Ontochronology?

### 2.1 The Gap in Existing Systems

Temporal data exists everywhere. But existing databases handle it poorly:

- **Relational databases** (SQL:2011 bitemporal tables) — temporal queries work within a fixed schema, but "give me the complete state of everything at time T" requires joining across every table. The temporal axis and the entity axis are orthogonal, and querying across both simultaneously is expensive.

- **Temporal knowledge graphs** (Wikidata-style qualifiers) — time is metadata on edges, not structure. The query model is "find triples, then filter by time." This answers "was this true at T?" but not "what was the complete state at T?" efficiently.

- **Time-series databases** (InfluxDB, TimescaleDB) — time is the primary index, everything else is a tag. Fast for "value of X over interval" but useless for relational queries between entities. No concept of "who else was in the room."

- **Event sourcing systems** — model state changes as an append-only log, but reconstruction requires full replay from the beginning unless you maintain snapshots. No native graph traversal.

None of these answer the fundamental ontochronological question: **traverse the graph at a specific moment in time, seeing only what existed then.**

### 2.2 Use Cases

**Text-to-video continuity** — Track every entity (character, prop, location) across every frame or scene. Query: "Who is holding the knife in scene 7? What are they wearing? Were they in the room when the victim entered?" Continuity errors become constraint violations.

**Legal/investigative modeling** — Reconstruct timelines from depositions and evidence. Query: "Where was the defendant between 9am and 11am on March 14th? Which witnesses' statements conflict about that interval?" Chain of custody as a temporal graph trace.

**Historical knowledge bases** — Model entities with imprecise temporal bounds. "George Clayton Abel lived from 1909 to October 29, 1977" — the birth year has year-level precision, the death date has day-level precision. Both are representable without forcing false precision.

**GraphRAG provenance** — Track which extraction pass produced which triples, from which document chunk, at which confidence level. Temporal indexing gives you "what did we know at time T" vs "what do we know now."

**Enterprise state machines** — Loan lifecycles, contract state, approval chains. Query the complete state of a loan at any point in its history. Bitemporal: distinguish "what was actually true" from "what was recorded."

---

## 3. The Ordering Axis

### 3.1 Not Always a Clock

The "T" in TSPO is not necessarily a UTC timestamp. It is an **ordered scalar** — any value that can be sorted and range-scanned. The indexing is identical regardless of what the scalar represents, because the data structure is always a B-tree over ordered keys.

The default is UTC timestamps, because most databases deal with real-world time. But many domains have a natural ordering axis that is not a clock:

| Domain | Ordering Axis | Example Values |
|---|---|---|
| Historical events | UTC timestamps (default) | `"1804-05-18"`, `"2024-03-14T10:00:00"` |
| Screenplays | Scene numbers | `1`, `2`, `3.5` (for inserted scenes) |
| Video production | Frame numbers or timecodes | `0`, `1`, `2`, ..., `86400` |
| Film continuity | Minutes into movie | `0.0`, `12.5`, `90.3` |
| Novels / books | Page numbers or chapter.paragraph | `1`, `42`, `300` |
| Religious texts | Book.chapter.verse | `1.1.1`, `1.1.2`, `66.22.21` |
| Legal proceedings | Exhibit numbers or transcript page | `1`, `2`, `47` |
| Music | Measure numbers or seconds | `1`, `2`, `3`, `4` |
| Software | Version numbers or commit ordinals | `1`, `2`, `3`, `1000` |

The axis type is a **database-wide setting** configured at creation time. Once set, it applies to all temporal predicates in that database. You do not mix frame numbers and UTC timestamps in the same TSPO index — that would make range scans meaningless.

```
# Database creation with non-default axis
loka create movie.sdb --temporal-axis=integer    # frame/scene numbers
loka create scripture.sdb --temporal-axis=float   # chapter.verse as float
loka create events.sdb                            # default: UTC timestamp
```

The implementation cost of supporting different axis types is near zero. The B-tree doesn't care what the bytes represent — it only needs a total ordering. An integer axis, a float axis, and a timestamp axis all produce the same index structure with the same performance characteristics.

### 3.2 Implications for Non-Temporal Axes

When the ordering axis is not a clock, some concepts adapt:

- **"Assertion time"** becomes **"assertion position"** — "known to be the case at this point in the sequence."
- **"Start time" / "end time"** become **"start position" / "end position"** — the interval during which a fact holds.
- **Precision** may not apply (frame 42 is exact; there's no "decade-level precision" for scene numbers).
- **The world state query** becomes "give me the complete state at position P" instead of "at time T" — same range scan, different semantics.

The SPARQL+ operators (`AT_TIME`, `DURING`, `WORLD_STATE`, `TEMPORAL_DIFF`) work identically regardless of axis type. The operator names reference time because that's the common case, but they accept any ordered scalar matching the database's axis type.

---

## 4. Temporal Model (UTC Default)

### 4.1 Three Temporal Signifiers

Every triple in Loka can carry up to three temporal signifiers. None are required — some triples are intrinsically atemporal (definitional facts, ontological axioms). A triple can have zero, one, two, or all three.

| Signifier | Meaning | When to use |
|---|---|---|
| **Assertion time** | "This fact was known to be the case at this time" | When start/end times are unknown. The crutch — a proxy for "true as of when we recorded it." |
| **Start time** | "This fact became true at this time" | When the onset of the fact is known. |
| **End time** | "This fact stopped being true at this time" | When the termination of the fact is known. |

These map onto the bitemporal model from database theory:
- **Start time / end time** = valid time (when something was true in the world)
- **Assertion time** = transaction time (when it was recorded)

But the key insight is that **assertion time is a fallback, not a parallel axis.** In a well-instrumented system, most triples have start/end times. Assertion time is the crutch you reach for when the world-state transition happened but nobody was watching.

### 4.2 What Assertion Time Actually Is

Assertion time says: "We have evidence this fact was the case at this moment." It does not assert when it started or ended. It is a **point attestation** — like a photograph. The triple is grounded to a witness, a document, an observation at a specific moment.

For a very large amount of real-world data, start and end times are simply not known. We only know that something was observed to be the case at a particular time. A newspaper article from 1847 tells us a building existed in 1847. We don't know when it was built. We don't know when (or if) it was demolished. Assertion time handles this without forcing us to fabricate interval endpoints.

The inference rule: a fact asserted at time T is likely true indefinitely into the past and future unless contradicted, but its relevance decays with temporal distance from T.

### 4.3 Precision

Temporal signifiers carry a precision level:

| Precision | Example | Meaning |
|---|---|---|
| Millennium | 2000 | "Sometime in this millennium" |
| Century | 1800 | "Sometime in the 19th century" |
| Decade | 1840 | "Sometime in the 1840s" |
| Year | 1847 | "Sometime in 1847" |
| Month | 1847-03 | "Sometime in March 1847" |
| Day | 1847-03-15 | "On March 15, 1847" |
| Hour | 1847-03-15T09 | "During the 9am hour" |
| Minute | 1847-03-15T09:32 | "At 9:32am" |
| Second | 1847-03-15T09:32:00 | "At exactly 9:32:00am" |
| Millisecond | 1847-03-15T09:32:00.000 | Sub-second precision |

Precision is not confidence. A fact with year-level precision is not "less certain" — it's genuinely imprecise. The granularity is part of the truth claim, not an epistemic hedge. Historical facts especially have this property: you know something happened in a decade, not a day.

### 4.4 Open Intervals and Absence

- A triple with a start time and no end time: **open-ended interval** — "still true as far as we know."
- A triple with an end time and no start time: "existed before our record begins, ended here."
- A triple with only assertion time: "we know it was the case at this moment, nothing more."
- A triple with no temporal signifiers at all: **atemporal** — intrinsically and permanently true. "2 + 2 = 4" doesn't have a start time.

The absence of temporal signifiers is not null — it is the correct representation. In the RDF open world, if something is not stated, it is unknown, not false. A triple without a start time doesn't have an "unknown" start time — the start time is simply not asserted.

### 4.5 Multiple Valid Times

A single triple can be valid at multiple disjoint time intervals. A person can hold the same job title at different periods (left, returned). A building can be used as a school, converted to offices, then converted back to a school. Each interval is a separate temporal annotation on the same triple.

This means the temporal annotation is not a single (start, end) pair — it is a **set of intervals** attached to the triple. Implementation-wise, each interval is a separate index entry pointing to the same triple.

---

## 5. Indexing Strategy

### 5.1 Time-Primary Index (TSPO)

The core ontochronological index adds time as a **leading key component**:

```
Standard triple index:  (Subject, Predicate, Object)
Time-primary index:     (Time, Subject, Predicate, Object)
```

This is the same insight applied to any dimension: **promote the query axis to a leading index key.**

With TSPO, "give me the complete world state at time T" is a single range scan on the first key component. Every triple valid at T falls out with no joins, no filter passes, no graph traversal.

### 5.2 Why This Is Cheap

Time indexing is a 1D exact range query on ordered data. That's a B-tree — the simplest, most well-understood index structure in computer science.

Compare to Loka's existing indexes:

| Index | Dimensions | Data Structure | Algorithmic Complexity |
|---|---|---|---|
| SPO/POS/OSP | N/A (key permutations) | B-tree / LSM | O(log n) |
| VECTOR(p) | 768–1536 dimensions | HNSW graph | O(log n) approximate |
| **TSPO** | **1 dimension** | **B-tree / LSM** | **O(log n) exact** |

HNSW solves a genuinely hard problem — approximate search in high-dimensional space. Temporal indexing is trivially cheap by comparison. It's just "put time first in the key." The overhead is basically just storage for the additional index.

### 5.3 Coordinate Indexing (Optional)

The same pattern extends to spatial data:

```
Temporal:   (T, S, P, O)       — slice the world at any moment
Spatial:    (X, Y, S, P, O)    — slice the world at any location
Combined:   (T, X, Y, S, P, O) — everything at location L during interval T
```

Coordinate indexing is opt-in. Not every dataset has spatial data, and forcing it as a mandatory dimension would be a design mistake. But when present, spatial queries are just range scans on composite keys — textbook 1980s algorithms.

| Query | Index | Data Structure |
|---|---|---|
| "Everything at time T" | TSPO | B-tree (1D range scan) |
| "Everything at location (X,Y)" | XYSPO | R-tree or composite B-tree (2D range scan) |
| "Everything at location L during interval T" | TXYSPO | Composite range scan |

The dimensional spectrum for Loka's indexes:

- **1D** (time, confidence, version): B-tree
- **2–3D** (coordinates): R-tree or composite B-tree
- **4–20D** (low-dimensional embeddings): KD-tree variants
- **20D+** (embeddings): HNSW

Loka already pays the hard tax for the high-dimensional end. Low-dimensional indexing is a rounding error on implementation cost.

### 5.4 Provenance as a Low-Dimensional Index

The same pattern generalizes beyond time and space to any low-dimensional metadata axis:

```
Provenance:  (doc_id, chunk_id, pass_id, S, P, O) — where did this triple come from
Confidence:  (score, S, P, O)                     — how reliable is this
Version:     (model_version, S, P, O)              — which extraction model asserted this
```

These compose: `(T, doc_id, confidence, S, P, O)` answers "give me high-confidence triples from source X valid during interval T" as a single range scan.

For GraphRAG, this means traceable reasoning is essentially free. The audit trail of which sources, extraction steps, and confidence levels contributed to an answer becomes a query rather than a pipeline crawl.

---

## 6. World State Queries

### 6.1 The Fundamental Query

The defining query of an ontochronological database:

> **Give me the complete state of the world at time T.**

This returns every triple that was valid at T — every entity that existed, every relationship that held, every attribute that was asserted. It is the temporal equivalent of a full graph dump, but scoped to a moment.

With the TSPO index, this is a range scan: all entries where the time component contains T (either as a point attestation at T, or as an interval containing T).

### 6.2 Temporal Diff

> **What changed between T1 and T2?**

Two range scans (world state at T1, world state at T2), then compute the diff. Triples present at T2 but not T1 are assertions. Triples present at T1 but not T2 are retractions. This is the core operation for changelog reconstruction.

### 6.3 Entity History

> **Give me the complete history of entity E.**

This is a subject-primary query with temporal ordering. The standard SPO index handles the entity lookup; the temporal signifiers on each triple give the ordering. No TSPO scan needed — just read E's triples and sort by time.

### 6.4 Co-Presence

> **Which entities were co-present during interval [T1, T2]?**

Range scan on TSPO for the interval, then group by subject. Any subject appearing in the result was "active" (had at least one valid triple) during the interval. This is the alibi query — "who was where during the relevant window."

### 6.5 Temporal Graph Traversal

> **Starting from entity E at time T, traverse relationship R through the graph, but only follow edges that were valid at T.**

This is a standard graph traversal with a temporal filter: at each hop, check that the edge triple was valid at T before following it. The TSPO index can provide the temporal filter efficiently.

---

## 7. SPARQL+ Temporal Extensions

### 7.1 AT_TIME

Scope a graph pattern to a specific moment:

```sparql
SELECT ?person ?location WHERE {
  AT_TIME("2024-03-14T10:00:00"^^xsd:dateTime) {
    ?person :locatedIn ?location .
    ?person rdf:type :Suspect .
  }
}
```

Semantics: only match triples that were valid at the specified time. Triples with assertion time at or near T are included with lower priority than triples with valid-time intervals containing T.

### 7.2 DURING

Scope a graph pattern to an interval:

```sparql
SELECT ?person ?location WHERE {
  DURING("2024-03-14T09:00:00"^^xsd:dateTime,
         "2024-03-14T11:00:00"^^xsd:dateTime) {
    ?person :locatedIn ?location .
  }
}
```

Returns triples whose valid-time interval overlaps with the specified interval.

### 7.3 WORLD_STATE

Retrieve the complete state snapshot at a given time:

```sparql
SELECT ?s ?p ?o WHERE {
  WORLD_STATE("2024-03-14T10:00:00"^^xsd:dateTime) {
    ?s ?p ?o .
  }
}
```

This is the fundamental ontochronological query expressed in SPARQL+.

### 7.4 TEMPORAL_DIFF

Compute the difference between two world states:

```sparql
SELECT ?change_type ?s ?p ?o WHERE {
  TEMPORAL_DIFF(
    "2024-03-14T09:00:00"^^xsd:dateTime,
    "2024-03-14T11:00:00"^^xsd:dateTime
  ) {
    ?s ?p ?o .
    BIND(loka:changeType AS ?change_type)
  }
}
```

Returns triples annotated with whether they were added, removed, or modified between the two timestamps.

### 7.5 Combining Temporal and Vector Queries

Ontochronological queries compose with existing SPARQL+ vector operators:

```sparql
SELECT ?doc ?entity WHERE {
  AT_TIME("2024-06-01"^^xsd:dateTime) {
    ?entity rdf:type :Person .
    ?doc :mentions ?entity .
  }
  VECTOR_SIMILAR(?doc :hasEmbedding "..."^^loka:f32vec, 0.85)
}
```

This finds documents semantically similar to a query vector that mention persons who existed at the specified time. The temporal filter runs on the TSPO index; the vector search runs on the HNSW index; the query planner interleaves them.

---

## 8. RDF-star Representation

### 8.1 Temporal Signifiers as Annotations

Temporal data is stored using RDF-star annotations on triples:

```turtle
# Assertion time only (the crutch)
<< :building_42 :locatedIn :MainStreet >> loka:assertedAt "1847"^^loka:temporal .

# Full valid-time interval
<< :napoleon :heldPosition :Emperor >> loka:validFrom "1804-05-18"^^loka:temporal ;
                                       loka:validTo   "1814-04-11"^^loka:temporal .

# Open-ended interval (still true)
<< :alice :worksAt :Acme >> loka:validFrom "2023-01-15"^^loka:temporal .

# Atemporal fact (no temporal signifiers at all)
:water :chemicalFormula "H2O" .
```

### 8.2 The `loka:temporal` Datatype

A new literal type that encodes both a timestamp and its precision:

```
"1847"^^loka:temporal           → year precision
"1847-03"^^loka:temporal        → month precision
"1847-03-15"^^loka:temporal     → day precision
"1847-03-15T09:32"^^loka:temporal → minute precision
```

The precision is derived from the format of the literal, not from a separate field. This keeps the data model simple while preserving precision information.

### 8.3 Multiple Valid Intervals

```turtle
# Person held same title in two separate periods
<< :alice :jobTitle :Director >> loka:validFrom "2018-01-01"^^loka:temporal ;
                                 loka:validTo   "2020-06-30"^^loka:temporal .

<< :alice :jobTitle :Director >> loka:validFrom "2022-03-01"^^loka:temporal ;
                                 loka:validTo   "2024-01-15"^^loka:temporal .
```

Each interval is a separate RDF-star annotation. The TSPO index has separate entries for each interval, both pointing to the same underlying triple.

---

## 9. Persistence and World State Snapshots

### 9.1 Event Sourcing Model

The natural storage model for ontochronological data is a **changelog** — an ordered sequence of state changes. The world state at any time T is the result of replaying all changes up to T.

This means the TSPO index is fundamentally an index over events, not states. The "state" at time T is computed by:
1. Finding the nearest snapshot before T
2. Replaying all changes between the snapshot and T

### 9.2 Periodic Snapshots

To avoid full replay on every temporal query, Loka can maintain periodic world-state snapshots. These are complete copies of all valid triples at meaningful boundaries:

- **Automatic**: at configurable intervals (hourly, daily, on significant change volume)
- **User-directed**: at domain-meaningful boundaries (scene breaks, chapter ends, transaction batches)

The query model becomes:
- "World state at T" → nearest snapshot + delta replay (fast)
- "What changed between T1 and T2" → diff two snapshots or scan changelog between them
- "History of entity E" → SPO scan, temporally ordered

### 9.3 Persistence-First Inference

For text extraction and narrative modeling, the **default is persistence**: any asserted state propagates forward in time until contradicted.

If a character is described as wearing a red coat in chapter 1, that coat persists indefinitely. The changelog only records changes, not continuations. This means:

1. **Default persistence rule**: asserted state propagates forward until contradiction
2. **Convention-based termination**: domain-specific implicit rules that suspend states (sleeping, swimming, scene changes)
3. **Explicit termination**: direct contradictions in the source text override everything

The conventions are stored as ontology triples in the database itself — they're domain-configurable, not hardcoded.

---

## 9b. Temporal Containment Model

When evaluating whether a triple is "valid at time T," there are three distinct containment states. These are not boolean — they form a three-valued logic that drives query filtering, ranking, and the semantics of AT_TIME / DURING.

### 9b.1 The Three States

| State | Meaning | Condition |
|---|---|---|
| **Definite** | Triple is certainly valid at T | T falls within a closed interval: `validFrom ≤ T ≤ validTo`, or `validFrom ≤ T ≤ assertedAt` |
| **Open** | Triple is probably valid at T, with decaying certainty | T is on the unbounded side of a half-open interval (e.g., `validFrom` exists, no `validTo`, `T > validFrom`) |
| **Outside** | Triple is certainly not valid at T | T falls before `validFrom`, or after `validTo`, or completely outside all annotated intervals |

These states reflect the epistemic reality of temporal data. A closed interval is a complete claim: "this was true from X to Y." An open interval is a partial claim: "this started at X, and we have no record of it ending." The further T is from the known endpoint, the less certain we are — but the triple is never *ruled out* the way an Outside triple is.

### 9b.2 Definite Containment

A triple is **Definite** at time T when T falls within fully bounded temporal annotations:

```
Case 1: validFrom ≤ T ≤ validTo        (closed interval)
Case 2: validFrom ≤ T ≤ assertedAt     (start + attestation = bounded)
Case 3: assertedAt = T                  (point attestation, exact match)
```

Case 2 deserves explanation: if a triple has both a `validFrom` and an `assertedAt` but no `validTo`, the interval `[validFrom, assertedAt]` is closed — we know the triple was true at `assertedAt` because someone observed it, and we know it started at `validFrom`. Between those two points, it is certainly valid.

### 9b.3 Open Containment (Distance from Open Point)

A triple is **Open** at time T when T lies beyond the known endpoint of a half-open interval. The key metric here is **distance** — how far T is from the last known point.

```
Case 1: validFrom exists, no validTo, T > validFrom
         → distance = T - validFrom

Case 2: validTo exists, no validFrom, T < validTo
         → distance = validTo - T

Case 3: assertedAt only, T ≠ assertedAt
         → distance = |T - assertedAt|
```

Distance is measured in the database's temporal axis units (seconds for UTC, integers for frame numbers, etc.). It serves two purposes:

1. **Ranking**: in AT_TIME queries, Definite triples rank above Open triples, and closer Open triples rank above distant ones.
2. **Decay threshold**: applications can set a maximum distance beyond which Open triples are excluded. A character wearing a red coat in scene 1 is probably still wearing it in scene 2 (distance=1), but maybe not in scene 50 (distance=49).

The distance is not stored — it is computed at query time from the TSPO index entries and the query's target time T.

### 9b.4 Outside

A triple is **Outside** at time T when it is definitively excluded:

```
Case 1: T < validFrom                  (hasn't started yet)
Case 2: T > validTo                    (already ended)
Case 3: Closed interval [validFrom, validTo] and T ∉ [validFrom, validTo]
```

Outside triples are never returned by AT_TIME or DURING queries. They are invisible at time T.

### 9b.5 Atemporal Triples

Triples with no temporal annotations at all are **atemporal** — they are valid at every time. They always match AT_TIME and DURING queries regardless of T. This is correct: `2 + 2 = 4` has no start time and no end time because it is timelessly true.

### 9b.6 The TemporalContainment Enum

The containment result for a triple at time T:

```rust
enum TemporalContainment {
    /// T is within a closed interval. Certainly valid.
    Definite,
    /// T is on the open side. Distance = seconds (or axis units) from
    /// the nearest known temporal endpoint.
    Open { distance: i64 },
    /// T is outside all temporal intervals. Not valid.
    Outside,
    /// No temporal annotations. Valid at all times.
    Atemporal,
}
```

Query operators use this enum to filter and rank:
- `AT_TIME`: returns Definite + Open (optionally filtered by max distance) + Atemporal. Excludes Outside.
- `DURING`: same logic applied to interval overlap rather than point containment.
- `WORLD_STATE`: same as AT_TIME over all triples.
- `TEMPORAL_DIFF`: compares containment at T1 vs T2 for every triple.

### 9b.7 Ordering in Query Results

When a temporal query returns multiple triples, they are ordered by containment quality:

1. **Atemporal** — always true, highest priority (definitional facts)
2. **Definite** — within closed bounds, second priority
3. **Open (ascending distance)** — closer to known endpoint = higher priority
4. **Outside** — never returned

This ordering ensures that well-annotated data always surfaces above uncertain data, and that temporal decay is reflected in result ranking.

---

## 10. Relationship to Existing Loka Architecture

### 10.1 New Index Type

TSPO joins SPO/POS/OSP/VECTOR as a fifth index type. Like VECTOR indexes, it is opt-in — enabled when temporal predicates (`loka:assertedAt`, `loka:validFrom`, `loka:validTo`) are present in the data.

The query planner treats TSPO the same way it treats all other indexes: as an access path with cost estimates. Temporal queries that benefit from TSPO get routed there; queries that don't simply ignore it.

### 10.2 New Predicates

| Predicate | Domain | Range | Purpose |
|---|---|---|---|
| `loka:assertedAt` | Quoted triple | `loka:temporal` | Point attestation time |
| `loka:validFrom` | Quoted triple | `loka:temporal` | Interval start time |
| `loka:validTo` | Quoted triple | `loka:temporal` | Interval end time |

These are reserved predicates that trigger TSPO indexing when used.

### 10.3 New Literal Type

`loka:temporal` — a timestamp with embedded precision. Stored internally as a (i64 timestamp, u8 precision) pair for efficient comparison and range scanning.

### 10.4 Compatibility

Ontochronological features are purely additive:
- Existing SPO/POS/OSP indexes are unaffected
- Existing HNSW vector indexes are unaffected
- Existing SPARQL+ queries work unchanged
- Databases without temporal data pay zero cost
- The TSPO index is only built when temporal predicates are present

---

## 11. Design Decisions

### 11.1 Why Not a Separate Temporal Database?

For the same reason Loka doesn't use a separate vector database. The whole point is that temporal queries compose with graph traversal and vector search in a single query. Splitting temporal data into a separate system creates the same JSON-handoff problem that Loka already solved for vectors.

### 11.2 Why RDF-star Annotations, Not Named Graphs?

Named graphs (the traditional RDF approach to context/provenance) are heavyweight and don't compose well. RDF-star annotations let you attach temporal metadata directly to the triple being annotated, which is the natural structure — "this relationship was valid from X to Y" is a statement about the relationship.

### 11.3 Why Precision, Not Confidence?

Temporal precision and temporal confidence are different things. Precision says "this timestamp has year-level granularity" — it's a fact about the data. Confidence says "we're 80% sure this timestamp is correct" — it's a fact about our belief. Precision is stored on the temporal literal itself. Confidence, if needed, goes on the triple as a separate predicate — it's not specific to temporal data.

### 11.4 Why Assertion Time Is a Crutch

In a perfect world, every fact would have known start and end times. Assertion time exists because the world is imperfect — for most historical data, most extracted data, and most real-time observations, we don't know when a state began or ended. We only know it was observed at a certain time.

As data quality improves, assertion time becomes less necessary. A well-instrumented system should produce mostly start/end time intervals, with assertion time as a fallback for the genuinely unknown.

---

## 12. Implementation Priority

1. ~~**`loka:temporal` literal type** — timestamp + precision, stored as (i64, u8)~~ ✅ Done
2. ~~**Reserved temporal predicates** — `loka:assertedAt`, `loka:validFrom`, `loka:validTo`~~ ✅ Done
3. ~~**TSPO index** — B-tree with time as leading key, built when temporal predicates are detected~~ ✅ Done
4. ~~**AT_TIME / DURING** — SPARQL+ temporal scope operators~~ ✅ Done
5. ~~**WORLD_STATE** — complete state snapshot query~~ ✅ Done
6. ~~**TEMPORAL_DIFF** — world state comparison~~ ✅ Done
7. **Periodic snapshots** — configurable snapshot boundaries
8. **Coordinate indexing** — XYSPO index (optional, same pattern)
9. **Convention ontology** — persistence rules for text extraction

---

## 13. Implementation Notes (Phases 1–4)

This section documents how the temporal operators were actually implemented, covering architecture decisions, execution models, and known limitations.

### 13.1 Storage Layer (Phase 1–3)

**TSPO index key format (33 bytes):**
```
[signifier:1 | timestamp:8 | subject:8 | predicate:8 | object:8]
```

The `signifier` byte (0=AssertedAt, 1=ValidFrom, 2=ValidTo) is the leading key component. This means all entries for a given signifier type are contiguous, enabling efficient range scans like "all ValidFrom entries before time T."

The timestamp uses **sign-bit-flip encoding** (`XOR` with `0x8000000000000000`) so that signed `i64` values sort correctly as unsigned bytes. This is the same technique RocksDB uses for ordered integer keys.

**Inline temporal encoding:** Temporal values are packed into 56-bit TermId payloads: 48-bit signed timestamp (seconds since epoch, ±4.4M years) + 4-bit precision level. This avoids dictionary lookups for temporal literals — they are compared and range-scanned as integers.

**Temporal annotations are on quoted triples, not regular triples.** The RDF-star pattern is:
```turtle
<< :alice :worksAt :acme >> loka:validFrom "2023-01-15"^^loka:temporal .
```
The TSPO index stores the *inner* triple `(:alice, :worksAt, :acme)` with the timestamp from the outer triple's object. The store's `insert_temporal()` method takes the pre-extracted (signifier, timestamp, S, P, O) — it does not parse RDF-star structure itself. The ingestion layer is responsible for detecting temporal predicates and calling `insert_temporal()`.

### 13.2 Annotation Gathering

To evaluate temporal containment for a specific triple, the executor calls `TripleStore::gather_temporal_annotations(s, p, o)`. This method scans the TSPO index for all three signifiers, filtering by (S, P, O), and returns a `TemporalAnnotations` struct containing all `asserted_at`, `valid_from`, and `valid_to` timestamps.

**Current limitation:** Because the TSPO key sorts by `[signifier | timestamp | S | P | O]`, we cannot do a prefix scan on (S, P, O) directly — we must scan all timestamps for each signifier and filter. This is O(N) per signifier where N is the total number of TSPO entries for that signifier.

This is acceptable because:
1. The TSPO index is much smaller than SPO (only temporally-annotated triples contribute entries).
2. The alternative — a reverse index (S,P,O → timestamps) — would add write amplification and storage overhead for a query pattern that's already fast enough in practice.

**Future optimization:** For large-scale deployments, a secondary SPOT (subject-predicate-object-time) index could provide O(1) lookups. This should be benchmarked against the current scan approach before committing to the extra write cost.

### 13.3 Temporal Query Execution Model

All four temporal operators (AT_TIME, DURING, WORLD_STATE, TEMPORAL_DIFF) follow the same **evaluate-then-filter** execution model:

```
1. Evaluate inner patterns normally against SPO/POS/OSP indexes
2. For each result row:
   a. Extract all bound triples from the row
   b. Gather temporal annotations for each triple
   c. Evaluate containment (AT_TIME) or overlap (DURING)
   d. Keep or discard the row based on the result
```

This is a **post-filter** strategy: the graph patterns run first, producing candidate rows, and the temporal check prunes them. The alternative — **pre-filter** via TSPO scan — would first find all triples valid at T, then evaluate patterns only against those triples. Pre-filtering would be faster for WORLD_STATE on large graphs but requires deeper integration with the query planner.

**Why post-filter first:** It's simpler, correct, and composes with all existing pattern types (OPTIONAL, UNION, VECTOR_SIMILAR, property paths). Pre-filter optimization is planned for WORLD_STATE specifically, where the TSPO scan can replace the inner pattern evaluation entirely.

### 13.4 AT_TIME Semantics

`AT_TIME(T) { patterns }` evaluates containment at a single point:

- **Definite** (T in closed interval) → visible
- **Open** (T past open endpoint, with distance) → visible
- **Atemporal** (no annotations) → visible
- **Outside** (T before start or after end) → invisible

A row passes the filter only if **all** triples in the row are visible. This is an AND semantic: if a row binds `?person :locatedIn ?place` and `?person :worksAt ?company`, both must be temporally visible at T.

### 13.5 DURING Semantics

`DURING(start, end) { patterns }` checks **interval overlap**, not point containment:

A triple overlaps `[q_start, q_end]` if:
- It's atemporal (always overlaps), or
- Any `assertedAt` point falls within `[q_start, q_end]`, or
- Any closed interval `[validFrom_i, validTo_i]` intersects with `[q_start, q_end]` (i.e., `start_i ≤ q_end AND end_i ≥ q_start`), or
- Any open-ended interval reaches into `[q_start, q_end]` (e.g., `validFrom` with no `validTo`, and `validFrom ≤ q_end`)

### 13.6 WORLD_STATE

`WORLD_STATE(T) { patterns }` is currently a semantic alias for `AT_TIME(T)` — it delegates to the same execution path. The distinction exists for two reasons:

1. **Intent signaling:** WORLD_STATE is meant for full graph dumps (`?s ?p ?o`), while AT_TIME is for scoped queries with bound patterns. This distinction will drive future optimization.
2. **Future TSPO-first execution:** For `WORLD_STATE(T) { ?s ?p ?o }`, the optimal execution plan is a TSPO range scan (all ValidFrom ≤ T, minus all ValidTo ≤ T), not an SPO full scan with post-filtering. This optimization requires query planner changes and will be implemented when benchmarks show the post-filter approach is a bottleneck.

### 13.7 TEMPORAL_DIFF

`TEMPORAL_DIFF(T1, T2) { patterns }` computes the set difference between two world states:

1. Evaluate inner patterns to get all candidate triples.
2. For each row, check visibility at T1 and T2 independently.
3. Classify:
   - **Added**: not visible at T1, visible at T2
   - **Removed**: visible at T1, not visible at T2
   - **Unchanged**: visible at both T1 and T2
   - Not visible at either → **skipped** (not in results)
4. Bind `?change_type` to the interned string `"added"`, `"removed"`, or `"unchanged"`.

**Design note:** The change type strings must be pre-interned in the TermDictionary for binding to work. If they are not interned, the `?change_type` variable will not be bound in the result row. This is a limitation of the current executor design where the dictionary is immutable during query execution. Applications that use TEMPORAL_DIFF should ensure these strings exist in the dictionary (e.g., by inserting a triple that references them, or by interning them at database creation time).

### 13.8 Timestamp Resolution

Temporal operators accept timestamps in multiple formats:

| Input | Interpretation |
|---|---|
| `"2024-03-14T10:00:00"^^xsd:dateTime` | Parsed as temporal literal → seconds since epoch |
| `"1847"^^loka:temporal` | Year precision → start of year in seconds |
| `"hello"` (plain literal) | Parsed as temporal string (ISO-like format) |
| `42` (integer literal) | Raw integer — used as-is (seconds, frames, scenes) |
| `?var` (bound variable) | Decoded from inline temporal TermId, or resolved from dictionary and parsed |

The `resolve_timestamp()` function in the executor handles all these cases. For databases using non-UTC ordering axes (integer frames, float chapter.verse), integer literals are the natural input format.

### 13.9 Query Planner Integration

Temporal operators are treated as subquery-weight patterns in the cost-based planner:

- **Cost weight:** Same as subqueries (12) — they should execute after binding patterns to minimize the candidate set that needs temporal filtering.
- **Variable collection:** Inner pattern variables are propagated through. TEMPORAL_DIFF also contributes `?change_type`.
- **Filter pushdown:** Temporal blocks are opaque to filter pushdown — a FILTER on a variable bound inside an AT_TIME block stays inside the block.

### 13.10 Temporal-Aware Property Path Traversal

Property paths (`?s :knows+ ?o`, `?s :knows* ?o`) inside temporal scope operators (AT_TIME, DURING, WORLD_STATE) are temporally aware: each edge in the BFS traversal is checked against the active temporal filter before being followed.

**How it works:**

1. When AT_TIME/DURING/WORLD_STATE evaluates its inner patterns, it sets a `TemporalFilter` on the `ExecutionContext` (either `AtTime(i64)` or `During(i64, i64)`).
2. During BFS in `evaluate_property_path()`, each candidate edge `(node, predicate, target)` is passed through `is_edge_temporally_valid()`.
3. This function calls `gather_temporal_annotations()` on the edge triple, then checks containment (for AtTime) or overlap (for During).
4. Edges that fail the check are pruned — neither added to results nor added to the BFS frontier.
5. After the temporal block finishes, the previous filter is restored (supporting nested temporal blocks).

**Example:**

```sparql
# At time 175, traverse :knows edges — only follow edges valid at T=175
SELECT ?person WHERE {
  AT_TIME(175) {
    :alice :knows+ ?person .
  }
}
```

If `:alice :knows :bob` is valid [100, 200] and `:bob :knows :charlie` is valid [150, 300] and `:charlie :knows :dave` is valid [250, 400], then at T=175:
- alice→bob: valid (175 ∈ [100, 200]) ✓
- bob→charlie: valid (175 ∈ [150, 300]) ✓
- charlie→dave: **outside** (175 < 250) ✗

Result: `?person` = `:bob`, `:charlie` (not `:dave`).

**Performance note:** Each edge check calls `gather_temporal_annotations()`, which is O(N) per signifier where N is the TSPO index size. For deep traversals on large temporally-annotated graphs, this could be slow. The SPOT reverse index (future work) would reduce this to O(1) per edge.

### 13.11 Known Limitations and Future Work

1. **No TSPO-first execution for WORLD_STATE.** Currently evaluates all triples then filters. For full graph dumps on large databases, this is O(total triples) instead of O(valid triples at T). Pre-filter via TSPO scan is the planned optimization.

2. **O(N) annotation gathering.** Scanning the full TSPO range per signifier for a specific (S,P,O) triple. A secondary SPOT index would give O(1) lookups at the cost of write amplification.

3. **Immutable dictionary during execution.** TEMPORAL_DIFF requires change type strings to be pre-interned. A future change could allow the executor to intern strings on-the-fly, or use a fixed set of well-known TermIds for change types.

4. ~~**No temporal-aware property path traversal.**~~ ✅ **Resolved.** Property paths inside AT_TIME/DURING/WORLD_STATE blocks now check each edge against the active temporal filter during BFS traversal. A `TemporalFilter` enum (AtTime/During) is threaded through the `ExecutionContext`, and `is_edge_temporally_valid()` prunes edges that are Outside or don't overlap the query interval.

5. **No temporal index statistics for the planner.** The planner cannot estimate cardinality of temporal patterns (e.g., "how many triples are valid at T?"). This means temporal blocks are always treated as subquery-cost, even when TSPO statistics could improve ordering decisions.
