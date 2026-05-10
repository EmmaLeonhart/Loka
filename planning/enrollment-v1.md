# Enrollment v1 — Non-RDF Source Ingestion

**Status:** Design captured. Implementation deferred until after world-model training corpus work.
**Date:** 2026-05-07
**Origin:** Pramana's `adapters/`, with the data-model regression fixed.
**Related:** `planning/symbolic-layer-and-naming.md`, `planning/world-model-thesis.md`.

---

## 1. What enrollment is

Non-destructive ingestion of arbitrary source artifacts (CSV, SQL, JSON, PDF, HTML, Mongo, Excel, DOCX) into Loka as RDF triples, **with full provenance back to the original byte / cell / row / DOM-node**.

Enrollment is read-only over the source. It produces:
- A `loka:Artifact` IRI with `sha256`, original path, mtime.
- One or more entity IRIs per row/document.
- Triples for each cell/field, where each triple is reified via RDF-star and annotated with a `loka:locator` back to the source.

Enrollment does NOT interpret the content. Semantic extraction (LLM-driven entity recognition, relation extraction) is a separate downstream step, e.g. `loka extract`, operating over already-enrolled spans.

---

## 2. Why deferred — and what enrollment actually is

**Enrollment is two-way sync between live source artifacts and Loka.** This is the whole reason it is hard, and the whole reason it is deferred about a year out. The user's framing:

> *"Enrollment is something that is two-way but we're going to wait like a fucking year for it after we've trained the models."*

Two-way means: when the source (CSV / SQL / PDF / etc.) changes, the enrolled triples update; when the enrolled triples are edited (by curators, by the world model writing back inferences, by SDK clients), those edits propagate back to a representation of the source. Either direction alone is straightforward; together they are a real distributed-systems problem with conflict resolution, change detection, and coupling between Loka and the source's storage.

Deferral timeline: **about a year**, after the world-model layer is trained and producing useful inferences. Pramana attempted enrollment early without that discipline (and without actually solving the sync problem) and the result was the "big mess" the user diagnosed.

### 2.1 What enrollment is NOT

To avoid confusion: bulk one-way import of an RDF dump (Wikidata, DBpedia, OpenAlex, MusicBrainz) is **not enrollment**. That is straight RDF ingestion — read triples out of a `.ttl` / `.nt` / `.hdt` file, write them into the `.sdb`, done. No two-way sync, no change detection, no source-tracking beyond a `loka:fromDataset` provenance edge. RDF ingestion is what the world-model layer needs in the near term; it is covered in `planning/world-model-thesis.md` §3 and is mostly already in flight via `tools/wikidata_bfs_import.py`.

Enrollment specifically refers to two-way sync of **non-RDF** source artifacts where the source remains live and authoritative for its own state, and Loka needs to stay in agreement with it.

### 2.2 What's captured below

The rest of this document is a sketch design — predicates, locator vocabulary, CLI shape — that anchors what enrollment will look like when we eventually build it. The sketch covers only the import direction (source → Loka); the back-propagation direction (Loka → source) is the harder half and is intentionally not specified yet. **Treat everything below §3 as reference material for a year-from-now planning revisit, not as a v1 implementation target.**

---

## 3. Pramana lessons (what NOT to do)

The Pramana enrollment system shipped two parallel adapter generations that disagreed on the data model. Specific lessons:

1. **Don't ship two parallel adapter generations.** Pick a data model, freeze it, build all adapters against it. Any second-generation rewrite must replace the first, not coexist with it.
2. **Don't drop the snapshot-with-hash discipline.** Every enrolled artifact has `sha256`, `mtime`, `enrolled_at`, `adapter_version`. Without these, lineage is irrecoverable when the source changes.
3. **Don't conflate enrollment with interpretation.** Enrollment answers "what is at byte X of file F." Interpretation answers "what does that mean." Keep them in separate tools — `loka enroll` and `loka extract`.
4. **Don't mutate source schemas under the guise of non-destructive copy.** Pramana's SQL adapter `ALTER TABLE`d the copy; the Excel adapter inserted columns mid-sheet, breaking external formula references. The shadow copy must be byte-identical to the source. Provenance lives in Loka triples, not in the source artifact.
5. **Don't trust LLM-supplied byte offsets.** Pramana's text adapter asked the LLM for `start`/`end` character positions; they drifted. Use deterministic span detection — exact-string matches with collision detection, paragraph-boundary heuristics, parser-driven offsets.
6. **Don't reinvent provenance per format.** Pramana ended up with five different ways to encode "where did this claim come from." A single canonical `Locator` schema avoids this.

---

## 4. Architecture

```
┌─────────────────────┐    ┌─────────────────────────┐    ┌────────────┐
│  Source artifact    │    │  loka-enroll-py        │    │  loka-    │
│  (CSV/SQL/PDF/...)  │───►│  format reader          │───►│  enroll-   │
│                     │    │  (yields locator+value) │    │  core      │
└─────────────────────┘    └─────────────────────────┘    │  (Rust)    │
                                                          │            │
                                                          │  emits     │
                                                          │  N-Triples │
                                                          │  + RDF-star│
                                                          └─────┬──────┘
                                                                │
                                                                ▼
                                                          ┌────────────┐
                                                          │  Loka   │
                                                          │  (.sdb)    │
                                                          └────────────┘
```

