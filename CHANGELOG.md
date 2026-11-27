# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2] - 2026-11-27

### Fixed

- After an upgrade, `ask --doctor` in a shell that was already open said the shell had
  not loaded alash. It had: a shell keeps the `ask` function it started with, and the old
  function did not pass what the new doctor expects. The shell code now carries the
  version that printed it, and the doctor says the shell loaded an older alash and how to
  reload it.

## [0.2.1] - 2026-11-27

### Changed

- `ask --doctor` runs its slow checks side by side, so it takes as long as the slowest one
  and not the sum. With `--live`, a harness that hangs no longer adds its whole time limit
  on top of the other harness's round trip.

### Fixed

- What a failing or hanging codex said is readable. Its log lines carry hundreds of
  characters of tracing context; alash now drops the INFO lines, keeps the level and the
  message of a WARN or ERROR line, and cuts any line at 300 characters.

## [0.2.0] - 2026-11-27

### Added

- `ask --doctor` says when a newer alash has been released and how to update. It asks
  github.com by running `curl`; this is the only network request alash makes itself, the
  doctor is the only place it happens, and `ALASH_NO_UPDATE_CHECK=1` switches it off.
- A warning a harness prints when it succeeds is passed on, after the answer and on
  stderr. claude's notice that a sign-in is about to expire used to be dropped, so the
  first sign of it was `ask` failing. The key binding shows a warning once per shell
  session, and `ask --doctor --live` reports it as a warn on the round-trip row.

### Changed

- `ask --doctor` reports as a table: one row per check with a status of pass, warn, fail
  or skip, and the fixes listed underneath. The rows are the same on every run, and the
  result of each check no longer has to be read out of a sentence. Colour is used only
  on a terminal and never when `NO_COLOR` is set.
- The crate is published to crates.io by the release workflow through Trusted Publishing,
  so a release no longer needs a token on someone's machine.

## [0.1.1] - 2026-11-27

### Added

- `ALASH_TIMEOUT`: seconds a harness may take. The default is 60, and 30 for the key
  binding, where a frozen prompt is worse than a slow answer.

### Fixed

- A harness that never exits no longer makes `ask`, `why` or the key binding hang. codex
  retries without end when its provider is unreachable; alash now stops the harness at the
  time limit, prints the last lines it wrote, and exits non-zero.
- Stopping alash (Ctrl-C, hangup, `kill`) stops the harness too. It used to be left
  running.
- Release binaries are attached again. The repository has immutable releases on, so the
  `v0.1.0` release was locked before its archives could be uploaded and has none. The
  release is now created as a draft and published after the uploads.

## [0.1.0] - 2026-11-27

First release.

### Added

- `ask <question>`: one answer from the AI CLI you are logged in to, with piped input
  included. `ask @codex` picks codex; the default is claude.
- `why`: explains why the previous command failed, from its command line and exit status,
  in a few lines.
- A Ctrl-X Ctrl-A key binding for zsh and bash that turns the typed text into one command
  and places it on the command line. It never runs it, and shows anything that is not a
  single line as a message.
- `ask --doctor`: reports each harness, whether its tools are off, whether `why` and the
  key binding are still alash's, and the base URL. `--live` sends one request per harness.
- Tools off for both harnesses, because a one-shot helper must not be able to act: claude
  runs with `--tools=`; codex runs in a read-only sandbox with its command tools and MCP
  servers disabled, and is refused when that cannot be done. The proofs are in
  `docs/tools-off.md`.
- Shell integration printed by `alash init zsh` and `alash init bash`.
- Configuration by environment variable: `ASK_MODEL`, `ALASH_CODEX_MODEL`,
  `ALASH_HARNESS`, `ALASH_BASE_URL`.

[Unreleased]: https://github.com/hmedkouri/alash/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/hmedkouri/alash/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/hmedkouri/alash/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/hmedkouri/alash/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/hmedkouri/alash/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/hmedkouri/alash/releases/tag/v0.1.0
