# Enrollment v1 — Non-RDF Source Ingestion

**Status:** Design captured. Implementation deferred until after world-model training corpus work.
**Date:** 2026-05-07
**Origin:** Pramana's `adapters/`, with the data-model regression fixed.
**Related:** `planning/symbolic-layer-and-naming.md`, `planning/world-model-thesis.md`.

---

## 1. What enrollment is

Non-destructive ingestion of arbitrary source artifacts (CSV, SQL, JSON, PDF, HTML, Mongo, Excel, DOCX) into SutraDB as RDF triples, **with full provenance back to the original byte / cell / row / DOM-node**.

Enrollment is read-only over the source. It produces:
- A `sutra:Artifact` IRI with `sha256`, original path, mtime.
- One or more entity IRIs per row/document.
- Triples for each cell/field, where each triple is reified via RDF-star and annotated with a `sutra:locator` back to the source.

Enrollment does NOT interpret the content. Semantic extraction (LLM-driven entity recognition, relation extraction) is a separate downstream step, e.g. `sutra extract`, operating over already-enrolled spans.

---

## 2. Why deferred

The world-model training corpus is initially Wikidata + DBpedia + other public RDF dumps. These are already RDF. Enrollment is for **non-RDF sources** — a later phase, after the model is producing useful inferences and we want to ingest local CSVs, PDFs, SQL dumps to expand the corpus.

This document captures the design while context is fresh. **Implementation is deferred substantially.** The user's own framing: *"enrollment might be a very useful thing at some point but it's probably to be deferred a lot."* Implementation should not start until:

- The first iteration of the world-model layer is producing cited inferences.
- There is a concrete need for non-RDF sources in the training pipeline or in user-facing workflows.

### 2.1 One-way only — no two-way sync

A hard architectural rule, settled in advance:

> *"One-way imports into a database that are used for training are different and significantly better than trying to do any kind of two-way sync."*

Enrollment is **read-only over the source artifact, write-only into SutraDB**. The shadow copy and provenance triples represent a snapshot of the source at enrollment time. They do not mirror the source forward, and they do not propagate edits back.

Concretely:

- If the user updates the original CSV/SQL/PDF, the enrolled triples in SutraDB do not change. The user re-runs `sutra enroll --re-enroll` to capture a new snapshot, which is linked to the prior one via `sutra:supersededBy`.
- If a downstream process (the world-model layer, an SDK, a human curator) edits enrolled triples in SutraDB, those edits do **not** propagate back to the source artifact. The source is canonical for *its* state at enrollment time; SutraDB is canonical for everything after.

Two-way sync would require change-detection on both sides, conflict resolution, and a coupling between SutraDB and the source's storage system. All three are explicitly out of scope. Enrollment is import, not integration.

---

## 3. Pramana lessons (what NOT to do)

The Pramana enrollment system shipped two parallel adapter generations that disagreed on the data model. Specific lessons:

1. **Don't ship two parallel adapter generations.** Pick a data model, freeze it, build all adapters against it. Any second-generation rewrite must replace the first, not coexist with it.
2. **Don't drop the snapshot-with-hash discipline.** Every enrolled artifact has `sha256`, `mtime`, `enrolled_at`, `adapter_version`. Without these, lineage is irrecoverable when the source changes.
3. **Don't conflate enrollment with interpretation.** Enrollment answers "what is at byte X of file F." Interpretation answers "what does that mean." Keep them in separate tools — `sutra enroll` and `sutra extract`.
4. **Don't mutate source schemas under the guise of non-destructive copy.** Pramana's SQL adapter `ALTER TABLE`d the copy; the Excel adapter inserted columns mid-sheet, breaking external formula references. The shadow copy must be byte-identical to the source. Provenance lives in SutraDB triples, not in the source artifact.
5. **Don't trust LLM-supplied byte offsets.** Pramana's text adapter asked the LLM for `start`/`end` character positions; they drifted. Use deterministic span detection — exact-string matches with collision detection, paragraph-boundary heuristics, parser-driven offsets.
6. **Don't reinvent provenance per format.** Pramana ended up with five different ways to encode "where did this claim come from." A single canonical `Locator` schema avoids this.

---

## 4. Architecture

```
┌─────────────────────┐    ┌─────────────────────────┐    ┌────────────┐
│  Source artifact    │    │  sutra-enroll-py        │    │  sutra-    │
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
                                                          │  SutraDB   │
                                                          │  (.sdb)    │
                                                          └────────────┘
```

**`sutra-enroll-core` (Rust crate):**
- Lineage data model: `Artifact { uri, sha256, size, mtime, adapter, adapter_version, enrolled_at }`.
- Canonical `Locator` enum: `SqlCell { table, rowid, column }`, `CsvCell { row, col }`, `JsonPath { path }`, `TextSpan { byte_start, byte_end }`, `ExcelCell { sheet, addr }`, `PdfBlock { page, bbox }`, `HtmlNode { xpath, byte_start, byte_end }`. Each variant serializes to a stable string for RDF emission.
- Triple emitter: given `(artifact, locator, value)`, emits the canonical RDF-star triples (see §5).
- Deterministic property URI minting via UUIDv5 from `(adapter, artifact_stem, column)`.
- Streaming writer to `.sdb` so 65k-claim enrollments don't blow memory.

