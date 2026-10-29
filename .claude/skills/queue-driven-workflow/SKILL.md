---
name: queue-driven-workflow
description: Use when the work in a cleanvibe project is software development or any other multi-step build — plan into queue.md first, the todo.md → queue.md → devlog.md flow, delete-don't-check completion, task-tool mirroring, and tests/CI discipline.
---

# Queue-driven workflow (development practice)

In a cleanvibe 2 project nothing is set up for this in advance. When the work
turns into building something over several steps, create `queue.md`, `todo.md`
and `devlog.md` if they don't exist yet, and work this way from then on.

## Workflow Rules
- **Commit early and often.** Every meaningful change gets a commit with a clear message explaining *why*, not just what.
- **Plan into `queue.md` first, then execute.** When entering planning mode (or doing any non-trivial multi-step work), the FIRST action is to write the plan into `queue.md` as concrete items. Only then begin executing. This means an interrupted session can resume from the queue — the plan does not live only in chat context.
- **Finishing an item = delete from `queue.md` + append to `devlog.md`, then commit and push.** When a queue item is done, **delete the item from `queue.md`** and **append a dated entry to `devlog.md`** recording what was completed, in the *same commit as the work*, then push (if the repo has a remote). Never mark an item done in place (no `[x]`, no "✓", no "DONE"). `queue.md` only ever holds not-yet-done work; `devlog.md` is where "done" lives.
- **Mirror `queue.md` into the task tool.** TaskCreate items as you add them to queue.md; mark `in_progress` when starting; `completed` when done. The two views must not drift.
- **Keep CLAUDE.md up to date.** As the project takes shape, record architectural decisions, conventions, and anything needed to work effectively in this repo.
- **Update README.md regularly.** It should always reflect the current state of the project for human readers.

## Queue and longer-horizon work
- **`queue.md`** — what's being worked on right now. Items get deleted on completion; do not leave checkmarks or status indicators behind. If it's not in `queue.md`, it's not in scope for the current session.
- **`todo.md`** — the **long-term horizon** of the project. Multi-session goals, architectural ambitions, future capabilities. Items in `todo.md` are *abstract*: they describe a destination, not a step. When work begins, an item is pulled from `todo.md`, decomposed into concrete executable steps in `queue.md`, mirrored into the task tool, and executed. As `queue.md` drains, refill it from the next `todo.md` item.
- **`devlog.md`** — where **"done" lives**. Every finished queue item is deleted from `queue.md` and appended as a dated entry here, in the same commit as the work. Releases (tag + one-line note) and notable milestones also go here.
- **Flow:** `todo.md` (abstract horizons) → `queue.md` (concrete steps) → task tool (in-flight work) → `devlog.md` + `git log` (history). Items only ever flow forward.
- **When to stop and hand back:** `queue.md` is empty, what is left in `todo.md` is still too abstract to break down, and tests (and CI, if the project has a remote) pass.

## Testing
- **Write unit tests early.** As soon as there is testable logic, create a test file. Use `pytest` for Python projects or the appropriate test framework for the language in use.
- **Set up CI once the project has a GitHub remote.** A `.github/workflows/ci.yml` that runs the test suite on push and pull request. Keep it simple — install dependencies and run tests.
- **Keep tests passing.** Do not commit code that breaks existing tests. If a change requires updating tests, update them in the same commit.
