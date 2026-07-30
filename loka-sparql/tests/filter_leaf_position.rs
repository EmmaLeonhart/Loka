//! Every FILTER leaf form composes in any position, not just as the whole filter.
//!
//! `LANGMATCHES`, `LANG(?v) =`, `COALESCE`, `IF`, `DATATYPE(?v) =`, `STR(?v) =`
//! and the parenthesised `EXISTS` / `NOT EXISTS` used to be parsed in
//! `parse_filter`, where each consumed FILTER's *own* closing paren before
//! returning. That made every one of them work as an entire filter and fail as
//! an operand:
//!
//! ```text
//! FILTER(STR(?name) = "Ada")               ok
//! FILTER(STR(?name) = "Ada" && ?age > 5)   "expected ')', got '&'"
//! ```
//!
//! They now live in `parse_filter_inner`. These tests assert **row counts**, not
//! parse success: a filter branch that silently matches nothing also parses, and
//! that is exactly how the string-equality defect (TODO.md, 2026-07-29) hid.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

/// Three subjects with a name, a language-tagged label and an integer age, so a
/// mis-parse or a dead branch shows up as a row count.
fn fixture() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();

    let name = dict.intern("http://example.org/name");
    let label = dict.intern("http://example.org/label");
    let age = dict.intern("http://example.org/age");

    for (iri, nm, lbl, ageval) in [
        ("http://example.org/a", "\"Ada\"", "\"Ada\"@en", 10),
        ("http://example.org/b", "\"Bo\"", "\"Bo\"@en", 20),
        ("http://example.org/c", "\"Cy\"", "\"Cy\"@fr", 30),
    ] {
        let s = dict.intern(iri);
        store.insert(Triple::new(s, name, dict.intern(nm))).unwrap();
        store
            .insert(Triple::new(s, label, dict.intern(lbl)))
            .unwrap();
        store
            .insert(Triple::new(
                s,
                age,
                loka_core::inline_integer(ageval).unwrap(),
            ))
            .unwrap();
    }
    (store, dict)
}

fn rows(filter: &str) -> usize {
    let (store, dict) = fixture();
    let q = format!(
        "PREFIX ex: <http://example.org/> \
         SELECT ?s WHERE {{ ?s ex:name ?name . ?s ex:label ?lbl . ?s ex:age ?age . \
         FILTER({}) }}",
        filter
    );
    let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
    execute(&parsed, &store, &dict)
        .unwrap_or_else(|e| panic!("did not execute: {}\n{}", e, q))
        .rows
        .len()
}

