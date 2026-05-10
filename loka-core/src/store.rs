//! In-memory triple store with SPO/POS/OSP indexes.
//!
//! This is the v0.1 implementation using `BTreeSet` as the index structure.
//! A future version will replace this with an LSM-tree for persistence and
//! better write throughput on bulk ingestion.

use std::collections::BTreeSet;

use crate::error::{CoreError, Result};
use crate::id::TermId;
use crate::temporal::{decode_tspo_key, tspo_key, TemporalSignifier};
use crate::triple::Triple;

/// An in-memory triple store backed by three sorted indexes.
///
/// Each index stores the same triples in a different key order so that
/// any access pattern (subject-first, predicate-first, object-first)
/// can be served by a range scan rather than a full scan.
pub struct TripleStore {
    /// Subject → Predicate → Object index.
    spo: BTreeSet<[u8; 24]>,
    /// Predicate → Object → Subject index.
    pos: BTreeSet<[u8; 24]>,
    /// Object → Subject → Predicate index.
    osp: BTreeSet<[u8; 24]>,
    /// Time → Subject → Predicate → Object index (ontochronological).
    /// 33-byte keys: [signifier:1 | timestamp:8 | S:8 | P:8 | O:8].
    tspo: BTreeSet<[u8; 33]>,
    /// Materialized adjacency list: subject → list of (predicate, object) pairs.
    /// Provides O(1) lookup for star-shaped queries (all edges from a node).
    adjacency: std::collections::HashMap<TermId, Vec<(TermId, TermId)>>,
    /// Total number of triples stored.
    count: usize,
}

impl TripleStore {
    /// Create an empty triple store.
    pub fn new() -> Self {
        Self {
            spo: BTreeSet::new(),
            pos: BTreeSet::new(),
            osp: BTreeSet::new(),
            tspo: BTreeSet::new(),
            adjacency: std::collections::HashMap::new(),
            count: 0,
        }
    }

    /// Insert a triple. Returns `Err(DuplicateTriple)` if already present.
    pub fn insert(&mut self, triple: Triple) -> Result<()> {
        let spo_key = triple.spo_key();
        if !self.spo.insert(spo_key) {
            return Err(CoreError::DuplicateTriple);
        }
        self.pos.insert(triple.pos_key());
        self.osp.insert(triple.osp_key());
        self.adjacency
            .entry(triple.subject)
            .or_default()
            .push((triple.predicate, triple.object));
        self.count += 1;
        Ok(())
    }

    /// Remove a triple. Returns true if it was present.
    pub fn remove(&mut self, triple: &Triple) -> bool {
        let removed = self.spo.remove(&triple.spo_key());
        if removed {
            self.pos.remove(&triple.pos_key());
            self.osp.remove(&triple.osp_key());
            if let Some(adj) = self.adjacency.get_mut(&triple.subject) {
                adj.retain(|&(p, o)| p != triple.predicate || o != triple.object);
            }
            self.count -= 1;
        }
        removed
    }

    /// Check whether a triple exists.
    pub fn contains(&self, triple: &Triple) -> bool {
        self.spo.contains(&triple.spo_key())
    }

    /// Number of triples in the store.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Find all triples with the given subject.
    pub fn find_by_subject(&self, subject: TermId) -> Vec<Triple> {
        let mut lo = [0u8; 24];
        lo[0..8].copy_from_slice(&subject.to_be_bytes());

        let mut hi = [0u8; 24];
        hi[0..8].copy_from_slice(&subject.to_be_bytes());
        hi[8..24].fill(0xFF);

        self.spo.range(lo..=hi).map(Triple::from_spo_key).collect()
    }

    /// Find all triples with the given predicate.
    pub fn find_by_predicate(&self, predicate: TermId) -> Vec<Triple> {
        let mut lo = [0u8; 24];
        lo[0..8].copy_from_slice(&predicate.to_be_bytes());

        let mut hi = [0u8; 24];
        hi[0..8].copy_from_slice(&predicate.to_be_bytes());
        hi[8..24].fill(0xFF);

        self.pos.range(lo..=hi).map(Triple::from_pos_key).collect()
    }

    /// Find all triples with the given object.
    pub fn find_by_object(&self, object: TermId) -> Vec<Triple> {
        let mut lo = [0u8; 24];
        lo[0..8].copy_from_slice(&object.to_be_bytes());

        let mut hi = [0u8; 24];
        hi[0..8].copy_from_slice(&object.to_be_bytes());
        hi[8..24].fill(0xFF);

        self.osp.range(lo..=hi).map(Triple::from_osp_key).collect()
    }

    /// Find all triples with the given subject and predicate.
    pub fn find_by_subject_predicate(&self, subject: TermId, predicate: TermId) -> Vec<Triple> {
        let mut lo = [0u8; 24];
        lo[0..8].copy_from_slice(&subject.to_be_bytes());
        lo[8..16].copy_from_slice(&predicate.to_be_bytes());

        let mut hi = [0u8; 24];
        hi[0..8].copy_from_slice(&subject.to_be_bytes());
        hi[8..16].copy_from_slice(&predicate.to_be_bytes());
        hi[16..24].fill(0xFF);

        self.spo.range(lo..=hi).map(Triple::from_spo_key).collect()
    }

