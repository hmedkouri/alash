//! `ask --doctor`: what is installed, whether tools are off, and what to fix.
//!
//! The checks produce rows; `render` turns the rows into a table. Nothing is printed
//! until every check has run.

use crate::harness::{base_url, url_answers, Harness};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Pass,
    Warn,
    /// Something that stops the default harness working. Sets the exit status.
    Fail,
    /// Not applicable or not run; the summary says why.
    Skip,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Pass => "✓ pass",
            Status::Warn => "⚠ warn",
            Status::Fail => "✗ fail",
            Status::Skip => "· skip",
        }
    }

    fn colour(self) -> &'static str {
        match self {
            Status::Pass => "\x1b[32m",
            Status::Warn => "\x1b[33m",
            Status::Fail => "\x1b[31m",
            Status::Skip => "\x1b[2m",
        }
    }
}

pub struct Row {
    check: String,
    status: Status,
    summary: String,
    /// What to do about a warn or fail; printed under the table.
    fix: Option<String>,
}

fn row(check: impl Into<String>, status: Status, summary: impl Into<String>) -> Row {
    Row {
        check: check.into(),
        status,
        summary: summary.into(),
        fix: None,
    }
}

impl Row {
    fn fix(mut self, fix: impl Into<String>) -> Row {
        self.fix = Some(fix.into());
        self
    }
}

/// Break text into lines of at most `width` characters, at spaces where it can.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        if !line.is_empty() && line.chars().count() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        // A word longer than the cell, such as a path, is cut. The line is empty here.
        while word.len() > width {
            lines.push(word.drain(..width).collect());
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.extend(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// The whole report as text. `width` is the terminal's, when known: summaries wrap inside
/// their cell so the frame never breaks. Colour touches the status cell only.
pub fn render(title: &str, rows: &[Row], colour: bool, width: Option<usize>) -> String {
    let len = |s: &str| s.chars().count();
    let check_w = rows
        .iter()
        .map(|r| len(&r.check))
        .max()
        .unwrap_or(0)
        .max(len("check"));
    let status_w = len("✓ pass");
    // "│ " + check + " │ " + status + " │ " + summary + " │"
    let frame = check_w + status_w + 10;
    let longest = rows
        .iter()
        .map(|r| len(&r.summary))
        .max()
        .unwrap_or(0)
        .max(len("summary"));
    let summary_w = match width {
        Some(w) => longest.min(w.saturating_sub(frame).max(20)),
        None => longest,
    };
    let rule = |left: &str, mid: &str, right: &str| {
        format!(
            "{left}{}{mid}{}{mid}{}{right}\n",
            "─".repeat(check_w + 2),
            "─".repeat(status_w + 2),
            "─".repeat(summary_w + 2)
        )
    };
    let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w.saturating_sub(len(s))));
    let mut out = format!("{title}\n\n{}", rule("┌", "┬", "┐"));
    out += &format!(
        "│ {} │ {} │ {} │\n",
        pad("check", check_w),
        pad("status", status_w),
        pad("summary", summary_w)
    );
    out += &rule("├", "┼", "┤");
    for r in rows {
        for (i, line) in wrap(&r.summary, summary_w).iter().enumerate() {
            let (check, status) = if i == 0 {
                (r.check.as_str(), r.status.label())
            } else {
                ("", "")
            };
            let status = if colour && i == 0 {
                format!("{}{}\x1b[0m", r.status.colour(), pad(status, status_w))
            } else {
                pad(status, status_w)
            };
            out += &format!(
                "│ {} │ {status} │ {} │\n",
                pad(check, check_w),
                pad(line, summary_w)
            );
        }
    }
    out += &rule("└", "┴", "┘");
    let fixes: Vec<String> = rows
        .iter()
        .filter_map(|r| Some(format!("{}: {}", r.check, r.fix.as_ref()?)))
        .collect();
    if !fixes.is_empty() {
        out += &format!("\n{}\n", fixes.join("\n"));
    }
    out
}

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

/// Redirects to `/releases/tag/vX.Y.Z`, so the latest version is in the address: no JSON
/// to parse and no API rate limit.
const LATEST: &str = "https://github.com/hmedkouri/alash/releases/latest";

