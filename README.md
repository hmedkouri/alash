# alash

`ask`, `why` and a command-line key binding for zsh and bash, answered by the AI CLI you
are already logged in to. No API key: alash runs `claude` or `codex` for one answer, with
the model's tools switched off.

*Alash* is "why" in Moroccan Darija.

Site: [alash.app](https://alash.app)

```console
$ ask how do I list listening ports
$ make |& ask why does this fail
$ ask @codex what does set -o pipefail do

$ tar xf missing.tar
tar: missing.tar: Cannot open: No such file or directory
$ why

$ five biggest files here▌            # press Ctrl-X Ctrl-A
$ find . -type f -printf '%s\t%p\n' | sort -rn | head -5▌
```

The key binding replaces what you typed with one command and stops. It never runs it.

## Install

alash needs [Claude Code](https://claude.com/claude-code) or the
[Codex CLI](https://github.com/openai/codex) installed and logged in.

1. Get the binary (needs Rust's cargo):

   ```sh
   cargo install --git https://github.com/hmedkouri/alash
   ```
2. Add one line to `~/.zshrc` or `~/.bashrc`:

   ```sh
   eval "$(alash init zsh)"     # or: alash init bash
   ```

3. Open a new shell and run `ask --doctor`.

## Use

| You type | What happens |
|---|---|
| `ask <question>` | Answers the question. Piped input is included. |
| `ask @codex <question>` | Uses codex for this question. `@claude` also works. |
| `ask -- <question>` | For a question that starts with `@` or `--`. |
| `why` | Explains why the previous command failed, from its command line and exit status. |
| `why @codex` | The same, with codex. |
| Ctrl-X Ctrl-A | Turns the text on the command line into one command. Start the text with `@codex` to use codex. |
| `ask --doctor` | Checks each harness, whether its tools are off, whether `why` and the key binding are still alash's, and the base URL. |
| `ask --doctor --live` | Also sends one real request per harness. |

`why` sees the command you ran, not what it printed. To include the output, pipe it:
`cmd |& ask why does this fail`.

## Configuration

Environment variables only.

| Variable | Meaning | Default |
|---|---|---|
| `ASK_MODEL` | claude model | `haiku` |
| `ALASH_CODEX_MODEL` | codex model | codex's own default |
| `ALASH_HARNESS` | harness used without a marker: `claude` or `codex` | `claude` |
| `ALASH_BASE_URL` | base URL for claude, such as a local proxy | not set |

## What it will not do

- **Give the model tools.** Each harness is started so the model cannot run a command or
  write a file. If alash cannot do that for the installed version, it refuses to run it.
  The arguments and the proofs are in [docs/tools-off.md](docs/tools-off.md).
- **Run what the model returns.** The key binding places a reply on the command line only
  when it is a single line that does not end in a question mark. Anything else is shown
  as a message. It warns when the first word is not an installed command.
- **Capture or log.** It sends your question, what you pipe in, the previous command line
  for `why`, and the typed text for the key binding. It writes nothing to disk.

## Limits

- **codex keeps your `~/.codex/config.toml`**, so your model provider and login are
  used. Its MCP servers are switched off by name for each call; if alash cannot read the
  list of servers, it refuses to run codex.
- **A model may still talk as if it had tools.** Asked to change something, it can
  answer "Done" or print a tool call as text. Nothing was changed: alash never lets it.
  Every `ask` and `why` question tells the model it has no tools, which stopped both
  in testing, but a model can ignore that.
- **codex still applies a global `~/.codex/AGENTS.md`.** No option removes it. The doctor
  warns when the file exists.
- **codex is still offered a file-patch tool** with gpt-5 models. The read-only sandbox
  rejects it.
- **Windows is experimental.** A binary is built, but there is no PowerShell integration
  and it has not been run on Windows.
- Answers are printed when complete, not streamed, and each question is independent:
  there are no follow-ups yet.

## Similar tools

alash is not the first tool to drive a logged-in CLI in place of an API key.
[zsh-claude-code](https://github.com/matheus-poli/zsh-claude-code) and
[claude-cmd-deluxe](https://github.com/bvleur/claude-cmd-deluxe) do it for claude in zsh,
and [shell-ai](https://github.com/Deltik/shell-ai) can use claude or codex as a provider.
[shell_gpt](https://github.com/TheR1D/shell_gpt), [aichat](https://github.com/sigoden/aichat)
and [llm](https://github.com/simonw/llm) call an API with a key.

What alash puts together: `ask` with piped input, `why`, and a key binding that never
runs anything, in one binary with no dependencies, for zsh and bash, for claude and
codex, with the tools-off arguments tested and the proofs written down.

## Licence

MIT. See [LICENSE](LICENSE).
