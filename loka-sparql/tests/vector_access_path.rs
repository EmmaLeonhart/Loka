//! Cost-based choice between the HNSW index and graph-first evaluation for
//! VECTOR_SIMILAR (`planning/cost-based-hnsw.md`).

use std::collections::BTreeSet;

use loka_core::{TermDictionary, TermId, Triple, TripleStore};
use loka_hnsw::{DistanceMetric, VectorPredicateConfig, VectorRegistry};
use loka_sparql::parser::Pattern;
use loka_sparql::{execute_with_vectors, optimize_full, optimize_with_vectors, parse};

const EX: &str = "http://example.org/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

struct Db {
    store: TripleStore,
    dict: TermDictionary,
    vectors: VectorRegistry,
    /// (name, class, vector) as added, for brute-force ground truth.
    raw: Vec<(String, Option<String>, Vec<f32>)>,
}

impl Db {
    fn new(dimensions: usize) -> Self {
        let mut dict = TermDictionary::new();
        let emb = dict.intern(&format!("{EX}emb"));
        dict.intern(RDF_TYPE);
        let mut vectors = VectorRegistry::new();
        vectors
            .declare(VectorPredicateConfig {
                predicate_id: emb,
                dimensions,
                m: 8,
                ef_construction: 40,
                metric: DistanceMetric::Cosine,
            })
            .unwrap();
        Db {
            store: TripleStore::new(),
            dict,
            vectors,
            raw: Vec::new(),
        }
    }

    /// `ex:<name>` with an embedding, typed `ex:<class>` if given.
    fn add(&mut self, name: &str, class: Option<&str>, v: Vec<f32>) -> TermId {
        let s = self.dict.intern(&format!("{EX}{name}"));
        let emb = self.dict.lookup(&format!("{EX}emb")).unwrap();
        let vid = self
            .dict
            .intern(&format!("\"{name}-vec\"^^<http://loka.dev/f32vec>"));
        self.store.insert(Triple::new(s, emb, vid)).unwrap();
        self.raw
            .push((name.to_string(), class.map(str::to_string), v.clone()));
        self.vectors.insert(emb, v, vid).unwrap();
        if let Some(c) = class {
            let ty = self.dict.lookup(RDF_TYPE).unwrap();
            let c = self.dict.intern(&format!("{EX}{c}"));
            self.store.insert(Triple::new(s, ty, c)).unwrap();
        }
        s
    }

    fn subjects(&self, query: &str) -> BTreeSet<String> {
        let q = parse(query).unwrap();
        execute_with_vectors(&q, &self.store, &self.dict, &self.vectors)
            .unwrap()
            .rows
            .iter()
            .map(|r| {
                self.dict
                    .resolve(*r.get("s").unwrap())
                    .unwrap()
                    .trim_start_matches(EX)
                    .to_string()
            })
            .collect()
    }
}

/// 600 vectors almost identical to the query, then `target` at cosine 0.9
/// and `far` at cosine 0.5. `target` is above a 0.85 threshold but is not
/// among the 500 nearest.
fn crowded() -> Db {
    let mut db = Db::new(4);
    for i in 0..600 {
        let (a, b) = ((i % 25) as f32, (i / 25) as f32);
        db.add(
            &format!("near{i}"),
            None,
            vec![1.0, 0.0, 0.001 * a, 0.001 * b],
        );
    }
    db.add(
        "target",
        Some("Marked"),
        vec![0.9, (1.0f32 - 0.81).sqrt(), 0.0, 0.0],
    );
    db.add(
        "far",
        Some("Marked"),
        vec![0.5, (1.0f32 - 0.25).sqrt(), 0.0, 0.0],
    );
    db
}

const MARKED_THEN_VECTOR: &str = "PREFIX ex: <http://example.org/> \
    SELECT ?s WHERE { ?s a ex:Marked . \
    VECTOR_SIMILAR(?s ex:emb \"1 0 0 0\"^^<http://loka.dev/f32vec>, 0.85";

#[test]
fn bound_subject_above_threshold_is_found_outside_the_ann_top_k() {
    let db = crowded();
    let got = db.subjects(&format!("{MARKED_THEN_VECTOR}) }}"));
    // `target` (0.9) is in, `far` (0.5) is out.
    assert_eq!(got, BTreeSet::from(["target".to_string()]));
}

