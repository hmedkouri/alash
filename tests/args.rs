//! A fake harness first on the PATH records the exact arguments alash passes. This is what
//! catches a dropped `--tools=` or `-s read-only`.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

const FAKE: &str = r#"#!/bin/sh
if [ "$1" = features ]; then
  for f in $FAKE_FEATURES; do echo "$f stable true"; done
  exit 0
fi
if [ "$1" = mcp ]; then
  printf '%s' "$FAKE_MCP"
  exit 0
fi
if [ -n "$FAKE_SLEEP" ]; then
  echo "$FAKE_STDERR" >&2
  echo $$ > "$FAKE_PID"
  exec sleep "$FAKE_SLEEP"
fi
printf '%s\n' "$@" > "$FAKE_ARGS"
echo "base=${ANTHROPIC_BASE_URL-unset}" >> "$FAKE_ARGS"
[ -n "$FAKE_WARN" ] && printf '%s\n' "$FAKE_WARN" >&2
printf '%s\n' "$FAKE_REPLY"
exit "${FAKE_EXIT:-0}"
"#;

/// Stands in for curl, so no test reaches the network. Prints what `-w %{redirect_url}`
/// would: the address GitHub redirects the latest release to.
const FAKE_CURL: &str = r#"#!/bin/sh
[ -n "$FAKE_CURL_SLEEP" ] && exec sleep "$FAKE_CURL_SLEEP"
printf '%s' "$FAKE_LATEST"
exit "${FAKE_CURL_EXIT:-0}"
"#;

const ALL_FEATURES: &str = "shell_tool unified_exec view_image goals multi_agent apps plugins";

const MCP_LIST: &str = "[\n  {\n    \"name\": \"probe\",\n    \"enabled\": true,\n    \"transport\": {\n      \"type\": \"stdio\"\n    }\n  },\n  {\n    \"name\": \"remote_x\",\n    \"enabled\": true,\n    \"transport\": {\n      \"type\": \"streamable_http\"\n    }\n  }\n]\n";

struct Run {
    out: Output,
    args: String,
}