**`loka-enroll-core` (Rust crate):**
- Lineage data model: `Artifact { uri, sha256, size, mtime, adapter, adapter_version, enrolled_at }`.
- Canonical `Locator` enum: `SqlCell { table, rowid, column }`, `CsvCell { row, col }`, `JsonPath { path }`, `TextSpan { byte_start, byte_end }`, `ExcelCell { sheet, addr }`, `PdfBlock { page, bbox }`, `HtmlNode { xpath, byte_start, byte_end }`. Each variant serializes to a stable string for RDF emission.
- Triple emitter: given `(artifact, locator, value)`, emits the canonical RDF-star triples (see §5).
- Deterministic property URI minting via UUIDv5 from `(adapter, artifact_stem, column)`.
- Streaming writer to `.sdb` so 65k-claim enrollments don't blow memory.

**`loka-enroll-py` (Python package):**
- Format-specific readers: `csv`, `sqlite`, `mongo`, `xlsx`, `pdf`, `html`, `docx`.
- Each reader yields `(locator, value)` pairs to the Rust core via PyO3 or stdin-streamed N-Triples.
- Python because the libraries (openpyxl, pdfplumber, BeautifulSoup, PyPDF2, python-docx, pymongo) live there. Reimplementing PDF text extraction in Rust is a year of yak-shaving for no gain.

**Hard rule:** Rust owns the data model. Python owns format I/O. The contract between them is `(Locator, Value)` pairs.

---

## 5. v1 surface — CSV only

CSV is the simplest format with zero library dependencies and exercises every load-bearing concept (artifact identity, row identity, cell identity, property identity, byte-range provenance, deterministic re-enrollment).

### 5.1 Predicates

All in the `loka:` namespace:

| Predicate | Subject | Object | Purpose |
|---|---|---|---|
| `loka:Artifact` (class) | — | — | Source-file type marker |
| `loka:artifactSha256` | artifact | xsd:string | Content hash |
| `loka:artifactPath` | artifact | xsd:string | Original path/URI |
| `loka:artifactMtime` | artifact | xsd:dateTime | mtime at enrollment |
| `loka:enrolledAt` | artifact | xsd:dateTime | When enrolled |
| `loka:enrolledBy` | artifact | xsd:string | Adapter name + version |
| `loka:rowOf` | row | artifact | Row → artifact link |
| `loka:rowIndex` | row | xsd:int | 1-based row number |
| `loka:fromArtifact` | quoted-triple | artifact | Provenance back-ref |
| `loka:locator` | quoted-triple | xsd:string | Canonical locator string |
| `loka:cellByteStart`/`loka:cellByteEnd` | quoted-triple | xsd:int | Optional precise span |
| `loka:supersededBy` | artifact | artifact | Re-enrollment chain |

### 5.2 Triple shape per CSV row

For `data.csv` row 5 with `name=Alice`, `email=alice@x.com`:

```turtle
# Artifact (emitted once per file)
<file:///abs/path/data.csv> a loka:Artifact ;
    loka:artifactSha256 "abc123..." ;
    loka:artifactPath "data.csv" ;
    loka:artifactMtime "2026-05-07T10:00:00Z"^^xsd:dateTime ;
    loka:enrolledAt "2026-05-07T10:05:00Z"^^xsd:dateTime ;
    loka:enrolledBy "loka-enroll-csv/0.1.0" .

# Row entity (UUIDv4)
<urn:loka:row:7f3a-...> loka:rowOf <file:///abs/path/data.csv> ;
                         loka:rowIndex 5 .

# Cells with RDF-star provenance
<urn:loka:row:7f3a-...> <urn:loka:prop:csv/data/name> "Alice" .
<< <urn:loka:row:7f3a-...> <urn:loka:prop:csv/data/name> "Alice" >>
    loka:fromArtifact <file:///abs/path/data.csv> ;
    loka:locator "csv:row=5;col=name" ;
    loka:cellByteStart 142 ;
    loka:cellByteEnd 147 .
```

Property IRI `<urn:loka:prop:csv/data/name>` = `uuid5(LOKA_PROP_NAMESPACE, "csv|data|name")`. Same column re-enrolled in a new file produces the same property IRI — cross-source schema reconciliation falls out for free.

### 5.3 CLI surface

```
loka enroll <path>                            # auto-detect format
loka enroll csv data.csv                      # explicit format
loka enroll csv data.csv --db ./mydata.sdb    # target DB
loka enroll csv data.csv --dry-run -o out.nt  # emit N-Triples, don't ingest
loka enroll csv data.csv --re-enroll          # detect prior artifact by sha, replace
```

`--dry-run` is what training-corpus auditors use: read-only, emits triples to stdout/file. The auditor verifies provenance round-trips before committing to the database.

### 5.4 Idempotency

Before emitting, query the DB for an artifact with matching `loka:artifactSha256`. If found and `--re-enroll` not passed, skip with a warning. If `--re-enroll`, soft-deprecate prior row IRIs (set `loka:supersededBy <new-artifact>`, do not delete) and emit a fresh enrollment with a new `enrolledAt`. Append-only history.

---

## 6. Deferred to v2+

- **Other formats** — XLSX, PDF, HTML, Mongo, SQL, DOCX. Each adds a `Locator` variant; reuses the same emitter.
- **LLM-based span extraction** — separate `loka extract` command operating on already-enrolled text. Produces additional cited claims pointing back to enrolled spans. This is where Pramana's text adapter blurred enrollment with interpretation; Loka keeps them separate.
- **Schema-variation telemetry** — Mongo's `schema_consistency` percentage, useful but not load-bearing.
- **Forward-compatibility under source schema evolution** — handled via append-only + the `supersededBy` chain, but the migration UX needs design.

---

## 7. References

- Pramana origin (lessons doc, not code to port): `C:\Users\Immanuelle\Documents\Github\Pramana\adapters\`
- `planning/symbolic-layer-and-naming.md` §2 — provenance edges and source-exempt classes
- `planning/world-model-thesis.md` §3 — training corpus pipeline, where enrollment plugs in
