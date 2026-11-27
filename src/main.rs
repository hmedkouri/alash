//! alash: `ask`, `why` and a command-line key binding, answered by an AI CLI the user is
//! already logged in to. Users reach this binary through the shell functions that
//! `alash init <shell>` prints.

/// `println!` that does not panic when the reader has gone away (`ask ... | head -1`).
macro_rules! say {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

mod doctor;
mod harness;
mod widget;

use harness::{split_marker, Harness};
use std::process::{ExitCode, Stdio};
use widget::Decision;

const USAGE: &str = "\
alash: ask an AI CLI you are already logged in to, with its tools switched off.

Setup, in ~/.zshrc or ~/.bashrc:
  eval \"$(alash init zsh)\"      (or: alash init bash)

Then:
  ask <question>            answer a question; piped input is included (cmd |& ask why)
  ask @codex <question>     use codex for this question (default: claude)
  ask -- <question>         for a question that starts with @ or --
  ask --doctor [--live]     check the setup; --live sends one request per harness
  why                       explain why the previous command failed
  Ctrl-X Ctrl-A             turn what is typed into one command, placed and never run

Environment:
  ASK_MODEL           claude model (default: haiku)
  ALASH_CODEX_MODEL   codex model (default: codex's own)
  ALASH_HARNESS       default harness: claude or codex
  ALASH_BASE_URL      optional base URL for claude, such as a local proxy
  ALASH_TIMEOUT       seconds a harness may take (default: 60, key binding 30)";

/// Put before every `ask` and `why` question. With its tools off a model may still answer
/// "Done" or print a tool call as text; this tells it what it can do. See docs/tools-off.md.
const NO_TOOLS: &str = "You have no tools in this session: you cannot run commands, read or \
    write files, or browse. Answer in plain text only. If asked to do something, never say \
    you did it and never write a tool call: say that you cannot, and give the command the \
    user can run.";

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::FAILURE
}

fn shell() -> String {
    std::env::var("ALASH_SHELL").unwrap_or_else(|_| "shell".into())
}

fn init(rest: &[String]) -> ExitCode {
    match rest.first().map(String::as_str) {
        Some("zsh") => print!("{}", include_str!("init.zsh")),
        Some("bash") => print!("{}", include_str!("init.bash")),
        _ => return fail("usage: alash init zsh|bash"),
    }
    ExitCode::SUCCESS
}

/// Sends one question and prints the answer. `picked` is the harness and the question.
fn answer(picked: Result<(Harness, &str), String>, stdin: Stdio) -> ExitCode {
    let reply = picked.and_then(|(h, q)| {
        if q.is_empty() {
            Err(USAGE.to_string())
        } else {
            h.run(&format!("{NO_TOOLS}\n\n{q}"), false, stdin)
        }
    });
    match reply {
        Ok(text) => {
            say!("{}", text.trim_end());
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn ask(rest: &[String]) -> ExitCode {
    match rest.first().map(String::as_str) {
        None | Some("--help") => fail(USAGE),
        Some("--doctor") => {
            if doctor::run(rest.iter().any(|a| a == "--live")) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Some("--") => {
            let question = rest[1..].join(" ");
            answer(
                Harness::default_from_env().map(|h| (h, question.trim())),
                Stdio::inherit(),
            )
        }
        Some(_) => answer(split_marker(&rest.join(" ")), Stdio::inherit()),
    }
}

/// `alash why <exit status> <previous command> [@harness]`
fn why(rest: &[String]) -> ExitCode {
    let [status, command, marker @ ..] = rest else {
        return fail("why is a shell function: add `eval \"$(alash init zsh)\"` to your shell.");
    };
    let command = command.trim();
    if command.is_empty() {
        return fail("There is no previous command in this shell's history.");
    }
    let question = format!(
        "{} This {} command exited with status {status}: {command}\n\
         You cannot see what it printed. Answer in at most four lines: the most likely cause \
         in one or two sentences, then the one most likely fix as a command. Do not restate \
         the command, do not guess at its output, and do not list alternatives.",
        marker.join(" "),
        shell()
    );
    answer(split_marker(&question), Stdio::null())
}

fn machine() -> String {
    let os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            let line = s.lines().find_map(|l| l.strip_prefix("PRETTY_NAME="))?;
            Some(line.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| "none".into());
    let mut ctx = format!("{os}, {}", std::env::consts::ARCH);
    if cfg!(target_os = "linux") {
        ctx += &format!(
            ", session {}, DISPLAY={}, WAYLAND_DISPLAY={}",
            var("XDG_SESSION_TYPE"),
            var("DISPLAY"),
            var("WAYLAND_DISPLAY")
        );
    }
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    format!("{ctx}, {}, cwd {cwd}", shell())
}

/// `alash cmd <typed text>`: everything goes to stdout because the key binding shows it.
/// Exit 0 means stdout is one command to place; anything else means it is a message.
fn cmd(rest: &[String]) -> ExitCode {
    let reply = split_marker(&rest.join(" ")).and_then(|(h, request)| {
        let prompt = format!(
            "Machine: {}. Output only one {} command for exactly this machine, no if/else \
             for other setups, no prose, no code fences, that does: {request}",
            machine(),
            shell()
        );
        h.run(&prompt, true, Stdio::null())
    });
    match reply.as_deref().map(widget::decide) {
        Ok(Decision::Place(command)) => {
            say!("{command}");
            ExitCode::SUCCESS
        }
        Ok(Decision::Message(text)) => {
            say!("{text}");
            ExitCode::FAILURE
        }
        Err(e) => {
            say!("{}", widget::clean(e));
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("init") => init(rest),
        Some("ask") => ask(rest),
        Some("why") => why(rest),
        Some("cmd") => cmd(rest),
        Some("--version" | "-V") => {
            say!("alash {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => fail(USAGE),
    }
}