#[test]
fn the_index_path_alone_misses_it() {
    // An explicit k:= forces the index path: membership in the ANN top k.
    // With k = 500 that is what every bound-subject query used to do, and
    // `target` (the 601st nearest) is dropped.
    let db = crowded();
    let got = db.subjects(&format!("{MARKED_THEN_VECTOR}, k:=500) }}"));
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn scores_are_exact_on_the_bound_path() {
    let db = crowded();
    let q = parse(&format!("{MARKED_THEN_VECTOR}) }}")).unwrap();
    let result = execute_with_vectors(&q, &db.store, &db.dict, &db.vectors).unwrap();
    assert_eq!(result.rows.len(), 1);
    let score = result.scores[0].values().next().copied().unwrap();
    assert!(
        (score - 0.9).abs() < 1e-4,
        "cosine of target = 0.9, got {score}"
    );
}

/// 5 `ex:Rare` and 1000 `ex:Common` subjects, all with 8-d vectors.
fn skewed() -> Db {
    let mut db = Db::new(8);
    let mut seed: u64 = 0x9e3779b97f4a7c15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 1000) as f32 / 1000.0
    };
    for i in 0..1005 {
        let class = if i < 5 { "Rare" } else { "Common" };
        let v: Vec<f32> = (0..8).map(|_| next()).collect();
        db.add(&format!("e{i}"), Some(class), v);
    }
    db
}

fn vector_first(class: &str) -> String {
    format!(
        "PREFIX ex: <http://example.org/> SELECT ?s WHERE {{ \
         VECTOR_SIMILAR(?s ex:emb \"1 1 1 1 1 1 1 1\"^^<http://loka.dev/f32vec>, 0.5) . \
         ?s a ex:{class} }}"
    )
}

#[test]
fn a_rare_type_goes_before_the_vector_search() {
    let db = skewed();
    let mut q = parse(&vector_first("Rare")).unwrap();
    optimize_with_vectors(&mut q, Some(&db.store), Some(&db.dict), Some(&db.vectors));
    assert!(
        matches!(q.patterns[0], Pattern::Triple { .. }),
        "{:?}",
        q.patterns[0]
    );
    assert!(matches!(q.patterns[1], Pattern::VectorSimilar { .. }));
}

#[test]
fn a_common_type_stays_after_the_vector_search() {
    let db = skewed();
    let mut q = parse(&vector_first("Common")).unwrap();
    optimize_with_vectors(&mut q, Some(&db.store), Some(&db.dict), Some(&db.vectors));
    assert!(matches!(q.patterns[0], Pattern::VectorSimilar { .. }));
}

#[test]
fn without_the_vector_index_the_old_order_is_kept() {
    let db = skewed();
    let mut q = parse(&vector_first("Rare")).unwrap();
    optimize_full(&mut q, Some(&db.store), Some(&db.dict));
    assert!(matches!(q.patterns[0], Pattern::VectorSimilar { .. }));
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

#[test]
fn the_graph_first_plan_is_exact_and_the_vector_first_plan_is_capped_at_k() {
    // Every one of the 1005 vectors passes the 0.5 threshold, so the
    // vector-first plan (unbound search, default k = 500) can only see 500
    // of them, and some Rare subjects fall outside. The graph-first plan the
    // cost model picks scores the 5 Rare subjects exactly. Found while
    // writing this test: it first asserted the two plans were equal.
    let db = skewed();
    let ones = [1.0f32; 8];
    let truth: BTreeSet<String> = db
        .raw
        .iter()
        .filter(|(_, c, v)| c.as_deref() == Some("Rare") && cosine(v, &ones) >= 0.5)
        .map(|(n, _, _)| n.clone())
        .collect();
    assert_eq!(truth.len(), 5);

    let mut q = parse(&vector_first("Rare")).unwrap();
    optimize_with_vectors(&mut q, Some(&db.store), Some(&db.dict), Some(&db.vectors));
    let planned: BTreeSet<String> = execute_with_vectors(&q, &db.store, &db.dict, &db.vectors)
        .unwrap()
        .rows
        .iter()
        .map(|r| {
            db.dict
                .resolve(*r.get("s").unwrap())
                .unwrap()
                .trim_start_matches(EX)
                .to_string()
        })
        .collect();
    assert_eq!(planned, truth);

    let as_written = db.subjects(&vector_first("Rare"));
    assert!(as_written.is_subset(&truth), "{as_written:?}");
}

#[test]
fn prefixed_names_count_in_cardinality_estimates() {
    // Written Common-first. With prefixed names estimated, the Rare pattern
    // (5 matches) is cheaper and moves first; before, both were estimated
    // as every rdf:type triple and the written order was kept.
    let db = skewed();
    let mut q = parse(
        "PREFIX ex: <http://example.org/> \
         SELECT ?s ?x WHERE { ?s a ex:Common . ?x a ex:Rare }",
    )
    .unwrap();
    optimize_full(&mut q, Some(&db.store), Some(&db.dict));
    let Pattern::Triple { object, .. } = &q.patterns[0] else {
        panic!("{:?}", q.patterns[0]);
    };
    assert!(format!("{object:?}").contains("Rare"), "{object:?}");
}
