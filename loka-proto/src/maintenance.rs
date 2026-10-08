//! Background maintenance: rebuild HNSW indexes while the server is idle,
//! off the registry lock, then swap the result in atomically
//! (`planning/background-maintenance.md`).
//!
//! A rebuild has three steps. *Snapshot* copies an index's active vectors
//! under the read lock. *Build* makes a fresh index from the copy with no
//! lock held, so queries keep reading the old index. *Commit* takes the
//! write lock, applies whatever changed in the old index during the build,
//! and replaces it in one `mem::replace`. No query sees a missing or
//! half-built index.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use loka_core::{
    discover_deep_pseudo_tables, discover_pseudo_tables, extract_node_properties,
    PseudoTableRegistry, TermId, TripleStore,
};
use loka_hnsw::{HnswIndex, IndexSnapshot};

use crate::error::ProtoError;
use crate::server::AppState;

/// Request activity and maintenance counters for one server.
#[derive(Debug, Default)]
pub struct Activity {
    /// Milliseconds since the Unix epoch of the last counted request; 0 if none.
    last_request_ms: AtomicU64,
    requests: AtomicU64,
    cycles: AtomicU64,
    tombstones_removed: AtomicU64,
    pseudo_table_refreshes: AtomicU64,
    pseudo_table_hits: AtomicU64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Activity {
    /// Record a request now.
    pub fn touch(&self) {
        self.last_request_ms.store(now_ms(), Ordering::Relaxed);
        self.requests.fetch_add(1, Ordering::Relaxed);
    }

    /// How long since the last counted request (since the epoch if none).
    pub fn idle_for(&self) -> Duration {
        let last = self.last_request_ms.load(Ordering::Relaxed);
        Duration::from_millis(now_ms().saturating_sub(last))
    }

    /// Requests counted so far.
    pub fn requests(&self) -> u64 {
        self.requests.load(Ordering::Relaxed)
    }

    /// Maintenance cycles that rebuilt at least one index.
    pub fn cycles(&self) -> u64 {
        self.cycles.load(Ordering::Relaxed)
    }

    /// Tombstoned HNSW nodes removed by rebuilds so far.
    pub fn tombstones_removed(&self) -> u64 {
        self.tombstones_removed.load(Ordering::Relaxed)
    }

    /// Pseudo-table discoveries run by maintenance so far.
    pub fn pseudo_table_refreshes(&self) -> u64 {
        self.pseudo_table_refreshes.load(Ordering::Relaxed)
    }

    /// Triple patterns answered from pseudo-table columns so far.
    pub fn pseudo_table_hits(&self) -> u64 {
        self.pseudo_table_hits.load(Ordering::Relaxed)
    }
}

/// Pseudo-tables and the store generation they were discovered at.
pub struct DiscoveredTables {
    /// The store's [`TripleStore::generation`] at discovery.
    pub generation: u64,
    /// The tables.
    pub registry: PseudoTableRegistry,
}

/// Run a query with the server's pseudo-tables, counting columnar hits and
/// recording its performance in `state.query_metrics`.
/// Columns are used only while exact and current, so results equal the
/// triple-index path's.
pub fn execute_served(
    state: &AppState,
    query: &loka_sparql::Query,
    store: &TripleStore,
    dict: &loka_core::TermDictionary,
    vectors: &loka_hnsw::VectorRegistry,
) -> Result<loka_sparql::QueryResult, ProtoError> {
    let tables = state.pseudo_tables.read().map_err(lock_err)?;
    let (result, hits) = loka_sparql::execute_instrumented(
        query,
        store,
        dict,
        vectors,
        &loka_core::DatabaseConfig::default(),
        tables.as_ref().map(|t| &t.registry),
        Some(&state.query_metrics),
    )?;
    state
        .activity
        .pseudo_table_hits
        .fetch_add(hits as u64, Ordering::Relaxed);
    Ok(result)
}

/// Rediscover pseudo-tables if there are none yet or the store has changed
/// since the last discovery. Holds the store's read lock for the discovery
/// (queries go on; writes wait), so it runs only from the idle cycle.
/// Returns the number of tables if it ran.
pub fn refresh_pseudo_tables(state: &AppState) -> Result<Option<usize>, ProtoError> {
    let store = state.store.read().map_err(lock_err)?;
    let current = store.generation();
    let due = match state.pseudo_tables.read().map_err(lock_err)?.as_ref() {
        None => true,
        Some(t) => t.generation != current,
    };
    if !due {
        return Ok(None);
    }
    let mut registry = discover_pseudo_tables(&extract_node_properties(&store), &store);
    // Deep (multi-hop) tables answer chain queries from exact path columns
    // (planning/deep-pseudo-table-serving.md).
    registry.deep = discover_deep_pseudo_tables(&store);
    let count = registry.len() + registry.deep.len();
    *state.pseudo_tables.write().map_err(lock_err)? = Some(DiscoveredTables {
        generation: current,
        registry,
    });
    drop(store);
    state
        .activity
        .pseudo_table_refreshes
        .fetch_add(1, Ordering::Relaxed);
    Ok(Some(count))
}

/// When background maintenance runs.
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// Rebuild only after this long without a request.
    pub idle: Duration,
    /// Rebuild an index only if at least this fraction of its nodes is deleted.
    pub min_deleted_ratio: f64,
    /// How often the loop checks.
    pub check_every: Duration,
}