/// Runs alash with fake `claude` and `codex` as the only programs on the PATH.
fn alash(test: &str, args: &[&str], env: &[(&str, &str)]) -> Run {
    let dir: PathBuf = std::env::temp_dir().join(format!("alash-{}-{test}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for name in ["claude", "codex"] {
        let path = dir.join(name);
        fs::write(&path, FAKE).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let curl = dir.join("curl");
    fs::write(&curl, FAKE_CURL).unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let record = dir.join("args");
    let out = Command::new(env!("CARGO_BIN_EXE_alash"))
        .args(args)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FAKE_ARGS", &record)
        .env("FAKE_REPLY", "ls -la")
        .env("FAKE_FEATURES", ALL_FEATURES)
        .env("FAKE_MCP", MCP_LIST)
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let args = fs::read_to_string(&record).unwrap_or_default();
    fs::remove_dir_all(&dir).unwrap();
    Run { out, args }
}

fn stdout(r: &Run) -> String {
    String::from_utf8_lossy(&r.out.stdout).into_owned()
}

#[test]
fn claude_gets_exactly_the_tools_off_arguments() {
    let r = alash("claude", &["ask", "-n", "means", "what?"], &[]);
    assert!(r.out.status.success());
    assert_eq!(stdout(&r), "ls -la\n");
    let (args, prompt) = r.args.split_once("--\n").unwrap();
    assert_eq!(
        args,
        "-p\n--model\nhaiku\n--strict-mcp-config\n--setting-sources\n\n--tools=\n"
    );
    assert!(prompt.starts_with("You have no tools in this session"));
    assert!(prompt.ends_with("\n\n-n means what?\nbase=unset\n"));
}

#[test]
fn codex_gets_exactly_the_tools_off_arguments() {
    let r = alash(
        "codex",
        &["ask", "@codex", "hello", "there"],
        &[("ALASH_CODEX_MODEL", "m1")],
    );
    assert!(r.out.status.success());
    let (args, prompt) = r.args.split_once("--\n").unwrap();
    assert!(prompt.starts_with("You have no tools in this session"));
    assert!(prompt.ends_with("\n\nhello there\nbase=unset\n"));
    assert_eq!(
        args,
        "exec\n--ignore-rules\n-s\nread-only\n--ephemeral\n\
         --skip-git-repo-check\n--color\nnever\n-c\nproject_doc_max_bytes=0\n-c\n\
         web_search=disabled\n--disable\nshell_tool\n--disable\nunified_exec\n--disable\n\
         view_image\n--disable\ngoals\n--disable\nmulti_agent\n--disable\napps\n--disable\n\
         plugins\n-c\nmcp_servers.probe.enabled=false\n-c\n\
         mcp_servers.remote_x.enabled=false\n-m\nm1\n"
    );
}

#[test]
fn codex_is_refused_when_a_tool_cannot_be_switched_off() {
    let r = alash(
        "refuse",
        &["ask", "@codex", "hi"],
        &[("FAKE_FEATURES", "unified_exec goals")],
    );
    assert!(!r.out.status.success());
    assert_eq!(r.args, "", "codex must not be run");
    assert!(String::from_utf8_lossy(&r.out.stderr).contains("shell_tool"));
}

#[test]
fn codex_is_refused_when_its_mcp_servers_cannot_be_listed() {
    let r = alash(
        "mcp",
        &["ask", "@codex", "hi"],
        &[("FAKE_MCP", "something else")],
    );
    assert!(!r.out.status.success());
    assert_eq!(r.args, "", "codex must not be run");
    assert!(String::from_utf8_lossy(&r.out.stderr).contains("MCP server list"));
}

#[test]
fn environment_selects_model_harness_and_base_url() {
    let env = [
        ("ASK_MODEL", "opus"),
        ("ALASH_BASE_URL", "http://127.0.0.1:1"),
    ];
    let r = alash("env", &["ask", "hi"], &env);
    assert!(r.args.contains("--model\nopus\n"));
    assert!(r.args.ends_with("base=http://127.0.0.1:1\n"));

    let r = alash(
        "env2",
        &["ask", "codex", "is", "what"],
        &[("ALASH_HARNESS", "codex")],
    );
    assert!(
        r.args.starts_with("exec\n"),
        "the default harness comes from ALASH_HARNESS"
    );
    assert!(
        r.args.contains("\n\ncodex is what\n"),
        "a bare first word is never a marker"
    );

    let r = alash("env3", &["ask", "--", "@codex", "hi"], &[]);
    assert!(r.args.starts_with("-p\n") && r.args.contains("\n\n@codex hi\n"));
}

#[test]
fn why_sends_the_command_and_its_status() {
    let r = alash(
        "why",
        &["why", "2", "\t ls /nope"],
        &[("ALASH_SHELL", "zsh")],
    );
    assert!(r.out.status.success());
    assert!(r.args.contains(
        "\n\nThis zsh command exited with status 2: ls /nope\nYou cannot see what it printed. \
         Answer in at most four lines: the most likely cause in one or two sentences, then \
         the one most likely fix as a command. Do not restate the command, do not guess at \
         its output, and do not list alternatives.\n"
    ));
    assert!(!r.args.contains("--max-turns"));
}

#[test]
fn cmd_places_one_line_and_shows_anything_else() {
    let r = alash(
        "place",
        &["cmd", "list files"],
        &[("FAKE_REPLY", "```sh\nls -la\n```")],
    );
    assert!(r.out.status.success());
    assert_eq!(stdout(&r), "ls -la\n");
    assert!(r.args.contains("--tools=\n--max-turns\n1\n--\n"));
    assert!(r.args.contains("that does: list files"));

    let r = alash(
        "question",
        &["cmd", "delete it"],
        &[("FAKE_REPLY", "Delete what?")],
    );
    assert!(!r.out.status.success());
    assert_eq!(stdout(&r), "Delete what?\n");

    let r = alash(
        "failed",
        &["cmd", "x"],
        &[("FAKE_REPLY", "Not logged in"), ("FAKE_EXIT", "1")],
    );
    assert!(!r.out.status.success());
    assert!(stdout(&r).contains("claude is not logged in"));
    assert!(r.out.stderr.is_empty(), "the key binding shows stdout only");
}

#[test]
fn a_missing_harness_says_what_to_do() {
    let out = Command::new(env!("CARGO_BIN_EXE_alash"))
        .args(["ask", "hi"])
        .env_clear()
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("claude is not installed"));
}

/// A directory holding fake harnesses that hang, and the file their pid is written to.
fn hanging(test: &str) -> (PathBuf, PathBuf) {
    let dir: PathBuf = std::env::temp_dir().join(format!("alash-{}-{test}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for name in ["claude", "codex"] {
        let path = dir.join(name);
        fs::write(&path, FAKE).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let pid = dir.join("pid");
    (dir, pid)
}

fn hanging_alash(dir: &std::path::Path, pid: &std::path::Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_alash"));
    cmd.args(args)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .env("FAKE_FEATURES", ALL_FEATURES)
        .env("FAKE_MCP", "[]\n")
        .env("FAKE_SLEEP", "30")
        .env("FAKE_STDERR", "ERROR: Reconnecting... waiting for network")
        .env("FAKE_PID", pid);
    cmd
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn a_harness_that_never_answers_is_stopped_and_what_it_said_is_shown() {
    let (dir, pid) = hanging("timeout");
    let started = std::time::Instant::now();
    let out = hanging_alash(&dir, &pid, &["ask", "@codex", "hi"])
        .env("ALASH_TIMEOUT", "1")
        .output()
        .unwrap();
    assert!(
        started.elapsed().as_secs() < 5,
        "alash must not wait for the harness"
    );
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("ERROR: Reconnecting... waiting for network"),
        "{err}"
    );
    assert!(err.contains("codex did not answer within 1 s"), "{err}");
    assert!(
        !alive(&fs::read_to_string(&pid).unwrap()),
        "the harness must be gone"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_key_binding_reports_a_timeout_on_stdout() {
    let (dir, pid) = hanging("timeout-cmd");
    let out = hanging_alash(&dir, &pid, &["cmd", "list files"])
        .env("ALASH_TIMEOUT", "1")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("claude did not answer within 1 s"));
    assert!(!alive(&fs::read_to_string(&pid).unwrap()));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stopping_alash_stops_the_harness() {
    let (dir, pid) = hanging("sigterm");
    let mut child = hanging_alash(&dir, &pid, &["ask", "hi"])
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let waited = std::time::Instant::now();
    while !pid.exists() || fs::read_to_string(&pid).unwrap().trim().is_empty() {
        assert!(
            waited.elapsed().as_secs() < 5,
            "the fake harness never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let harness = fs::read_to_string(&pid).unwrap();
    assert!(alive(&harness));
    Command::new("kill")
        .arg(child.id().to_string())
        .status()
        .unwrap();
    assert!(!child.wait().unwrap().success());
    assert!(!alive(&harness), "the harness must not outlive alash");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_bad_time_limit_says_what_to_do() {
    let r = alash("badlimit", &["ask", "hi"], &[("ALASH_TIMEOUT", "soon")]);
    assert!(!r.out.status.success());
    assert_eq!(r.args, "", "the harness must not be run");
    assert!(String::from_utf8_lossy(&r.out.stderr).contains("ALASH_TIMEOUT=soon is not a number"));
}

const ALL_FLAGS: &str = "v1 --print --model --tools --setting-sources --strict-mcp-config \
     --sandbox --ephemeral --skip-git-repo-check --ignore-rules --disable --color";

#[test]
fn the_doctor_reports_every_check_as_a_table_row() {
    let r = alash("doctor", &["ask", "--doctor"], &[("FAKE_REPLY", ALL_FLAGS)]);
    let out = stdout(&r);
    assert!(r.out.status.success(), "{out}");
    assert!(out.starts_with("alash doctor v"), "{out}");
    assert!(!out.contains('\x1b'), "piped output has no colour codes");
    for check in [
        "version",
        "shell",
        "why",
        "key binding",
        "claude",
        "claude tools",
        "claude round trip",
        "codex",
        "codex tools",
        "codex mcp",
        "codex instructions",
        "codex round trip",
        "base URL",
    ] {
        assert!(
            out.contains(&format!("\n│ {check} ")),
            "missing row {check}:\n{out}"
        );
    }
    assert!(out.contains("switched off by name: probe, remote_x"));
    assert!(out.contains("claude round trip  │ · skip"));
}

#[test]
fn the_doctor_fails_only_when_the_default_harness_cannot_work() {
    // codex cannot switch a tool off: a warning while claude is the default.
    let broken = [("FAKE_REPLY", ALL_FLAGS), ("FAKE_FEATURES", "goals")];
    let r = alash("doctor-warn", &["ask", "--doctor"], &broken);
    assert!(r.out.status.success());
    assert!(stdout(&r).contains("codex tools        │ ⚠ warn │ NOT off"));

    let default_codex = [broken[0], broken[1], ("ALASH_HARNESS", "codex")];
    let r = alash("doctor-fail", &["ask", "--doctor"], &default_codex);
    assert!(!r.out.status.success());
    assert!(stdout(&r).contains("codex tools        │ ✗ fail │ NOT off"));
}

const SIGN_IN: &str = "Warning: this sign-in ends between 2026-12-03 and 2026-12-06.";

#[test]
fn a_warning_from_a_harness_that_succeeds_is_passed_on() {
    let r = alash("warn", &["ask", "hi"], &[("FAKE_WARN", SIGN_IN)]);
    assert!(r.out.status.success());
    assert_eq!(stdout(&r), "ls -la\n", "a pipe receives only the answer");
    assert_eq!(
        String::from_utf8_lossy(&r.out.stderr),
        format!("{SIGN_IN}\n")
    );

    let r = alash(
        "warn-why",
        &["why", "2", "ls /nope"],
        &[("FAKE_WARN", SIGN_IN)],
    );
    assert_eq!(
        String::from_utf8_lossy(&r.out.stderr),
        format!("{SIGN_IN}\n")
    );

    let r = alash("no-warn", &["ask", "hi"], &[]);
    assert!(r.out.stderr.is_empty());
}

#[test]
fn the_key_binding_gets_the_command_first_and_the_warning_after() {
    let r = alash(
        "warn-cmd",
        &["cmd", "list files"],
        &[("FAKE_WARN", SIGN_IN)],
    );
    assert!(r.out.status.success(), "the command is still placed");
    assert_eq!(stdout(&r), format!("ls -la\n{SIGN_IN}\n"));
    assert!(r.out.stderr.is_empty());
}

#[test]
fn only_codex_lines_that_are_warnings_are_passed_on() {
    let banner = "OpenAI Codex v0.171.0\n--------\nworkdir: /tmp\n--------\nuser\n\
                  warning: Falling back from WebSockets to HTTPS transport.\ncodex\ntokens used";
    let r = alash(
        "warn-codex",
        &["ask", "@codex", "hi"],
        &[("FAKE_WARN", banner)],
    );
    assert!(r.out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&r.out.stderr),
        "warning: Falling back from WebSockets to HTTPS transport.\n"
    );
}

const TAG: &str = "https://github.com/hmedkouri/alash/releases/tag/v";

#[test]
fn the_doctor_reports_a_newer_release() {
    let newer = format!("{TAG}999.0.0");
    let env = [("FAKE_REPLY", ALL_FLAGS), ("FAKE_LATEST", newer.as_str())];
    let r = alash("version-new", &["ask", "--doctor"], &env);
    let out = stdout(&r);
    assert!(
        r.out.status.success(),
        "a newer release is a warning, not a failure"
    );
    assert!(out.contains("│ version            │ ⚠ warn │"), "{out}");
    assert!(out.contains("999.0.0 is available"));
    assert!(out.contains("version: update with `cargo install alash`"));

    let same = format!("{TAG}{}", env!("CARGO_PKG_VERSION"));
    let env = [("FAKE_REPLY", ALL_FLAGS), ("FAKE_LATEST", same.as_str())];
    let r = alash("version-same", &["ask", "--doctor"], &env);
    assert!(stdout(&r).contains("│ version            │ ✓ pass │"));
    assert!(stdout(&r).contains("is the latest release"));
}

#[test]
fn the_doctor_survives_a_version_check_that_cannot_be_made() {
    let failing = [("FAKE_REPLY", ALL_FLAGS), ("FAKE_CURL_EXIT", "6")];
    let r = alash("version-fail", &["ask", "--doctor"], &failing);
    assert!(r.out.status.success());
    assert!(stdout(&r).contains("│ version            │ · skip │"));
    assert!(stdout(&r).contains("could not be checked"));

    let started = std::time::Instant::now();
    let hanging = [("FAKE_REPLY", ALL_FLAGS), ("FAKE_CURL_SLEEP", "60")];
    let r = alash("version-hang", &["ask", "--doctor"], &hanging);
    assert!(
        started.elapsed().as_secs() < 15,
        "the doctor must not wait for curl"
    );
    assert!(r.out.status.success());
    assert!(stdout(&r).contains("could not be checked"));

    let off = [
        ("FAKE_REPLY", ALL_FLAGS),
        ("ALASH_NO_UPDATE_CHECK", "1"),
        ("FAKE_LATEST", "x/v9.9.9"),
    ];
    let r = alash("version-off", &["ask", "--doctor"], &off);
    assert!(stdout(&r).contains("check switched off by ALASH_NO_UPDATE_CHECK"));
}

#[test]
fn init_stamps_the_shell_code_with_the_version() {
    for shell in ["zsh", "bash"] {
        let r = alash("init", &["init", shell], &[]);
        let code = stdout(&r);
        assert!(r.out.status.success());
        assert!(
            code.contains(concat!("ALASH_INIT=", env!("CARGO_PKG_VERSION"), " ")),
            "{code}"
        );
        assert!(!code.contains("@VERSION@"));
    }
}

#[test]
fn the_doctor_tells_a_shell_from_before_an_upgrade_from_one_never_set_up() {
    let version = env!("CARGO_PKG_VERSION");
    let current = [
        ("FAKE_REPLY", ALL_FLAGS),
        ("ALASH_SHELL", "zsh"),
        ("ALASH_WHY", "1"),
        ("ALASH_KEY", "1"),
        ("ALASH_INIT", version),
    ];
    let out = stdout(&alash("shell-current", &["ask", "--doctor"], &current));
    assert!(
        out.contains("│ shell              │ ✓ pass │ called from the zsh `ask` function"),
        "{out}"
    );

    // What a shell started under an older alash passes: no version, no evidence.
    let old = [("FAKE_REPLY", ALL_FLAGS), ("ALASH_SHELL", "zsh")];
    let r = alash("shell-old", &["ask", "--doctor"], &old);
    let out = stdout(&r);
    assert!(r.out.status.success(), "a stale shell is a warning");
    assert!(
        out.contains("│ shell              │ ⚠ warn │ this zsh loaded alash from an older version"),
        "{out}"
    );
    assert!(out.contains("open a new shell, or run: eval \"$(alash init zsh)\""));
}
