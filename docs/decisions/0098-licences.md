# 0098 — Licences: a signed file checked on the machine, a module a command, the Viewer when it lapses

Date: 2026-11-22 · Status: accepted (Tim, 2026-11-22, the Licensing plan tab: "just build the initial idea to make it
easy as a first step and with the ability to add extra steps in the future if we got it to a commercial state")

## Why

Tim, 21 November: "I never expected to get this far - but it is genuinely better than I expected", and asked about
licensing, selling and deploying. On 22 November he chose to keep the desktop app as the product, with no web or cloud
version for now, and to add a licence module so the app can be shared as a product if it comes to that. The odds of
selling are low, in his words, so the first step is kept small, with room to add floating seats, activation and updates
later.

Before this the app had no licence code, and every command ran for anyone with the exe or `import zd`. All of them, 84,
run through one place, `RunJob::run` in `zd-commands`, for the app and for Python alike.

## Decision

1. **A signed licence file, `.zdlic`.** A few lines of JSON (licence number, licensed to, kind, modules, machine codes,
   issued, runs until, updates until) signed with an Ed25519 key only Zovlyn holds, through the `ed25519-dalek` crate,
   a new dependency Tim agreed. The app carries the public keys it trusts, by number, and checks the signature on the
   machine with no internet: change one character and it fails.
2. **Modules, every command in one.** Viewer (free: open, show, measure, interrogate, compare, import, export, layers),
   Design (strings, faces, ramps, surfaces, workflows, editing objects), Solids and blocks (solids, cuts, blocks,
   benches, seam models), Reserves (parameters, dataflows and their formulas, reserves runs), and Python (`import zd`,
   checked by the bindings). Each command names its module where it's registered, in `builtin::register_all`, read
   there as one list.
3. **Tied to machines.** A machine code is a hash of Windows' machine id, never the id; a licence lists the codes it
   runs on, or none for any machine. Floating seats on a site server, seats counted on a shared server, and online
   activation wait until a customer needs them. A licence carries `kind` and `seats` already so they can be added.
4. **The Viewer when there's no licence or it lapses.** A command outside the modules on is refused, naming its module
   (`refused.not_licensed`), and nothing is changed; projects always open, show, measure and export. Warnings from 30
   days before the end, 14 days' grace after it.
5. **The engine knows only modules.** `zd-commands` refuses a command whose module is off; which modules are on is the
   host's to say (`modules::set_modules_on`), and until it says, every module is on, as for the engine's tests. Reading
   and checking a licence is a new crate, `zd-licence`, used by the app and the Python bindings, never by the engine.
6. **Stamps.** Who it's licensed to and the licence number in each report's footer and each `.zdp` manifest.
7. **`zdlic`, run by Zovlyn.** `zdlic keygen`, `issue`, `show` and `machine`. The signing key is kept off the repo and
   off any build machine; keys are numbered, and a number in `RETIRED` is no longer trusted.
8. **Light guards only.** The check on every command; the newest day the app has seen kept, so a clock wound back more
   than a day before it is caught; no packers or obfuscators, and no need to be online.

## Built

**Engine (PR 1, `licence/engine`).**

- `zd-commands`: `Module`, and `register(module, command)`; `Registry::module_of` and `modules`. `RunJob::run`
  refuses a command whose module is off before it reads anything, so every path through the engine is covered: the
  app's runs, workflow steps and Python. 18 commands in the Viewer, 52 in Design, 11 in Solids and blocks, 3 in
  Reserves.
- `zd-licence`: the licence, signing and reading, the check (`check`, `check_this_machine`), the machine code, the
  folders a licence is looked for in (`ZD_LICENCE`, then `%APPDATA%\Zovlyn\licence.zdlic`, then
  `%PROGRAMDATA%\Zovlyn\licence.zdlic` for every user of a machine), `install`, and the newest day seen
  (`%APPDATA%\Zovlyn\seen`). The machine id is read with Windows' own `reg.exe`, so nothing more is built in.
- `zdlic`. Key 1 made on Tim's machine 2026-11-22, outside the repo; licence ZD-0001 (every module, Tim's machine)
  issued and installed there.

Tests: `tests/modules.rs` (every command has a module; with only the Viewer on, `string.create` is refused as
`refused.not_licensed`, naming Design, and the project is unchanged; Design on, it runs). In `zd-licence`: a licence
signed reads back the same; one character changed fails the signature, as does a licence signed by another key under the
same number; a retired or unknown key is refused; another machine's licence is the Viewer, one listing no machines runs
anywhere; 10 days before the end warns, the day after is grace with 13 days left, 15 days after is the Viewer; a clock
a week behind the newest day seen is the Viewer, a day behind isn't; no file is the Viewer; calendar days against known
counts (2000-03-01 is day 11,017). All exact.

## Known limits

- A licence's dates are UTC: one issued on a morning in Queensland says the day before.
- The newest day seen is a plain file a user can delete; it's a guard against a clock wound back, not a lock.
- Seats aren't counted: a licence tied to one shared Windows Server covers everyone logged in to it.
