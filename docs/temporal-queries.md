# Loka — Temporal Query Guide

> Practical guide to using Loka's temporal (ontochronological) query operators.
> For the theory and design behind these features, see `docs/ontochronology.md`.

---

## Quick Start

Loka treats time as a structural axis of the database, not metadata on triples. Four SPARQL+ operators let you query the graph at specific moments or intervals:

| Operator | Purpose |
|---|---|
| `AT_TIME(T)` | Scope patterns to a single moment |
| `DURING(T1, T2)` | Scope patterns to an interval (overlap semantics) |
| `WORLD_STATE(T)` | Complete state snapshot at a moment |
| `TEMPORAL_DIFF(T1, T2)` | Diff between two world states |

---

## 1. Storing Temporal Data

Temporal data is stored using RDF-star annotations with three reserved predicates:

| Predicate | IRI | Meaning |
|---|---|---|
| Assertion time | `loka:assertedAt` | "Known to be the case at this time" (point attestation) |
| Start time | `loka:validFrom` | "Became true at this time" (interval start) |
| End time | `loka:validTo` | "Stopped being true at this time" (interval end) |

### Insert temporal triples

```turtle
# Full interval: Napoleon was Emperor from 1804-05-18 to 1814-04-11
<< :napoleon :heldPosition :Emperor >> loka:validFrom "1804-05-18"^^loka:temporal .
<< :napoleon :heldPosition :Emperor >> loka:validTo "1814-04-11"^^loka:temporal .

# Open-ended: Alice works at Acme since 2023 (still employed)
<< :alice :worksAt :Acme >> loka:validFrom "2023-01-15"^^loka:temporal .

# Assertion time: building observed in 1847 (no known start/end)
<< :building_42 :locatedIn :MainStreet >> loka:assertedAt "1847"^^loka:temporal .

# Atemporal fact: no temporal predicates needed
:water :chemicalFormula "H2O" .
```

### Precision is automatic

The precision level is derived from the format of the temporal literal:

| Input | Precision |
|---|---|
| `"1847"` | Year |
| `"1847-03"` | Month |
| `"1847-03-15"` | Day |
| `"1847-03-15T09"` | Hour |
| `"1847-03-15T09:32"` | Minute |
| `"1847-03-15T09:32:00"` | Second |
| `"-0500"` | Year (501 BCE) |

Year-level precision means "sometime in 1847" — the timestamp covers the full year interval `[1847-01-01, 1848-01-01)`. This is not imprecision or uncertainty; it is the correct granularity of the data.

### Multiple valid intervals

A triple can be valid at multiple disjoint intervals:

```turtle
# Alice was Director twice: 2018-2020 and 2022-2024
<< :alice :jobTitle :Director >> loka:validFrom "2018-01-01"^^loka:temporal ;
                                 loka:validTo   "2020-06-30"^^loka:temporal .

<< :alice :jobTitle :Director >> loka:validFrom "2022-03-01"^^loka:temporal ;
                                 loka:validTo   "2024-01-15"^^loka:temporal .
```

---

## 2. The Ordering Axis

The "T" in temporal queries is not always a clock. Loka supports three axis types, configured at database creation:

```bash
loka create events.sdb                        # default: UTC timestamps
loka create movie.sdb --temporal-axis=integer  # frame/scene numbers
loka create scripture.sdb --temporal-axis=float # chapter.verse as float
```

| Axis | Input format | Use case |
|---|---|---|
| UTC (default) | ISO dates, `xsd:dateTime` | Historical events, enterprise state |
| Integer | Plain integers | Frame numbers, scene numbers, page numbers |
| Float | Floating-point numbers | Chapter.verse, timecodes |

All four temporal operators work identically regardless of axis type. The operator names reference "time" because that is the common case, but they accept any ordered scalar.

---

## 3. AT_TIME — Query at a Moment

`AT_TIME` scopes inner patterns to a single moment. Only triples valid at that moment are returned.

### Basic usage

```sparql
# Who was located where on March 14, 2024 at 10am?
SELECT ?person ?location WHERE {
  AT_TIME("2024-03-14T10:00:00"^^xsd:dateTime) {
    ?person :locatedIn ?location .
    ?person rdf:type :Suspect .
  }
}
```