impl MaintenanceConfig {
    /// Idle threshold `idle`, deleted-ratio threshold 0.1, checking every
    /// `idle` (at most every 30 s, at least every 100 ms).
    pub fn with_idle(idle: Duration) -> Self {
        Self {
            idle,
            min_deleted_ratio: 0.1,
            check_every: idle.clamp(Duration::from_millis(100), Duration::from_secs(30)),
        }
    }
}

/// What one index rebuild did.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RebuildReport {
    /// The vector predicate whose index was rebuilt.
    pub predicate_id: TermId,
    /// Nodes in the old index, tombstones included.
    pub nodes_before: usize,
    /// Nodes in the new index.
    pub nodes_after: usize,
    /// Active nodes in the new index.
    pub active_after: usize,
    /// Tombstones the rebuild removed.
    pub tombstones_removed: usize,
    /// Vectors inserted into the old index during the build, carried over.
    pub inserted_during_build: usize,
    /// Vectors deleted from the old index during the build, carried over.
    pub deleted_during_build: usize,
}

fn lock_err(e: impl std::fmt::Display) -> ProtoError {
    ProtoError::BadRequest(format!("lock: {e}"))
}

/// Step 1: snapshots of every index whose deleted ratio is at least
/// `min_deleted_ratio` and which has at least one tombstone. Read lock only.
pub fn snapshot_due(
    state: &AppState,
    min_deleted_ratio: f64,
) -> Result<Vec<(TermId, IndexSnapshot)>, ProtoError> {
    let vectors = state.vectors.read().map_err(lock_err)?;
    Ok(vectors
        .predicates()
        .into_iter()
        .filter_map(|p| {
            let index = vectors.get(p)?;
            let due =
                index.len() > index.active_count() && index.deleted_ratio() >= min_deleted_ratio;
            due.then(|| (p, index.active_snapshot()))
        })
        .collect())
}

/// Step 2: build fresh indexes. No lock held.
pub fn build(snapshots: Vec<(TermId, IndexSnapshot)>) -> Vec<(TermId, HnswIndex)> {
    snapshots
        .into_iter()
        .map(|(p, s)| (p, HnswIndex::from_snapshot(s)))
        .collect()
}

/// Step 3: under the write lock, bring each fresh index up to date with its
/// old one and swap it in.
pub fn commit(
    state: &AppState,
    built: Vec<(TermId, HnswIndex)>,
) -> Result<Vec<RebuildReport>, ProtoError> {
    let mut vectors = state.vectors.write().map_err(lock_err)?;
    let mut reports = Vec::new();
    for (p, mut fresh) in built {
        let Some(old) = vectors.get(p) else {
            continue; // predicate gone meanwhile
        };
        let (inserted, deleted) = fresh.catch_up(old);
        let tombstones = |i: &HnswIndex| i.len() - i.active_count();
        let report = RebuildReport {
            predicate_id: p,
            nodes_before: old.len(),
            nodes_after: fresh.len(),
            active_after: fresh.active_count(),
            tombstones_removed: tombstones(old).saturating_sub(tombstones(&fresh)),
            inserted_during_build: inserted,
            deleted_during_build: deleted,
        };
        vectors.replace_index(p, fresh);
        reports.push(report);
    }
    Ok(reports)
}

/// Snapshot, build and commit every index that is due, and count the cycle.
pub fn rebuild_indexes(
    state: &AppState,
    min_deleted_ratio: f64,
) -> Result<Vec<RebuildReport>, ProtoError> {
    let snapshots = snapshot_due(state, min_deleted_ratio)?;
    if snapshots.is_empty() {
        return Ok(Vec::new());
    }
    let reports = commit(state, build(snapshots))?;
    if !reports.is_empty() {
        let removed: usize = reports.iter().map(|r| r.tombstones_removed).sum();
        state.activity.cycles.fetch_add(1, Ordering::Relaxed);
        state
            .activity
            .tombstones_removed
            .fetch_add(removed as u64, Ordering::Relaxed);
    }
    Ok(reports)
}

/// Every `config.check_every`, if no request has arrived for `config.idle`,
/// rebuild the indexes that are due on a blocking thread. Runs until the
/// task is dropped.
pub async fn maintenance_loop(state: Arc<AppState>, config: MaintenanceConfig) {
    loop {
        tokio::time::sleep(config.check_every).await;
        if state.activity.idle_for() < config.idle {
            continue;
        }
        let s = state.clone();
        let ratio = config.min_deleted_ratio;
        match tokio::task::spawn_blocking(move || rebuild_indexes(&s, ratio)).await {
            Ok(Ok(reports)) if !reports.is_empty() => {
                tracing::info!("maintenance: rebuilt {} HNSW index(es)", reports.len());
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!("maintenance: rebuild failed: {e}"),
            Err(e) => tracing::warn!("maintenance: rebuild task failed: {e}"),
        }
        let s = state.clone();
        match tokio::task::spawn_blocking(move || refresh_pseudo_tables(&s)).await {
            Ok(Ok(Some(n))) => tracing::info!("maintenance: {n} pseudo-table(s) discovered"),
            Ok(Ok(None)) => {}
            Ok(Err(e)) => tracing::warn!("maintenance: pseudo-table discovery failed: {e}"),
            Err(e) => tracing::warn!("maintenance: pseudo-table task failed: {e}"),
        }
    }
}
