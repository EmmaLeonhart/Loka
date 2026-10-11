//! `loka query --lang cypher|gql` runs Cypher and GQL against a real data dir:
//! import a small graph, then ask the same question in all three languages and
//! check the rows agree.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_loka");

const GRAPH: &str = r#"<http://loka.dev/ada> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://loka.dev/Person> .
<http://loka.dev/bob> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://loka.dev/Person> .
<http://loka.dev/london> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://loka.dev/City> .
<http://loka.dev/ada> <http://loka.dev/name> "Ada" .
<http://loka.dev/bob> <http://loka.dev/name> "Bob" .
<http://loka.dev/ada> <http://loka.dev/KNOWS> <http://loka.dev/bob> .
"#;

fn fresh_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("loka-ql-e2e-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A data dir holding `GRAPH`, imported through `loka import`.
fn loaded_data_dir() -> PathBuf {
    let dir = fresh_dir();
    let nt = dir.join("graph.nt");
    std::fs::write(&nt, GRAPH).unwrap();
    let data = dir.join("data");
    let out = Command::new(BIN)
        .args([
            "import",
            nt.to_str().unwrap(),
            "--data-dir",
            data.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "import failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    data
}

fn query(data: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("query")
        .args(args)
        .args(["--data-dir", data.to_str().unwrap()])
        .output()
        .unwrap()
}

/// The data rows of a result table (header, rule and row count dropped), sorted.
fn rows(out: &Output) -> Vec<String> {
    assert!(
        out.status.success(),
        "query failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    let rule = lines
        .iter()
        .position(|l| !l.is_empty() && l.chars().all(|c| c == '-'))
        .expect("no table rule in output");
    let mut rows: Vec<String> = lines[rule + 1..]
        .iter()
        .take_while(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();
    rows.sort();
    rows
}

#[test]
fn cypher_and_gql_match_sparql() {
    let data = loaded_data_dir();

    let sparql = rows(&query(
        &data,
        &["SELECT ?p WHERE { ?p a <http://loka.dev/Person> }"],
    ));
    let cypher = rows(&query(
        &data,
        &["MATCH (p:Person) RETURN p", "--lang", "cypher"],
    ));
    let gql = rows(&query(
        &data,
        &["MATCH (p IS Person) RETURN p", "--lang", "gql"],
    ));

    assert_eq!(sparql.len(), 2, "expected Ada and Bob, got {sparql:?}");
    assert!(sparql.iter().any(|r| r.contains("loka.dev/ada")));
    assert!(sparql.iter().any(|r| r.contains("loka.dev/bob")));
    assert_eq!(cypher, sparql);
    assert_eq!(gql, sparql);
}

#[test]
fn cypher_traversal_runs() {
    let data = loaded_data_dir();
    let out = rows(&query(
        &data,
        &[
            "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN b",
            "--lang",
            "cypher",
        ],
    ));
    assert_eq!(out.len(), 1, "got {out:?}");
    assert!(out[0].contains("loka.dev/bob"));
}

#[test]
fn show_sparql_prints_the_transpiled_query() {
    let data = loaded_data_dir();
    let out = query(
        &data,
        &[
            "MATCH (p IS Person) RETURN p",
            "--lang",
            "gql",
            "--show-sparql",
        ],
    );
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("SELECT"), "no SPARQL printed: {text}");
    assert!(
        text.contains("rdf:type loka:Person"),
        "no SPARQL printed: {text}"
    );
}

#[test]
fn unsupported_gql_fails_with_the_reason() {
    let data = loaded_data_dir();
    let out = query(
        &data,
        &["MATCH (p IS Person|City) RETURN p", "--lang", "gql"],
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.to_lowercase().contains("unsupported"), "stderr: {err}");
}
