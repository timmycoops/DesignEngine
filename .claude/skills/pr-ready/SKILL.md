---
name: pr-ready
description: Run locally what CI runs, in CI's order, fix what fails, and report each step's result and time. Use before pushing a branch or opening a PR.
allowed-tools: Bash(cargo test *) Bash(cargo clippy *) Bash(pnpm install --frozen-lockfile) Bash(pnpm run typecheck) Bash(pnpm run test) Bash(pnpm run build) Bash(.venv/Scripts/maturin develop *) Bash(.venv/Scripts/python crates/zd-py/tests/test_session.py)
---
Run CI's checks (`.github/workflows/ci.yml`) from the repo root, in this order. Stop at the first failure, fix it, and
run that step again before going on. Run each Rust step with a 10-minute timeout; a cold build takes several minutes.

1. Engine tests: `cargo test --workspace --exclude zd-app --exclude bench --exclude zd-py`
2. Engine lint: `cargo clippy --workspace --exclude zd-app --exclude bench --exclude zd-py -- -D warnings`
3. Python bindings:
   - `cargo clippy -p zd-py -- -D warnings`
   - `VIRTUAL_ENV="$PWD/.venv" .venv/Scripts/maturin develop --release -m crates/zd-py/Cargo.toml`
   - `.venv/Scripts/python crates/zd-py/tests/test_session.py`
   (No `.venv` yet: make it as CLAUDE.md's Build and test says, then carry on.)
4. App, in `app/`: `pnpm install --frozen-lockfile`, `pnpm run typecheck`, `pnpm run test`, `pnpm run build`
5. Tauri shell, after the app build (it embeds it): `cargo clippy -p zd-app --all-targets -- -D warnings`, then
   `cargo test -p zd-app`

Skip step 4 and 5's pnpm parts only when the branch touches nothing under `app/`, and say so.

Rules:
- Never run `cargo fmt` on the workspace: the repo isn't default-rustfmt formatted and it rewrites every file.
- A fix that changes behaviour (not just lint) needs its own test, as CLAUDE.md says.
- CI also runs steps 1 to 3 on Linux, which can't be checked here; say so if the change touches paths, line endings or
  anything platform-specific.

Report one table: step, result (pass, fixed, failed), time, and what was fixed.
