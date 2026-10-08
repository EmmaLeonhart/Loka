//! Deep (multi-hop) pseudo-table columns answer chain queries only while
//! exact and current, with the same rows as the triple indexes
//! (`planning/deep-pseudo-table-serving.md`).

use loka_core::{
    discover_deep_pseudo_tables, discover_pseudo_tables, extract_node_properties, DatabaseConfig,
    PseudoTableRegistry, TermDictionary, Triple, TripleStore,
};
use loka_hnsw::VectorRegistry;
use loka_sparql::{execute_instrumented, parse, QueryMetrics};

const EX: &str = "http://example.org/";

struct Db {
    store: TripleStore,
    dict: TermDictionary,
}

impl Db {
    fn id(&mut self, local: &str) -> u64 {
        self.dict.intern(&format!("{EX}{local}"))
    }

    fn add(&mut self, s: &str, p: &str, o: &str) {
        let t = Triple::new(self.id(s), self.id(p), self.id(o));
        self.store.insert(t).unwrap();
    }

    fn discover(&self) -> PseudoTableRegistry {
        let mut r = discover_pseudo_tables(&extract_node_properties(&self.store), &self.store);
        r.deep = discover_deep_pseudo_tables(&self.store);
        r
    }

    /// Sorted rows (with duplicates) and whether a deep chain answered.
    fn run(&self, query: &str, registry: Option<&PseudoTableRegistry>) -> (Vec<String>, bool) {
        let q = parse(&format!("PREFIX ex: <{EX}> {query}")).unwrap();
        let metrics = QueryMetrics::new();
        let (result, _) = execute_instrumented(
            &q,
            &self.store,
            &self.dict,
            &VectorRegistry::new(),
            &DatabaseConfig::default(),
            registry,
            Some(&metrics),
        )
        .unwrap();
        let cols = result.columns.clone();
        let mut rows: Vec<String> = result
            .rows
            .iter()
            .map(|r| {
                cols.iter()
                    .map(|c| {
                        let v = r
                            .get(c)
                            .and_then(|id| self.dict.resolve(*id))
                            .unwrap_or("-");
                        format!("{c}={}", v.trim_start_matches(EX))
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        rows.sort();
        let deep = metrics
            .report()
            .patterns
            .iter()
            .any(|p| p.shape.starts_with("deep_chain"));
        (rows, deep)
    }

    /// Same rows with and without the registry; returns whether a deep chain
    /// answered with the registry.
    fn same_rows(&self, query: &str, registry: &PseudoTableRegistry) -> bool {
        let (plain, plain_deep) = self.run(query, None);
        let (served, deep) = self.run(query, Some(registry));
        assert!(!plain_deep);
        assert_eq!(served, plain, "{query}");
        deep
    }
}

/// 60 countries, each `-hasCapital-> capital -hasMayor-> mayor`, plus names
/// and populations: a tree, so discovery builds a deep table.
fn countries() -> Db {
    let mut db = Db {
        store: TripleStore::new(),
        dict: TermDictionary::new(),
    };
    for i in 0..60 {
        let (c, k, m) = (
            format!("country{i}"),
            format!("capital{i}"),
            format!("mayor{i}"),
        );
        db.add(&c, "hasCapital", &k);
        db.add(&k, "hasMayor", &m);
        db.add(&c, "name", &format!("cn{i}"));
        db.add(&c, "population", &format!("pop{i}"));
        db.add(&k, "name", &format!("kn{i}"));
        db.add(&m, "name", &format!("mn{i}"));
    }
    db
}

const CHAIN: &str = "SELECT ?c ?m WHERE { ?c ex:hasCapital ?k . ?k ex:hasMayor ?m }";

#[test]
fn the_chain_is_answered_from_the_deep_column_with_the_same_rows() {
    let db = countries();
    let registry = db.discover();
    assert!(!registry.deep.is_empty(), "the countries form a deep table");
    assert!(db.same_rows(CHAIN, &registry), "served by the deep column");
    let (rows, _) = db.run(CHAIN, Some(&registry));
    assert_eq!(rows.len(), 60);
    assert!(rows.contains(&"c=country7 m=mayor7".to_string()));
}

#[test]
fn a_constant_leaf_filters_the_column() {
    let db = countries();
    let registry = db.discover();
    let q = "SELECT ?c WHERE { ?c ex:hasCapital ?k . ?k ex:hasMayor ex:mayor7 }";
    assert!(db.same_rows(q, &registry));
    assert_eq!(db.run(q, Some(&registry)).0, vec!["c=country7".to_string()]);
}

#[test]
fn reading_the_intermediate_variable_is_not_served() {
    let db = countries();
    let registry = db.discover();
    for q in [
        "SELECT ?c ?k ?m WHERE { ?c ex:hasCapital ?k . ?k ex:hasMayor ?m }",
        "SELECT ?c ?m WHERE { ?c ex:hasCapital ?k . ?k ex:hasMayor ?m . FILTER(?k != ex:capital3) }",
        "SELECT * WHERE { ?c ex:hasCapital ?k . ?k ex:hasMayor ?m }",
    ] {
        assert!(!db.same_rows(q, &registry), "{q}");
    }
}

#[test]
fn a_non_member_root_makes_the_column_inexact() {
    let mut db = countries();
    // A chain whose root has nothing else in common with the countries.
    db.add("outpost", "hasCapital", "camp");
    db.add("camp", "hasMayor", "warden");
    let registry = db.discover();
    assert!(!db.same_rows(CHAIN, &registry), "not served");
    assert!(db
        .run(CHAIN, Some(&registry))
        .0
        .contains(&"c=outpost m=warden".to_string()));
}

#[test]
fn a_second_leaf_makes_the_column_inexact() {
    let mut db = countries();
    db.add("capital7", "hasMayor", "deputy7");
    let registry = db.discover();
    assert!(!db.same_rows(CHAIN, &registry));
    let rows = db.run(CHAIN, Some(&registry)).0;
    assert!(rows.contains(&"c=country7 m=mayor7".to_string()));
    assert!(rows.contains(&"c=country7 m=deputy7".to_string()));
}

#[test]
fn a_leaf_reached_through_two_middles_makes_the_column_inexact() {
    let mut db = countries();
    db.add("country7", "hasCapital", "capital7b");
    db.add("capital7b", "hasMayor", "mayor7");
    let registry = db.discover();
    assert!(!db.same_rows(CHAIN, &registry));
    let rows = db.run(CHAIN, Some(&registry)).0;
    let n = rows.iter().filter(|r| *r == "c=country7 m=mayor7").count();
    assert_eq!(n, 2, "two solutions, one per middle");
}

#[test]
fn a_write_to_any_hop_stops_serving_until_rediscovery() {
    let mut db = countries();
    let registry = db.discover();
    assert!(db.same_rows(CHAIN, &registry));

    // A write to an unrelated predicate leaves the column servable.
    db.add("country3", "anthem", "anthem3");
    assert!(db.same_rows(CHAIN, &registry));

    // A write to either hop's predicate stops it.
    db.add("capital60", "hasMayor", "mayor60");
    assert!(!db.same_rows(CHAIN, &registry));
    let mut db = countries();
    let registry = db.discover();
    db.add("country60", "hasCapital", "capital60");
    assert!(!db.same_rows(CHAIN, &registry));
}
