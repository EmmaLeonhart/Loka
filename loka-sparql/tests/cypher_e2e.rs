//! End-to-end tests for the Cypher → SPARQL transpiler.
//!
//! The unit tests in `src/cypher.rs` check the emitted text and that it parses.
//! These go the rest of the way — transpile, parse, execute against a real
//! store, and check the rows — so a mapping that parses but retrieves the wrong
//! thing is caught.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse, transpile};

/// A small social graph in the transpiler's default `http://loka.dev/`
/// namespace, so Cypher bare names resolve against it.
fn social_graph() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();

    let rdf_type = dict.intern("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let person = dict.intern("http://loka.dev/Person");
    let city = dict.intern("http://loka.dev/City");
    let knows = dict.intern("http://loka.dev/KNOWS");
    let lives_in = dict.intern("http://loka.dev/LIVES_IN");
    let name = dict.intern("http://loka.dev/name");
    let age = dict.intern("http://loka.dev/age");

    let ada = dict.intern("http://loka.dev/ada");
    let bob = dict.intern("http://loka.dev/bob");
    let cy = dict.intern("http://loka.dev/cy");
    let london = dict.intern("http://loka.dev/london");

    let ada_name = dict.intern("\"Ada\"");
    let bob_name = dict.intern("\"Bob\"");
    let cy_name = dict.intern("\"Cy\"");
    // Numbers are inline-encoded TermIds, not interned strings — an interned
    // "36" is a different term from the integer 36 and will not compare.
    let age_36 = loka_core::inline_integer(36).unwrap();
    let age_25 = loka_core::inline_integer(25).unwrap();
    let age_50 = loka_core::inline_integer(50).unwrap();

    for (s, p, o) in [
        (ada, rdf_type, person),
        (bob, rdf_type, person),
        (cy, rdf_type, person),
        (london, rdf_type, city),
        (ada, name, ada_name),
        (bob, name, bob_name),
        (cy, name, cy_name),
        (ada, age, age_36),
        (bob, age, age_25),
        (cy, age, age_50),
        (ada, knows, bob),
        (bob, knows, cy),
        (ada, lives_in, london),
    ] {
        store.insert(Triple::new(s, p, o)).unwrap();
    }

    (store, dict)
}

/// Transpile, parse, execute — the whole path. Rows keep their variable names
/// so a test can assert on a specific binding rather than an unordered bag.
fn run(cypher: &str) -> Vec<std::collections::HashMap<String, loka_core::TermId>> {
    let (store, dict) = social_graph();
    let sparql = transpile(cypher).unwrap_or_else(|e| panic!("transpile failed: {}", e));
    let q = parse(&sparql)
        .unwrap_or_else(|e| panic!("emitted SPARQL did not parse: {}\n---\n{}", e, sparql));
    let result = execute(&q, &store, &dict)
        .unwrap_or_else(|e| panic!("execution failed: {}\n---\n{}", e, sparql));

    result
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .collect()
}

#[test]
fn label_match_returns_all_people() {
    let rows = run("MATCH (a:Person) RETURN a");
    assert_eq!(rows.len(), 3, "expected 3 Person nodes, got {:?}", rows);
}

#[test]
fn relationship_traversal_returns_the_right_pair() {
    let rows = run("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a, b");
    // ada KNOWS bob, bob KNOWS cy
    assert_eq!(rows.len(), 2, "{:?}", rows);
}

#[test]
fn inbound_direction_is_not_symmetric() {
    // Ada KNOWS Bob, and nobody KNOWS Ada. So from Ada, outbound finds one
    // node and inbound finds none. If the transpiler dropped edge direction,
    // the inbound query would also return a row.
    let outbound = run("MATCH (a:Person {name: \"Ada\"})-[:KNOWS]->(b) RETURN b");
    assert_eq!(
        outbound.len(),
        1,
        "Ada should KNOW one person: {:?}",
        outbound
    );

    let inbound = run("MATCH (a:Person {name: \"Ada\"})<-[:KNOWS]-(b) RETURN b");
    assert_eq!(
        inbound.len(),
        0,
        "nobody KNOWS Ada, so inbound must be empty: {:?}",
        inbound
    );

    // And the reverse end: Bob is known by exactly one person (Ada).
    let known_by = run("MATCH (a:Person {name: \"Bob\"})<-[:KNOWS]-(b) RETURN b");
    assert_eq!(known_by.len(), 1, "{:?}", known_by);
}

