//! `retract_set` checked against an independent brute-force reference on
//! randomly generated provenance graphs.
//!
//! The generator builds real triples, generated triples that cite earlier
//! real or generated triples via `propositionInferredFrom` (plus some
//! back-citations to later generated triples, so cycles occur), and a
//! `propositionGeneratedBy` row per generated triple. The reference computes
//! the retraction closure from the generator's own lists, never touching the
//! store's indexes:
//!
//! - every row whose subject or object is the root is removed;
//! - a generated triple is removed when any triple it cites is removed;
//! - a removed generated triple takes all of its provenance annotations with it;
//! - nothing else is removed (real→real edges are never followed).

use std::collections::HashSet;

use loka_core::{retract_set, TermDictionary, TermId, Triple, TripleStore, PROP_INFERRED_FROM};

/// Small deterministic xorshift RNG (no dependency needed).
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

type Key = (TermId, TermId, TermId);

fn key(t: &Triple) -> Key {
    (t.subject, t.predicate, t.object)
}

pub struct Graph {
    pub store: TripleStore,
    pub dict: TermDictionary,
    pub entities: Vec<TermId>,
    /// Every row inserted into the store.
    pub all_rows: Vec<Triple>,
    /// (generated triple, cited source triple).
    pub cites: Vec<(Triple, Triple)>,
    /// (generated triple, one of its annotation rows).
    pub annotations: Vec<(Triple, Triple)>,
}

/// `n_entities` entities, `n_real` real triples, `n_gen` generated triples,
/// each citing 1–3 sources; about 5 % of generated triples also cite a
/// *later* generated triple, which creates cycles.
pub fn generate(seed: u64, n_entities: usize, n_real: usize, n_gen: usize) -> Graph {
    let mut rng = Rng::new(seed);
    let mut dict = TermDictionary::new();
    let mut store = TripleStore::new();
    let entities: Vec<TermId> = (0..n_entities)
        .map(|i| dict.intern(&format!("http://ex.org/e{i}")))
        .collect();
    let preds: Vec<TermId> = (0..20)
        .map(|i| dict.intern(&format!("http://ex.org/p{i}")))
        .collect();
    let gen_preds: Vec<TermId> = (0..5)
        .map(|i| dict.intern(&format!("http://ex.org/g{i}")))
        .collect();
    let inferred = dict.intern(PROP_INFERRED_FROM);
    let gen_by = dict.intern("http://loka.dev/provenance/propositionGeneratedBy");
    let model = dict.intern("\"model-x\"");

    let mut all_rows = Vec::new();
    let mut seen: HashSet<Key> = HashSet::new();
    let mut real = Vec::new();
    while real.len() < n_real {
        let t = Triple::new(
            entities[rng.below(n_entities)],
            preds[rng.below(preds.len())],
            entities[rng.below(n_entities)],
        );
        if seen.insert(key(&t)) {
            store.insert(t).unwrap();
            all_rows.push(t);
            real.push(t);
        }
    }

    let mut gens: Vec<Triple> = Vec::new();
    while gens.len() < n_gen {
        let t = Triple::new(
            entities[rng.below(n_entities)],
            gen_preds[rng.below(gen_preds.len())],
            entities[rng.below(n_entities)],
        );
        if seen.insert(key(&t)) {
            store.insert(t).unwrap();
            all_rows.push(t);
            gens.push(t);
        }
    }

    let mut cites = Vec::new();
    let mut annotations = Vec::new();
    let cite = |store: &mut TripleStore,
                dict: &mut TermDictionary,
                all_rows: &mut Vec<Triple>,
                cites: &mut Vec<(Triple, Triple)>,
                annotations: &mut Vec<(Triple, Triple)>,
                g: Triple,
                src: Triple| {
        let gq = dict.register_quoted(g.subject, g.predicate, g.object);
        let sq = dict.register_quoted(src.subject, src.predicate, src.object);
        let a = Triple::new(gq, inferred, sq);
        if !store.contains(&a) {
            store.insert(a).unwrap();
            all_rows.push(a);
            cites.push((g, src));
            annotations.push((g, a));
        }
    };

    for i in 0..gens.len() {
        let g = gens[i];
        for _ in 0..(1 + rng.below(3)) {
            // Cite a real triple or an earlier generated one.
            let src = if i == 0 || rng.below(2) == 0 {
                real[rng.below(real.len())]
            } else {
                gens[rng.below(i)]
            };
            cite(
                &mut store,
                &mut dict,
                &mut all_rows,
                &mut cites,
                &mut annotations,
                g,
                src,
            );
        }
        if i + 1 < gens.len() && rng.below(20) == 0 {
            let later = gens[i + 1 + rng.below(gens.len() - i - 1)];
            cite(
                &mut store,
                &mut dict,
                &mut all_rows,
                &mut cites,
                &mut annotations,
                g,
                later,
            );
        }
        let gq = dict.register_quoted(g.subject, g.predicate, g.object);
        let a = Triple::new(gq, gen_by, model);
        store.insert(a).unwrap();
        all_rows.push(a);
        annotations.push((g, a));
    }

    Graph {
        store,
        dict,
        entities,
        all_rows,
        cites,
        annotations,
    }
}

/// The intended closure, computed by fixpoint over the generator's lists.
pub fn reference(graph: &Graph, root: TermId) -> HashSet<Key> {
    let mut removed: HashSet<Key> = graph
        .all_rows
        .iter()
        .filter(|t| t.subject == root || t.object == root)
        .map(key)
        .collect();
    loop {
        let mut grew = false;
        for (g, src) in &graph.cites {
            if removed.contains(&key(src)) && removed.insert(key(g)) {
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    for (g, a) in &graph.annotations {
        if removed.contains(&key(g)) {
            removed.insert(key(a));
        }
    }
    removed
}

fn check(seed: u64, n_entities: usize, n_real: usize, n_gen: usize, roots: usize) {
    let graph = generate(seed, n_entities, n_real, n_gen);
    let mut rng = Rng::new(seed ^ 0xABCD);
    for _ in 0..roots {
        let root = graph.entities[rng.below(graph.entities.len())];
        let got: HashSet<Key> = retract_set(root, &graph.store, &graph.dict)
            .all()
            .map(key)
            .collect();
        let want = reference(&graph, root);
        let missing: Vec<_> = want.difference(&got).take(5).collect();
        let extra: Vec<_> = got.difference(&want).take(5).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "seed {seed} root {root}: {} missing (e.g. {missing:?}), {} extra (e.g. {extra:?}) \
             of {} expected",
            want.difference(&got).count(),
            got.difference(&want).count(),
            want.len()
        );
    }
}

#[test]
fn matches_reference_small_graphs() {
    for seed in 1..=50 {
        check(seed, 30, 60, 60, 10);
    }
}

#[test]
fn matches_reference_medium_graphs() {
    for seed in 100..110 {
        check(seed, 500, 2_000, 2_000, 20);
    }
}
