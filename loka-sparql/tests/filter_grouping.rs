//! Parenthesised grouping in FILTER expressions.
//!
//! Every query here was a **parse error** before grouping was added, which is
//! what makes the change additive: no query that parsed previously reaches the
//! new branch. These tests pin both halves — that the shapes now parse, and
//! that they evaluate to the right rows.

use loka_core::{TermDictionary, Triple, TripleStore};
use loka_sparql::{execute, parse};

/// Three people with distinct ages, so each boolean shape selects a different
/// subset and a mis-parse shows up as a row count rather than passing silently.
fn people() -> (TripleStore, TermDictionary) {
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();

    let age = dict.intern("http://example.org/age");
    let rank = dict.intern("http://example.org/rank");
    let a = dict.intern("http://example.org/a");
    let b = dict.intern("http://example.org/b");
    let c = dict.intern("http://example.org/c");

    for (s, ageval, rankval) in [(a, 10, 1), (b, 20, 2), (c, 30, 3)] {
        store
            .insert(Triple::new(
                s,
                age,
                loka_core::inline_integer(ageval).unwrap(),
            ))
            .unwrap();
        store
            .insert(Triple::new(
                s,
                rank,
                loka_core::inline_integer(rankval).unwrap(),
            ))
            .unwrap();
    }
    (store, dict)
}

fn rows(filter: &str) -> usize {
    let (store, dict) = people();
    let q = format!(
        "PREFIX ex: <http://example.org/> \
         SELECT ?s WHERE {{ ?s ex:age ?age . ?s ex:rank ?rank . FILTER({}) }}",
        filter
    );
    let parsed = parse(&q).unwrap_or_else(|e| panic!("did not parse: {}\n{}", e, q));
    execute(&parsed, &store, &dict)
        .unwrap_or_else(|e| panic!("did not execute: {}\n{}", e, q))
        .rows
        .len()
}

#[test]
fn grouped_operands_parse() {
    // The canonical shape a generator emits. Previously "expected term".
    assert_eq!(rows("(?age = 10) && (?rank = 1)"), 1);
    assert_eq!(rows("(?age = 10) || (?age = 30)"), 2);
}

#[test]
fn group_on_the_right_of_a_conjunction() {
    // a && (b || c) — the shape that motivated this.
    assert_eq!(rows("?rank > 0 && (?age = 10 || ?age = 30)"), 2);
    assert_eq!(rows("?age > 15 && (?rank = 2 || ?rank = 99)"), 1);
}

#[test]
fn conjunction_nested_in_a_disjunction() {
    // (a && b) || c — not expressible in the flat chain at all, because the
    // chain always associates to the right. This is the case the Cypher
    // transpiler still rejects and points at DNF.
    assert_eq!(rows("(?age = 10 && ?rank = 1) || ?age = 30"), 2);
    // Both branches false for everyone.
    assert_eq!(rows("(?age = 10 && ?rank = 99) || ?age = 77"), 0);
}

#[test]
fn negation_of_a_group() {
    // !(...) with a real expression inside, not just !bound().
    assert_eq!(rows("!(?age = 10)"), 2);
    assert_eq!(rows("!(?age = 10 || ?age = 20)"), 1);
}

#[test]
fn nested_groups() {
    assert_eq!(rows("((?age = 10))"), 1);
    assert_eq!(rows("(?rank > 0) && ((?age = 20) || (?age = 30))"), 2);
}

#[test]
fn ungrouped_chain_still_behaves_as_before() {
    // Regression guard on the untouched path: the flat chain is unchanged.
    assert_eq!(rows("?age > 5"), 3);
    assert_eq!(rows("?age > 5 && ?age < 25"), 2);
    assert_eq!(rows("?age = 10 || ?age = 20"), 2);
    assert_eq!(rows("bound(?age)"), 3);
    assert_eq!(rows("!bound(?nope)"), 3);
}

#[test]
fn chains_of_three_or_more_terms() {
    // Any filter with 3+ terms was a parse error: the chain accepted exactly
    // one `&&`/`||` continuation and then demanded `)`.
    assert_eq!(rows("?age > 5 && ?age < 60 && ?rank > 0"), 3);
    assert_eq!(rows("?age = 10 || ?age = 20 || ?age = 30"), 3);
    assert_eq!(rows("?age = 10 || ?age = 20 || ?age = 99"), 2);
    assert_eq!(rows("?age > 5 && ?age < 60 && ?rank > 0 && ?age != 20"), 2);
}

