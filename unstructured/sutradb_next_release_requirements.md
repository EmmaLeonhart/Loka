# Loka Next Release Requirements — From ManuForge Integration Testing

> **Based on:** ManuForge pipeline integration testing against Loka v0.3.3, v0.3.5, v0.3.6
> **Written:** 2026-04-03
> **Purpose:** Actionable fix list for the next Loka release, prioritized by ManuForge pipeline impact
> **See also:** [Full limitations report](sutradb_limitations.md) for reproduction steps and workarounds

---

## P0 — Blockers (pipeline features that don't work without these)

### 1. Star triple wildcard query support

**What:** SPARQL queries with variables inside `<< >>` return 0 rows. Only fully-bound inner triples work.

```sparql
-- ✅ Works (all 3 inner positions specified)
SELECT ?page WHERE {
  << :doctor-steve rdfs:label "Doctor Steve" >> :page_source ?page
}

-- ❌ Returns 0 rows (variable in predicate position)
SELECT ?prop ?page WHERE {
  << :doctor-steve ?prop ?val >> :page_source ?page
}

-- ❌ Returns 0 rows (fully unbound)
SELECT ?s ?p ?o ?mp ?mo WHERE {
  << ?s ?p ?o >> ?mp ?mo
}
```

**Why it matters:** ManuForge stores page citations as star triple annotations. Without wildcard support, we can't query "show all facts about character X with their page sources" — we have to enumerate every known predicate/value pair manually. This also blocks temporal operators (see P1 #4).

**Suggested approach:** The star triple lookup uses a content-addressed hash of the inner s/p/o. When any position is a variable, the hash can't be computed. Needs a secondary index: inner-subject → star-triple-IDs (and/or inner-predicate, inner-object indexes) so the query planner can resolve partial patterns.

**Minimum viable fix:** Support at least one bound position in the inner triple (e.g., `<< :subject ?p ?o >>` where subject is bound). Fully unbound `<< ?s ?p ?o >>` is nice-to-have.

---

### 2. HTTP bulk import must support star triples

**What:** `POST /triples` with `Content-Type: application/n-triples` silently drops star triples. Reports them in the `inserted` count but doesn't store them.

```bash
# Import via HTTP — star triples silently lost
curl -X POST http://localhost:3030/triples \
  --data-binary @graph.nt \
  -H "Content-Type: application/n-triples"
# Reports: {"inserted": 723} but star triples are missing from queries
```

**Why it matters:** Server mode is the production deployment path. The CLI `loka import` works but requires filesystem access to the `.sdb` directory. Any deployment behind an API (Docker, cloud, multi-user) needs HTTP import to work with star triples.

**Root cause:** The HTTP import handler uses a different N-Triples parser path than the CLI. The CLI parser was updated in v0.3.6 for `<< >>` syntax; the HTTP handler was not.

---

## P1 — Important (causes pain but has workarounds)

### 3. CRLF line endings cause import failures

**What:** The N-Triples parser rejects lines with `\r\n` endings. On Windows, most tools write CRLF by default.

- v0.3.5: partial import (569-748/751 lines)
- v0.3.6: near-total rejection (0-48/751 lines)

**Current workaround:** ManuForge forces LF via `newline="\n"` in Python's `write_text()`.

**Suggested fix:** Strip `\r` during N-Triples line parsing. One-line fix in the parser. Every Windows user will hit this otherwise.

---

### 4. Temporal operators untestable until #1 is fixed

**What:** `AT_TIME`, `DURING`, `WORLD_STATE`, `TEMPORAL_DIFF` operators are documented and implemented, but they rely on star triple annotations (`loka:assertedAt`). Since wildcard star queries don't work (#1), temporal operators can't scan annotations across triples.

**ManuForge use case:** Pages as narrative time axis (integer type). We want:
```sparql
-- "What did we know about Daniel Carver by page 3?"
SELECT ?prop ?value WHERE {
  WORLD_STATE(3) {
    :daniel-carver ?prop ?value .
  }
}
```

**Unblocked by:** Fix #1 (star triple wildcard queries). Once that lands, ManuForge can immediately test temporal operators.

---

## P2 — Nice to have

### 5. TLS certificate for sutradb.org

The site returns `ERR_TLS_CERT_ALTNAME_INVALID`. Certificate doesn't include `sutradb.org` in Subject Alternative Names. Documentation is only accessible via the GitHub repo.

**Fix:** Reissue cert with `sutradb.org` and `www.sutradb.org` in the SAN list.

---

## Already Fixed (v0.3.6) — No Action Needed

| Issue | Status |
|-------|--------|
| RDF-star triples not stored | ✅ Fixed — stored and queryable (fully bound) |
| Self-update downloads wrong archive | ✅ Fixed — excludes "studio" assets |
| No verbose import error output | ✅ Fixed — `--show-errors` flag added |
| Python SDK not on PyPI | ✅ Fixed — `pip install sutradb` works |

---

## What Works Well (v0.3.6)

These features are solid and ManuForge depends on them:

- Regular triple import (751/751 via CLI, 0 errors)
- RDF-star import + query via CLI (fully bound inner triple)
- RDF-star via `INSERT DATA` SPARQL
- SPARQL 1.1: SELECT, FILTER, ORDER BY, COUNT, OPTIONAL
- Full-text search in literals
- CLI: `loka serve`, `loka query`, `loka import`, `loka info`
- Serverless mode (direct `.sdb` access)
- Memory-only mode (`--memory-only`)
- Performance (instant queries at ~750 triples)
- Python SDK (HTTP client + OWL + LangChain + Jupyter)
- Import error reporting (`--show-errors`)
