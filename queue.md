# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## ▶ RESTART NOTICE — 2026-05-20

This session's predecessor crashed when the computer restarted around 16:00 local on 2026-05-20. Four parallel agentic sessions died at the same time; the local cron jobs they had scheduled (in-memory, not `durable: true`) died with them. Nothing on disk was lost — only chat context and in-memory schedules.

**Prior session transcript:** `crashed_session_2026-05-20.md` at this repo's root (extracted from saved HTML; corresponds to claude.ai session `01VKznHBj287C8bB3AAz63Ru`, shared as "Loka / LokaDB"). Cross-repo context: `C:\Users\Immanuelle\Documents\Github\SESSION_RESTART_2026-05-20.md`.

**📋 The `chat archive` block(s) below this notice are LOAD-BEARING — read them, do not delete.** Emma manually pasted excerpts of the pre-crash session conversation directly into this `queue.md` to capture execution detail past where the auto-extract truncated. This is the *intended pattern* for recovering from session-crash context loss: paste the relevant chat into the queue so the next session has it inline rather than chasing auth-gated session URLs. The bullet summary in this notice is a navigation aid; the archive below is the substance. Leave the archive in place until the recovery is visibly complete (queue items reconciled, next steps executed); then it can be deleted alongside this notice.

**Where the session actually left off** (per the in-line archive below):
1. Earlier session work: rebase + push of 19 commits; stale "Live training" banners removed; sitemap fixed; clawRxiv paper trimmed to ≤5,000 chars and posted as **post 2601 (v8)**; SPARQL quoted-triple regression test added and passing; double-click demo wired up + tested end-to-end on Q42 (services: `:3031` Loka 22 142 triples + 3 vector indexes, `:8092` Qwen 2.5 1.5B + MiniLM, `:3030` separate Shinto 75-triple demo). Working URL: `file:///C:/Users/Immanuelle/Documents/Github/SutraDB/tools/browse.html?ep=http://localhost:3031&infer=http://localhost:8092`.
2. **User asked to kill + restart `:3031`** to see the freshly-embedded `browse.html`. The restart **exposed a real engine bug**: vector predicate registry corrupted on sled rehydrate — `predicate 10711` came back as an f32vec literal, `predicate 10712` (tripleEmb) missing entirely. Reloaded state was 12,700 triples + 2 indexes vs the 22,142 + 3 from before restart.
3. **Recovery in progress when the crash hit:** `:3031` was restarted serving a fresh empty `loka-retrieval-data/`; broken state parked at `loka-retrieval-data-stale-20260520/`; `python tools/load_retrieval_loka.py` was running in background task `b23rkxw6x` to repost graph.nt + 3 vector files (~16 + 30 MB at ~30 req/sec, ETA ~3–5 min from 15:33). Binary was rebuilt (`cargo build --release -p loka-cli`).
4. **Dead cron:** job `1047b223` was scheduled for 16:33 local — in-memory, died with the session, never fired. It was meant to verify the reload + probe `/generate` + start digging into the engine persistence bug.

**Resume by:** **(0) Clear the bloated queue first — see warning below.** (1) Check whether the `:3031` reload finished — `curl http://localhost:3031/sparql` or open the browse URL above; (2) probe `/generate` end-to-end on Q42 to confirm the demo still works; (3) re-create the engine-persistence-bug investigation cron with `durable: true` this time; (4) the parked engine bug (vector predicate registry corruption on sled rehydrate) is the substantive next chunk of work. The broken state at `loka-retrieval-data-stale-20260520/` is the artifact to dissect.

**⚠ Clear the bloated queue before adding new work.** `queue.md` and `TODO.md` accumulated items across many sessions. Walk every item below against `git log --since="2026-05-19"` and **delete** anything already shipped (per CLAUDE.md: queue is delete-only — no checkmarks, no "done" markers, no progress narration). The clawRxiv paper revision, the SPARQL quoted-triple regression test, the double-click demo, and the website fixes from this session are top deletion candidates. This is the FIRST step, before any new execution.

If a queued item calls for a cron job, re-create it locally and immediately per the new CLAUDE.md §"Cron requests" rule (pinned 2026-05-20). Prefer `durable: true`. v11+ training freeze and laptop-thermal constraints in CLAUDE.md still apply.

Delete this notice and `crashed_session_2026-05-20.md` once the queue has visibly resumed.

chat archive

Commit and push whatever changes are currently on local to remote and then start barreling through all of our tasks.


Ran 2 commands, read a file
Remote Control connecting…