/// `1.2.3` as numbers, so 0.10.0 is newer than 0.9.0. None for anything else, which
/// includes a pre-release: one is never offered.
fn parse_version(v: &str) -> Option<[u64; 3]> {
    let mut parts = v.trim().split('.').map(|p| p.parse::<u64>().ok());
    let version = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(version)
}

/// The latest released version, asked of GitHub by running `curl`: the standard library
/// cannot speak HTTPS and alash takes no dependency for it. This is the only network
/// request alash makes itself, and only the doctor makes it. None when curl is missing,
/// fails, or takes longer than a few seconds.
fn latest_release() -> Option<String> {
    let sink = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut child = Command::new("curl")
        .args(["-s", "-o", sink, "-m", "5", "-w", "%{redirect_url}", LATEST])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait().ok()? {
            Some(status) => break status,
            None if started.elapsed() > Duration::from_secs(6) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    };
    let mut url = String::new();
    child.stdout.take()?.read_to_string(&mut url).ok()?;
    let tag = url.trim().rsplit('/').next()?.strip_prefix('v')?;
    (status.success() && parse_version(tag).is_some()).then(|| tag.to_string())
}

/// The `version` row, from the running version and what `latest_release` returned.
fn version_row(running: &str, checked: bool, latest: Option<&str>) -> Row {
    if !checked {
        return row(
            "version",
            Status::Skip,
            format!("{running}; check switched off by ALASH_NO_UPDATE_CHECK"),
        );
    }
    let Some(latest) = latest else {
        return row(
            "version",
            Status::Skip,
            format!("{running}; the latest release could not be checked (no curl, or no answer from github.com)"),
        );
    };
    match (parse_version(running), parse_version(latest)) {
        (Some(mine), Some(theirs)) if theirs > mine => row(
            "version",
            Status::Warn,
            format!("{running}; {latest} is available"),
        )
        .fix(format!(
            "update with `cargo install alash`, or download {latest} from {LATEST}"
        )),
        (Some(mine), Some(theirs)) if mine > theirs => row(
            "version",
            Status::Pass,
            format!("{running}, newer than the latest release {latest}"),
        ),
        _ => row(
            "version",
            Status::Pass,
            format!("{running} is the latest release"),
        ),
    }
}

/// The rows for one harness. `sev` is Fail for the default harness and Warn for another:
/// only the default one decides the exit status.
fn check(h: Harness, sev: Status, live: bool, rows: &mut Vec<Row>) {
    let name = h.name();
    let tools = format!("{name} tools");
    let trip = format!("{name} round trip");
    let Some(version) = output_of(name, &["--version"]) else {
        rows.push(row(name, sev, "not on the PATH").fix(format!(
            "install {name} and log in, or use the other harness"
        )));
        rows.push(row(tools, Status::Skip, format!("{name} was not found")));
        rows.push(row(trip, Status::Skip, format!("{name} was not found")));
        return;
    };
    let version = version
        .trim()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let flags = missing_flags(h);
    let features = h.missing_features().unwrap_or_default();
    let mcp_servers = h.mcp_servers();
    let ok = flags.is_empty() && features.is_empty() && mcp_servers.is_ok();

    if flags.is_empty() && features.is_empty() {
        rows.push(row(
            name,
            Status::Pass,
            format!("{version}, all flags present"),
        ));
    } else {
        let mut missing = flags.clone();
        missing.extend(&features);
        rows.push(
            row(
                name,
                sev,
                format!("{version}, MISSING {}", missing.join(", ")),
            )
            .fix(format!(
                "this {name} version is not supported; update alash or {name}"
            )),
        );
    }
    rows.push(match (ok, h) {
        (false, _) => row(tools, sev, "NOT off, so alash will not run it"),
        (true, Harness::Claude) => row(
            tools,
            Status::Pass,
            "off (--tools= gives the model no tools)",
        ),
        (true, Harness::Codex) => row(
            tools,
            Status::Pass,
            "off (no command tool; a file-patch tool is still offered to gpt-5 models and the \
             read-only sandbox rejects it)",
        ),
    });
    if h == Harness::Codex {
        rows.push(match &mcp_servers {
            Ok(servers) if servers.is_empty() => {
                row("codex mcp", Status::Skip, "no MCP servers configured")
            }
            Ok(servers) => row(
                "codex mcp",
                Status::Pass,
                format!("switched off by name: {}", servers.join(", ")),
            ),
            Err(_) => row("codex mcp", sev, "the list of servers could not be read")
                .fix("run `codex mcp list --json` to see why; alash refuses codex until it works"),
        });
        rows.push(match codex_global_instructions() {
            Some(file) => row(
                "codex instructions",
                Status::Warn,
                format!("{} is applied to every answer", file.display()),
            )
            .fix("codex has no option to switch this file off; remove it if answers look odd"),
            None => row("codex instructions", Status::Pass, "no global AGENTS.md"),
        });
    }
    rows.push(if !live {
        row(
            trip,
            Status::Skip,
            "not run; `ask --doctor --live` checks login and network with one request",
        )
    } else if !ok {
        row(
            trip,
            Status::Skip,
            "not run: alash will not start this harness",
        )
    } else {
        match h.run("Reply with the single word: ok", true, Stdio::null()) {
            Ok(reply) if reply.warnings.is_empty() => row(
                trip,
                Status::Pass,
                format!("answered ({})", reply.text.trim()),
            ),
            Ok(reply) => {
                row(trip, Status::Warn, format!("answered, and {name} warned")).fix(reply.warnings)
            }
            Err(e) => row(trip, sev, "no answer").fix(e),
        }
    });
}