    /// Find all triples with the given predicate and object.
    /// Uses the POS index for efficient lookup.
    pub fn find_by_predicate_object(&self, predicate: TermId, object: TermId) -> Vec<Triple> {
        let mut lo = [0u8; 24];
        lo[0..8].copy_from_slice(&predicate.to_be_bytes());
        lo[8..16].copy_from_slice(&object.to_be_bytes());

        let mut hi = [0u8; 24];
        hi[0..8].copy_from_slice(&predicate.to_be_bytes());
        hi[8..16].copy_from_slice(&object.to_be_bytes());
        hi[16..24].fill(0xFF);

        self.pos.range(lo..=hi).map(Triple::from_pos_key).collect()
    }

    // --- Temporal (TSPO) index operations ---

    /// Insert a temporal index entry.
    ///
    /// The caller is responsible for detecting temporal predicates on quoted
    /// triples and extracting the inner (S, P, O) and timestamp. The store
    /// itself is dumb — it just indexes what it's told.
    pub fn insert_temporal(
        &mut self,
        signifier: TemporalSignifier,
        timestamp: i64,
        subject: TermId,
        predicate: TermId,
        object: TermId,
    ) -> bool {
        let key = tspo_key(signifier, timestamp, subject, predicate, object);
        self.tspo.insert(key)
    }

    /// Remove a temporal index entry. Returns true if it was present.
    pub fn remove_temporal(
        &mut self,
        signifier: TemporalSignifier,
        timestamp: i64,
        subject: TermId,
        predicate: TermId,
        object: TermId,
    ) -> bool {
        let key = tspo_key(signifier, timestamp, subject, predicate, object);
        self.tspo.remove(&key)
    }

    /// Find all temporal entries for a given signifier type within a time range
    /// `[start, end)`. Returns `(signifier, timestamp, Triple)` tuples.
    pub fn find_temporal_range(
        &self,
        signifier: TemporalSignifier,
        start: i64,
        end: i64,
    ) -> Vec<(i64, Triple)> {
        let lo = tspo_key(signifier, start, 0, 0, 0);
        let hi = tspo_key(signifier, end, u64::MAX, u64::MAX, u64::MAX);
        self.tspo
            .range(lo..hi)
            .map(|key| {
                let (_, ts, s, p, o) = decode_tspo_key(key);
                (ts, Triple::new(s, p, o))
            })
            .collect()
    }

