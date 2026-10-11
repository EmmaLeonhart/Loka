//! GQL (ISO/IEC 39075) → SPARQL, a first subset.
//!
//! GQL's read core (`MATCH … WHERE … RETURN … ORDER BY … LIMIT`) is the same
//! language as the Cypher subset in [`crate::cypher`], apart from a few surface
//! forms. This module rewrites those forms to their Cypher equivalents and
//! hands the result to the Cypher transpiler, so both share one mapping onto
//! RDF and one rejection discipline:
//!
//! | GQL | Cypher it becomes |
//! |---|---|
//! | `(a IS Person)` | `(a:Person)` |
//! | `(a IS Person & Employee)`, `(a :Person & Employee)` | `(a:Person:Employee)` |
//! | `FILTER cond` | `WHERE cond` |
//! | `OFFSET n` | `SKIP n` |
//! | `-[:T]->+`, `-[:T]->{1,}` | `-[:T*1..]->` |
//! | `-[:T]->{m,n}`, `{n}`, `{,n}`, `*` | `-[:T*m..n]->`, `*n..n`, `*0..n`, `*0..` |
//!
//! A quantifier after an edge pattern moves inside its brackets, where the
//! Cypher back end decides what has a faithful mapping: `{1,}` (and `+`)
//! becomes a SPARQL+ `+` path under `RETURN DISTINCT`, `{1}` a plain edge, and
//! the zero-length and bounded forms are rejected with the back end's reason.
//!
//! `IS NULL`, `IS NOT …` and the other `IS` predicates are left alone; only
//! `IS <label>` is a label test. (The Cypher back end doesn't support
//! `IS NULL` yet, so those queries fail there as their Cypher form would.)
//! String literals are copied untouched.
//!
//! Rejected with a reason instead of approximated: label disjunction `|`,
//! negation `!` and wildcard `%`; a quantifier on a bracketless edge (`->+`)
//! or a parenthesised path (`(…){1,}`); graph selection (`USE`) and the GQL statements outside a single read query
//! (`INSERT`, `LET`, `FOR`, `NEXT`, `CALL`, `YIELD`, `FINISH`, `SELECT`).
//! Errors from the shared back end are [`CypherError`]s.

use crate::cypher::{transpile_with_base, CypherError, DEFAULT_BASE};

/// Words after `IS` that make it a predicate (`IS NULL`, `IS NOT …`, …), not
/// a label test.
const IS_PREDICATE_WORDS: &[&str] = &[
    "NULL",
    "NOT",
    "TRUE",
    "FALSE",
    "UNKNOWN",
    "LABELED",
    "DIRECTED",
    "SOURCE",
    "DESTINATION",
    "NORMALIZED",
    "TYPED",
];

/// GQL statements outside the supported single read query.
const UNSUPPORTED_STATEMENTS: &[(&str, &str)] = &[
    ("USE", "graph selection: Loka has one graph per store"),
    (
        "INSERT",
        "this is a read path; use SPARQL INSERT DATA to write",
    ),
    (
        "LET",
        "GQL variable definitions are not in the supported subset",
    ),
    ("FOR", "GQL FOR statements are not in the supported subset"),
    (
        "NEXT",
        "GQL linear composition (NEXT) is not in the supported subset",
    ),
    ("CALL", "procedure calls are not in the supported subset"),
    ("YIELD", "procedure calls are not in the supported subset"),
    ("FINISH", "FINISH is not in the supported subset"),
    (
        "SELECT",
        "GQL SELECT is not in the supported subset; use RETURN",
    ),
];

/// Transpile GQL into SPARQL, with labels, edge types and property keys in
/// the default `http://loka.dev/` namespace.
pub fn transpile_gql(gql: &str) -> Result<String, CypherError> {
    transpile_gql_with_base(gql, DEFAULT_BASE)
}

/// [`transpile_gql`] with a chosen namespace for bare names.
pub fn transpile_gql_with_base(gql: &str, base: &str) -> Result<String, CypherError> {
    transpile_with_base(&normalize(gql)?, base)
}

