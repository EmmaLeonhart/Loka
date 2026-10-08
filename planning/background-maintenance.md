# Background maintenance: idle-triggered HNSW rebuild with an atomic swap (Phase 5)

Phase 5 of `planning/large-features.md`. Written before the code.

## What happens now

- Deleting a vector tombstones its HNSW node; the node stays in the graph (still traversed
  for connectivity) until a rebuild.
- The only rebuild is `POST /vectors/rebuild` → `HnswIndex::compact()`, which rebuilds **in
  place under the registry's write lock**. Every query (which takes the read lock) blocks for
  the whole rebuild. Nothing triggers it automatically.

## What to build

1. **Rebuild off the lock, in three steps** (`loka-hnsw`):
   - *snapshot* (read lock, a copy): the active `(vector, triple_id)` pairs plus the config;
   - *build* (no lock): a fresh index from the snapshot;
   - *commit* (write lock, short): bring the fresh index up to date with what changed during
     the build (vectors inserted since the snapshot are inserted; vectors deleted since are
     deleted), then swap it in with `std::mem::replace`.
   The index is replaced inside one write-lock section, so no query sees a missing or
   half-built index; queries during the build read the old index.
   API: `HnswIndex::active_snapshot()`, `HnswIndex::from_snapshot(..)`,
   `HnswIndex::catch_up(&mut self, old: &HnswIndex)`, `VectorRegistry::replace_index(..)`.
2. **`loka-proto::maintenance`**:
   - `Activity`: the last request time and a request count, updated by a middleware on every
     request except `/health` (so a health probe doesn't keep the server "busy").
   - `MaintenanceConfig { idle_secs, min_deleted_ratio, check_every }`.
   - `rebuild_indexes(state, min_deleted_ratio)`: the three steps for each index whose
     deleted ratio is at least the threshold. `POST /vectors/rebuild` uses it too (ratio 0),
     so the manual rebuild stops blocking queries.
   - `maintenance_loop(state, config)`: every `check_every`, if no request for `idle_secs`,
     run `rebuild_indexes` on a blocking thread. It counts cycles and tombstones removed, and
     `/vectors/health` reports the counts.
3. **`loka serve --maintenance-idle-secs N`**: off by default (0). Opt-in, per the
   SQLite-defaults rule. It is also opt-in for the laptop's thermal envelope: a rebuild is
   CPU-heavy, and here it only ever runs when the server is idle. Default deleted-ratio
   threshold is 0.1.
4. **Pseudo-table rediscovery** in the same cycle belongs to Phase 6 (it needs the
   invalidation tracking that phase builds). The loop gets a hook point there, not a guess.

## Tests

- **Queries during a rebuild answer from the old index.** Snapshot and build; between
  build and commit, a query still answers. Insert one vector and delete another in that gap.
  After the commit: the new index has no tombstones (`len == active_count`), contains the
  vector inserted during the build, and lacks the one deleted during it.
- **No window without an index.** One thread runs queries in a loop while another runs
  rebuild cycles repeatedly. No query errors, and every query returns rows.
- **Idle detection.** The loop does not rebuild while requests keep arriving, and does once
  they stop (tokio test with short intervals, no HTTP).
- **Below the threshold, nothing is rebuilt.**