### Historical query with year precision

```sparql
# Who held which position in 1810?
SELECT ?person ?position WHERE {
  AT_TIME("1810"^^loka:temporal) {
    ?person :heldPosition ?position .
  }
}
```

### Integer axis (frame numbers)

```sparql
# What entities are present in frame 42?
SELECT ?entity ?property ?value WHERE {
  AT_TIME(42) {
    ?entity ?property ?value .
  }
}
```

### With graph traversal

```sparql
# At time T, traverse :knows edges — only edges valid at T are followed
SELECT ?person WHERE {
  AT_TIME(175) {
    :alice :knows+ ?person .
  }
}
```

If `:alice :knows :bob` is valid [100, 200] and `:bob :knows :charlie` is valid [150, 300] and `:charlie :knows :dave` is valid [250, 400], then at T=175:
- alice -> bob: valid (175 in [100, 200])
- bob -> charlie: valid (175 in [150, 300])
- charlie -> dave: **outside** (175 < 250)

Result: `?person` = `:bob`, `:charlie` (not `:dave`).

### With vector search

```sparql
# Find semantically similar documents that mention people who existed in 2024
SELECT ?doc ?entity WHERE {
  AT_TIME("2024-06-01"^^xsd:dateTime) {
    ?entity rdf:type :Person .
    ?doc :mentions ?entity .
  }
  VECTOR_SIMILAR(?doc :hasEmbedding "..."^^loka:f32vec, 0.85)
}
```

Temporal filtering runs on the TSPO index; vector search runs on the HNSW index; the query planner interleaves them.

### Containment semantics

AT_TIME uses three-valued temporal containment:

| Containment | Meaning | Included? |
|---|---|---|
| **Definite** | T is within a closed interval (`validFrom <= T <= validTo`) | Yes |
| **Open** | T is past the open end of a half-open interval (distance from known endpoint) | Yes |
| **Atemporal** | Triple has no temporal annotations (timelessly true) | Yes |
| **Outside** | T is before `validFrom` or after `validTo` | No |

A row passes the filter only if **all** triples in the row are visible. If a row binds both `?person :locatedIn ?place` and `?person :worksAt ?company`, both triples must be temporally visible at T.

---

## 4. DURING — Query over an Interval

`DURING` returns triples whose valid-time interval **overlaps** with the query interval. This is not containment — any overlap is sufficient.

### Basic usage

```sparql
# Who was located where between 9am and 11am on March 14?
SELECT ?person ?location WHERE {
  DURING("2024-03-14T09:00:00"^^xsd:dateTime,
         "2024-03-14T11:00:00"^^xsd:dateTime) {
    ?person :locatedIn ?location .
  }
}
```

### Historical interval

```sparql
# What positions were held during the Napoleonic era (1799-1815)?
SELECT ?person ?position WHERE {
  DURING("1799"^^loka:temporal, "1815"^^loka:temporal) {
    ?person :heldPosition ?position .
  }
}
```

### Scene range (integer axis)

```sparql
# What entities appear in scenes 5 through 10?
SELECT ?entity ?property ?value WHERE {
  DURING(5, 10) {
    ?entity ?property ?value .
  }
}
```

### Overlap semantics

A triple overlaps the query interval `[Q_start, Q_end]` if:
- It is atemporal (always overlaps), or
- Any `assertedAt` point falls within `[Q_start, Q_end]`, or
- Any closed interval `[validFrom, validTo]` intersects (i.e., `validFrom <= Q_end AND validTo >= Q_start`), or
- Any open-ended interval reaches into the query range

---

## 5. WORLD_STATE — Complete Snapshot

`WORLD_STATE` retrieves every triple valid at time T. It is semantically equivalent to `AT_TIME` but named to signal intent: "give me the complete state of everything at this moment."

### Full graph dump at a moment

```sparql
# Complete world state at a specific moment
SELECT ?s ?p ?o WHERE {
  WORLD_STATE("2024-03-14T10:00:00"^^xsd:dateTime) {
    ?s ?p ?o .
  }
}
```

### Scoped world state

```sparql
# All properties of a specific entity at a given time
SELECT ?property ?value WHERE {
  WORLD_STATE("1810"^^loka:temporal) {
    :napoleon ?property ?value .
  }
}
```

