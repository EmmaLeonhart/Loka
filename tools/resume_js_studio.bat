@echo off
REM Durable local resume of the JS/HTML Studio build (Flutter Dart -> JS).
REM Registered as the one-shot Scheduled Task "loka-js-studio-resume" so
REM it survives this Claude session ending on usage limits. Plan +
REM running-process/restart notes live in planning/js-studio.md; this
REM only bootstraps a headless Claude run pointed at it.
cd /d "C:\Users\Immanuelle\Documents\Github\SutraDB"
if not exist "training\logs" mkdir "training\logs"
echo === js-studio resume %DATE% %TIME% === >> "training\logs\js_studio_resume.log"
"C:\Users\Immanuelle\.local\bin\claude" -p "Autonomous resume (chat context is gone — work only from files). Read planning/js-studio.md and the 'NEXT (paused) -> JS/HTML Studio' block in queue.md, then barrel through the JS/HTML Loka Studio build incrementally: slices 1 (shell + embedded /browse vis-network graph) through 6 (polish), in a new web-studio/ dir, porting the Dart screens in loka-studio/lib to JS. The graph is ALWAYS the engine's /browse viewer, never a ported canvas. Keep the Flutter Studio frozen (do not delete/break it). Commit AND push per slice with a why-focused message; update queue.md in the same commit. Do NOT re-run 'flutter build web' or 'npm install' (already done; thermal-sensitive laptop per CLAUDE.md). If the local endpoint is needed, restart per planning/js-studio.md (playground_server on :3030 = Shinto demo; kill the old exe first, it locks the binary). Keep going as far as you can; leave queue.md + the commit trail so a later session can continue." --dangerously-skip-permissions >> "training\logs\js_studio_resume.log" 2>&1
echo === js-studio resume done %DATE% %TIME% === >> "training\logs\js_studio_resume.log"