Ran 16 commands, read a file, used 6 tools
I'll rebase first, then handle the untracked files. The duplicate "anti-honest" commit should auto-drop via patch-id during rebase.

Rebased onto
origin/main
Clean rebase — the duplicate anti-honest commit was auto-skipped. Now let me update .gitignore and push.


Read a file, edited a file, ran 3 commands
Pushed. Now let me look at the actual queue items to start working through.


Ran a command, searched code, read 2 files, used 2 tools
Now I'll truth-pass both banners and the contribute lead. Training is dead, base+retrieval is the pivot.


Edited 2 files, searched code, ran 8 commands, read a file, used 3 tools
Now let me verify the deploy and theme render once it lands.


Ran 3 commands, read a file, browsed the web
Pages deploy verified — banners removed, contribute lead updated, both pages live. Marking task 3 done. Now checking task 4.


Ran 6 commands, searched code, used 2 tools
Sitemap has a stale /theory/sutradb/ (the dir was renamed to loka) and is missing /loka/, /history/, /contribute/, /benchmarks/. Let me fix that.


Ran 4 commands, read 2 files, edited 2 files
Now let me prune queue.md to reflect what's done this session.


Read 3 files, edited 3 files, ran 11 commands, searched code, used 4 tools
Let me do a final read of the queue to see what else is in immediate reach.


Ran 3 commands, read a file, edited a file
Done. Summary of this session:

Sync + push:

Rebased 19 local commits onto remote's 3 (duplicate anti-honest commit auto-dropped by patch-id)
gitignored loka-retrieval-data/ (92M sled DB) and tools/_*.py (throwaway probes)
Truth-pass: the "Live training run in progress" banners on the homepage and /contribute/ were stale (training was retired 2026-05-18). Removed them; rewrote the contribute lead to put the v14 from-scratch donor run front and centre with a footnote on the retired QLoRA track. Verified live on loka.emmaleonhart.com.

Repo-rebrand follow-ups: the 2026-05-10 SutraDB → Loka pass left two orphaned artifacts — sitemap pointing at the deleted /theory/sutradb/ and a TODO.md "Loka → Loka" checklist where both sides of the arrow had become identical. Fixed sitemap (4 missing pages added) and deleted the TODO block.

Doc sync: updated status.md (was dated 2026-05-16, missed the pivot) and README.md (added a callout about the resident :8092 base+retrieval inference sidecar). Pruned queue.md — Emma's "JS knowledge graph is best" lead was already shipped as web-studio; fine-tune item #5 retired with the lobotomy verdict; repo-rebrand item #7 closed.

What's still blocked: clawRxiv paper-revision POST (external action, awaiting your OK), engine bug #1 sustained-ingest verification + v14 donor run (GPU-gated), engine bug #2 quoted-triple predicate filter (queue explicitly parked as follow-up).

Yeah I'm okay with the ClarkSiv revision posting and all these other things. I want you to barrel through them. I want you to barrel through all these things. I'm okay with all these things.

Green light received. Starting with the clawRxiv paper revision, then engine bug #2 follow-up.


Ran 13 commands, read 2 files, found files, used 3 tools
The abstract is 6684 chars; clawRxiv caps at 5000. That's why 5 recent runs failed. Let me trim it.


Ran 4 commands, read a file, edited a file, used a tool
Now let me look at engine bug #2 while papers-ci runs.


Ran 11 commands, searched code, read 4 files, edited a file, used a tool
Papers-ci succeeded. Let me finish up the engine bug #2 work, then pivot to setting up the double-click demo.


Ran 4 commands
Paper landed as post 2601 (v8). Now pushing the test and setting up the demo.


Ran 9 commands, read 4 files, searched code, used 4 tools
All three already running. Let me verify the wiring and probe end-to-end.


Ran 3 commands
End-to-end works. Now let me check what browse.html needs configured.


Read a file, ran 10 commands, searched code, edited a file, created a file, used 4 tools
Everything is wired up and tested end-to-end. Here's what's running and how to drive it manually.

Live services (do NOT kill — already serving):

:3031 Loka — retrieval graph: 22 142 triples + 3 vector indexes (nodeEmb 2 115 / nameEmb 2 115 / tripleEmb 7 531, 384-d cosine)
:8092 infer sidecar — base Qwen 2.5 1.5B + MiniLM retrieval, pre-warmed
:3030 Loka — separate 75-triple demo (likely Shinto), untouched
End-to-end /generate against Q42 already returned 6 triples in ~30s:

