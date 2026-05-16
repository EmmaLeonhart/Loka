//! World-model cascade-retraction (the pure set computation).
//!
//! Remove a node — real data **or** AI-generated — and every *generated*
//! inference that transitively cited it disappears with it. Propagation
//! follows **only** RDF-star provenance back-edges
//! (`<<G>> http://loka.dev/provenance/propositionInferredFrom <<src>>`),
//! never ordinary data edges: real→real is not a dependency.
//!
//! This module is the non-destructive core (cascade-retraction Phase 1 in
//! `planning/cascade-retraction.md`). It computes *what would be removed*;
//! it never mutates the store. The preview endpoint and the `retract_node`
//! MCP tool build on top of it.
//!
//! Correctness rests on the quoted-triple reverse map (Phase 0): a
//! `propositionInferredFrom` object is a content-addressed
//! `quoted_triple_id`, and the cascade must decide whether that quoted
//! source dereferences a removed row — impossible without
//! [`TermDictionary::resolve_quoted`].

use std::collections::HashSet;

use crate::id::{quoted_triple_id, TermDictionary, TermId};
use crate::store::TripleStore;
use crate::triple::Triple;

/// The reserved system namespace. Cascade traversal is bounded to it — a
/// regular predicate is never followed as if it were a derivation edge, and
/// only predicates under this prefix are swept when a generated node is
/// removed. Mirrors `training/preprocess.py`'s `RESERVED_PROVENANCE_PREFIX`.
pub const RESERVED_PROVENANCE_PREFIX: &str = "http://loka.dev/provenance/";

/// `RESERVED_PROVENANCE_PREFIX + "propositionInferredFrom"` — the single
/// predicate the cascade recurses along.
pub const PROP_INFERRED_FROM: &str = "http://loka.dev/provenance/propositionInferredFrom";

/// The result of [`retract_set`]: every triple that would be removed,
/// grouped by cascade depth (index = depth). Depth 0 is the root node's own
/// rows; depth *d* > 0 is everything reached after *d* provenance hops.
#[derive(Debug, Clone, Default)]
pub struct RetractSet {
    pub root: TermId,
    pub by_depth: Vec<Vec<Triple>>,
}

impl RetractSet {
    /// Total number of triples in the set.
    pub fn total(&self) -> usize {
        self.by_depth.iter().map(|v| v.len()).sum()
    }

    /// Deepest cascade level reached (0 if only the root's own rows).
    pub fn max_depth(&self) -> usize {
        self.by_depth.len().saturating_sub(1)
    }

    /// All triples, flattened (depth order preserved).
    pub fn all(&self) -> impl Iterator<Item = &Triple> {
        self.by_depth.iter().flat_map(|v| v.iter())
    }
}

/// Compute the cascade-retraction set rooted at `root_id`.
///
/// Non-destructive. The store is read-only here; the caller decides whether
/// to actually delete the returned triples.
///
/// Algorithm (see `planning/cascade-retraction.md` §4):
/// 1. Depth 0 = the node's own rows (`find_by_subject ∪ find_by_object`).
/// 2. Repeatedly: for every triple `T` removed in the previous round, find
///    `<<G>> propositionInferredFrom <<T>>` annotation rows; reverse
///    `<<G>>` via the quoted map; remove `G`'s asserted row **and** all of
///    `G`'s provenance annotation rows (predicate under the reserved
///    namespace only); recurse on `G`.
///
/// Bounded, cycle-safe (a triple is never processed twice), and never
/// follows a non-provenance edge.
pub fn retract_set(root_id: TermId, store: &TripleStore, dict: &TermDictionary) -> RetractSet {
    let mut out = RetractSet {
        root: root_id,
        by_depth: Vec::new(),
    };
    let mut seen: HashSet<(TermId, TermId, TermId)> = HashSet::new();

    // --- Depth 0: the node's own data rows. ---
    let mut frontier: Vec<Triple> = Vec::new();
    let mut depth0: Vec<Triple> = Vec::new();
    for t in store
        .find_by_subject(root_id)
        .into_iter()
        .chain(store.find_by_object(root_id))
    {
        if seen.insert((t.subject, t.predicate, t.object)) {
            depth0.push(t);
            frontier.push(t);
        }
    }
    out.by_depth.push(depth0);
    if frontier.is_empty() {
        return out;
    }

    // The cascade only follows `propositionInferredFrom`. If that predicate
    // was never interned, there are no provenance edges — depth 0 is all.
    let inferred_from = match dict.lookup(PROP_INFERRED_FROM) {
        Some(id) => id,
        None => return out,
    };

    // --- Provenance closure. ---
    while !frontier.is_empty() {
        let mut next: Vec<Triple> = Vec::new();
        let mut this_depth: Vec<Triple> = Vec::new();

        for t in &frontier {
            let qid_t = quoted_triple_id(t.subject, t.predicate, t.object);

            // Generated triples that cited `t` as a source:
            // `<<G>> propositionInferredFrom <<t>>`.
            for ann in store.find_by_predicate_object(inferred_from, qid_t) {
                let g_qid = ann.subject; // == quoted_triple_id(G)
                let (gs, gp, go) = match dict.resolve_quoted(g_qid) {
                    Some(g) => g,
                    None => continue, // not a reversible quoted triple — skip
                };

                // G's asserted row.
                if seen.insert((gs, gp, go)) {
                    let g_triple = Triple::new(gs, gp, go);
                    this_depth.push(g_triple);
                    next.push(g_triple);
                }

                // ALL of G's provenance annotation rows — bounded to the
                // reserved namespace so a non-provenance edge on the quoted
                // node is never swept.
                for a in store.find_by_subject(g_qid) {
                    if is_reserved_predicate(a.predicate, dict)
                        && seen.insert((a.subject, a.predicate, a.object))
                    {
                        this_depth.push(a);
                    }
                }
            }
        }

        if this_depth.is_empty() {
            break;
        }
        out.by_depth.push(this_depth);
        frontier = next;
    }

    out
}

