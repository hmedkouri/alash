# Security Policy

## Scope

`alash` sends your question, and whatever you pipe into `ask`, to an AI CLI you are
logged in to (`claude` or `codex`). It makes no network request of its own, writes no
file and keeps no log.

The properties it promises:

* **The model has no way to act.** Each harness is started so the model cannot run a
  command or write a file. How that was proved is in [docs/tools-off.md](docs/tools-off.md).
* **Nothing the model returns is executed.** The key binding only places text on the
  command line, and only when the reply is a single line that does not end in a question
  mark. Control characters are replaced before anything is displayed.
* **Nothing is captured.** Only the question, piped input, the previous command line
  (`why`) and the text you typed (the key binding) are sent.

What you pipe in is sent to the model provider. Do not pipe secrets.

## Reporting a vulnerability

Please report privately rather than opening a public issue: use GitHub's
[private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability)
on this repository.

Include what you did, what happened, and what you expected. Expect an initial response
within a couple of weeks: this is a small personal project, not a staffed one.

## In scope

* Any way for a model reply to run a command, write a file or reach a tool.
* Any way for a model reply to be executed by the shell integration, or to drive the
  terminal through escape sequences.
* A harness version on which the adapter's arguments no longer switch tools off while
  `ask --doctor` still reports "tools off: yes".

## Not in scope

* What the provider does with the text you send.
* A model suggesting a harmful command that you then choose to run.
* The global `~/.codex/AGENTS.md` being applied to codex answers: this is documented and
  reported by `ask --doctor`.