### Frame snapshot (integer axis)

```sparql
# Everything in the scene at frame 1000
SELECT ?entity ?property ?value WHERE {
  WORLD_STATE(1000) {
    ?entity ?property ?value .
  }
}
```

---

## 6. TEMPORAL_DIFF — What Changed?

`TEMPORAL_DIFF` computes the difference between two world states. It binds a `?change_type` variable to `"added"`, `"removed"`, or `"unchanged"` for each triple.

### Basic diff

```sparql
# What changed between 9am and 11am?
SELECT ?change_type ?s ?p ?o WHERE {
  TEMPORAL_DIFF("2024-03-14T09:00:00"^^xsd:dateTime,
                "2024-03-14T11:00:00"^^xsd:dateTime) {
    ?s ?p ?o .
  }
}
```

### Historical diff

```sparql
# What changed between 1804 and 1814?
SELECT ?change_type ?person ?position WHERE {
  TEMPORAL_DIFF("1804"^^loka:temporal, "1814"^^loka:temporal) {
    ?person :heldPosition ?position .
  }
}
```

### Scene diff (integer axis)

```sparql
# What changed between scene 5 and scene 6?
SELECT ?change_type ?entity ?property ?value WHERE {
  TEMPORAL_DIFF(5, 6) {
    ?entity ?property ?value .
  }
}
```

### Filter by change type

```sparql
# Only show what was added between the two timestamps
SELECT ?s ?p ?o WHERE {
  TEMPORAL_DIFF("2024-01-01"^^xsd:dateTime,
                "2024-06-01"^^xsd:dateTime) {
    ?s ?p ?o .
  }
  FILTER(?change_type = "added")
}
```

### Change type semantics

| `?change_type` | Meaning |
|---|---|
| `"added"` | Not visible at T1, visible at T2 |
| `"removed"` | Visible at T1, not visible at T2 |
| `"unchanged"` | Visible at both T1 and T2 |
| *(skipped)* | Not visible at either — excluded from results |

---

## 7. Timestamp Formats

All temporal operators accept timestamps in multiple formats:

| Format | Example | Notes |
|---|---|---|
| `xsd:dateTime` typed literal | `"2024-03-14T10:00:00"^^xsd:dateTime` | Standard XML Schema datetime |
| `loka:temporal` typed literal | `"1847"^^loka:temporal` | Precision derived from format |
| Plain string literal | `"1847-03-15"` | Parsed as ISO-like date |
| Integer literal | `42` | Raw integer — frames, scenes, pages |
| Bound variable | `?timestamp` | Decoded from inline temporal TermId |

For databases using the integer or float temporal axis, integer literals are the natural input format.

---

## 8. Use Case Recipes

### Investigation timeline

```sparql
# Where was the defendant during the relevant window?
SELECT ?location WHERE {
  DURING("2024-03-14T09:00:00"^^xsd:dateTime,
         "2024-03-14T11:00:00"^^xsd:dateTime) {
    :defendant :locatedIn ?location .
  }
}
```

### Film continuity check

```sparql
# Is the character wearing the same outfit in scene 7 as in scene 3?
SELECT ?outfit WHERE {
  AT_TIME(3) {
    :protagonist :wearing ?outfit .
  }
}

# Compare with:
SELECT ?outfit WHERE {
  AT_TIME(7) {
    :protagonist :wearing ?outfit .
  }
}

# Or use TEMPORAL_DIFF to spot the discrepancy directly:
SELECT ?change_type ?character ?property ?value WHERE {
  TEMPORAL_DIFF(3, 7) {
    ?character :wearing ?value .
  }
}
```

### GraphRAG provenance: what did we know and when?

```sparql
# What did our knowledge graph look like after the first extraction pass?
SELECT ?s ?p ?o WHERE {
  AT_TIME("2024-06-01T12:00:00"^^xsd:dateTime) {
    ?s ?p ?o .
  }
}

# What was added by the second extraction pass?
SELECT ?s ?p ?o WHERE {
  TEMPORAL_DIFF("2024-06-01T12:00:00"^^xsd:dateTime,
                "2024-06-02T12:00:00"^^xsd:dateTime) {
    ?s ?p ?o .
  }
  FILTER(?change_type = "added")
}
```