/// Runs every check. The rows are the same ones, in the same order, on every run.
fn rows(default: Option<Harness>, live: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    let checked = std::env::var_os("ALASH_NO_UPDATE_CHECK").is_none_or(|v| v.is_empty());
    let latest = if checked { latest_release() } else { None };
    rows.push(version_row(
        env!("CARGO_PKG_VERSION"),
        checked,
        latest.as_deref(),
    ));
    // The `ask` function reports what it found in the calling shell. Only that is
    // repeated here: this process cannot see the shell's functions or key bindings.
    let found = |var: &str| std::env::var(var).ok().map(|v| v == "1");
    let eval = "add `eval \"$(alash init zsh)\"` (or bash) at the end of your shell's rc file";
    match (
        std::env::var("ALASH_SHELL"),
        found("ALASH_WHY"),
        found("ALASH_KEY"),
    ) {
        (Ok(shell), Some(why), Some(key)) => {
            rows.push(row(
                "shell",
                Status::Pass,
                format!("called from the {shell} `ask` function"),
            ));
            rows.push(if why {
                row("why", Status::Pass, "is the alash function")
            } else {
                row(
                    "why",
                    Status::Warn,
                    "is not the alash function (missing or redefined)",
                )
                .fix(eval)
            });
            rows.push(if key {
                row(
                    "key binding",
                    Status::Pass,
                    "Ctrl-X Ctrl-A is bound to alash",
                )
            } else {
                row(
                    "key binding",
                    Status::Warn,
                    "Ctrl-X Ctrl-A is not bound to alash",
                )
                .fix(format!(
                    "never bound, or rebound by something loaded later; {eval}"
                ))
            });
        }
        _ => {
            let why = "not checked: run `ask --doctor` in a shell that has loaded alash";
            rows.push(row("shell", Status::Skip, why));
            rows.push(row("why", Status::Skip, why));
            rows.push(row("key binding", Status::Skip, why));
        }
    }
    for h in Harness::ALL {
        let sev = if default == Some(h) {
            Status::Fail
        } else {
            Status::Warn
        };
        check(h, sev, live, &mut rows);
    }
    rows.push(match base_url() {
        None => row("base URL", Status::Skip, "not set"),
        Some(url) if url_answers(&url) => row(
            "base URL",
            Status::Pass,
            format!("{url} answers (used for claude only)"),
        ),
        Some(url) => {
            let sev = if default == Some(Harness::Claude) {
                Status::Fail
            } else {
                Status::Warn
            };
            row("base URL", sev, format!("{url} does not answer"))
                .fix("start that proxy, or unset ALASH_BASE_URL to call Anthropic directly")
        }
    });
    rows
}

