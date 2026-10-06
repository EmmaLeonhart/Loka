@echo off
cd /d "%~dp0"
claude "Read queue.md and work its FIRST item, on its own, never combined with another item. Do not skip it, even if it says it is waiting on Emma or blocked: if it is stuck on a question, make the call yourself, do it, and record the decision and why in devlog.md. Only when it is done go on to the next item, the new first one. Follow the queue-driven-workflow skill: finish an item, delete it from queue.md, append a dated devlog.md entry in the same commit, then push. Ask me before anything destructive." --name Loka --remote-control Loka
