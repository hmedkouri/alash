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
printf '%s\n' "$@" > "$FAKE_ARGS"
echo "base=${ANTHROPIC_BASE_URL-unset}" >> "$FAKE_ARGS"
printf '%s\n' "$FAKE_REPLY"
exit "${FAKE_EXIT:-0}"
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
    assert_eq!(
        r.args,
        "-p\n--model\nhaiku\n--strict-mcp-config\n--setting-sources\n\n--tools=\n--\n-n means what?\nbase=unset\n"
    );
}

#[test]
fn codex_gets_exactly_the_tools_off_arguments() {
    let r = alash(
        "codex",
        &["ask", "@codex", "hello", "there"],
        &[("ALASH_CODEX_MODEL", "m1")],
    );
    assert!(r.out.status.success());
    assert_eq!(
        r.args,
        "exec\n--ignore-rules\n-s\nread-only\n--ephemeral\n\
         --skip-git-repo-check\n--color\nnever\n-c\nproject_doc_max_bytes=0\n-c\n\
         web_search=disabled\n--disable\nshell_tool\n--disable\nunified_exec\n--disable\n\
         view_image\n--disable\ngoals\n--disable\nmulti_agent\n--disable\napps\n--disable\n\
         plugins\n-c\nmcp_servers.probe.enabled=false\n-c\n\
         mcp_servers.remote_x.enabled=false\n-m\nm1\n--\nhello there\nbase=unset\n"
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
        r.args.contains("--\ncodex is what\n"),
        "a bare first word is never a marker"
    );

    let r = alash("env3", &["ask", "--", "@codex", "hi"], &[]);
    assert!(r.args.starts_with("-p\n") && r.args.contains("--\n@codex hi\n"));
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
        "This zsh command exited with status 2: ls /nope\nExplain why it failed and give the fix."
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
