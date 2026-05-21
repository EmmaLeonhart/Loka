//! Rebuild a `VectorRegistry` from a populated `TripleStore` + `TermDictionary`.
//!
//! On `loka serve` startup, the HNSW registry is empty — it has no
//! persistence of its own. The triples themselves are the source of truth:
//! for every triple whose object is an f32vec literal, the predicate is
//! taken as a vector-predicate declaration and the vector is inserted into
//! the corresponding index.
//!
//! Robustness: skip triples whose predicate is itself a literal or an
//! inline-encoded integer/boolean. These rows are malformed (typically
//! engine-bug-#2 leftovers where a literal-id ended up in the predicate
//! slot on disk) — declaring a vector index keyed by a literal-id is the
//! corruption mode observed on the 2026-05-20 sled rehydrate of
//! `loka-retrieval-data-stale-20260520/`, where `/vectors/health`
//! reported predicate slots resolving to f32vec literals instead of IRIs.

use loka_core::{is_inline, TermDictionary, TripleStore};

use crate::registry::{VectorPredicateConfig, VectorRegistry};
use crate::vector::DistanceMetric;

/// Marker used at the end of an interned f32vec literal: `"…"^^<…/f32vec>`.
const F32VEC_TYPE_SUFFIX: &str = "^^<http://loka.dev/f32vec>";

/// Default HNSW tuning used when a vector predicate is discovered at
/// rebuild time (no explicit `loka:declareVectorPredicate` triple was
/// found). Matches the legacy hard-coded defaults from `loka-cli serve`.
pub struct RebuildDefaults {
    pub m: usize,
    pub ef_construction: usize,
    pub metric: DistanceMetric,
}

impl Default for RebuildDefaults {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            metric: DistanceMetric::Cosine,
        }
    }
}

/// Rebuild a `VectorRegistry` by scanning every triple in `store`.
///
/// Returns `(registry, inserted_count)`. `inserted_count` counts only
/// vectors that were *successfully* inserted into a declared index —
/// triples skipped for malformed-predicate or unparseable-object reasons
/// are not counted.
pub fn rebuild_from_store(
    store: &TripleStore,
    dict: &TermDictionary,
    defaults: &RebuildDefaults,
) -> (VectorRegistry, usize) {
    let mut registry = VectorRegistry::new();
    let mut count = 0;

    for triple in store.iter() {
        // Predicate sanity. A vector predicate must be an IRI (or at
        // least, must NOT be a literal or inline-encoded value). Skip
        // malformed rows rather than declaring a registry slot keyed by
        // a literal-id.
        if is_inline(triple.predicate) {
            continue;
        }
        let pred_str = match dict.resolve(triple.predicate) {
            Some(s) => s,
            None => continue,
        };
        if pred_str.starts_with('"') {
            continue;
        }

        // Object must be an interned f32vec literal.
        let obj_str = match dict.resolve(triple.object) {
            Some(s) => s,
            None => continue,
        };
        if !obj_str.contains(F32VEC_TYPE_SUFFIX) {
            continue;
        }

        let floats = match parse_f32vec_literal(obj_str) {
            Some(f) if !f.is_empty() => f,
            _ => continue,
        };
        let dims = floats.len();

        if !registry.has_index(triple.predicate) {
            let config = VectorPredicateConfig {
                predicate_id: triple.predicate,
                dimensions: dims,
                m: defaults.m,
                ef_construction: defaults.ef_construction,
                metric: defaults.metric,
            };
            // `declare` only errors on duplicate predicate, which we just
            // checked. Treat any error here as fatal-to-this-triple, not
            // fatal-to-the-rebuild, and move on.
            if registry.declare(config).is_err() {
                continue;
            }
        }

        if registry
            .insert(triple.predicate, floats, triple.object)
            .is_ok()
        {
            count += 1;
        }
    }

    (registry, count)
}