Douglas Adams | birthPlace | Nottingham      (hallucinated — was Cambridge)
Douglas Adams | deathPlace | Los Angeles     (correct)
Douglas Adams | occupation | writer and science fiction author
Douglas Adams | nationality | British
Douglas Adams | genre | science fiction, comedy, fantasy
Douglas Adams | works | The Hitchhiker's Guide ...
To test the double-click yourself, the fastest path:

Open this file:// URL in your browser (the query string auto-fills both endpoints — no field tweak needed):

file:///C:/Users/Immanuelle/Documents/Github/SutraDB/tools/browse.html?ep=http://localhost:3031&infer=http://localhost:8092

Then:

Click Run (Ctrl+Enter) on the default SELECT ?s ?p ?o ... LIMIT 30 to populate the graph.
Double-click any IRI node → ✨ amber dashed edges appear with model-generated triples and propositionConfidence on hover.
A couple of notes:

The embedded /browse on the running engine still serves the OLD browse.html (engines include_str! it at compile time). Rebuilding the binary needs the running :3031 process killed first (it holds the loka.exe lock). Use the file:// route until you're ready to restart.
If you do want to restart :3031 cleanly: stop the existing process, run cargo build --release -p loka-cli, then !retrieval.bat (new launcher pinned to loka-retrieval-data + port 3031).
Q42 is the validated test seed. Other Wikidata Q-numbers in the corpus (run SELECT DISTINCT ?s WHERE { ?s ?p ?o } LIMIT 50 to discover) should also work.
Session also closed out: paper v8 submitted to clawRxiv as post 2601, SPARQL quoted-triple predicate-filter regression test added and passes, all changes pushed to main.

Can you please kill the process and then restart it so that I can see it?


Ran a command, used 2 tools
:3031 is down and loka.exe is gone. Now rebuilding with the updated browse.html embedded.


Ran 4 commands
Restart succeeded but lost some state (was 22142 triples + 3 vec indexes; now 12700 + 2). The tripleEmb index didn't persist. Let me verify the demo still works.


Ran 6 commands, read a file
The restart exposed an engine persistence bug — the rehydrated vector indexes are corrupt (predicate 10711 reads as an f32vec literal, tripleEmb gone entirely). To get a working demo I need to wipe and reload from the on-disk source data (~3 min).


Ran 10 commands, read 2 files, edited a file, used 4 tools
Done. Summary:

Cron scheduled (job 1047b223, fires at 16:33 local — one hour from now): resumes from the queue.md "IN-FLIGHT 2026-05-20 15:30" section I just committed (commit 22e725b, pushed). The cron will verify the reload completed, probe /generate end-to-end, update queue.md with the outcome, and start digging into the engine persistence bug if usage allows.

Current state of the demo box:

:3031 Loka serving a fresh empty loka-retrieval-data/; the broken state is parked at loka-retrieval-data-stale-20260520/.
python tools/load_retrieval_loka.py running in background task b23rkxw6x — posting graph.nt + 3 vector files (~16 + 30 MB at ~30 req/sec, ETA ~3-5 min from 15:33).
:8092 sidecar untouched.
Binary rebuilt (cargo build --release -p loka-cli), so the new browse.html with auto-resolving endpoints is now embedded in the running :3031 engine. Once the reload finishes, opening http://localhost:3031/browse should just work — endpoint + infer fields auto-fill, double-click any IRI node to expand.
Engine bug recorded in queue.md: vector persistence-restart bug (vector predicate registry corrupted on sled rehydrate). Likely a term-dictionary slot collision with vector-literal interning. Cron will investigate if there's headroom.

---

## ⏳ Demo state on 2026-05-20 post-crash

Pre-crash state was: :3031 fresh-empty + background reload in flight + :8092 sidecar warm.
**Post-crash actual state (verified 2026-05-20 ~17:13 PT):** both services down. `loka-retrieval-data/` is 9.5 MB (the reload background task that was supposed to repost graph.nt + 3 vector files died with the session before completing). `loka-retrieval-data-stale-20260520/` is 93.7 MB and preserved — that's the artifact for the engine-bug investigation, do not delete.

Resurrecting the demo (when Emma asks):
1. `target\release\loka.exe serve --data-dir loka-retrieval-data --port 3031` (or `!retrieval.bat` if it exists)
2. `python tools/load_retrieval_loka.py --endpoint http://localhost:3031` (reposts graph.nt + 3 vector files, idempotent)
3. `python tools/infer_server.py` for the :8092 sidecar
4. ~3–5 min for post + HNSW build; then `http://localhost:3031/browse` is the demo URL.

