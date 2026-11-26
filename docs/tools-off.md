# Tools off: what was proved, and how to prove it again

The rule: an adapter ships only if the model it starts cannot run a command or write a
file. The exact arguments are in `src/harness.rs` and pinned by `tests/args.rs`. This page
records why those arguments are the right ones. Re-run the checks when a harness has a
major release.

Proved on 2026-11-26 with claude 2.1.332 and codex-cli 0.171.0, on Linux.

## claude

```
claude -p --model <m> --strict-mcp-config --setting-sources "" --tools= -- <prompt>
```

| Argument | Why |
|---|---|
| `--tools=` | The model gets no tools. Written with `=`: the option is variadic and `--tools ""` swallows the prompt. |
| `--setting-sources ""` | Drops hooks and the user's `CLAUDE.md`, whose instructions spoil a one-shot answer. It also drops any guard hook, which is why `--tools=` is required. |
| `--strict-mcp-config` | Starts no MCP servers. |
| not `--bare` | It accepts only an API key and ignores the subscription login. |
| `--` | A question that starts with a dash is not read as an option. |

## codex

codex has no switch that removes all tools. The adapter removes what it can and relies on
the sandbox for the one tool it cannot remove.

```
codex exec --ignore-rules -s read-only --ephemeral
  --skip-git-repo-check --color never
  -c project_doc_max_bytes=0 -c web_search=disabled
  --disable shell_tool --disable unified_exec --disable view_image --disable goals
  --disable multi_agent --disable apps --disable plugins
  -c mcp_servers.<name>.enabled=false   (once per configured server)
  -- <prompt>
```

What each part was shown to do, by reading the `tools` list in the request codex sends
(`scripts/capture-tools.py`; no real request is made):

| Arguments | Tools sent |
|---|---|
| none | `exec_command`, `write_stdin`, `request_user_input`, `apply_patch`, `view_image`, `get_goal`, `create_goal`, `update_goal`, `tool_search`, `web_search` |
| the full set above, model gpt-5.5 | `request_user_input`, `apply_patch` |
| the full set above, a model codex does not know | `request_user_input` |

* **`apply_patch` cannot be removed** for gpt-5 models. `-s read-only` rejects it: a fake
  model endpoint (`scripts/fake-model.py`) ordered a patch that adds a file, codex
  answered "patch rejected: writing is blocked by read-only sandbox", and no file
  appeared. The same order under `-s workspace-write` did create the file, so the test
  can fail.
* **MCP servers are switched off one by one.** A configured MCP server starts with codex
  and its tools reach the model. alash reads the names from `codex mcp list --json`, run
  in the same directory so a trusted project's servers are included, and passes
  `-c mcp_servers.<name>.enabled=false` for each. With a user server, a remote server and
  a trusted-project server configured, none was started and the tool list was unchanged.
  If the list cannot be read, or a name is not plain letters, digits, `_` or `-`, alash
  refuses to run codex.
* **Not `--ignore-user-config`.** It stops MCP servers too, but it drops the user's model
  provider while keeping the stored login. On a machine whose stored key belongs to
  another provider, codex then sent that key to OpenAI.
* **`-c project_doc_max_bytes=0`** drops the project's `AGENTS.md`.
* **The global `~/.codex/AGENTS.md` is still applied.** No argument removes it. The doctor
  warns when the file exists.
* **An unknown `--disable` name is a hard error.** alash reads `codex features list` first
  and refuses to run codex if any feature it disables is missing, because a renamed
  feature would be a tool left on.

## Proving it again

Tool list, at no cost. Start the capture, then point codex at it:

```sh
python3 scripts/capture-tools.py 18911 /tmp/cap.log &
codex exec <the arguments above, without the mcp_servers lines> --disable enable_request_compression \
  -c 'model_provider="cap"' -c 'model_providers.cap.name="cap"' \
  -c 'model_providers.cap.base_url="http://127.0.0.1:18911/v1"' \
  -c 'model="gpt-5.5"' -- "hi" </dev/null
cat /tmp/cap.log    # expect only request_user_input and apply_patch
```

Write is blocked, at no cost. In an empty directory, with `scripts/fake-model.py 18912
/tmp/fake.log` running and the same `-c` lines on port 18912: `proof.txt` must not appear,
and the log must show "patch rejected".

MCP servers, at no cost. Make a throwaway `CODEX_HOME` whose `config.toml` sets the `cap`
provider above and an `[mcp_servers.probe]` whose command touches a file, then run
`CODEX_HOME=<dir> ask @codex hi </dev/null`: the file must not appear and the captured
tool list must hold no `mcp` tool.

Against the real models, which costs one request each:

```sh
cd "$(mktemp -d)"
ask "Create a file named proof.txt containing the word hi. Use any tool you have."
ask @codex "Create a file named proof.txt containing the word hi. Use any tool you have."
ls    # expect nothing
```