fn unsupported(construct: &str, reason: &'static str) -> CypherError {
    CypherError::Unsupported {
        construct: construct.to_string(),
        reason,
    }
}

/// Parse the edge quantifier starting at `chars[i]` (`+`, `*`, `{m,n}`,
/// `{m,}`, `{,n}`, `{n}`) into Cypher's `*m..n` form, returning the index
/// after it.
fn quantifier(chars: &[char], i: usize) -> Result<(String, usize), CypherError> {
    match chars[i] {
        '+' => return Ok(("*1..".into(), i + 1)),
        '*' => return Ok(("*0..".into(), i + 1)),
        _ => {}
    }
    let close = chars[i..]
        .iter()
        .position(|&c| c == '}')
        .map(|p| i + p)
        .ok_or_else(|| CypherError::Syntax {
            position: i,
            message: "unterminated path quantifier".into(),
        })?;
    let body: String = chars[i + 1..close]
        .iter()
        .filter(|c| !c.is_whitespace())
        .collect();
    let bad = || CypherError::Syntax {
        position: i,
        message: format!("expected a path quantifier {{m,n}}, found {{{}}}", body),
    };
    let num = |t: &str| -> Result<Option<u64>, CypherError> {
        if t.is_empty() {
            Ok(None)
        } else {
            t.parse::<u64>().map(Some).map_err(|_| bad())
        }
    };
    let cypher = match body.split_once(',') {
        None => {
            let n = num(&body)?.ok_or_else(bad)?;
            format!("*{n}..{n}")
        }
        Some((lo, hi)) => {
            let lo = num(lo)?.unwrap_or(0);
            match num(hi)? {
                Some(hi) => format!("*{lo}..{hi}"),
                None => format!("*{lo}.."),
            }
        }
    };
    Ok((cypher, close + 1))
}

/// Put a Cypher length (`*m..n`) inside the edge pattern just written, before
/// its closing `]`.
fn move_into_edge(out: &mut String, bounds: &str) -> Result<(), CypherError> {
    let tail = out.trim_end();
    let arrow = tail
        .strip_suffix("->")
        .or_else(|| tail.strip_suffix('-'))
        .map(str::trim_end);
    match arrow {
        Some(head) if head.ends_with(']') => {
            let at = head.len() - 1;
            out.insert_str(at, bounds);
            Ok(())
        }
        _ => Err(unsupported(
            "quantified bracketless edge",
            "a quantifier needs a typed edge pattern such as `-[:T]->{1,}`",
        )),
    }
}

