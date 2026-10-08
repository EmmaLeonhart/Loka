//! Exit conditions on `+`/`*` path traversal: `UNTIL(expr)` and
//! `GREEDY(vector)` (`planning/until-syntax.md`).

use std::collections::BTreeSet;

use loka_core::{DatabaseConfig, HnswEdgeMode, TermDictionary, TermId, Triple, TripleStore};
use loka_hnsw::{DistanceMetric, VectorPredicateConfig, VectorRegistry};
use loka_sparql::{execute, execute_with_config, parse};

const EX: &str = "http://example.org/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// A category tree over `:broader`, with `:Top` marking some nodes:
///
/// ```text
/// a -> b -> d* -> g*
/// a -> c -> e  -> f*
/// ```
///
/// `*` = `a :Top`. `g` lies beyond the first match `d` on its branch.
fn tree() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let broader = dict.intern(&format!("{EX}broader"));
    let ty = dict.intern(RDF_TYPE);
    let top = dict.intern(&format!("{EX}Top"));
    let mut n = |s: &str| dict.intern(&format!("{EX}{s}"));
    let (a, b, c, d, e, f, g) = (n("a"), n("b"), n("c"), n("d"), n("e"), n("f"), n("g"));
    for (s, o) in [(a, b), (a, c), (b, d), (c, e), (e, f), (d, g)] {
        store.insert(Triple::new(s, broader, o)).unwrap();
    }
    for t in [d, f, g] {
        store.insert(Triple::new(t, ty, top)).unwrap();
    }
    (store, dict)
}

fn names(query: &str, store: &TripleStore, dict: &TermDictionary) -> Vec<String> {
    let q = parse(query).unwrap();
    execute(&q, store, dict)
        .unwrap()
        .rows
        .iter()
        .map(|r| {
            dict.resolve(*r.get("n").unwrap())
                .unwrap()
                .trim_start_matches(EX)
                .to_string()
        })
        .collect()
}

#[test]
fn until_stops_each_branch_at_its_first_match() {
    let (store, dict) = tree();
    let got = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:a ex:broader+ ?n UNTIL(EXISTS { ?n a ex:Top }) }",
        &store,
        &dict,
    );
    // d (depth 2) and f (depth 3): the nearest :Top on each branch. Not g,
    // which is only reachable through d.
    assert_eq!(got, vec!["d", "f"]);
}

#[test]
fn until_differs_from_a_post_filter() {
    let (store, dict) = tree();
    let filtered: BTreeSet<String> = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:a ex:broader+ ?n . FILTER EXISTS { ?n a ex:Top } }",
        &store,
        &dict,
    )
    .into_iter()
    .collect();
    let until: BTreeSet<String> = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:a ex:broader+ ?n UNTIL(EXISTS { ?n a ex:Top }) }",
        &store,
        &dict,
    )
    .into_iter()
    .collect();
    assert_eq!(
        filtered,
        BTreeSet::from(["d".into(), "f".into(), "g".into()])
    );
    assert_eq!(until, BTreeSet::from(["d".into(), "f".into()]));
}

#[test]
fn until_accepts_a_plain_filter_expression() {
    let (store, dict) = tree();
    let got = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:a ex:broader+ ?n UNTIL(?n = ex:e) }",
        &store,
        &dict,
    );
    assert_eq!(got, vec!["e"]);
}

#[test]
fn until_with_no_match_returns_nothing() {
    let (store, dict) = tree();
    let got = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:a ex:broader+ ?n UNTIL(?n = ex:nowhere) }",
        &store,
        &dict,
    );
    assert!(got.is_empty());
}

#[test]
fn zero_or_more_checks_the_start_node_first() {
    let (store, dict) = tree();
    let from_d = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:d ex:broader* ?n UNTIL(EXISTS { ?n a ex:Top }) }",
        &store,
        &dict,
    );
    assert_eq!(from_d, vec!["d"], "d is :Top itself: the zero-length path");
    let plus_from_d = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:d ex:broader+ ?n UNTIL(EXISTS { ?n a ex:Top }) }",
        &store,
        &dict,
    );
    assert_eq!(plus_from_d, vec!["g"], "`+` needs at least one step");
}

