//! Pseudo-table columns answer triple patterns only while they are exact and
//! current, and then give the same rows as the triple indexes
//! (`planning/pseudo-table-serving.md`).

use std::collections::BTreeSet;

use loka_core::{
    discover_pseudo_tables, extract_node_properties, DatabaseConfig, PseudoTableRegistry,
    TermDictionary, Triple, TripleStore,
};
use loka_hnsw::VectorRegistry;
use loka_sparql::{execute_with_pseudo_tables, parse};

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

    fn remove(&mut self, s: &str, p: &str, o: &str) {
        let t = Triple::new(self.id(s), self.id(p), self.id(o));
        assert!(self.store.remove(&t));
    }

    fn discover(&self) -> PseudoTableRegistry {
        discover_pseudo_tables(&extract_node_properties(&self.store), &self.store)
    }

    /// Rows (as sorted strings) and pseudo-table hits.
    fn run(&self, query: &str, registry: Option<&PseudoTableRegistry>) -> (BTreeSet<String>, usize) {
        let q = parse(&format!("PREFIX ex: <{EX}> {query}")).unwrap();
        let (result, hits) = execute_with_pseudo_tables(
            &q,
            &self.store,
            &self.dict,
            &VectorRegistry::new(),
            &DatabaseConfig::default(),
            registry,
        )
        .unwrap();
        let rows = result
            .rows
            .iter()
            .map(|r| {
                let mut cells: Vec<String> = r
                    .iter()
                    .map(|(k, v)| format!("{k}={}", self.dict.resolve(*v).unwrap_or("?")))
                    .collect();
                cells.sort();
                cells.join(" ")
            })
            .collect();
        (rows, hits)
    }

    /// Whether any row binds `var` to `ex:<local>`.
    fn has(&self, query: &str, var: &str, local: &str, registry: &PseudoTableRegistry) -> bool {
        let want = format!("{var}={EX}{local}");
        self.run(query, Some(registry))
            .0
            .iter()
            .any(|row| row.split(' ').any(|cell| cell == want))
    }

    /// Same rows with and without the registry; returns the hit count.
    fn same_rows(&self, query: &str, registry: &PseudoTableRegistry) -> usize {
        let (plain, plain_hits) = self.run(query, None);
        let (served, hits) = self.run(query, Some(registry));
        assert_eq!(plain_hits, 0);
        assert_eq!(served, plain, "{query}");
        hits
    }
}

/// 40 people sharing six properties: a clean characteristic set.
fn people() -> Db {
    let mut db = Db {
        store: TripleStore::new(),
        dict: TermDictionary::new(),
    };
    for i in 0..40 {
        let p = format!("p{i}");
        db.add(&p, "type", "Person");
        db.add(&p, "name", &format!("name{i}"));
        db.add(&p, "age", &format!("age{}", 20 + i % 30));
        db.add(&p, "city", &format!("city{}", i % 4));
        db.add(&p, "knows", &format!("p{}", (i + 1) % 40));
        db.add(&p, "email", &format!("mail{i}"));
    }
    db
}

#[test]
fn a_single_pattern_is_served_with_the_same_rows() {
    let db = people();
    let registry = db.discover();
    assert!(!registry.is_empty(), "the people form a pseudo-table");
    let hits = db.same_rows("SELECT ?s ?n WHERE { ?s ex:name ?n }", &registry);
    assert!(hits > 0, "answered from the pseudo-table");
}

#[test]
fn a_star_query_is_fused_with_the_same_rows() {
    let db = people();
    let registry = db.discover();
    let hits = db.same_rows(
        "SELECT ?s ?n ?a ?c WHERE { ?s ex:name ?n . ?s ex:age ?a . ?s ex:city ?c }",
        &registry,
    );
    assert!(hits >= 2, "fused scan over several columns, got {hits}");
}

#[test]
fn constant_objects_match_the_triple_path() {
    let db = people();
    let registry = db.discover();
    db.same_rows("SELECT ?s WHERE { ?s ex:city ex:city2 }", &registry);
    // Not in the dictionary: no rows. Read as unbound it would match all 40.
    let (rows, _) = db.run("SELECT ?s WHERE { ?s ex:city ex:nowhere }", Some(&registry));
    assert!(rows.is_empty(), "{rows:?}");
    db.same_rows(
        "SELECT ?s ?n WHERE { ?s ex:city ex:nowhere . ?s ex:name ?n }",
        &registry,
    );
}

#[test]
fn a_non_member_with_the_predicate_is_still_returned() {
    let mut db = people();
    // A dog with a name, and nothing else in common with the people.
    db.add("rex", "name", "Rex");
    let registry = db.discover();
    let hits = db.same_rows("SELECT ?s ?n WHERE { ?s ex:name ?n }", &registry);
    assert_eq!(hits, 0, "the name column doesn't hold every name triple");
    assert!(db.has("SELECT ?n WHERE { ?s ex:name ?n }", "n", "Rex", &registry));
}

#[test]
fn a_second_value_is_still_returned() {
    let mut db = people();
    db.add("p0", "email", "mail0-other");
    let registry = db.discover();
    let hits = db.same_rows("SELECT ?s ?m WHERE { ?s ex:email ?m }", &registry);
    assert_eq!(hits, 0, "a cell holds one value, so the column isn't exact");
    let (rows, _) = db.run(
        "SELECT ?m WHERE { ex:p0 ex:email ?m }",
        Some(&registry),
    );
    assert_eq!(rows.len(), 2);
    // Other columns are unaffected.
    assert!(db.same_rows("SELECT ?s ?n WHERE { ?s ex:name ?n }", &registry) > 0);
}

#[test]
fn a_column_is_not_served_after_its_predicate_changes() {
    let mut db = people();
    let registry = db.discover();
    assert!(db.same_rows("SELECT ?s ?n WHERE { ?s ex:name ?n }", &registry) > 0);

    // Writes after discovery: a new name, and a removed one.
    db.add("p40", "name", "name40");
    db.remove("p3", "name", "name3");
    let hits = db.same_rows("SELECT ?s ?n WHERE { ?s ex:name ?n }", &registry);
    assert_eq!(hits, 0, "stale column not served");
    let q = "SELECT ?n WHERE { ?s ex:name ?n }";
    assert!(db.has(q, "n", "name40", &registry));
    assert!(!db.has(q, "n", "name3", &registry));
    assert!(db.has(q, "n", "name4", &registry), "the check can see a present name");

    // A write to another predicate leaves the age column servable.
    assert!(db.same_rows("SELECT ?s ?a WHERE { ?s ex:age ?a }", &registry) > 0);
}
