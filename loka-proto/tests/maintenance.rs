//! Background HNSW maintenance: rebuild off the lock, atomic swap, idle
//! trigger (`planning/background-maintenance.md`).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use loka_core::{TermDictionary, TermId, TripleStore};
use loka_hnsw::{DistanceMetric, VectorPredicateConfig, VectorRegistry};
use loka_proto::maintenance::{
    build, commit, maintenance_loop, rebuild_indexes, snapshot_due, MaintenanceConfig,
};
use loka_proto::AppState;

const PRED: TermId = 1;
const DIMS: usize = 8;

fn vector(i: u64) -> Vec<f32> {
    let mut seed = i.wrapping_mul(0x9e3779b97f4a7c15) | 1;
    (0..DIMS)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 1000) as f32 / 1000.0 + 0.001
        })
        .collect()
}

/// `n` vectors with ids 1000.., the first `deleted` of them tombstoned.
fn state(n: u64, deleted: u64) -> AppState {
    let mut vectors = VectorRegistry::new();
    vectors
        .declare(VectorPredicateConfig {
            predicate_id: PRED,
            dimensions: DIMS,
            m: 8,
            ef_construction: 40,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();
    for i in 0..n {
        vectors.insert(PRED, vector(i), 1000 + i).unwrap();
    }
    for i in 0..deleted {
        assert!(vectors.delete(PRED, 1000 + i));
    }
    AppState {
        store: RwLock::new(TripleStore::new()),
        dict: RwLock::new(TermDictionary::new()),
        vectors: RwLock::new(vectors),
        persistent: None,
        passcode: None,
        rate_limit_per_min: 0,
        rate_counter: AtomicU64::new(0),
        activity: Default::default(),
        pseudo_tables: Default::default(),
    }
}

fn search(state: &AppState, i: u64) -> Vec<TermId> {
    state
        .vectors
        .read()
        .unwrap()
        .search(PRED, &vector(i), 10, 50)
        .unwrap()
        .iter()
        .map(|r| r.triple_id)
        .collect()
}

fn tombstones(state: &AppState) -> usize {
    let v = state.vectors.read().unwrap();
    let index = v.get(PRED).unwrap();
    index.len() - index.active_count()
}

#[test]
fn old_index_serves_during_the_build_and_changes_carry_over() {
    let state = state(200, 50);
    let snapshots = snapshot_due(&state, 0.1).unwrap();
    assert_eq!(snapshots.len(), 1);
    let built = build(snapshots);

    // In the gap between build and commit, the old index still answers,
    // and it changes: one vector added, one more deleted.
    let before = search(&state, 120);
    assert!(!before.is_empty());
    assert!(
        before.iter().all(|id| *id >= 1050),
        "no tombstoned id returned"
    );
    {
        let mut v = state.vectors.write().unwrap();
        v.insert(PRED, vector(9999), 5000).unwrap();
        assert!(v.delete(PRED, 1100));
    }

    let reports = commit(&state, built).unwrap();
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!(r.inserted_during_build, 1);
    assert_eq!(r.deleted_during_build, 1);
    assert_eq!(
        r.tombstones_removed, 50,
        "51 before; 1 left (deleted during build)"
    );

    let v = state.vectors.read().unwrap();
    let index = v.get(PRED).unwrap();
    assert_eq!(index.active_count(), 150);
    assert_eq!(index.len(), 151);
    assert!(index.vector_of(5000).is_some(), "insert during build kept");
    assert!(index.vector_of(1100).is_none(), "delete during build kept");
    assert!((0..50).all(|i| index.vector_of(1000 + i).is_none()));
    drop(v);

    let after = search(&state, 120);
    assert!(!after.is_empty());
    assert!(after.iter().all(|id| *id >= 1050 && *id != 1100));
}

#[test]
fn queries_never_see_a_missing_index_during_repeated_swaps() {
    let state = Arc::new(state(400, 0));
    let done = AtomicBool::new(false);
    let queries = AtomicU64::new(0);
    std::thread::scope(|s| {
        s.spawn(|| {
            let mut i = 0;
            while !done.load(Ordering::Relaxed) {
                let got = state
                    .vectors
                    .read()
                    .unwrap()
                    .search(PRED, &vector(i % 400), 5, 30);
                let got = got.expect("index present and searchable");
                assert!(!got.is_empty(), "a query came back empty");
                queries.fetch_add(1, Ordering::Relaxed);
                i += 1;
            }
        });
        for cycle in 0..20u64 {
            {
                let mut v = state.vectors.write().unwrap();
                for j in 0..5 {
                    v.delete(PRED, 1000 + cycle * 5 + j);
                }
            }
            let reports = rebuild_indexes(&state, 0.0).unwrap();
            assert_eq!(reports.len(), 1);
        }
        done.store(true, Ordering::Relaxed);
    });
    assert!(queries.load(Ordering::Relaxed) > 0);
    assert_eq!(state.activity.cycles(), 20);
    assert_eq!(state.activity.tombstones_removed(), 100);
    assert_eq!(tombstones(&state), 0);
}

#[test]
fn below_the_threshold_nothing_is_rebuilt() {
    let state = state(100, 5); // 5% deleted
    assert!(rebuild_indexes(&state, 0.1).unwrap().is_empty());
    assert_eq!(tombstones(&state), 5);
    let clean = self::state(100, 0);
    assert!(
        rebuild_indexes(&clean, 0.0).unwrap().is_empty(),
        "no tombstones"
    );
    assert_eq!(clean.activity.cycles(), 0);
}

#[tokio::test]
async fn the_loop_waits_for_idle() {
    let state = Arc::new(state(200, 60));
    let config = MaintenanceConfig {
        idle: Duration::from_millis(400),
        min_deleted_ratio: 0.1,
        check_every: Duration::from_millis(50),
    };
    let task = tokio::spawn(maintenance_loop(state.clone(), config));

    // Busy for 1.2 s: a request every 50 ms.
    for _ in 0..24 {
        state.activity.touch();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(state.activity.cycles(), 0, "no rebuild while busy");
    assert_eq!(tombstones(&state), 60);

    // Idle: the rebuild runs once, then there is nothing left to do.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(state.activity.cycles(), 1);
    assert_eq!(tombstones(&state), 0);
    task.abort();
}
