# Loka Limitations & Issues — Found During ManuForge Integration

> **Loka version tested:** v0.3.5 (also v0.3.3)
> **Test date:** 2026-03-31
> **Tested on:** Windows 11, `loka serve --memory-only`
> **Test data:** `output/excelsior/graph.nt` — 751 N-Triples-star lines from manuscript extraction

---

## 1. RDF-Star Triples Import But Are Not Queryable

**Severity: Critical — blocks page citation and temporal annotation features**

RDF-star triples in N-Triples format import without errors:

```
<< <http://example.org/s> <http://example.org/p> "hello" >> <http://example.org/meta> "world" .
```

`loka import` reports these as successfully imported (748/751 lines, only 3 errors on the full Excelsior graph). However, **no SPARQL query syntax can retrieve them**:

```sparql
-- Returns 0 rows despite data being imported
SELECT ?mp ?mo WHERE { << ?s ?p ?o >> ?mp ?mo }

-- Also 0 rows for specific triples known to exist
SELECT ?page WHERE {
  << <http://storyforge.dev/ns/character/doctor-steve>
     <http://www.w3.org/2000/01/rdf-schema#label>
     "Doctor Steve" >>
  <http://storyforge.dev/ns/meta/page_source> ?page
}

-- Even INSERT DATA followed by immediate SELECT returns 0 rows
INSERT DATA {
  << <http://example.org/s> <http://example.org/p> "val" >>
  <http://example.org/meta> "annotation" .
}
-- then:
SELECT ?mo WHERE { << <http://example.org/s> <http://example.org/p> "val" >> ?mp ?mo }
-- 0 rows
```

**What was tested:**
- Import via `loka import file.nt` — reports success
- Import via `loka import` after base triple already exists — reports success
- Query via `<< ?s ?p ?o >> ?mp ?mo` pattern — 0 rows
- Query via specific subject/predicate/object in `<< >>` — 0 rows
- Query treating the annotation predicate as a regular triple (`?s <meta:page_source> ?o`) — 0 rows
- INSERT DATA with star syntax then immediate SELECT — 0 rows
- Tested on both v0.3.3 and v0.3.5 — same behavior

**Impact on ManuForge:**
Our entire page citation system depends on RDF-star annotations:
```
<< :DanielCarver :hairColor "brown" >> :page_source 3 .
<< :DanielCarver :hairColor "brown" >> loka:assertedAt 3 .
```
Until star triples are queryable, we cannot:
- Query which page a fact was extracted from
- Use `AT_TIME` / `WORLD_STATE` temporal operators on page-as-time annotations
- Do cross-scene consistency validation ("does this character's description change between pages?")

**Suggested fix:** The N-Triples parser accepts `<< >>` syntax and the import counter increments, so the parser recognizes them. The issue is likely in the query engine's triple pattern matcher — it doesn't match against the stored star annotations. The storage layer may need to expose star triples to the SPARQL evaluator.

---

## 2. Self-Update Downloads Wrong Archive

**Severity: Low**

`loka update` on v0.3.3 detects v0.3.5 correctly but downloads the wrong zip:

```
$ loka update
Loka v0.3.3
Checking for updates...
New version available: v0.3.3 → v0.3.5
Downloading https://github.com/.../loka-studio-windows-x64.zip...
Error: Binary 'sutra.exe' not found in archive
```

It downloads `loka-studio-windows-x64.zip` instead of `sutra-windows-x64.zip`. The studio zip contains the GUI app, not the CLI binary.

**Workaround:** Manually download the correct zip:
```
curl -sL https://github.com/EmmaLeonhart/Loka/releases/download/v0.3.5/sutra-windows-x64.zip -o sutra.zip
unzip sutra.zip
cp sutra.exe ~/.cargo/bin/sutra.exe
```

**Suggested fix:** The update logic likely iterates release assets and picks the first matching `*windows*x64*` pattern. It should prefer the asset without "studio" in the name, or match `sutra-windows-x64.zip` exactly.

---

## 3. 3 Import Errors on Valid-Looking N-Triples

**Severity: Low — 748/751 lines imported successfully**

3 lines out of 751 fail to import with no error detail. The failing lines are not identified in the output — `loka import` only reports the count.

**Suggested improvement:** Add a `--verbose` or `--show-errors` flag that prints the failing lines and the parse error reason. Currently debugging import failures requires bisecting the file manually.

---

## 4. sutradb.org TLS Certificate Error

**Severity: Medium — blocks documentation access**

The documentation website at sutradb.org returns `ERR_TLS_CERT_ALTNAME_INVALID`. The site's TLS certificate doesn't include `sutradb.org` in its Subject Alternative Names.

**Workaround:** Use documentation from the GitHub repo's `docs/` folder directly.

**Suggested fix:** Reissue the TLS certificate with `sutradb.org` and `www.sutradb.org` in the SAN list.

---

## 5. Python SDK Not on PyPI

**Severity: Medium — blocks Python integration**

The README documents `pip install sutradb` but the package does not exist on PyPI as of 2026-03-31.

**Workaround:** Use the HTTP API directly:
```python
import requests

# Insert triples
requests.post("http://localhost:3030/triples",
    data=open("graph.nt").read(),
    headers={"Content-Type": "application/n-triples"})

# Query
resp = requests.post("http://localhost:3030/sparql",
    data={"query": "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10"})
```

---

## 6. Temporal Query Operators Untested

**Severity: Unknown — documented but could not verify**

The docs describe `AT_TIME(T)`, `DURING(T1, T2)`, `WORLD_STATE(T)`, and `TEMPORAL_DIFF(T1, T2)` operators, along with `loka:assertedAt`, `loka:validFrom`, `loka:validTo` predicates. These could not be tested because they depend on RDF-star annotations working (Issue #1).

Once Issue #1 is fixed, these need to be validated with our page-as-narrative-time data model:
```sparql
-- "What did we know about Daniel Carver by page 3?"
SELECT ?prop ?value WHERE {
  WORLD_STATE(3) {
    <http://storyforge.dev/ns/character/daniel-carver> ?prop ?value
  }
}
```

---

## What Works Well

For reference, these features work correctly:

- **Regular triple import** — N-Triples files load reliably (748/751)
- **SPARQL 1.1 queries** — SELECT, FILTER, ORDER BY, COUNT, OPTIONAL all work
- **Full-text search in literals** — string matching in FILTER works
- **CLI ergonomics** — `loka serve`, `loka query`, `loka import`, `loka info` all work as documented
- **Memory-only mode** — `--memory-only` flag works for ephemeral testing
- **Performance** — queries over ~750 triples return instantly

---

## Priority Fix Order for ManuForge Integration

1. **RDF-star query support** (Critical — everything else depends on this)
2. **Verbose import errors** (Nice to have — helps debugging)
3. **Self-update asset selection** (Nice to have)
4. **TLS cert for sutradb.org** (Medium — documentation access)
5. **Publish Python SDK to PyPI** (Medium — enables direct Python integration)
6. **Validate temporal operators** (After #1 is fixed)
