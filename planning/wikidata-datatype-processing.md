# Wikidata datatype → training-corpus processing

The complete decision table for what `training/preprocess.py` does with every
Wikidata property datatype. There are 18 datatypes total (queried fresh from
`wikibase:propertyType`). Each row says what happens to a triple
`<S, P, O>` when `P` has that datatype.

| Wikidata datatype | # of properties | Decision | Object normalisation |
|---|---|---|---|
| `WikibaseItem` | 1,737 | **KEEP** | Substitute `<entityIRI>` with English label from `rdfs:label`. If no label, drop the row. |
| `Quantity` | 685 | **KEEP** | Strip leading `+` from positive numbers. `+1234` → `1234`. `-99.5` stays. Unit/precision metadata already stripped by `wikidata_random_seed.py` at extraction. |
| `String` | 354 | **KEEP** | Verbatim. No normalisation. |
| `Time` | 70 | **KEEP** | Strip leading `+` (Wikidata era marker for AD); strip trailing `Z`; drop `T00:00:00` time portion when zero. `+2012-10-15T00:00:00Z` → `2012-10-15`. Pre-1AD dates keep the `-` sign: `-0044-03-15T00:00:00Z` → `-0044-03-15`. |
| `Monolingualtext` | 64 | **KEEP** | Strip the `@lang` suffix from the value. All languages kept (was English-only in v6). Model sees `Tokyo` and `東京` as plain string values; the language information is lost. |
| `WikibaseProperty` | 22 | **KEEP** | Same as `WikibaseItem`: label-substitute. Rare in practice. |
| `ExternalId` | 10,206 | **DROP** | Catalog cross-references (Freebase, ISNI, GND, LCCN, Dewey, etc.). The dominant noise source: 76% of v6 corpus by volume. Confirmed in v6 inference to teach the model catalog formats that leak onto unrelated predicates. |
| `Url` | 120 | **DROP** | URLs to external sites — same role as external IDs. |
| `CommonsMedia` | 91 | **DROP** | Wikimedia Commons file names like `"Jerry Seinfeld 1992.jpg"`. No semantic transferable content. |
| `Math` | 36 | **DROP** | LaTeX formulae. Specialised, model can't tokenise meaningfully. |
| `WikibaseSense` | 19 | **DROP** | Lexeme senses — half of Wikidata is the lexeme namespace, separate from the entity (Q-numbered) namespace. Out of scope for the entity-graph model. |
| `WikibaseLexeme` | 16 | **DROP** | Same. |
| `GlobeCoordinate` | 10 | **DROP** | `Point(lat lon)` strings. Structured geographic data; mixing with the language stream confuses the tokenizer. |
| `WikibaseForm` | 10 | **DROP** | Same as WikibaseSense. |
| `MusicalNotation` | 6 | **DROP** | Extremely rare. |
| `TabularData` | 6 | **DROP** | Extremely rare. |
| `GeoShape` | 3 | **DROP** | Extremely rare. |
| `WikibaseEntitySchema` | 2 | **DROP** | Extremely rare. |

**Totals:** 2,231 properties **kept** (17.5% of types) producing the bulk of
*semantic* triples; 10,525 properties **dropped** (82.5% of types) producing
the bulk of *catalog* triples.

On the v6 corpus (757,592 triples, label-substituted), applying these rules:
- 184,458 triples kept (24.3%)
- 573,134 triples dropped (75.7% — almost entirely external IDs)
- 119,382 object values normalised (mostly `+1066` → `1066`, dates losing `T00:00:00Z`)

## Things this spec does NOT cover

- **Language tagging.** The `@lang` is dropped from values. The model can no
  longer distinguish "Tokyo" (English label) from "Tokyo" (Japanese
  romanisation). For most properties this is fine because objects of any one
  property tend to be in one consistent language; for cross-language labels
  this loses information. Restoration would need a multilingual extension
  (per-token language ID, or a separate `lang` slot).
- **Subject deduplication.** Same `<S, P, O>` triple can appear multiple
  times with different `xml:lang` tags; we don't dedupe currently. Probably
  worth doing if perplexity needs more headroom.
- **Object normalisation for non-Wikidata literals.** Plain literals not
  matching the `+\d` shape pass through unchanged. If a property normally has
  Wikidata-shaped values but a particular row has them in a different
  format, no normalisation runs.
- **Subject/predicate position validation.** We trust that the predicate
  position is always a URI (Loka SPARQL has a known quirk surfacing literals
  there; we drop those rows defensively).

## What changed between v6 and v7 corpora

| | v6 | v7 |
|---|---|---|
| External-id rows | included | **dropped** |
| URL / CommonsMedia rows | included | **dropped** |
| `+` prefix on positive years/quantities | kept | **stripped** |
| `T00:00:00Z` on date-only times | kept | **dropped** |
| Non-English monolingualtext | dropped | **kept** (lang stripped) |
| Triple count | 757,592 | 184,458 |
| Final perplexity (5 epochs) | 194.98 | 192.63 |

Perplexity is statistically tied; the qualitative test on the same Q42 seed
(see `training/data/test_propgen_Q42_v6.nt` vs `_v7.nt`) shows the catalog
hallucinations gone in v7, at the cost of emission volume because the model
no longer confidently produces format-shaped garbage on prompts it doesn't
know.