/// Prints the report and returns whether the default harness can work.
pub fn run(live: bool) -> bool {
    let default = Harness::default_from_env();
    let title = format!(
        "alash doctor v{} · default harness {}",
        env!("CARGO_PKG_VERSION"),
        default.as_ref().map_or("unknown", |h| h.name())
    );
    let mut rows = rows(default.as_ref().ok().copied(), live);
    if let Err(e) = &default {
        rows.insert(0, row("default harness", Status::Fail, e.as_str()));
    }
    let stdout = std::io::stdout();
    let tty = stdout.is_terminal();
    let colour = tty && std::env::var_os("NO_COLOR").is_none();
    // The shell knows the width and the `ask` function passes it as COLUMNS. Without it,
    // a terminal gets a safe 80 and a pipe gets unwrapped lines.
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse::<usize>().ok())
        .or(tty.then_some(80));
    say!("{}", render(&title, &rows, colour, width).trim_end());
    rows.iter().all(|r| r.status != Status::Fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Row> {
        vec![
            row("shell", Status::Pass, "called from the zsh `ask` function"),
            row(
                "key binding",
                Status::Warn,
                "Ctrl-X Ctrl-A is not bound to alash",
            )
            .fix("rebind it"),
            row("codex", Status::Fail, "not on the PATH").fix("install codex"),
            row("base URL", Status::Skip, "not set"),
        ]
    }

    #[test]
    fn versions_compare_as_numbers() {
        assert!(parse_version("0.10.0") > parse_version("0.9.0"));
        assert_eq!(parse_version("1.2.3"), Some([1, 2, 3]));
        for bad in ["", "1.2", "1.2.3.4", "1.2.3-rc1", "v1.2.3", "a.b.c"] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_version_row_says_what_was_found() {
        let r = version_row("0.1.1", true, Some("0.2.0"));
        assert_eq!(
            (r.status, r.summary.as_str()),
            (Status::Warn, "0.1.1; 0.2.0 is available")
        );
        assert!(r.fix.unwrap().contains("cargo install alash"));
        let r = version_row("0.9.0", true, Some("0.10.0"));
        assert_eq!(r.status, Status::Warn, "0.10.0 is newer than 0.9.0");
        assert_eq!(
            version_row("0.1.1", true, Some("0.1.1")).status,
            Status::Pass
        );
        assert_eq!(
            version_row("0.2.0", true, Some("0.1.1")).status,
            Status::Pass
        );
        assert_eq!(version_row("0.1.1", true, None).status, Status::Skip);
        assert_eq!(version_row("0.1.1", false, None).status, Status::Skip);
    }

    #[test]
    fn every_line_of_the_frame_has_the_same_width() {
        for width in [None, Some(60), Some(200)] {
            let out = render("title", &sample(), false, width);
            let table: Vec<&str> = out
                .lines()
                .filter(|l| l.starts_with(['│', '┌', '├', '└']))
                .collect();
            assert!(
                table.len() >= 8,
                "three frame lines, the header and four rows"
            );
            let w = table[0].chars().count();
            assert!(table.iter().all(|l| l.chars().count() == w), "{out}");
        }
    }

    #[test]
    fn statuses_and_fixes_are_shown() {
        let out = render("alash doctor", &sample(), false, None);
        assert!(out.starts_with("alash doctor\n\n┌"));
        for label in ["✓ pass", "⚠ warn", "✗ fail", "· skip"] {
            assert!(out.contains(label), "{label}");
        }
        assert!(out.ends_with("\nkey binding: rebind it\ncodex: install codex\n"));
        assert!(!out.contains("shell:"), "a passing row has no fix line");
    }

    #[test]
    fn colour_is_only_added_when_asked_for() {
        assert!(!render("t", &sample(), false, None).contains('\x1b'));
        let out = render("t", &sample(), true, None);
        assert!(out.contains("\x1b[32m✓ pass\x1b[0m"));
        assert_eq!(out.matches("\x1b[0m").count(), 4, "the status cell only");
    }

    #[test]
    fn a_long_summary_wraps_inside_its_cell() {
        let long = "word ".repeat(40);
        let rows = vec![row("codex tools", Status::Pass, long.trim())];
        let out = render("t", &rows, false, Some(70));
        assert!(out.lines().all(|l| l.chars().count() <= 70), "{out}");
        assert!(
            out.lines()
                .filter(|l| l.starts_with("│             │"))
                .count()
                >= 2
        );
        assert_eq!(
            wrap("/a/very/long/path/without/spaces", 10).concat(),
            "/a/very/long/path/without/spaces"
        );
        assert!(wrap("/a/very/long/path/without/spaces", 10)
            .iter()
            .all(|l| l.chars().count() <= 10));
    }
}