/// Whether `pred_id` resolves to an IRI under the reserved provenance
/// namespace. Inline ids (literals) are never reserved predicates.
fn is_reserved_predicate(pred_id: TermId, dict: &TermDictionary) -> bool {
    dict.render_term(pred_id)
        .is_some_and(|s| s.starts_with(RESERVED_PROVENANCE_PREFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a store + dict with a small real graph and a two-level
    /// generated-inference chain. The generated rows have **distinct
    /// subjects** from the root so they are reached *only* via provenance
    /// (true transitive depth, not scooped into depth 0 as the root's own
    /// rows).
    ///
    /// Real:       Q42  P_pob  Q350           (place of birth)
    ///             Q42  P_name "Douglas"      (a second real row on Q42)
    ///             Q350 P_in   Q1             (real→real — must NOT cascade)
    /// Generated:  <<Q350 G_died Q999>> inferredFrom <<Q42  P_pob  Q350>>
    ///             <<Q999 G_city Q1  >> inferredFrom <<Q350 G_died Q999>>
    /// plus each generated row carries propositionGeneratedBy.
    fn fixture() -> (TripleStore, TermDictionary, TermId) {
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();

        let q42 = dict.intern("http://wd/Q42");
        let q350 = dict.intern("http://wd/Q350");
        let q999 = dict.intern("http://wd/Q999");
        let q1 = dict.intern("http://wd/Q1");
        let p_pob = dict.intern("http://wd/P_pob");
        let p_name = dict.intern("http://wd/P_name");
        let p_in = dict.intern("http://wd/P_in");
        let g_died = dict.intern("http://wd/G_died");
        let g_city = dict.intern("http://wd/G_city");
        let douglas = dict.intern("\"Douglas\"");

        let inferred = dict.intern(PROP_INFERRED_FROM);
        let gen_by = dict.intern("http://loka.dev/provenance/propositionGeneratedBy");
        let model = dict.intern("\"loka-v14\"");

        // Real rows.
        store.insert(Triple::new(q42, p_pob, q350)).unwrap();
        store.insert(Triple::new(q42, p_name, douglas)).unwrap();
        store.insert(Triple::new(q350, p_in, q1)).unwrap();

        // G1: << Q350 G_died Q999 >> inferredFrom << Q42 P_pob Q350 >>
        store.insert(Triple::new(q350, g_died, q999)).unwrap();
        let g1 = dict.register_quoted(q350, g_died, q999);
        let src1 = dict.register_quoted(q42, p_pob, q350);
        store.insert(Triple::new(g1, inferred, src1)).unwrap();
        store.insert(Triple::new(g1, gen_by, model)).unwrap();

        // G2: << Q999 G_city Q1 >> inferredFrom << Q350 G_died Q999 >>
        store.insert(Triple::new(q999, g_city, q1)).unwrap();
        let g2 = dict.register_quoted(q999, g_city, q1);
        let src2 = dict.register_quoted(q350, g_died, q999); // == g1
        store.insert(Triple::new(g2, inferred, src2)).unwrap();
        store.insert(Triple::new(g2, gen_by, model)).unwrap();

        (store, dict, q42)
    }

    fn has(set: &RetractSet, dict: &TermDictionary, s: &str, p: &str, o: &str) -> bool {
        let sid = dict.lookup(s);
        let pid = dict.lookup(p);
        let oid = dict.lookup(o);
        match (sid, pid, oid) {
            (Some(s), Some(p), Some(o)) => set
                .all()
                .any(|t| t.subject == s && t.predicate == p && t.object == o),
            _ => false,
        }
    }

    #[test]
    fn cascade_drops_transitive_generated_chain() {
        let (store, dict, q42) = fixture();
        let set = retract_set(q42, &store, &dict);

        // The real row Q42 P_pob Q350 is depth 0.
        assert!(has(&set, &dict, "http://wd/Q42", "http://wd/P_pob", "http://wd/Q350"));
        // G1's asserted row cascades out (reached via provenance, depth 1).
        assert!(has(&set, &dict, "http://wd/Q350", "http://wd/G_died", "http://wd/Q999"));
        // G2 (cited G1) cascades transitively (depth 2).
        assert!(has(&set, &dict, "http://wd/Q999", "http://wd/G_city", "http://wd/Q1"));
        // Provenance annotation rows for G1 are swept (reserved namespace).
        let g1 = quoted_triple_id(
            dict.lookup("http://wd/Q350").unwrap(),
            dict.lookup("http://wd/G_died").unwrap(),
            dict.lookup("http://wd/Q999").unwrap(),
        );
        let inferred = dict.lookup(PROP_INFERRED_FROM).unwrap();
        assert!(
            set.all().any(|t| t.subject == g1 && t.predicate == inferred),
            "G1's inferredFrom annotation is retracted"
        );
        assert!(set.max_depth() >= 2, "two provenance hops");
    }

    #[test]
    fn real_to_real_is_not_a_dependency() {
        let (store, dict, q42) = fixture();
        let set = retract_set(q42, &store, &dict);
        // Q350 P_in Q1 is a real→real edge (Q350 is reachable as the object
        // of Q42's depth-0 row) but it is NOT a derivation; deleting Q42
        // must not chase it.
        assert!(
            !has(&set, &dict, "http://wd/Q350", "http://wd/P_in", "http://wd/Q1"),
            "ordinary edges are never followed as derivations"
        );
        // Q42's second real row IS removed (it's the node's own row).
        assert!(has(&set, &dict, "http://wd/Q42", "http://wd/P_name", "\"Douglas\""));
    }

    #[test]
    fn child_to_parent_is_not_a_dependency() {
        // Provenance points child→parent (G2 inferredFrom G1). Cascade only
        // propagates parent→child. Retract Q1 — it appears only in G2's
        // asserted row (<<Q999 G_city Q1>>) and one real row — so the leaf
        // G2 goes but its parent G1 and the real source must survive.
        let (store, dict, _q42) = fixture();
        let q1 = dict.lookup("http://wd/Q1").unwrap();
        let set = retract_set(q1, &store, &dict);
        // Q1's own rows (depth 0): the generated leaf and a real row.
        assert!(has(&set, &dict, "http://wd/Q999", "http://wd/G_city", "http://wd/Q1"));
        assert!(has(&set, &dict, "http://wd/Q350", "http://wd/P_in", "http://wd/Q1"));
        // Parent G1 must survive (we don't walk child→parent).
        assert!(!has(&set, &dict, "http://wd/Q350", "http://wd/G_died", "http://wd/Q999"));
        // The real source must survive.
        assert!(!has(&set, &dict, "http://wd/Q42", "http://wd/P_pob", "http://wd/Q350"));
    }

    #[test]
    fn no_provenance_predicate_means_depth_zero_only() {
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();
        let a = dict.intern("http://x/a");
        let p = dict.intern("http://x/p");
        let b = dict.intern("http://x/b");
        store.insert(Triple::new(a, p, b)).unwrap();
        let set = retract_set(a, &store, &dict);
        assert_eq!(set.total(), 1);
        assert_eq!(set.max_depth(), 0);
    }

    #[test]
    fn cycle_safe() {
        // Pathological: a generated triple whose provenance cites itself.
        let mut dict = TermDictionary::new();
        let mut store = TripleStore::new();
        let s = dict.intern("http://x/s");
        let p = dict.intern("http://x/p");
        let o = dict.intern("http://x/o");
        store.insert(Triple::new(s, p, o)).unwrap();
        let q = dict.register_quoted(s, p, o);
        let inferred = dict.intern(PROP_INFERRED_FROM);
        // << s p o >> inferredFrom << s p o >>  (self-citation)
        store.insert(Triple::new(q, inferred, q)).unwrap();
        // Must terminate.
        let set = retract_set(s, &store, &dict);
        assert!(set.total() >= 1);
    }
}