#[test]
fn where_filter_narrows_results() {
    let all = run("MATCH (a:Person) RETURN a");
    assert_eq!(all.len(), 3);

    // ages are 36, 25, 50 — only two are > 30
    let older = run("MATCH (a:Person) WHERE a.age > 30 RETURN a");
    assert_eq!(older.len(), 2, "{:?}", older);

    let younger = run("MATCH (a:Person) WHERE a.age < 30 RETURN a");
    assert_eq!(younger.len(), 1, "{:?}", younger);
}

#[test]
fn not_is_the_exact_complement() {
    let eq = run("MATCH (a:Person) WHERE a.age = 36 RETURN a");
    let neq = run("MATCH (a:Person) WHERE NOT a.age = 36 RETURN a");
    assert_eq!(eq.len(), 1, "{:?}", eq);
    assert_eq!(neq.len(), 2, "{:?}", neq);
}

#[test]
fn de_morgan_disjunction_executes_correctly() {
    // NOT (age = 36 AND age = 25) is true for everyone (nobody is both).
    let rows = run("MATCH (a:Person) WHERE NOT (a.age = 36 AND a.age = 25) RETURN a");
    assert_eq!(rows.len(), 3, "{:?}", rows);

    // NOT (age = 36 OR age = 25) leaves only Cy (50).
    let rows = run("MATCH (a:Person) WHERE NOT (a.age = 36 OR a.age = 25) RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn conjunction_nested_in_disjunction_executes() {
    // (age = 36 AND age < 40) OR age = 50  ->  Ada (36) and Cy (50).
    // This shape was rejected outright until FILTER gained grouping, and
    // parsing is not enough — check it selects two rows, not all three.
    //
    // Numeric comparisons, kept from when this test was written: string
    // equality in FILTER matched nothing back then, so a `name = "Ada"`
    // conjunct made the branch silently dead and the test passed for the wrong
    // reason. That defect is fixed (TODO.md, 2026-07-29) and string conjuncts
    // now work — `conjunction_with_string_conjunct` below covers that. This
    // case stays numeric so it keeps testing grouping alone.
    let rows = run("MATCH (a:Person) WHERE (a.age = 36 AND a.age < 40) OR a.age = 50 RETURN a");
    assert_eq!(rows.len(), 2, "{:?}", rows);

    // The conjunctive branch must actually constrain: nobody is 36 AND over 40,
    // so only the disjunct survives.
    let rows = run("MATCH (a:Person) WHERE (a.age = 36 AND a.age > 40) OR a.age = 50 RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn conjunction_with_string_conjunct() {
    // The shape that first exposed the string-equality defect. It was written
    // as a grouping test, and the `name = "Ada"` branch was silently dead --
    // the row that came back was Cy via the disjunct, not Ada via the
    // conjunction. Both halves work now, so it is worth pinning directly.
    //
    // (age = 36 AND name = "Ada") OR age = 50  ->  Ada and Cy.
    let rows =
        run("MATCH (a:Person) WHERE (a.age = 36 AND a.name = \"Ada\") OR a.age = 50 RETURN a");
    assert_eq!(rows.len(), 2, "{:?}", rows);

    // Nobody is 36 and named "Bob", so the conjunctive branch must contribute
    // nothing and only the disjunct survives. If string equality regressed to
    // always-false this would still pass, so the case above is the real guard;
    // this one checks the conjunct actually constrains.
    let rows =
        run("MATCH (a:Person) WHERE (a.age = 36 AND a.name = \"Bob\") OR a.age = 50 RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);

    // And a string conjunct on its own, which is the direct regression guard:
    // always-false would give 0 here.
    let rows = run("MATCH (a:Person) WHERE a.name = \"Ada\" RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn top_level_and_conjoins_across_separate_filters() {
    // Emitted as two FILTER clauses; must still behave as a conjunction.
    let rows = run("MATCH (a:Person) WHERE a.age > 30 AND a.age < 40 RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn inline_property_selects_one_node() {
    let rows = run("MATCH (a:Person {name: \"Ada\"}) RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn return_property_projects_the_value() {
    let rows = run("MATCH (a:Person {name: \"Ada\"}) RETURN a.age");
    assert_eq!(rows.len(), 1, "{:?}", rows);
    let want = loka_core::inline_integer(36).unwrap();
    assert!(
        rows[0].values().any(|v| *v == want),
        "expected Ada's age 36 among the bindings: {:?}",
        rows
    );
}

#[test]
fn limit_truncates() {
    let rows = run("MATCH (a:Person) RETURN a LIMIT 2");
    assert_eq!(rows.len(), 2, "{:?}", rows);
}

#[test]
fn multi_hop_chain_executes() {
    // ada -KNOWS-> bob -KNOWS-> cy
    let rows = run("MATCH (a)-[:KNOWS]->(b)-[:KNOWS]->(c) RETURN a, c");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn anonymous_node_still_constrains() {
    // Only ada LIVES_IN a City.
    let rows = run("MATCH (a:Person)-[:LIVES_IN]->(:City) RETURN a");
    assert_eq!(rows.len(), 1, "{:?}", rows);
}

#[test]
fn optional_match_keeps_unmatched_rows() {
    // All 3 people are returned even though only ada LIVES_IN somewhere.
    let rows = run("MATCH (a:Person) OPTIONAL MATCH (a)-[:LIVES_IN]->(c) RETURN a, c");
    assert_eq!(rows.len(), 3, "{:?}", rows);
}

/// The values a transpiled query binds `var` to, as IRIs, sorted.
fn bindings(sparql: &str, store: &TripleStore, dict: &TermDictionary, var: &str) -> Vec<String> {
    let q = parse(sparql)
        .unwrap_or_else(|e| panic!("emitted SPARQL did not parse: {e}\n---\n{sparql}"));
    let result = execute(&q, store, dict).unwrap_or_else(|e| panic!("{e}\n---\n{sparql}"));
    let mut out: Vec<String> = result
        .rows
        .iter()
        .map(|row| dict.resolve(row[var]).unwrap_or_default().to_string())
        .collect();
    out.sort();
    out
}

#[test]
fn variable_length_paths_reach_through_a_cycle() {
    // a -> b -> c -> a (a cycle), c -> d. Everything is reachable from a,
    // a itself included (round the cycle); d reaches nothing.
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let knows = dict.intern("http://loka.dev/KNOWS");
    let name = dict.intern("http://loka.dev/name");
    let iri = |n: &str| format!("http://loka.dev/{n}");
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|n| dict.intern(&iri(n)));
    let [na, nd] = ["\"A\"", "\"D\""].map(|n| dict.intern(n));
    for (s, p, o) in [
        (a, knows, b),
        (b, knows, c),
        (c, knows, a),
        (c, knows, d),
        (a, name, na),
        (d, name, nd),
    ] {
        store.insert(Triple::new(s, p, o)).unwrap();
    }
    let all: Vec<String> = ["a", "b", "c", "d"].map(iri).to_vec();

    for sparql in [
        transpile(r#"MATCH (x {name: "A"})-[:KNOWS*]->(y) RETURN DISTINCT y"#).unwrap(),
        transpile(r#"MATCH (x {name: "A"})-[:KNOWS*1..]->(y) RETURN DISTINCT y"#).unwrap(),
        loka_sparql::transpile_gql(r#"MATCH (x {name: "A"})-[:KNOWS]->{1,}(y) RETURN DISTINCT y"#)
            .unwrap(),
        loka_sparql::transpile_gql(r#"MATCH (x {name: "A"})-[:KNOWS]->+(y) RETURN DISTINCT y"#)
            .unwrap(),
    ] {
        assert!(sparql.contains("loka:KNOWS+"), "{sparql}");
        assert_eq!(bindings(&sparql, &store, &dict, "y"), all, "{sparql}");
    }

    // Inbound: who reaches d? a, b and c.
    let sparql = transpile(r#"MATCH (y {name: "D"})<-[:KNOWS*]-(x) RETURN DISTINCT x"#).unwrap();
    assert_eq!(
        bindings(&sparql, &store, &dict, "x"),
        all[..3].to_vec(),
        "{sparql}"
    );

    // Nothing is reachable from d.
    let sparql = transpile(r#"MATCH (x {name: "D"})-[:KNOWS*]->(y) RETURN DISTINCT y"#).unwrap();
    assert!(bindings(&sparql, &store, &dict, "y").is_empty(), "{sparql}");

    // Exactly one step is a plain edge: from a, only b.
    let sparql = transpile(r#"MATCH (x {name: "A"})-[:KNOWS*1..1]->(y) RETURN y"#).unwrap();
    assert_eq!(
        bindings(&sparql, &store, &dict, "y"),
        vec![iri("b")],
        "{sparql}"
    );
}

/// A `+` path with an unbound subject, in plain SPARQL: until 2026-10-11 the
/// executor only walked from a bound subject, so all of these returned nothing.
#[test]
fn plus_path_with_an_unbound_subject() {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let knows = dict.intern("http://loka.dev/KNOWS");
    let name = dict.intern("http://loka.dev/name");
    let iri = |n: &str| format!("http://loka.dev/{n}");
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|n| dict.intern(&iri(n)));
    let nd = dict.intern("\"D\"");
    // a -> b -> c -> a (a cycle), c -> d.
    for (s, p, o) in [
        (a, knows, b),
        (b, knows, c),
        (c, knows, a),
        (c, knows, d),
        (d, name, nd),
    ] {
        store.insert(Triple::new(s, p, o)).unwrap();
    }
    let run = |q: &str| {
        let sparql = format!("PREFIX loka: <http://loka.dev/> {q}");
        execute(&parse(&sparql).unwrap(), &store, &dict)
            .unwrap_or_else(|e| panic!("{q}: {e}"))
            .rows
    };
    let names = |rows: &[std::collections::HashMap<String, loka_core::TermId>], v: &str| {
        let mut out: Vec<String> = rows
            .iter()
            .map(|r| dict.resolve(r[v]).unwrap().to_string())
            .collect();
        out.sort();
        out
    };
    let abc: Vec<String> = ["a", "b", "c"].map(iri).to_vec();

    // Bound object: everyone who reaches d.
    let rows = run("SELECT ?x WHERE { ?x loka:KNOWS+ <http://loka.dev/d> }");
    assert_eq!(names(&rows, "x"), abc);

    // Object bound by an earlier pattern.
    let rows = run(r#"SELECT ?x WHERE { ?y loka:name "D" . ?x loka:KNOWS+ ?y }"#);
    assert_eq!(names(&rows, "x"), abc);

    // Both ends free: a, b and c each reach all four, d reaches nothing.
    let rows = run("SELECT ?x ?y WHERE { ?x loka:KNOWS+ ?y }");
    assert_eq!(rows.len(), 12, "3 starts x 4 targets, each pair once");

    // The same variable at both ends: the nodes on a cycle.
    let rows = run("SELECT ?x WHERE { ?x loka:KNOWS+ ?x }");
    assert_eq!(names(&rows, "x"), abc);
}

/// A bound subject and an object variable bound by an earlier pattern: the
/// path must check the object, not overwrite it. (Before 2026-10-11 it
/// overwrote, so this returned a row per reachable node.)
#[test]
fn plus_path_checks_an_already_bound_object() {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let knows = dict.intern("http://loka.dev/KNOWS");
    let name = dict.intern("http://loka.dev/name");
    let [a, b, c] = ["a", "b", "c"].map(|n| dict.intern(&format!("http://loka.dev/{n}")));
    let nc = dict.intern("\"C\"");
    for (s, p, o) in [(a, knows, b), (b, knows, c), (c, name, nc)] {
        store.insert(Triple::new(s, p, o)).unwrap();
    }
    let sparql = r#"PREFIX loka: <http://loka.dev/>
        SELECT ?y WHERE { ?y loka:name "C" . <http://loka.dev/a> loka:KNOWS+ ?y }"#;
    let rows = execute(&parse(sparql).unwrap(), &store, &dict)
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["y"], c);
}
