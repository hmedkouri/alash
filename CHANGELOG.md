# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/hmedkouri/alash/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/hmedkouri/alash/releases/tag/v0.1.0