/// Each form on its own — the position that always worked. Establishes what the
/// form actually selects, so the composed assertions below mean something.
#[test]
fn leaf_forms_alone_select_what_is_expected() {
    assert_eq!(rows(r#"STR(?name) = "Ada""#), 1);
    assert_eq!(rows(r#"LANG(?lbl) = "en""#), 2);
    assert_eq!(rows(r#"LANGMATCHES(LANG(?lbl), "en")"#), 2);
    assert_eq!(rows("DATATYPE(?age) = xsd:integer"), 3);
    assert_eq!(rows("COALESCE(?age)"), 3);
    assert_eq!(rows("COALESCE(?nope, ?age)"), 3);
    assert_eq!(rows("COALESCE(?nope)"), 0);
}

#[test]
fn str_comparison_composes_in_either_position() {
    assert_eq!(rows(r#"STR(?name) = "Ada" && ?age > 5"#), 1);
    assert_eq!(rows(r#"?age > 5 && STR(?name) = "Ada""#), 1);
    assert_eq!(rows(r#"STR(?name) = "Ada" || STR(?name) = "Cy""#), 2);
    // Grouped, and narrowed by a conjunct that excludes one branch.
    assert_eq!(
        rows(r#"(STR(?name) = "Ada" || STR(?name) = "Cy") && ?age > 25"#),
        1
    );
}

#[test]
fn lang_forms_compose_in_either_position() {
    assert_eq!(rows(r#"LANG(?lbl) = "en" && ?age > 15"#), 1);
    assert_eq!(rows(r#"?age > 15 && LANG(?lbl) = "en""#), 1);
    assert_eq!(rows(r#"LANGMATCHES(LANG(?lbl), "en") && ?age > 15"#), 1);
    assert_eq!(rows(r#"?age > 15 && LANGMATCHES(LANG(?lbl), "en")"#), 1);
    assert_eq!(rows(r#"LANGMATCHES(LANG(?lbl), "fr") || ?age = 10"#), 2);
    // Negated, which requires it to be reachable as an operand of `!`.
    assert_eq!(rows(r#"!LANGMATCHES(LANG(?lbl), "en")"#), 1);
}

#[test]
fn datatype_and_coalesce_compose_in_either_position() {
    assert_eq!(rows("DATATYPE(?age) = xsd:integer && ?age > 15"), 2);
    assert_eq!(rows("?age > 15 && DATATYPE(?age) = xsd:integer"), 2);
    assert_eq!(rows("COALESCE(?age) && ?age > 15"), 2);
    assert_eq!(rows("?age > 15 && COALESCE(?nope, ?age)"), 2);
}

#[test]
fn if_composes_in_either_position() {
    // In FILTER context IF reduces to its condition; both then/else are skipped.
    assert_eq!(rows("IF(?age > 15, 1, 0)"), 2);
    assert_eq!(rows("IF(?age > 15, 1, 0) && ?age < 25"), 1);
    assert_eq!(rows("?age < 25 && IF(?age > 15, 1, 0)"), 1);
}

#[test]
fn exists_composes_in_either_position() {
    assert_eq!(rows("EXISTS { ?s ex:age ?a }"), 3);
    assert_eq!(rows("NOT EXISTS { ?s ex:missing ?x }"), 3);
    assert_eq!(rows("EXISTS { ?s ex:age ?a } && ?age > 15"), 2);
    assert_eq!(rows("?age > 15 && EXISTS { ?s ex:age ?a }"), 2);
    assert_eq!(rows("NOT EXISTS { ?s ex:missing ?x } && ?age > 15"), 2);
    assert_eq!(rows("?age > 15 && NOT EXISTS { ?s ex:missing ?x }"), 2);
    assert_eq!(rows("EXISTS { ?s ex:missing ?x } || ?age > 15"), 2);
}

/// The unparenthesised form is still parsed by `parse_filter` before FILTER's
/// `(` is consumed, and must keep working.
#[test]
fn unparenthesised_exists_still_parses() {
    let (store, dict) = fixture();
    for (body, expected) in [
        ("FILTER NOT EXISTS { ?s ex:missing ?x }", 3),
        ("FILTER EXISTS { ?s ex:age ?a }", 3),
    ] {
        let q = format!(
            "PREFIX ex: <http://example.org/> SELECT ?s WHERE {{ ?s ex:name ?name . {} }}",
            body
        );
        let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
        let got = execute(&parsed, &store, &dict).unwrap().rows.len();
        assert_eq!(got, expected, "{}", q);
    }
}

/// `peek_function` requires a `(` after the keyword, so a prefixed name that
/// merely starts with one of these keywords is still a term.
///
/// This matters *because* the forms moved: `peek_keyword("STR")` matches
/// `str:label` (`:` is not a word character), and that branch is now reachable
/// in operand position, where it would demand a `(` and reject a valid query.
#[test]
fn a_prefixed_name_starting_with_a_function_keyword_is_not_a_call() {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let p = dict.intern("http://example.org/str/label");
    let s = dict.intern("http://example.org/a");
    store
        .insert(Triple::new(s, p, dict.intern("\"x\"")))
        .unwrap();

    // `str:label` as the LEFT operand of a chained comparison — the position
    // that reaches the STR branch. An interned term, not an invented one: an
    // unresolvable term compares false by design (see filter_equality.rs), so
    // an invented IRI would make this assert 0 rows for an unrelated reason.
    let q = "PREFIX str: <http://example.org/str/> \
             SELECT ?s WHERE { ?s str:label ?l . \
             FILTER(?l = \"x\" && str:label != ?s) }";
    let parsed = parse(q).unwrap_or_else(|e| panic!("did not parse: {}", e));
    assert_eq!(execute(&parsed, &store, &dict).unwrap().rows.len(), 1);

    // Same for `lang:` and `if:`, the other short keywords.
    for prefix in ["lang", "if", "datatype", "coalesce"] {
        let q = format!(
            "PREFIX {p}: <http://example.org/{p}/> PREFIX str: <http://example.org/str/> \
             SELECT ?s WHERE {{ ?s str:label ?l . FILTER(?l = \"x\" && {p}:nobody != ?s) }}",
            p = prefix
        );
        parse(&q).unwrap_or_else(|e| panic!("did not parse ({}): {}", prefix, e));
    }
}
