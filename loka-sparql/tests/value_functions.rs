//! SPARQL value functions (`STR`, `LCASE`, `UCASE`, `STRLEN`, `REPLACE`,
//! `CONCAT`), their nesting, and the query shapes the engine's own consumer
//! sends.
//!
//! The second half of this file is the important half. Pramana — the ERP-for-agents
//! store that runs on Loka — sends these queries over HTTP for its entity page,
//! its search box and its entity resolver. **Five of nine distinct shapes failed
//! to parse**, and its client turns a failed query into an empty result, so the
//! pages rendered blank instead of reporting an error. Nothing in Loka's own test
//! suite noticed, because the suite only tested the shapes Loka's author thought
//! to write.
//!
//! Keeping the consumer's real shapes in here is the cheap fix for that class of
//! blindness: if a change breaks what Pramana actually sends, this file fails.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

fn labelled() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let label = dict.intern("http://pramana.org/prop/direct/EntityLabel");
    let uuid = dict.intern("http://pramana.org/ontology/uuid");
    let rdf_type = dict.intern("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let entity = dict.intern("http://pramana.org/ontology/Entity");

    for (local, lbl, id) in [
        (
            "water",
            "\"Water\"",
            "\"a5bc6b47-e25e-4262-9416-64bfcc00d5f3\"",
        ),
        (
            "iron",
            "\"Iron\"",
            "\"b1cd7c58-f36f-5373-a527-75cadd11e6e4\"",
        ),
    ] {
        let s = dict.intern(&format!("http://pramana.org/entity/{}", local));
        store
            .insert(Triple::new(s, label, dict.intern(lbl)))
            .unwrap();
        store.insert(Triple::new(s, uuid, dict.intern(id))).unwrap();
        store.insert(Triple::new(s, rdf_type, entity)).unwrap();
    }
    (store, dict)
}

fn rows(filter: &str) -> usize {
    let (store, dict) = labelled();
    let q = format!(
        "PREFIX pr: <http://pramana.org/ontology/> \
         PREFIX wdt: <http://pramana.org/prop/direct/> \
         SELECT ?item WHERE {{ ?item wdt:EntityLabel ?label . ?item pr:uuid ?uuid . \
         FILTER({}) }}",
        filter
    );
    let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
    execute(&parsed, &store, &dict)
        .unwrap_or_else(|e| panic!("did not execute: {}\n{}", e, q))
        .rows
        .len()
}

