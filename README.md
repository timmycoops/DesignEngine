# Zovlyn Pit Design Engine

Fast 3D pit design and coal reserves: face projection, ramps, surfaces, strips and blocks, block solids
and reserves — built with a Rust engine and a Tauri + React + three.js interface.

## Getting started (Windows)

1. Run the prerequisite check from PowerShell in the repo root:

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\check-prereqs.ps1
   ```

   Install anything it flags, then run it again until everything is green.
2. Start Claude Code in the repo root: `claude`
3. Paste the prompt in `FIRST_TASK.md` to begin Phase 0.

## Layout

See `CLAUDE.md` for the stack, engine rules and folder layout, and `docs/decisions/` for locked decisions.

## Status

Phase 0 — stress test.