**`sutra-enroll-py` (Python package):**
- Format-specific readers: `csv`, `sqlite`, `mongo`, `xlsx`, `pdf`, `html`, `docx`.
- Each reader yields `(locator, value)` pairs to the Rust core via PyO3 or stdin-streamed N-Triples.
- Python because the libraries (openpyxl, pdfplumber, BeautifulSoup, PyPDF2, python-docx, pymongo) live there. Reimplementing PDF text extraction in Rust is a year of yak-shaving for no gain.

**Hard rule:** Rust owns the data model. Python owns format I/O. The contract between them is `(Locator, Value)` pairs.

---

## 5. v1 surface — CSV only

CSV is the simplest format with zero library dependencies and exercises every load-bearing concept (artifact identity, row identity, cell identity, property identity, byte-range provenance, deterministic re-enrollment).

### 5.1 Predicates

All in the `sutra:` namespace:

| Predicate | Subject | Object | Purpose |
|---|---|---|---|
| `sutra:Artifact` (class) | — | — | Source-file type marker |
| `sutra:artifactSha256` | artifact | xsd:string | Content hash |
| `sutra:artifactPath` | artifact | xsd:string | Original path/URI |
| `sutra:artifactMtime` | artifact | xsd:dateTime | mtime at enrollment |
| `sutra:enrolledAt` | artifact | xsd:dateTime | When enrolled |
| `sutra:enrolledBy` | artifact | xsd:string | Adapter name + version |
| `sutra:rowOf` | row | artifact | Row → artifact link |
| `sutra:rowIndex` | row | xsd:int | 1-based row number |
| `sutra:fromArtifact` | quoted-triple | artifact | Provenance back-ref |
| `sutra:locator` | quoted-triple | xsd:string | Canonical locator string |
| `sutra:cellByteStart`/`sutra:cellByteEnd` | quoted-triple | xsd:int | Optional precise span |
| `sutra:supersededBy` | artifact | artifact | Re-enrollment chain |

### 5.2 Triple shape per CSV row

For `data.csv` row 5 with `name=Alice`, `email=alice@x.com`:

```turtle
# Artifact (emitted once per file)
<file:///abs/path/data.csv> a sutra:Artifact ;
    sutra:artifactSha256 "abc123..." ;
    sutra:artifactPath "data.csv" ;
    sutra:artifactMtime "2026-05-07T10:00:00Z"^^xsd:dateTime ;
    sutra:enrolledAt "2026-05-07T10:05:00Z"^^xsd:dateTime ;
    sutra:enrolledBy "sutra-enroll-csv/0.1.0" .

# Row entity (UUIDv4)
<urn:sutra:row:7f3a-...> sutra:rowOf <file:///abs/path/data.csv> ;
                         sutra:rowIndex 5 .

# Cells with RDF-star provenance
<urn:sutra:row:7f3a-...> <urn:sutra:prop:csv/data/name> "Alice" .
<< <urn:sutra:row:7f3a-...> <urn:sutra:prop:csv/data/name> "Alice" >>
    sutra:fromArtifact <file:///abs/path/data.csv> ;
    sutra:locator "csv:row=5;col=name" ;
    sutra:cellByteStart 142 ;
    sutra:cellByteEnd 147 .
```

Property IRI `<urn:sutra:prop:csv/data/name>` = `uuid5(SUTRA_PROP_NAMESPACE, "csv|data|name")`. Same column re-enrolled in a new file produces the same property IRI — cross-source schema reconciliation falls out for free.

### 5.3 CLI surface

```
sutra enroll <path>                            # auto-detect format
sutra enroll csv data.csv                      # explicit format
sutra enroll csv data.csv --db ./mydata.sdb    # target DB
sutra enroll csv data.csv --dry-run -o out.nt  # emit N-Triples, don't ingest
sutra enroll csv data.csv --re-enroll          # detect prior artifact by sha, replace
```

`--dry-run` is what training-corpus auditors use: read-only, emits triples to stdout/file. The auditor verifies provenance round-trips before committing to the database.

### 5.4 Idempotency

Before emitting, query the DB for an artifact with matching `sutra:artifactSha256`. If found and `--re-enroll` not passed, skip with a warning. If `--re-enroll`, soft-deprecate prior row IRIs (set `sutra:supersededBy <new-artifact>`, do not delete) and emit a fresh enrollment with a new `enrolledAt`. Append-only history.

---

## 6. Deferred to v2+

- **Other formats** — XLSX, PDF, HTML, Mongo, SQL, DOCX. Each adds a `Locator` variant; reuses the same emitter.
- **LLM-based span extraction** — separate `sutra extract` command operating on already-enrolled text. Produces additional cited claims pointing back to enrolled spans. This is where Pramana's text adapter blurred enrollment with interpretation; SutraDB keeps them separate.
- **Schema-variation telemetry** — Mongo's `schema_consistency` percentage, useful but not load-bearing.
- **Forward-compatibility under source schema evolution** — handled via append-only + the `supersededBy` chain, but the migration UX needs design.

---

## 7. References

- Pramana origin (lessons doc, not code to port): `C:\Users\Immanuelle\Documents\Github\Pramana\adapters\`
- `planning/symbolic-layer-and-naming.md` §2 — provenance edges and source-exempt classes
- `planning/world-model-thesis.md` §3 — training corpus pipeline, where enrollment plugs in