### Historical entity biography

```sparql
# All positions Napoleon held, with their time intervals
SELECT ?position ?start ?end WHERE {
  << :napoleon :heldPosition ?position >> loka:validFrom ?start .
  OPTIONAL {
    << :napoleon :heldPosition ?position >> loka:validTo ?end .
  }
}
ORDER BY ?start
```

### Co-presence query

```sparql
# Which people were at the same location during an interval?
SELECT ?person1 ?person2 ?location WHERE {
  DURING("2024-03-14T09:00:00"^^xsd:dateTime,
         "2024-03-14T11:00:00"^^xsd:dateTime) {
    ?person1 :locatedIn ?location .
    ?person2 :locatedIn ?location .
    FILTER(?person1 != ?person2)
  }
}
```

### Enterprise state machine: loan history

```sparql
# What was the state of loan L-1234 at the time of the audit?
SELECT ?property ?value WHERE {
  AT_TIME("2024-09-15"^^xsd:dateTime) {
    :loan_1234 ?property ?value .
  }
}

# What changed on the loan between quarterly reviews?
SELECT ?change_type ?property ?value WHERE {
  TEMPORAL_DIFF("2024-06-30"^^xsd:dateTime,
                "2024-09-30"^^xsd:dateTime) {
    :loan_1234 ?property ?value .
  }
}
```

---

## 9. Combining Temporal + Vector + Graph

The real power of Loka's temporal operators is that they compose freely with vector search and graph traversal in a single query.

### Temporal vector search

```sparql
# Find documents similar to a query that mention people who existed in 2024
SELECT ?doc ?person WHERE {
  AT_TIME("2024-06-01"^^xsd:dateTime) {
    ?person rdf:type :Person .
    ?doc :mentions ?person .
  }
  VECTOR_SIMILAR(?doc :hasEmbedding "..."^^loka:f32vec, 0.85)
}
ORDER BY DESC(VECTOR_SCORE(?doc :hasEmbedding "..."^^loka:f32vec))
```

### Temporal graph traversal + vector

```sparql
# At a specific time, find semantically similar shrines and traverse to their deities
SELECT ?shrine ?deity ?myth WHERE {
  AT_TIME("1200"^^loka:temporal) {
    ?shrine :enshrines ?deity .
    ?deity :appearsIn ?myth .
  }
  VECTOR_SIMILAR(?shrine :descriptionEmbedding "..."^^loka:f32vec, 0.75)
}
```

### Diff with vector scoring

```sparql
# What knowledge was added in the last extraction pass, ranked by relevance?
SELECT ?s ?p ?o ?score WHERE {
  TEMPORAL_DIFF("2024-06-01"^^xsd:dateTime,
                "2024-06-02"^^xsd:dateTime) {
    ?s ?p ?o .
  }
  FILTER(?change_type = "added")
  VECTOR_SIMILAR(?s :embedding "..."^^loka:f32vec, 0.60)
  BIND(VECTOR_SCORE(?s :embedding "..."^^loka:f32vec) AS ?score)
}
ORDER BY DESC(?score)
```

---

## 10. Performance Notes

### Execution model

All temporal operators use a **post-filter** strategy: inner patterns evaluate against SPO/POS/OSP indexes first, then temporal containment prunes the results. This composes with all pattern types (OPTIONAL, UNION, VECTOR_SIMILAR, property paths) without special-casing.

### Index usage

- **TSPO index** is built automatically when temporal predicates (`loka:assertedAt`, `loka:validFrom`, `loka:validTo`) are present.
- Databases without temporal data pay zero storage or query cost.
- The TSPO index is a B-tree with time as the leading key — range scans are O(log n).

### Tips

- **Bind subjects first.** `AT_TIME` on a query with bound subjects is fast (temporal check on a small result set). `WORLD_STATE` on `?s ?p ?o` is slower (checks every triple).
- **Use closed intervals when possible.** Triples with both `validFrom` and `validTo` produce `Definite` containment — no ambiguity.
- **Property paths inside temporal blocks** check each edge during traversal. Deep traversals on large temporal graphs can be slow — bind as many variables as possible before the path expression.