#[test]
fn mixed_connectives_in_a_long_chain() {
    // `a && b || c` associates left, which for this shape is (a && b) || c —
    // what SPARQL means.
    assert_eq!(rows("?age = 10 && ?rank = 1 || ?age = 30"), 2);
    assert_eq!(rows("?age = 10 && ?rank = 99 || ?age = 30"), 1);
}

#[test]
fn precedence_binds_and_tighter_than_or() {
    // SPARQL 1.1: `&&` binds tighter than `||`, so `a || b && c` means
    // `a || (b && c)`. This test replaces one that PINNED the opposite —
    // the parser used to run a single left-associative loop over both
    // connectives, reading it as `(a || b) && c`, which is a different
    // predicate and so returned different rows.
    //
    // The case is chosen so the two readings disagree. ages 10/20/30,
    // ranks 1/2/3:
    //   (age=10 || age=20) && rank=2   -> age-20 row only      = 1  (old, wrong)
    //   age=10 || (age=20 && rank=2)   -> age-10 + age-20 rows = 2  (SPARQL)
    assert_eq!(rows("?age = 10 || ?age = 20 && ?rank = 2"), 2);
    // Explicit parens agree with the implicit reading...
    assert_eq!(rows("?age = 10 || (?age = 20 && ?rank = 2)"), 2);
    // ...and the other grouping is still expressible, and still differs.
    assert_eq!(rows("(?age = 10 || ?age = 20) && ?rank = 2"), 1);

    // Same on the other side of the `||`: `a && b || c` was already correct
    // under left association, and must stay correct under precedence.
    assert_eq!(rows("?age = 20 && ?rank = 2 || ?age = 10"), 2);
    assert_eq!(rows("?age = 20 && ?rank = 99 || ?age = 10"), 1);

    // Two `&&` groups around one `||` — neither end anchors the parse.
    //   (age>5 && age<15) || (age>25 && rank=3)  -> age-10 + age-30 = 2
    assert_eq!(rows("?age > 5 && ?age < 15 || ?age > 25 && ?rank = 3"), 2);
    assert_eq!(
        rows("?age > 5 && ?age < 15 || ?age > 25 && ?rank = 3"),
        rows("(?age > 5 && ?age < 15) || (?age > 25 && ?rank = 3)")
    );
    // Under the old left-to-right reading that same string was
    // `((((age>5 && age<15) || age>25) && rank=3))` -> only the age-30 row.
    assert_ne!(rows("?age > 5 && ?age < 15 || ?age > 25 && ?rank = 3"), 1);
}

#[test]
fn bound_and_negation_compose_in_a_chain() {
    // These worked as an entire filter but not as the left operand of a chain,
    // because each consumed FILTER's own closing paren before returning.
    assert_eq!(rows("bound(?age) && ?age > 15"), 2);
    assert_eq!(rows("!bound(?nope) && ?age > 15"), 2);
    assert_eq!(rows("!(?age = 10) && ?age < 30"), 1);
    // And still work in trailing position, which always did.
    assert_eq!(rows("?age > 15 && bound(?age)"), 2);
}

#[test]
fn string_functions_compose_in_a_chain() {
    // These worked only as an entire filter; each consumed FILTER's own closing
    // paren, so using one as an operand was a parse error.
    //
    // The store has names via ex:name on the sibling fixture, but this fixture
    // is numeric-only, so these assert on parse-and-execute reaching zero rows
    // rather than on matches — the point is that they are no longer rejected.
    for f in [
        r#"CONTAINS(?s, "nothing") && ?age > 5"#,
        r#"?age > 5 && CONTAINS(?s, "nothing")"#,
        r#"STRSTARTS(?s, "no") || STRENDS(?s, "pe")"#,
        r#"REGEX(?s, "^zzz") && bound(?age)"#,
        "isIRI(?s) && isLiteral(?age)",
        r#"!(CONTAINS(?s, "nothing")) && ?age > 5"#,
    ] {
        // Must not panic: the helper unwraps parse and execute.
        let _ = rows(f);
    }

    // isIRI is true for all three subjects, so it composes with a real filter
    // and narrows exactly as the conjunction implies.
    assert_eq!(rows("isIRI(?s) && ?age > 15"), 2);
    assert_eq!(rows("isIRI(?s) || ?age > 99"), 3);
}

#[test]
fn grouping_matches_the_equivalent_flat_query() {
    // Adding redundant parens around a flat chain must not change the result.
    assert_eq!(
        rows("?age > 5 && ?age < 25"),
        rows("(?age > 5) && (?age < 25)")
    );
    assert_eq!(
        rows("?age = 10 || ?age = 30"),
        rows("(?age = 10) || (?age = 30)")
    );
}