/// Parse a stored f32vec literal `"a b c"^^<...>` into a `Vec<f32>`.
///
/// Returns `None` if no quoted segment is present. Returns an empty Vec
/// if the quoted segment has no parseable floats (caller treats both
/// as skip-this-triple).
fn parse_f32vec_literal(s: &str) -> Option<Vec<f32>> {
    let start = s.find('"')? + 1;
    let rest = &s[start..];
    let end_rel = rest.find('"')?;
    let body = &rest[..end_rel];
    Some(
        body.split_whitespace()
            .filter_map(|t| t.parse::<f32>().ok())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use loka_core::Triple;

    #[test]
    fn rebuild_skips_triples_with_literal_predicate() {
        // Engine-bug-#2 sled-rehydrate corruption mode: a malformed row
        // on disk has predicate = (vector-literal id) rather than an
        // IRI. Pre-fix, the rebuild would declare a vector index keyed
        // by that literal-id — visible in /vectors/health as a
        // predicate slot whose label is the f32vec literal string.
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();

        let good_iri = dict.intern("http://loka.dev/retrieval/nodeEmb");
        let bad_pred = dict.intern("\"-0.0076 0.0809 0.5\"^^<http://loka.dev/f32vec>");
        let subject_a = dict.intern("http://wd/Q42");
        let subject_b = dict.intern("http://wd/Q43");
        let vec1 = dict.intern("\"0.1 0.2 0.3\"^^<http://loka.dev/f32vec>");
        let vec2 = dict.intern("\"0.4 0.5 0.6\"^^<http://loka.dev/f32vec>");

        // Well-formed: predicate is an IRI.
        store
            .insert(Triple::new(subject_a, good_iri, vec1))
            .unwrap();
        // Malformed: predicate is a literal.
        store
            .insert(Triple::new(subject_b, bad_pred, vec2))
            .unwrap();

        let (registry, count) = rebuild_from_store(&store, &dict, &RebuildDefaults::default());

        assert_eq!(
            count, 1,
            "only the well-formed triple should contribute a vector"
        );
        assert!(
            registry.has_index(good_iri),
            "the IRI-predicate index must be declared"
        );
        assert!(
            !registry.has_index(bad_pred),
            "the literal-predicate index must NOT be declared (this is the bug)"
        );
        assert_eq!(
            registry.predicates().len(),
            1,
            "exactly one predicate in the registry"
        );
    }

    #[test]
    fn rebuild_declares_multiple_predicates() {
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();

        let node_emb = dict.intern("http://loka.dev/retrieval/nodeEmb");
        let name_emb = dict.intern("http://loka.dev/retrieval/nameEmb");
        let triple_emb = dict.intern("http://loka.dev/retrieval/tripleEmb");

        let s1 = dict.intern("http://wd/Q1");
        let s2 = dict.intern("http://wd/Q2");
        let s3 = dict.intern("http://wd/Q3");
        let v1 = dict.intern("\"0.1 0.2\"^^<http://loka.dev/f32vec>");
        let v2 = dict.intern("\"0.3 0.4\"^^<http://loka.dev/f32vec>");
        let v3 = dict.intern("\"0.5 0.6\"^^<http://loka.dev/f32vec>");

        store.insert(Triple::new(s1, node_emb, v1)).unwrap();
        store.insert(Triple::new(s2, name_emb, v2)).unwrap();
        store.insert(Triple::new(s3, triple_emb, v3)).unwrap();

        let (registry, count) = rebuild_from_store(&store, &dict, &RebuildDefaults::default());

        assert_eq!(count, 3);
        assert!(registry.has_index(node_emb));
        assert!(registry.has_index(name_emb));
        assert!(registry.has_index(triple_emb));
        assert_eq!(registry.predicates().len(), 3);
    }

    #[test]
    fn rebuild_ignores_non_vector_triples() {
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();

        let label = dict.intern("http://www.w3.org/2000/01/rdf-schema#label");
        let s1 = dict.intern("http://wd/Q1");
        let name = dict.intern("\"Douglas Adams\"");

        store.insert(Triple::new(s1, label, name)).unwrap();

        let (registry, count) = rebuild_from_store(&store, &dict, &RebuildDefaults::default());

        assert_eq!(count, 0);
        assert!(registry.predicates().is_empty());
    }

    #[test]
    fn parse_f32vec_handles_normal_literal() {
        let s = "\"0.1 0.2 0.3\"^^<http://loka.dev/f32vec>";
        assert_eq!(parse_f32vec_literal(s), Some(vec![0.1, 0.2, 0.3]));
    }

    #[test]
    fn parse_f32vec_returns_none_on_missing_quotes() {
        assert!(parse_f32vec_literal("no quotes here").is_none());
    }
}