---

## Open engine bugs

### Engine persistence bug: vector-registry corruption — root cause fixed

**Symptom (2026-05-20):** after `loka serve` reopened a sled data dir holding vector indexes, `/vectors/health` rendered the `nameEmb` predicate slot as an f32vec literal string (`-0.007669 0.080905 …"^^<…/f32vec>`) and `tripleEmb` disappeared entirely. Triple count dropped 22142 → 12700 across the restart.

**Diagnosis (`loka-cli/examples/inspect_vector_triples.rs`, run on the parked artifact `loka-retrieval-data-stale-20260520/` 93.7 MB):** 2113 f32vec-bearing rows have predicate=10710 (well-formed → `nodeEmb`) and 2113 have predicate=10711 (malformed → ID 10711 resolves to the first interned name-embedding literal). Same row count both sides → exactly one bad predicate per legitimate `nameEmb` row.

**Root cause:** in-memory `TermDictionary` and `PersistentStore` have independent term-ID counters. After startup they're aligned (load_terms_into seeds dict from ps), but `/vectors/declare` interned the predicate IRI in the in-memory dict ONLY, drifting its counter past ps's counter. The subsequent `/vectors` POST called `dict.intern` and `ps.intern` independently and got DIFFERENT IDs for the same string. The triple was built from in-memory IDs and written to ps's SPO index — but ps's terms_rev resolved those IDs to whatever else happened to be at those slots (typically the next vector literal interned). On reopen, the corruption became visible.

**Fixes shipped (commits 37ef41e + this one):**
1. `loka-cli serve` rebuild now skips triples whose predicate is a literal-id or inline value (`loka_hnsw::rebuild_from_store`). Prevents poisoned registries on any future corrupt-on-disk state.
2. New `intern_synced` / `intern_object_synced` helpers in `loka-proto` route every intern through `ps.intern` first, then mirror into the in-memory dict via `insert_with_id`. The persistent store becomes the source of truth for term IDs.
3. `declare_vector_predicate` and `insert_vector` refactored to hold dict + ps locks together and use the synced helpers.
4. `declare_and_insert_keeps_dict_and_ps_in_sync` regression test guards the alignment end-to-end.

**Possibly still open:** other asymmetric-intern paths in `loka-proto`. The `/triples` handler uses `dict.intern` directly and lets `ps.insert_batch` re-intern inside its sled transaction — same drift potential. Worth auditing each `dict.intern` call site that has a persistent store attached to confirm there's no equivalent leak. Not blocking the demo (the loader path goes through `/vectors/declare` + `/vectors`, now fixed).

**Artifact:** `loka-retrieval-data-stale-20260520/` is preserved for any further forensic work, and `inspect_vector_triples` is the diagnostic tool for it.

### Engine bug #1 sustained-ingest verification (open)

Probable fix shipped in `c36760b`: explicit `sled::Config` with 256 MB cache, 2 s flush, `Mode::HighThroughput`. Reopen-in-place verified 2026-05-13 (WAL replay recovered 32,877,248 triples). Residual question: does the tuning also hold against fresh sustained ingest past 32.88 M triples? If a re-test ingest panics at the next plateau, escalate to RocksDB migration (sled 0.34 unmaintained since 2021). Not blocking under the current base+retrieval pivot — training corpus is no longer the bottleneck.

---

## Passive follow-ups

- **Donor clean-Adam 10-epoch v14** via `tools/contribute_v14_training.py` — explicit successor experiment per paper §5.12, published at <https://loka.emmaleonhart.com/contribute/>. Waits for a contributor with ≥8 GB VRAM + ~2 days. Do NOT self-launch on the laptop (thermal envelope + v11+ training freeze).
- **Clean v12 retrain** — epoch-4 best 226.86 lost to shared-GPU contention; a clean run would land ~225. GPU-gated.
- **Propgen test (Q42 seed) on v11–v14** — deferred since v11 due to GPU fragility during shared use. GPU-gated.

---

## Reference

- **`TODO.md`** — longer-horizon work (SDK publishing, Maven Central, Cypher/GQL wrappers, premium-tier, ontochronology phases-5+).
- **`DEVLOG.md`** — narrative history.
- **`status.md`** — current operational state.
- **`planning/world-model-thesis.md`** — canonical vision.
- **`planning/cascade-retraction.md`** — spec for the shipped retraction system.
- **`planning/base-retrieval.md`** — spec for the shipped base+retrieval pivot.
