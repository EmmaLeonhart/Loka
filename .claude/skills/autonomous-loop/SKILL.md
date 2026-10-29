---
name: autonomous-loop
description: Use when the user wants a long stretch of autonomous work (hours, overnight, or while they are away) — set up one local cron that every half hour commits and pushes everything and keeps working the queue. Also use when deciding whether that cron should keep running.
---

# Autonomous loop — one cron, every half hour

When the user wants you to keep working on your own for a long stretch (hours,
overnight, while they are away), set up **one** local `CronCreate` job:

- **Schedule:** `7,37 * * * *` (every half hour, off the busy :00/:30 marks),
  recurring.
- **Prompt:** `[cleanvibe cron] Commit and push any and all changes. Then
  work the FIRST item in queue.md, on its own: do it, delete it from
  queue.md, and log it in devlog.md in the same commit. Then the next item is
  the new first item: work it the same way, one at a time, until the queue is
  empty or about 25 minutes have passed. Never look past the current first
  item for another one; if it is stuck on a question, make the call yourself
  and record why. If the queue is empty, refill it
  from the open questions. Re-read INTENT.md and update it if
  your understanding has changed; fill in README.md if the purpose is now
  clear. Check the clock before writing any time down.`

That's the whole loop. Each time it fires:

1. **Commit and push** everything that has changed, with messages that say what
   and why. Push only if the repo has a remote. No empty commits.
2. **Work the first item, then the new first item, one at a time.** Do the item
   at the top of `queue.md`, then delete it from `queue.md` and log it in
   `devlog.md` in the same commit. The item below it is now the first item: work
   it the same way, and keep going until the queue is empty or about 25 minutes
   of the tick have passed (the next tick picks up where this one stopped). Do
   not read down the queue looking for an item you can do, and do not skip the
   first item or move it to the end to get past it: if it is stuck on a
   question, make the call yourself, do it, and record the decision.
   **Why it keeps going (v2.0.6):** with "only that item", a tick did one item
   and sat idle for the rest of the half hour. On 2026-10-17 a queue of 81
   one-minute checks was going to take about 40 hours. Emma: "why does it do it
   once an hour lol" and "Change the loop rule".
   **Why:** told to take "the top item you can do", agents read the whole
   queue, sorted it into what they could and could not do, and raced through
   it. On 2026-10-15 one session closed about forty items overnight, most by
   parking the hard part at the end, and six in twelve minutes that afternoon,
   shipping work it knew was wrong. Emma: "You have to tell it to only do the
   first item and then remove it."
3. **When the queue is empty, refill it before idling.** In a research project,
   take the top open question in `research/SUMMARY.md` and plan it into
   `queue.md`. Otherwise take the next `todo.md` item that is bounded and
   checkable. If there is truly nothing, the tick is **idle**: say so in one
   line. An idle tick is normal, not a problem to solve.
4. **Keep the standing files current.** Re-read `INTENT.md` and update it if
   your understanding has changed (including constraints the user gave in
   chat). Fill in `README.md` once the purpose is clear.

**Why the prompt names these duties:** in the study of cleanvibe's own
transcripts (case study 06), loop ticks did what the tick prompt named
(commit, work the queue) and skipped everything it didn't: no tick in 22 read
`INTENT.md`, which went stale for hours; the README stayed a stub; and an
empty queue was reported idle instead of refilled. Naming a duty in the
prompt is what makes it reliably happen.

The job is session-local (`durable: false`): it fires only while this session
runs, so a later session sets it up again if the user still wants autonomous
work.

## The work keeps the project's normal standards
- Claim something works only after running it. Never weaken, skip or delete a
  test to get green; record the defect instead.
- If you don't understand something well enough to do it, write the question
  down (a queue item, or `INTENT.md`) instead of guessing.

These are how the work is done, not reasons to stop the loop.

## Keep it running
- **Do not turn the cron off yourself.** Not because the queue is empty, not
  because a tick failed, not because something looks risky, and not at the
  end of a burst of work. Only the user stops it (directly, or through the
  `emergency-stop` skill).
- If something goes wrong, say so plainly in your next message and carry on
  with whatever is still safe to do.
- In a cleanvibe project, the switch to work mode in CLAUDE.md decides when
  the loop starts: the user says to start working, or an hour passes without
  a message from them. Elsewhere, start it when the user asks for autonomous
  work.

**Why one cron:** earlier versions ran separate hourly work, flush and status
crons. In practice the flushes and status reports weren't useful, and one
item per hour left long idle stretches (case study 05). One half-hourly
"commit, push, keep going" does the work with less ceremony.

Replication projects (`cleanvibe replicate`) are bounded jobs and do not use
the loop.