#[test]
fn a_node_reached_by_two_branches_is_emitted_once() {
    // Diamond: s -> x, s -> y, x -> t, y -> t; t matches.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern(&format!("{EX}p"));
    let mut n = |s: &str| dict.intern(&format!("{EX}{s}"));
    let (s, x, y, t) = (n("s"), n("x"), n("y"), n("t"));
    for (a, b) in [(s, x), (s, y), (x, t), (y, t)] {
        store.insert(Triple::new(a, p, b)).unwrap();
    }
    let got = names(
        "PREFIX ex: <http://example.org/> SELECT ?n WHERE { ex:s ex:p+ ?n UNTIL(?n = ex:t) }",
        &store,
        &dict,
    );
    assert_eq!(got, vec!["t"]);
}

#[test]
fn matches_within_one_depth_come_out_in_value_order() {
    // All three children match; inserted in reverse order of their IRIs.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern(&format!("{EX}p"));
    let root = dict.intern(&format!("{EX}root"));
    for c in ["zeta", "mu", "alpha"] {
        let id = dict.intern(&format!("{EX}{c}"));
        store.insert(Triple::new(root, p, id)).unwrap();
    }
    let got = names(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:root ex:p+ ?n UNTIL(isIRI(?n)) }",
        &store,
        &dict,
    );
    assert_eq!(got, vec!["alpha", "mu", "zeta"]);
}

#[test]
fn until_on_a_non_path_predicate_is_a_parse_error() {
    assert!(
        parse("SELECT ?n WHERE { <http://e/a> <http://e/p> ?n UNTIL(?n = <http://e/b>) }").is_err()
    );
    assert!(
        parse("SELECT ?n WHERE { <http://e/a> <http://e/p>+ <http://e/b> UNTIL(true) }").is_err()
    );
}

// ── GREEDY ──────────────────────────────────────────────────────────────

const EMB: &str = "http://example.org/hasEmbedding";

const POINTS: [[f32; 3]; 8] = [
    [1.0, 0.0, 0.0],
    [0.9, 0.1, 0.0],
    [0.8, 0.2, 0.1],
    [0.0, 1.0, 0.0],
    [0.1, 0.9, 0.1],
    [0.0, 0.0, 1.0],
    [0.1, 0.1, 0.9],
    [0.5, 0.5, 0.5],
];