    /// Find all `ValidFrom` entries with timestamp ≤ `at`, i.e. triples
    /// whose validity started on or before the given time.
    pub fn find_valid_from_before(&self, at: i64) -> Vec<(i64, Triple)> {
        let lo = tspo_key(TemporalSignifier::ValidFrom, i64::MIN, 0, 0, 0);
        let hi = tspo_key(
            TemporalSignifier::ValidFrom,
            at,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        self.tspo
            .range(lo..=hi)
            .map(|key| {
                let (_, ts, s, p, o) = decode_tspo_key(key);
                (ts, Triple::new(s, p, o))
            })
            .collect()
    }

    /// Find all `ValidTo` entries with timestamp ≤ `at`, i.e. triples
    /// whose validity ended on or before the given time.
    pub fn find_valid_to_before(&self, at: i64) -> Vec<(i64, Triple)> {
        let lo = tspo_key(TemporalSignifier::ValidTo, i64::MIN, 0, 0, 0);
        let hi = tspo_key(TemporalSignifier::ValidTo, at, u64::MAX, u64::MAX, u64::MAX);
        self.tspo
            .range(lo..=hi)
            .map(|key| {
                let (_, ts, s, p, o) = decode_tspo_key(key);
                (ts, Triple::new(s, p, o))
            })
            .collect()
    }

    /// Find all `AssertedAt` entries at a specific time.
    pub fn find_asserted_at(&self, at: i64) -> Vec<Triple> {
        let lo = tspo_key(TemporalSignifier::AssertedAt, at, 0, 0, 0);
        let hi = tspo_key(
            TemporalSignifier::AssertedAt,
            at,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        );
        self.tspo
            .range(lo..=hi)
            .map(|key| {
                let (_, _, s, p, o) = decode_tspo_key(key);
                Triple::new(s, p, o)
            })
            .collect()
    }

    /// Gather all temporal annotations for a specific triple (S, P, O).
    ///
    /// Scans the TSPO index for all three signifiers and collects timestamps
    /// into a `TemporalAnnotations` struct. This is used by the executor to
    /// evaluate temporal containment for AT_TIME / DURING queries.
    pub fn gather_temporal_annotations(
        &self,
        subject: TermId,
        predicate: TermId,
        object: TermId,
    ) -> crate::temporal::TemporalAnnotations {
        use crate::temporal::TemporalAnnotations;

        let mut annotations = TemporalAnnotations::default();

        // For each signifier, scan the full range and filter by (S, P, O).
        // The TSPO key sorts by [signifier | timestamp | S | P | O], so we
        // can't skip to a specific (S,P,O) — we must scan all timestamps
        // for each signifier. The TSPO index is small (only annotated triples),
        // so this is acceptable for v1.
        for signifier in [
            TemporalSignifier::AssertedAt,
            TemporalSignifier::ValidFrom,
            TemporalSignifier::ValidTo,
        ] {
            let lo = tspo_key(signifier, i64::MIN, 0, 0, 0);
            let hi = tspo_key(signifier, i64::MAX, u64::MAX, u64::MAX, u64::MAX);
            for key in self.tspo.range(lo..=hi) {
                let (_, ts, s, p, o) = decode_tspo_key(key);
                if s == subject && p == predicate && o == object {
                    match signifier {
                        TemporalSignifier::AssertedAt => annotations.asserted_at.push(ts),
                        TemporalSignifier::ValidFrom => annotations.valid_from.push(ts),
                        TemporalSignifier::ValidTo => annotations.valid_to.push(ts),
                    }
                }
            }
        }

        annotations
    }

    /// Number of entries in the TSPO index.
    pub fn temporal_len(&self) -> usize {
        self.tspo.len()
    }

    /// Fast adjacency lookup: get all (predicate, object) pairs for a subject.
    /// O(1) HashMap lookup instead of BTreeSet range scan.
    pub fn adjacency(&self, subject: TermId) -> &[(TermId, TermId)] {
        self.adjacency
            .get(&subject)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Iterate all triples in SPO order.
    pub fn iter(&self) -> impl Iterator<Item = Triple> + '_ {
        self.spo.iter().map(Triple::from_spo_key)
    }

    /// Estimate the cardinality (number of matches) for a partial pattern.
    /// Used by the query planner for cost-based optimization.
    pub fn estimate_cardinality(
        &self,
        subject: Option<TermId>,
        predicate: Option<TermId>,
        object: Option<TermId>,
    ) -> usize {
        match (subject, predicate, object) {
            (Some(s), Some(p), Some(_)) => {
                // Fully bound: 0 or 1
                self.find_by_subject_predicate(s, p).len().min(1)
            }
            (Some(s), Some(p), None) => self.find_by_subject_predicate(s, p).len(),
            (Some(s), None, None) => self.find_by_subject(s).len(),
            (None, Some(p), Some(o)) => self.find_by_predicate_object(p, o).len(),
            (None, Some(p), None) => self.find_by_predicate(p).len(),
            (None, None, Some(o)) => self.find_by_object(o).len(),
            (None, None, None) => self.count,
            (Some(s), None, Some(_)) => self.find_by_subject(s).len(), // rough estimate
        }
    }
}

impl Default for TripleStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> TripleStore {
        let mut store = TripleStore::new();
        // :Alice :knows :Bob
        store.insert(Triple::new(1, 10, 2)).unwrap();
        // :Alice :knows :Charlie
        store.insert(Triple::new(1, 10, 3)).unwrap();
        // :Bob :knows :Alice
        store.insert(Triple::new(2, 10, 1)).unwrap();
        // :Alice :name "Alice"
        store.insert(Triple::new(1, 11, 100)).unwrap();
        store
    }

    #[test]
    fn insert_and_count() {
        let store = make_store();
        assert_eq!(store.len(), 4);
    }

    #[test]
    fn duplicate_rejected() {
        let mut store = make_store();
        let result = store.insert(Triple::new(1, 10, 2));
        assert!(result.is_err());
        assert_eq!(store.len(), 4);
    }

    #[test]
    fn find_by_subject() {
        let store = make_store();
        let results = store.find_by_subject(1);
        assert_eq!(results.len(), 3); // Alice has 3 triples as subject
    }

    #[test]
    fn find_by_predicate() {
        let store = make_store();
        let results = store.find_by_predicate(10); // :knows
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn find_by_object() {
        let store = make_store();
        let results = store.find_by_object(1); // things pointing to Alice
        assert_eq!(results.len(), 1); // Bob knows Alice
    }

    #[test]
    fn find_by_subject_predicate() {
        let store = make_store();
        let results = store.find_by_subject_predicate(1, 10); // Alice knows ?
        assert_eq!(results.len(), 2); // Bob and Charlie
    }

    #[test]
    fn remove() {
        let mut store = make_store();
        assert!(store.remove(&Triple::new(1, 10, 2)));
        assert_eq!(store.len(), 3);
        assert!(!store.contains(&Triple::new(1, 10, 2)));
        // Should still find Alice's other triples
        assert_eq!(store.find_by_subject(1).len(), 2);
    }

    #[test]
    fn iter_all() {
        let store = make_store();
        let all: Vec<_> = store.iter().collect();
        assert_eq!(all.len(), 4);
    }
}
