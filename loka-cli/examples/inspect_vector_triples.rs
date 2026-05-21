//! Inspect f32vec-bearing triples in a persistent Loka store.
//!
//! Classifies each f32vec-object row by whether its *predicate* is an
//! IRI (well-formed: a real vector-predicate declaration) or a literal
//! (malformed: a literal-id ended up in the predicate slot — the
//! corruption mode that poisoned the vector-registry rebuild on
//! 2026-05-20).
//!
//! Run:
//! ```text
//! cargo run --release --example inspect_vector_triples -p loka-cli \
//!     -- loka-retrieval-data-stale-20260520
//! ```

use std::collections::BTreeMap;
use std::env;
use std::process::ExitCode;

use loka_core::{is_inline, PersistentStore, TermDictionary};

const F32VEC_TYPE_SUFFIX: &str = "^^<http://loka.dev/f32vec>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: inspect_vector_triples <data-dir>");
        return ExitCode::from(2);
    }
    let data_dir = &args[1];

    let ps = match PersistentStore::open(data_dir) {
        Ok(ps) => ps,
        Err(e) => {
            eprintln!("failed to open {}: {}", data_dir, e);
            return ExitCode::from(1);
        }
    };

    let mut dict = TermDictionary::new();
    let term_count = ps.load_terms_into(&mut dict);
    let triple_count = ps.len();

    println!("# inspect_vector_triples — {}", data_dir);
    println!("triples_total: {}", triple_count);
    println!("terms_total:   {}", term_count);
    println!();

    // Predicate id -> (count, is_literal, sample row strings)
    #[derive(Default)]
    struct PredStat {
        count: usize,
        is_literal_pred: bool,
        is_inline_pred: bool,
        pred_str: String,
        sample_subject: Option<String>,
        sample_object: Option<String>,
    }
    let mut by_pred: BTreeMap<u64, PredStat> = BTreeMap::new();

    let mut f32vec_rows = 0usize;
    for triple in ps.iter() {
        let obj_str = match dict.resolve(triple.object) {
            Some(s) => s,
            None => continue,
        };
        if !obj_str.contains(F32VEC_TYPE_SUFFIX) {
            continue;
        }
        f32vec_rows += 1;

        let entry = by_pred.entry(triple.predicate).or_default();
        entry.count += 1;
        if entry.pred_str.is_empty() {
            if is_inline(triple.predicate) {
                entry.is_inline_pred = true;
                entry.pred_str = format!("(inline {})", triple.predicate);
            } else {
                match dict.resolve(triple.predicate) {
                    Some(s) => {
                        if s.starts_with('"') {
                            entry.is_literal_pred = true;
                        }
                        entry.pred_str = s.into();
                    }
                    None => {
                        entry.pred_str = "(unresolvable)".into();
                    }
                }
            }
            entry.sample_subject =
                dict.resolve(triple.subject).map(|s| truncate(&s, 60).into());
            entry.sample_object = Some(truncate(&obj_str, 60).into());
        }
    }

    println!("f32vec_rows_total: {}", f32vec_rows);
    println!();

    let well_formed: Vec<_> = by_pred
        .iter()
        .filter(|(_, v)| !v.is_literal_pred && !v.is_inline_pred && v.pred_str != "(unresolvable)")
        .collect();
    let malformed_literal: Vec<_> = by_pred.iter().filter(|(_, v)| v.is_literal_pred).collect();
    let malformed_inline: Vec<_> = by_pred.iter().filter(|(_, v)| v.is_inline_pred).collect();
    let malformed_unresolvable: Vec<_> = by_pred
        .iter()
        .filter(|(_, v)| v.pred_str == "(unresolvable)")
        .collect();

    println!("## well-formed predicates ({})", well_formed.len());
    for (pid, stat) in &well_formed {
        println!("  pid={} count={} pred={}", pid, stat.count, stat.pred_str);
    }
    println!();

    println!(
        "## malformed: literal-id in predicate slot ({})",
        malformed_literal.len()
    );
    for (pid, stat) in &malformed_literal {
        println!(
            "  pid={} count={} pred={} sample_subject={} sample_object={}",
            pid,
            stat.count,
            stat.pred_str,
            stat.sample_subject.as_deref().unwrap_or("?"),
            stat.sample_object.as_deref().unwrap_or("?"),
        );
    }
    println!();

    println!(
        "## malformed: inline-encoded value in predicate slot ({})",
        malformed_inline.len()
    );
    for (pid, stat) in &malformed_inline {
        println!("  pid={} count={} pred={}", pid, stat.count, stat.pred_str);
    }
    println!();

    println!(
        "## malformed: predicate id has no dictionary entry ({})",
        malformed_unresolvable.len()
    );
    for (pid, stat) in &malformed_unresolvable {
        println!("  pid={} count={}", pid, stat.count);
    }

    ExitCode::SUCCESS
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.into()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}