#[test]
fn case_functions() {
    assert_eq!(rows(r#"LCASE(?label) = "water""#), 1);
    assert_eq!(rows(r#"UCASE(?label) = "WATER""#), 1);
    // The un-cased comparison does not match, which is the point of LCASE.
    assert_eq!(rows(r#"?label = "water""#), 0);
    assert_eq!(rows(r#"LCASE(?label) = "Water""#), 0);
}

#[test]
fn functions_nest() {
    // Every one of these was "expected prefixed name (prefix:local)" before
    // arguments were parsed as expressions.
    assert_eq!(rows(r#"LCASE(STR(?label)) = "water""#), 1);
    assert_eq!(rows(r#"UCASE(LCASE(STR(?label))) = "WATER""#), 1);
    assert_eq!(rows(r#"CONTAINS(LCASE(?label), LCASE("WAT"))"#), 1);
    assert_eq!(rows(r#"STRSTARTS(LCASE(STR(?label)), "wa")"#), 1);
    assert_eq!(rows(r#"STRENDS(LCASE(?label), "ter")"#), 1);
}

#[test]
fn strlen_is_numeric_and_composes_with_arithmetic() {
    assert_eq!(rows("STRLEN(?label) = 5"), 1); // "Water"
    assert_eq!(rows("STRLEN(?label) = 4"), 1); // "Iron"
    assert_eq!(rows("STRLEN(?label) > 4"), 1);
    assert_eq!(rows("STRLEN(?label) + 1 = 6"), 1);
    assert_eq!(rows("STRLEN(?label) < 10"), 2);
    // A string is NOT coerced to a number, so this is not a numeric comparison
    // that happens to work — it matches nothing rather than guessing.
    assert_eq!(rows("STR(?label) > 4"), 0);
}

#[test]
fn replace_is_a_real_regex() {
    assert_eq!(rows(r#"REPLACE(?label, "^W", "X") = "Xater""#), 1);
    assert_eq!(
        rows(r#"REPLACE(?uuid, "-", "") = "a5bc6b47e25e4262941664bfcc00d5f3""#),
        1
    );
    // Anchoring matters: "r$" hits Water but not Iron.
    assert_eq!(rows(r#"REPLACE(?label, "r$", "!") = "Wate!""#), 1);
    // Capture groups.
    assert_eq!(rows(r#"REPLACE(?label, "^(W)(a)", "$2$1") = "aWter""#), 1);
    // An invalid pattern has no value, so the comparison is false — not a panic.
    assert_eq!(rows(r#"REPLACE(?label, "[", "x") = "Water""#), 0);
}

#[test]
fn regex_filter_actually_anchors() {
    // REGEX used to be a substring match with a comment saying real regex "would
    // need a regex crate", so an anchored pattern silently matched by containment.
    assert_eq!(rows(r#"REGEX(?label, "^Wat")"#), 1);
    assert_eq!(rows(r#"REGEX(?label, "^ater")"#), 0); // substring match would say 1
    assert_eq!(rows(r#"REGEX(?label, "r$")"#), 1);
    assert_eq!(rows(r#"REGEX(?label, "^(Water|Iron)$")"#), 2);
    assert_eq!(rows(r#"REGEX(?label, "[0-9]")"#), 0);
}

#[test]
fn concat() {
    assert_eq!(rows(r#"CONCAT(?label, "!") = "Water!""#), 1);
    assert_eq!(
        rows(r#"CONCAT(LCASE(?label), "-", UCASE(?label)) = "water-WATER""#),
        1
    );
}

#[test]
fn arity_is_checked() {
    for bad in [
        "SELECT ?s WHERE { ?s ?p ?o . FILTER(LCASE(?o, ?o) = \"x\") }",
        "SELECT ?s WHERE { ?s ?p ?o . FILTER(REPLACE(?o, \"a\") = \"x\") }",
        "SELECT ?s WHERE { ?s ?p ?o . FILTER(STR() = \"x\") }",
    ] {
        assert!(parse(bad).is_err(), "should not parse: {}", bad);
    }
}

// ---------------------------------------------------------------------------
// The consumer's real query shapes (Pramana, over HTTP)
// ---------------------------------------------------------------------------

/// Verbatim shapes from Pramana's `web/data_access.py`, `src/entity_resolver.py`,
/// `src/entity_detail.py` and `src/query_knowledge_graph.py`, with the
/// interpolated values filled in. Parse-level assertion: the failure being
/// pinned was a parse error that blanked whole pages.
#[test]
fn pramana_live_query_shapes_parse() {
    let prefixes = "PREFIX pr: <http://pramana.org/ontology/> \
                    PREFIX wdt: <http://pramana.org/prop/direct/> \
                    PREFIX wd: <http://pramana.org/entity/> ";
    let queries = [
        // entity_resolver: exact label match, case-insensitive
        r#"SELECT ?item WHERE { ?item wdt:EntityLabel ?label . FILTER(LCASE(STR(?label)) = LCASE("Water")) }"#,
        // data_access: search box
        r#"SELECT ?item WHERE { ?item wdt:EntityLabel ?label . FILTER(CONTAINS(LCASE(?label), LCASE("wat"))) }"#,
        // entity_detail: uuid prefix lookup with dashes stripped
        r#"SELECT ?item WHERE { ?item pr:uuid ?uuid . FILTER(STRSTARTS(REPLACE(STR(?uuid), "-", ""), "a5bc")) }"#,
        // query_knowledge_graph: direct-property filter
        r#"SELECT ?property WHERE { ?s ?property ?o . FILTER(STRSTARTS(STR(?property), "http://pramana.org/prop/direct/")) }"#,
        // full_ontology_tree: exclude propositions
        r#"SELECT ?item WHERE { ?item a ?t . FILTER NOT EXISTS { ?item a pr:Proposition } }"#,
        // data_access: literal-only objects
        r#"SELECT ?obj WHERE { ?s ?p ?obj . FILTER(isLiteral(?obj)) }"#,
        // data_access: exclude self by uuid
        r#"SELECT ?targetUuid WHERE { ?s pr:uuid ?targetUuid . FILTER(?targetUuid != "a5bc") }"#,
        // data_access: type filter
        r#"SELECT ?type WHERE { ?s a ?type . FILTER(?type != pr:Proposition) }"#,
        // entity page: BIND over a computed value
        r#"SELECT ?item ?typeLocal WHERE { ?item a ?type . BIND(REPLACE(STR(?type), "^.*/", "") AS ?typeLocal) }"#,
    ];
    for q in queries {
        let full = format!("{}{}", prefixes, q);
        parse(&full).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, full));
    }
}

/// The consumer's search and resolver queries must also RETURN the right rows,
/// not merely parse.
#[test]
fn pramana_search_and_resolver_return_rows() {
    let (store, dict) = labelled();
    let prefixes = "PREFIX pr: <http://pramana.org/ontology/> \
                    PREFIX wdt: <http://pramana.org/prop/direct/> ";
    for (q, expected) in [
        (
            r#"SELECT ?item WHERE { ?item wdt:EntityLabel ?label . FILTER(LCASE(STR(?label)) = LCASE("water")) }"#,
            1,
        ),
        (
            r#"SELECT ?item WHERE { ?item wdt:EntityLabel ?label . FILTER(CONTAINS(LCASE(?label), LCASE("O"))) }"#,
            1, // lower-cased: "iron" contains an "o", "water" does not
        ),
        (
            r#"SELECT ?item WHERE { ?item pr:uuid ?uuid . FILTER(STRSTARTS(REPLACE(STR(?uuid), "-", ""), "a5bc6b47e25e")) }"#,
            1,
        ),
    ] {
        let full = format!("{}{}", prefixes, q);
        let parsed = parse(&full).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, full));
        let got = execute(&parsed, &store, &dict).unwrap().rows.len();
        assert_eq!(got, expected, "{}", full);
    }
}

/// BIND over a computed expression — numeric and string alike.
///
/// The string case was an explicit "not supported yet" error until the per-query
/// value table landed (`planning/computed-values.md`): a computed string has no
/// dictionary id, and the dictionary is persisted and held immutably, so it could
/// not be interned there.
#[test]
fn bind_over_a_computed_value() {
    let (store, dict) = labelled();
    let prefixes = "PREFIX pr: <http://pramana.org/ontology/>                     PREFIX wdt: <http://pramana.org/prop/direct/> ";

    let numeric = format!(
        "{}SELECT ?item ?n WHERE {{ ?item wdt:EntityLabel ?label .          BIND(STRLEN(?label) + 1 AS ?n) }}",
        prefixes
    );
    let parsed = parse(&numeric).unwrap();
    let result = execute(&parsed, &store, &dict).unwrap();
    assert_eq!(result.rows.len(), 2);
    assert!(
        result.rows.iter().all(|r| r.contains_key("n")),
        "numeric BIND must actually bind"
    );

    // The query Pramana's entity page has always wanted to send.
    let stringy = format!(
        "{}SELECT ?item ?local WHERE {{ ?item a ?type .          BIND(REPLACE(STR(?type), \"^.*/\", \"\") AS ?local) }}",
        prefixes
    );
    let parsed = parse(&stringy).unwrap();
    let result = execute(&parsed, &store, &dict).unwrap();
    assert_eq!(result.rows.len(), 2);
    for row in &result.rows {
        let id = *row.get("local").expect("?local must be bound");
        // Resolving through the RESULT's value table, not the dictionary — a
        // renderer that consults only the dictionary shows an empty cell, which
        // is the trap stage 3 has to close in every output format.
        assert_eq!(result.values.get(id), Some("Entity"));
        assert_eq!(dict.resolve(id), None);
    }
}

/// Two rows computing the same string share one id, so a computed variable can
/// be grouped and de-duplicated. Without by-value interning this is the subtle
/// bug: equal values that compare unequal.
#[test]
fn equal_computed_values_share_an_id() {
    let (store, dict) = labelled();
    let q = "PREFIX pr: <http://pramana.org/ontology/>              SELECT ?item ?local WHERE { ?item a ?type .              BIND(REPLACE(STR(?type), \"^.*/\", \"\") AS ?local) }";
    let parsed = parse(q).unwrap();
    let result = execute(&parsed, &store, &dict).unwrap();

    let ids: std::collections::HashSet<_> = result
        .rows
        .iter()
        .map(|r| *r.get("local").unwrap())
        .collect();
    assert_eq!(ids.len(), 1, "both rows compute \"Entity\"");
    assert_eq!(result.values.len(), 1);
}

/// A query that computes nothing carries an empty table — the field is not a
/// per-query allocation tax on ordinary queries.
#[test]
fn a_query_with_no_computed_values_has_an_empty_table() {
    let (store, dict) = labelled();
    let q = "PREFIX wdt: <http://pramana.org/prop/direct/>              SELECT ?item WHERE { ?item wdt:EntityLabel ?label }";
    let result = execute(&parse(q).unwrap(), &store, &dict).unwrap();
    assert!(result.values.is_empty());
}

/// Non-integral arithmetic still has no representation (no inline float), so the
/// variable is unbound rather than rounded. Pinned so adding InlineType::Float
/// has to confront it.
#[test]
fn bind_over_a_non_integral_number_leaves_the_variable_unbound() {
    let (store, dict) = labelled();
    let q = "PREFIX wdt: <http://pramana.org/prop/direct/>              SELECT ?item ?half WHERE { ?item wdt:EntityLabel ?label .              BIND(STRLEN(?label) / 2 AS ?half) }";
    let result = execute(&parse(q).unwrap(), &store, &dict).unwrap();
    // "Water" is 5 chars -> 2.5, unrepresentable; "Iron" is 4 -> 2, fine.
    let bound = result
        .rows
        .iter()
        .filter(|r| r.contains_key("half"))
        .count();
    assert_eq!(bound, 1, "only the even-length label binds");
}