fn indexed() -> (
    TripleStore,
    TermDictionary,
    VectorRegistry,
    Vec<TermId>,
    Vec<TermId>,
) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let emb = dict.intern(EMB);
    dict.intern(loka_hnsw::HNSW_NEIGHBOR_IRI);
    let mut vectors = VectorRegistry::new();
    vectors
        .declare(VectorPredicateConfig {
            predicate_id: emb,
            dimensions: 3,
            m: 4,
            ef_construction: 20,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();
    let mut docs = Vec::new();
    let mut vecs = Vec::new();
    for (i, p) in POINTS.iter().enumerate() {
        let doc = dict.intern(&format!("{EX}doc{i}"));
        let v = dict.intern(&format!("\"v{i}\"^^<http://loka.dev/f32vec>"));
        store.insert(Triple::new(doc, emb, v)).unwrap();
        vectors.insert(emb, p.to_vec(), v).unwrap();
        docs.push(doc);
        vecs.push(v);
    }
    (store, dict, vectors, docs, vecs)
}

fn virtual_config() -> DatabaseConfig {
    DatabaseConfig {
        hnsw_edge_mode: HnswEdgeMode::Virtual,
        ..Default::default()
    }
}

fn run(
    query: &str,
    store: &TripleStore,
    dict: &TermDictionary,
    vectors: &VectorRegistry,
) -> Vec<TermId> {
    let q = parse(query).unwrap();
    execute_with_config(&q, store, dict, vectors, &virtual_config())
        .unwrap()
        .rows
        .iter()
        .map(|r| *r.get("n").unwrap())
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

const QUERY: [f32; 3] = [0.05, 0.0, 1.0];

fn greedy_from(
    start: usize,
    store: &TripleStore,
    dict: &TermDictionary,
    vectors: &VectorRegistry,
    docs: &[TermId],
) -> Vec<TermId> {
    run(
        &format!(
            "SELECT ?n WHERE {{ <{}> <{}>+ ?n GREEDY(\"{} {} {}\"^^<http://loka.dev/f32vec>) }}",
            dict.resolve(docs[start]).unwrap(),
            loka_hnsw::HNSW_NEIGHBOR_IRI,
            QUERY[0],
            QUERY[1],
            QUERY[2]
        ),
        store,
        dict,
        vectors,
    )
}

#[test]
fn greedy_ends_at_a_local_optimum_from_every_start() {
    let (store, dict, vectors, docs, _) = indexed();
    let sim = |doc: TermId| {
        let i = docs.iter().position(|d| *d == doc).unwrap();
        cosine(&POINTS[i], &QUERY)
    };
    for start in 0..docs.len() {
        let got = greedy_from(start, &store, &dict, &vectors, &docs);
        assert_eq!(got.len(), 1, "one result per start node");
        let end = got[0];
        // Independent check: the end node's neighbours, by a single-hop
        // query, are none of them closer to the query.
        let neighbours = run(
            &format!(
                "SELECT ?n WHERE {{ <{}> <{}> ?n }}",
                dict.resolve(end).unwrap(),
                loka_hnsw::HNSW_NEIGHBOR_IRI
            ),
            &store,
            &dict,
            &vectors,
        );
        assert!(!neighbours.is_empty());
        for n in neighbours {
            assert!(
                sim(n) <= sim(end) + 1e-6,
                "start doc{start}: neighbour closer than the greedy end"
            );
        }
        assert!(sim(end) >= sim(docs[start]) - 1e-6, "never moves away");
    }
}

#[test]
fn greedy_reaches_the_same_node_as_index_search_on_this_dataset() {
    // Greedy search on a graph is not guaranteed to find the global nearest
    // in general; this is stated for this small, well-connected index only.
    let (store, dict, vectors, docs, vecs) = indexed();
    let emb = dict.lookup(EMB).unwrap();
    let top = vectors.search(emb, &QUERY, 1, 20).unwrap();
    let nearest_vec = top[0].triple_id;
    let nearest_doc = docs[vecs.iter().position(|v| *v == nearest_vec).unwrap()];
    assert_eq!(nearest_doc, docs[5], "brute force agrees: doc5 = [0, 0, 1]");
    let got = greedy_from(0, &store, &dict, &vectors, &docs);
    assert_eq!(got, vec![nearest_doc]);
}

#[test]
fn greedy_on_a_stored_predicate_is_an_error() {
    let (store, dict) = tree();
    let q = parse(
        "PREFIX ex: <http://example.org/> \
         SELECT ?n WHERE { ex:a ex:broader+ ?n GREEDY(\"1 0 0\"^^<http://loka.dev/f32vec>) }",
    )
    .unwrap();
    assert!(execute(&q, &store, &dict).is_err());
}

// ── BEAM ────────────────────────────────────────────────────────────────

fn beam_from(
    start: usize,
    k: usize,
    store: &TripleStore,
    dict: &TermDictionary,
    vectors: &VectorRegistry,
    docs: &[TermId],
) -> Vec<TermId> {
    run(
        &format!(
            "SELECT ?n WHERE {{ <{}> <{}>+ ?n BEAM(\"{} {} {}\"^^<http://loka.dev/f32vec>, {k}) }}",
            dict.resolve(docs[start]).unwrap(),
            loka_hnsw::HNSW_NEIGHBOR_IRI,
            QUERY[0],
            QUERY[1],
            QUERY[2]
        ),
        store,
        dict,
        vectors,
    )
}

#[test]
fn beam_of_width_one_is_greedy_descent() {
    let (store, dict, vectors, docs, _) = indexed();
    for start in 0..docs.len() {
        assert_eq!(
            beam_from(start, 1, &store, &dict, &vectors, &docs),
            greedy_from(start, &store, &dict, &vectors, &docs),
            "start doc{start}"
        );
    }
}

#[test]
fn beam_returns_the_brute_force_top_k_on_this_dataset() {
    // Stated for this small, well-connected index only: beam search on a
    // graph is not guaranteed to find the exact top k in general.
    let (store, dict, vectors, docs, _) = indexed();
    let mut truth: Vec<(f32, TermId)> = POINTS
        .iter()
        .zip(&docs)
        .map(|(p, d)| (cosine(p, &QUERY), *d))
        .collect();
    truth.sort_by(|a, b| b.0.total_cmp(&a.0));
    for k in [1, 3, 5] {
        let got = beam_from(0, k, &store, &dict, &vectors, &docs);
        let want: Vec<TermId> = truth.iter().take(k).map(|(_, d)| *d).collect();
        assert_eq!(got, want, "k = {k}: most similar first");
    }
    // More than there are nodes: every node, still in order.
    let all = beam_from(0, 20, &store, &dict, &vectors, &docs);
    assert_eq!(all.len(), docs.len());
}

#[test]
fn beam_width_zero_is_a_parse_error() {
    assert!(parse(
        "SELECT ?n WHERE { <http://e/a> <http://e/p>+ ?n BEAM(\"1 0 0\"^^<http://loka.dev/f32vec>, 0) }"
    )
    .is_err());
}
