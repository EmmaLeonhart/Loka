# Loka — Work Queue

**This file is a queue, not a state snapshot.** It lists what is being worked on right at this moment. Finished work lives in `git log` and `DEVLOG.md`. Longer-horizon work lives in `TODO.md`. Items migrate `TODO.md` → `queue.md` → deleted on completion.

See the Loka-repo `CLAUDE.md` for the canonical convention; the short version is *update this file in the same commit as the work, and mirror items into the task tool.*

---

## Open engine bugs

### Engine bug #1 sustained-ingest verification (open)

Probable fix shipped in `c36760b`: explicit `sled::Config` with 256 MB cache, 2 s flush, `Mode::HighThroughput`. Reopen-in-place verified 2026-05-13 (WAL replay recovered 32,877,248 triples). Residual question: does the tuning also hold against fresh sustained ingest past 32.88 M triples? If a re-test ingest panics at the next plateau, escalate to RocksDB migration (sled 0.34 unmaintained since 2021). Not blocking under the current base+retrieval pivot — training corpus is no longer the bottleneck.

### Stuff Emma says to do

Take everything in this list as being the final part of the queue as you are barreling through it in order. 

So, yeah, I'm not prioritising this quite as much as I am with some other things. I feel like we're in a situation right now where I want this to be something that is mature and kind of able to be shown on a portfolio and usable by a general audience, such that they could at the very least use the graphical user interface and some level of querying. 

As far as things to do:
- Premium Tier is not something that I have any interest in doing anymore.
- Maven Central is nothing I have interest in doing anymore.
- The NPM Package and the Python Package are basically the last things I actually am interested in.
I don't know the degree to which I'll be willing to maintain them either.


The idea behind what we're doing right now is that I am trying to make this able to be used by people. 

I do want us to actually do a lot of work on organising this to figure out what we have for the paper and what is acceptable right now. I want us to viciously prioritise the stuff that we are doing in the paper, because I believe that we have a lot of stuff going on and it's definitely worth doing a general overview of the thing here. 

We are not going to, in the immediate term, do any more training. We have our prompt engineering that works fine for this, and I don't think that training is necessarily going to be that much more helpful. It's open in the long run. I am maintaining this project, but it isn't of extreme importance. 


I want there to be a relatively easy way to download this from the website and run it. I would say that if you are downloading with the.exe installer, the.exe file installer that we have does not install Loka Studio, and Loka Studio is basically essential for this. 

I would argue that, at this point, without Loka Studio being something that works well enough to show what's going on, the usage of this actually as a database is going to mostly just be kind of a niche thing of all of my projects and nothing else. This is because Loka Studio explains what this actually is and what it does. 

Important things. Important things here. We have, for some important things that we have going on for Loka Studio, the recursive deletion of incorrect information or dependencies. We need that recursive deletion to be something that is working. We need to have the growing of the information to be working. 

I am going to say no to doing much to build our importers into the Knowledge Graph form and our other stuff. No enrollment

I am going to say that I think that Loka Studio, as it is right now, is relatively bloated in structure. The main thing is it is way, way too easy to see the embedding HNSW stuff, which isn't what I want. Because I think it almost implies it's the default instead of a weird debug mode 

I'm also gonna say that I think that we have a lot of bloated content that doesn't really belong in the repository, and I'm not sure why it is still there. For example, I believe our GitHub repos. There's a lot of Flutter code, but there shouldn't be Flutter code, since I'm pretty sure we just skipped it there. We should be doing an audit of all the stuff in the repository that we might be able to potentially remove and not cause issues. 

### Repo audit — findings recorded, awaiting one decision

Audit done and written up: `planning/repo-audit.md` (measured 2026-05-30). Concrete plan:

- **Safe to remove (mechanical, follow-up ticks, CI-verified each):** committed `loka-studio/electron/node_modules/`; the `loka-retrieval-data-stale-20260520/` husk; the mojibake tracked file `\357\200\277qp`.
- **NEEDS EMMA'S DECISION:** the Flutter Studio tree (`loka-studio/`, ~92 files — the single largest dir). This *is* the "Flutter code that shouldn't be here", but DEVLOG 2026-05-17 shows it was deliberately frozen as a fallback after `web-studio/` (the live JS Studio) replaced it. Delete it entirely (B-i), keep it frozen (B-ii), or archive to `legacy/` (B-iii)? Recommend B-i once `web-studio/` is confirmed to cover the six Studio tabs.
- **Investigate before touching:** is `loka-ffi/` now orphaned (no Flutter consumer)?; stale root-level benchmark/stress JSONs; `.git` 138 MiB pack (history rewrite → TODO.md only).

Next autonomous step is blocked on the Flutter decision; the Category-A removals can proceed independently.

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