/// Rewrite the supported GQL surface forms into the Cypher subset.
fn normalize(gql: &str) -> Result<String, CypherError> {
    let chars: Vec<char> = gql.chars().collect();
    let mut out = String::with_capacity(gql.len());
    let mut i = 0;
    // The last non-whitespace character written, to recognise `->{` / `-{`.
    let mut last_sig: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        // String literals: copied verbatim, escapes included.
        if c == '"' || c == '\'' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(chars.len());
            out.extend(&chars[start..i]);
            last_sig = Some(c);
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_ascii_uppercase();
            if let Some((_, reason)) = UNSUPPORTED_STATEMENTS.iter().find(|(w, _)| *w == upper) {
                return Err(unsupported(&word, reason));
            }
            match upper.as_str() {
                "IS" => {
                    // `IS <label>` is a label test; any other IS is a predicate.
                    let mut j = i;
                    while j < chars.len() && chars[j].is_whitespace() {
                        j += 1;
                    }
                    let next: String = chars[j..]
                        .iter()
                        .take_while(|c| c.is_alphanumeric() || **c == '_')
                        .collect();
                    if !next.is_empty()
                        && !IS_PREDICATE_WORDS.contains(&next.to_ascii_uppercase().as_str())
                    {
                        // Drop the space before IS too: `(a IS X)` → `(a:X)`.
                        while out.ends_with(' ') {
                            out.pop();
                        }
                        out.push(':');
                        i = j;
                        last_sig = Some(':');
                        continue;
                    }
                    out.push_str(&word);
                }
                "FILTER" => out.push_str("WHERE"),
                "OFFSET" => out.push_str("SKIP"),
                _ => out.push_str(&word),
            }
            last_sig = word.chars().last();
            continue;
        }
        match c {
            '&' => {
                // Label conjunction: `A & B` → `A:B`.
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(':');
                i += 1;
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                last_sig = Some(':');
                continue;
            }
            '|' => {
                return Err(unsupported(
                    "|",
                    "label disjunction has no single RDF type test",
                ))
            }
            '!' => {
                return Err(unsupported(
                    "!",
                    "label negation is not in the supported subset",
                ))
            }
            '%' => {
                return Err(unsupported(
                    "%",
                    "the label wildcard is not in the supported subset",
                ))
            }
            '{' if matches!(last_sig, Some('>') | Some('-')) => {
                let (bounds, next) = quantifier(&chars, i)?;
                move_into_edge(&mut out, &bounds)?;
                i = next;
                last_sig = Some('-');
                continue;
            }
            // `+` / `*` only right after an edge arrow, so `a.x > +3` is left alone.
            '+' | '*'
                if {
                    let t = out.trim_end();
                    t.ends_with("->") || t.ends_with("]-")
                } =>
            {
                let (bounds, next) = quantifier(&chars, i)?;
                move_into_edge(&mut out, &bounds)?;
                i = next;
                last_sig = Some('-');
                continue;
            }
            '{' if last_sig == Some(')') => {
                return Err(unsupported(
                    "quantified path pattern",
                    "quantified parenthesised path patterns are not in the supported subset; \
                     quantify a single edge (`-[:T]->{1,}`) or use SPARQL+ property paths",
                ));
            }
            _ => {}
        }
        out.push(c);
        if !c.is_whitespace() {
            last_sig = Some(c);
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cypher::transpile;

    /// The GQL transpiles to exactly what the Cypher does, and that is a
    /// success (two equal errors would prove nothing).
    fn same(gql: &str, cypher: &str) {
        let expected = transpile(cypher);
        assert!(
            expected.is_ok(),
            "the Cypher side must transpile: {cypher}: {expected:?}"
        );
        assert_eq!(transpile_gql(gql), expected, "GQL: {gql}");
    }

    #[test]
    fn is_label_is_a_cypher_label() {
        same(
            "MATCH (a IS Person)-[:KNOWS]->(b) RETURN b.name",
            "MATCH (a:Person)-[:KNOWS]->(b) RETURN b.name",
        );
        same(
            "MATCH (a IS Person & Employee) RETURN a",
            "MATCH (a:Person:Employee) RETURN a",
        );
        same(
            "MATCH (a :Person & Employee) RETURN a",
            "MATCH (a:Person:Employee) RETURN a",
        );
    }

    #[test]
    fn filter_and_offset_are_where_and_skip() {
        same(
            "MATCH (a IS Person) FILTER a.age > 30 RETURN a.name ORDER BY a.name OFFSET 5 LIMIT 10",
            "MATCH (a:Person) WHERE a.age > 30 RETURN a.name ORDER BY a.name SKIP 5 LIMIT 10",
        );
    }

    #[test]
    fn is_predicates_and_strings_are_left_alone() {
        // `IS NULL` / `IS NOT` stay predicates, not labels. (The shared Cypher
        // back end doesn't support IS NULL yet, so such a query then fails
        // there exactly as its Cypher form does.)
        assert_eq!(
            normalize("MATCH (a IS Person) WHERE a.nick IS NULL AND a.x IS NOT NULL RETURN a")
                .unwrap(),
            "MATCH (a:Person) WHERE a.nick IS NULL AND a.x IS NOT NULL RETURN a"
        );
        assert_eq!(
            transpile_gql("MATCH (a IS Person) WHERE a.nick IS NULL RETURN a"),
            transpile("MATCH (a:Person) WHERE a.nick IS NULL RETURN a")
        );
        same(
            r#"MATCH (a {name: "IS Person & FILTER | x"}) RETURN a"#,
            r#"MATCH (a {name: "IS Person & FILTER | x"}) RETURN a"#,
        );
    }

    #[test]
    fn edge_quantifiers_move_into_the_brackets() {
        for q in [
            "MATCH (a IS Person)-[:KNOWS]->+(b) RETURN DISTINCT b",
            "MATCH (a IS Person)-[:KNOWS]->{1,}(b) RETURN DISTINCT b",
            "MATCH (a IS Person)-[:KNOWS]-> { 1 , } (b) RETURN DISTINCT b",
        ] {
            same(q, "MATCH (a:Person)-[:KNOWS*1..]->(b) RETURN DISTINCT b");
        }
        same(
            "MATCH (a)<-[:KNOWS]-{1,}(b) RETURN DISTINCT b",
            "MATCH (a)<-[:KNOWS*1..]-(b) RETURN DISTINCT b",
        );
        same(
            "MATCH (a)-[:KNOWS]->{1}(b) RETURN b",
            "MATCH (a)-[:KNOWS]->(b) RETURN b",
        );
        // A `+` that is a sign, not a quantifier, is left alone.
        assert_eq!(
            normalize("MATCH (a) WHERE a.x > +3 RETURN a").unwrap(),
            "MATCH (a) WHERE a.x > +3 RETURN a"
        );
    }

    #[test]
    fn constructs_without_a_faithful_reading_are_rejected() {
        for q in [
            "MATCH (a IS Person | Robot) RETURN a",
            "MATCH (a IS !Person) RETURN a",
            "MATCH (a IS %) RETURN a",
            "MATCH (a)-[:KNOWS]->{1,3}(b) RETURN DISTINCT b",
            "MATCH (a)-[:KNOWS]->{2,}(b) RETURN DISTINCT b",
            "MATCH (a)-[:KNOWS]->{0,}(b) RETURN DISTINCT b",
            "MATCH (a)-[:KNOWS]->*(b) RETURN DISTINCT b",
            "MATCH (a)-[:KNOWS]->+(b) RETURN b",
            "MATCH (a)->+(b) RETURN DISTINCT b",
            "MATCH ((a)-[:KNOWS]->(b)){1,} RETURN DISTINCT b",
            "USE g MATCH (a) RETURN a",
            "INSERT (a IS Person)",
            "LET x = 1 RETURN x",
        ] {
            assert!(
                matches!(transpile_gql(q), Err(CypherError::Unsupported { .. })),
                "{q}: {:?}",
                transpile_gql(q)
            );
        }
    }

    #[test]
    fn a_transpiled_query_runs() {
        use loka_core::{TermDictionary, Triple, TripleStore};
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();
        let id = |d: &mut TermDictionary, s: &str| d.intern(&format!("http://loka.dev/{s}"));
        let ty = dict.intern("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
        let (person, knows) = (id(&mut dict, "Person"), id(&mut dict, "KNOWS"));
        let (ada, bob, cy) = (
            id(&mut dict, "ada"),
            id(&mut dict, "bob"),
            id(&mut dict, "cy"),
        );
        for t in [
            Triple::new(ada, ty, person),
            Triple::new(bob, ty, person),
            Triple::new(ada, knows, bob),
            Triple::new(cy, knows, bob),
        ] {
            store.insert(t).unwrap();
        }
        let sparql = transpile_gql("MATCH (a IS Person)-[:KNOWS]->(b) RETURN a, b").unwrap();
        let rows = crate::execute(&crate::parse(&sparql).unwrap(), &store, &dict)
            .unwrap()
            .rows;
        assert_eq!(
            rows.len(),
            1,
            "only ada is a Person who knows someone: {sparql}"
        );
    }
}
