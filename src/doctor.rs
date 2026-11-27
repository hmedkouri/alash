//! `ask --doctor`: what is installed, whether tools are off, and what to fix.

use crate::harness::{base_url, url_answers, Harness};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Stdout and stderr of a command, or None when it cannot be started.
fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

/// The flags each adapter passes, checked against the harness's own help text.
/// claude's `--max-turns` is accepted but not listed in its help, so it is not checked.
fn missing_flags(h: Harness) -> Vec<&'static str> {
    let (help_args, flags): (&[&str], &[&'static str]) = match h {
        Harness::Claude => (
            &["--help"],
            &[
                "--print",
                "--model",
                "--tools",
                "--setting-sources",
                "--strict-mcp-config",
            ],
        ),
        Harness::Codex => (
            &["exec", "--help"],
            &[
                "--sandbox",
                "--ephemeral",
                "--skip-git-repo-check",
                "--ignore-rules",
                "--disable",
                "--color",
            ],
        ),
    };
    let help = output_of(h.name(), help_args).unwrap_or_default();
    flags
        .iter()
        .copied()
        .filter(|f| !help.contains(f))
        .collect()
}

fn codex_global_instructions() -> Option<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            Some(PathBuf::from(home).join(".codex"))
        })?;
    let file = home.join("AGENTS.md");
    file.exists().then_some(file)
}

/// Prints the report for one harness and says whether it can work.
fn check(h: Harness, live: bool) -> bool {
    let Some(version) = output_of(h.name(), &["--version"]) else {
        say!("{}: not on the PATH", h.name());
        return false;
    };
    say!("{}: {}", h.name(), version.trim());

    let flags = missing_flags(h);
    let features = h.missing_features().unwrap_or_default();
    let mcp_servers = h.mcp_servers();
    let mut ok = flags.is_empty() && features.is_empty() && mcp_servers.is_ok();
    if flags.is_empty() {
        say!("  flags: all present");
    } else {
        say!(
            "  flags: MISSING {} (this version is not supported)",
            flags.join(", ")
        );
    }
    if !features.is_empty() {
        say!(
            "  features: MISSING {} (cannot be switched off)",
            features.join(", ")
        );
    }
    match &mcp_servers {
        Ok(servers) if !servers.is_empty() => {
            say!("  MCP servers switched off by name: {}", servers.join(", "))
        }
        Ok(_) => {}
        Err(_) => say!("  MCP servers: the list COULD NOT BE READ, so they cannot be switched off"),
    }
    match (ok, h) {
        (false, _) => say!("  tools off: NO, so alash will not run it"),
        (true, Harness::Claude) => say!("  tools off: yes (--tools= gives the model no tools)"),
        (true, Harness::Codex) => say!(
            "  tools off: yes (no command tool; a file-patch tool is still offered to gpt-5 \
             models and the read-only sandbox rejects it)"
        ),
    }
    if h == Harness::Codex {
        if let Some(file) = codex_global_instructions() {
            say!(
                "  warning: {} exists; codex applies it to every answer and alash cannot \
                 switch it off",
                file.display()
            );
        }
    }
    if live && ok {
        match h.run("Reply with the single word: ok", true, Stdio::null()) {
            Ok(reply) => say!("  round trip: ok ({})", reply.trim()),
            Err(e) => {
                say!("  round trip: FAILED\n{e}");
                ok = false;
            }
        }
    }
    ok
}

/// Returns whether the default harness can work.
pub fn run(live: bool) -> bool {
    say!("alash {}", env!("CARGO_PKG_VERSION"));
    // The `ask` function reports what it found in the calling shell. Only that is
    // repeated here: this process cannot see the shell's functions or key bindings.
    let found = |var: &str| std::env::var(var).ok().map(|v| v == "1");
    match (
        std::env::var("ALASH_SHELL"),
        found("ALASH_WHY"),
        found("ALASH_KEY"),
    ) {
        (Ok(shell), Some(why), Some(key)) => {
            say!("shell integration: called from the {shell} `ask` function");
            if why {
                say!("  why: is the alash function");
            } else {
                say!("  why: NOT the alash function (missing or redefined)");
            }
            if key {
                say!("  Ctrl-X Ctrl-A: bound to the alash key binding");
            } else {
                say!(
                    "  Ctrl-X Ctrl-A: NOT bound to the alash key binding (never bound, or \
                     rebound by something loaded later)"
                );
            }
        }
        _ => say!(
            "shell integration: not checked, because this was not called from the `ask` \
             function. Add `eval \"$(alash init zsh)\"` to ~/.zshrc (or bash and ~/.bashrc)."
        ),
    }
    let default = match Harness::default_from_env() {
        Ok(h) => {
            say!("default harness: {}", h.name());
            Some(h)
        }
        Err(e) => {
            say!("default harness: {e}");
            None
        }
    };
    let mut default_ok = false;
    for h in Harness::ALL {
        let ok = check(h, live);
        if Some(h) == default {
            default_ok = ok;
        }
    }
    match base_url() {
        None => say!("base URL: not set"),
        Some(url) => {
            let up = url_answers(&url);
            say!(
                "base URL (claude only): {url} {}",
                if up { "answers" } else { "DOES NOT ANSWER" }
            );
            if default == Some(Harness::Claude) {
                default_ok &= up;
            }
        }
    }
    if !live {
        say!(
            "No request was sent, so login and network were not checked. `ask --doctor --live` \
             sends one request per harness."
        );
    }
    default_ok
}
