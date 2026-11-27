//! The closed set of AI CLIs alash can drive, and how each is run one-shot with tools off.
//!
//! The argument lists here are the safety property of the whole tool. Each one was proved
//! against the real CLI; the proofs are written down in docs/tools-off.md. Change them only
//! with a new proof.

use std::collections::HashSet;
use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Being told to stop (Ctrl-C, hangup, `kill`) and passing that on to the harness, so it
/// never outlives alash. Two libc calls declared by hand: no dependency.
#[cfg(unix)]
mod signals {
    use std::sync::atomic::{AtomicBool, Ordering};

    static STOP: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_signal(_: i32) {
        STOP.store(true, Ordering::SeqCst);
    }

    extern "C" {
        fn signal(signum: i32, handler: extern "C" fn(i32)) -> usize;
        fn kill(pid: i32, sig: i32) -> i32;
    }

    // The same numbers on Linux and macOS.
    const SIGHUP: i32 = 1;
    const SIGINT: i32 = 2;
    const SIGTERM: i32 = 15;

    /// From here on a signal sets a flag in place of ending alash at once.
    pub fn watch() {
        for sig in [SIGHUP, SIGINT, SIGTERM] {
            // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
            unsafe { signal(sig, on_signal) };
        }
    }

    pub fn stopped() -> bool {
        STOP.load(Ordering::SeqCst)
    }

    /// Ask the process to end, so a wrapper script can stop what it started.
    pub fn terminate(pid: u32) {
        // SAFETY: plain syscall; a pid that is already gone is an ignored error.
        unsafe { kill(pid as i32, SIGTERM) };
    }
}

#[cfg(not(unix))]
mod signals {
    pub fn watch() {}
    pub fn stopped() -> bool {
        false
    }
    pub fn terminate(_pid: u32) {}
}

/// Seconds a harness may take. The key binding gets less: a frozen prompt is worse than a
/// slow answer.
fn time_limit(one_turn: bool) -> Result<Duration, String> {
    match std::env::var("ALASH_TIMEOUT") {
        Ok(v) if !v.is_empty() => match v.parse::<u64>() {
            Ok(secs) if secs > 0 => Ok(Duration::from_secs(secs)),
            _ => Err(format!(
                "ALASH_TIMEOUT={v} is not a number of seconds. Set it to a whole number above \
                 0, or unset it."
            )),
        },
        _ => Ok(Duration::from_secs(if one_turn { 30 } else { 60 })),
    }
}

/// Collects a pipe on a thread, so what the harness said so far can be read while it runs.
struct Collected {
    buf: Arc<Mutex<Vec<u8>>>,
    reader: std::thread::JoinHandle<()>,
}

fn collect(mut pipe: impl Read + Send + 'static) -> Collected {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let shared = Arc::clone(&buf);
    let reader = std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 {
                break;
            }
            shared.lock().unwrap().extend_from_slice(&chunk[..n]);
        }
    });
    Collected { buf, reader }
}

impl Collected {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.buf.lock().unwrap()).into_owned()
    }
}

/// What a harness answered, and what else it had to say.
#[derive(Debug)]
pub struct Reply {
    pub text: String,
    /// Warnings the harness wrote on stderr although it succeeded, such as a sign-in about
    /// to expire. Empty when there were none. Safe to print.
    pub warnings: String,
}

/// Whether a line of codex's stderr is a warning or an error, as opposed to its banner.
/// Its own messages start with the word; its log lines carry the level after a timestamp.
fn is_notice(line: &str) -> bool {
    let low = line.trim_start().to_lowercase();
    low.starts_with("warning")
        || low.starts_with("error")
        || line.contains(" WARN ")
        || line.contains(" ERROR ")
}

/// The last few distinct lines: codex retries and repeats itself on stderr.
fn last_lines(said: &str) -> String {
    let mut lines: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.dedup();
    lines[lines.len().saturating_sub(4)..].join("\n")
}

/// End the harness: ask first, then force. It must not outlive alash.
fn stop(child: &mut Child) {
    signals::terminate(child.id());
    let asked = Instant::now();
    while asked.elapsed() < Duration::from_secs(2) {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Harness {
    Claude,
    Codex,
}

/// codex has no "no tools" switch. Disabling these features removes every tool that runs
/// a command, reads a file or reaches the network. `apply_patch` cannot be removed for
/// gpt-5 models; `-s read-only` rejects it.
pub const CODEX_DISABLED_FEATURES: [&str; 7] = [
    "shell_tool",
    "unified_exec",
    "view_image",
    "goals",
    "multi_agent",
    "apps",
    "plugins",
];

impl Harness {
    pub const ALL: [Harness; 2] = [Harness::Claude, Harness::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
        }
    }

    pub fn from_name(name: &str) -> Option<Harness> {
        Harness::ALL.into_iter().find(|h| h.name() == name)
    }

    /// The harness named by `ALASH_HARNESS`, or claude.
    pub fn default_from_env() -> Result<Harness, String> {
        match std::env::var("ALASH_HARNESS") {
            Ok(name) if !name.is_empty() => Harness::from_name(&name)
                .ok_or_else(|| format!("ALASH_HARNESS={name} is not a known harness. {KNOWN}")),
            _ => Ok(Harness::Claude),
        }
    }

    fn model(self) -> Option<String> {
        let var = match self {
            Harness::Claude => "ASK_MODEL",
            Harness::Codex => "ALASH_CODEX_MODEL",
        };
        match std::env::var(var) {
            Ok(m) if !m.is_empty() => Some(m),
            _ if self == Harness::Claude => Some("haiku".into()),
            _ => None,
        }
    }

    /// The full argument list for a one-shot, non-interactive, tools-off call.
    /// `mcp_servers` are the codex MCP servers to switch off by name.
    pub fn args(self, prompt: &str, one_turn: bool, mcp_servers: &[String]) -> Vec<String> {
        let mut a: Vec<String> = Vec::new();
        let mut push = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
        match self {
            Harness::Claude => {
                push(&["-p"]);
                if let Some(m) = self.model() {
                    push(&["--model", &m]);
                }
                // `--tools=` and not `--tools ""`: the option is variadic and would
                // swallow the prompt. Not `--bare`: it ignores the subscription login.
                push(&["--strict-mcp-config", "--setting-sources", "", "--tools="]);
                if one_turn {
                    push(&["--max-turns", "1"]);
                }
            }
            Harness::Codex => {
                // The user's config is kept, so their model provider and login work.
                push(&["exec", "--ignore-rules"]);
                push(&["-s", "read-only", "--ephemeral", "--skip-git-repo-check"]);
                push(&["--color", "never"]);
                push(&["-c", "project_doc_max_bytes=0", "-c", "web_search=disabled"]);
                for f in CODEX_DISABLED_FEATURES {
                    push(&["--disable", f]);
                }
                for server in mcp_servers {
                    push(&["-c", &format!("mcp_servers.{server}.enabled=false")]);
                }
                if let Some(m) = self.model() {
                    push(&["-m", &m]);
                }
            }
        }
        // `--` so a question that starts with a dash is not read as an option.
        push(&["--", prompt]);
        a
    }

    /// Features this codex does not know. `--disable` on an unknown name is a hard error,
    /// and a renamed feature would mean a tool alash can no longer switch off.
    pub fn missing_features(self) -> Result<Vec<&'static str>, String> {
        if self != Harness::Codex {
            return Ok(Vec::new());
        }
        let out = Command::new("codex")
            .args(["features", "list"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| self.spawn_error(&e))?;
        let listed = String::from_utf8_lossy(&out.stdout).into_owned();
        let known: HashSet<&str> = listed
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        Ok(CODEX_DISABLED_FEATURES
            .into_iter()
            .filter(|f| !known.contains(f))
            .collect())
    }

    /// The MCP servers codex would start here, from the user's config and a trusted
    /// project's. Each is a tool, so each is switched off by name; `Err` means the list
    /// could not be read and codex must not be run.
    pub fn mcp_servers(self) -> Result<Vec<String>, String> {
        if self != Harness::Codex {
            return Ok(Vec::new());
        }
        let out = Command::new("codex")
            .args(["mcp", "list", "--json"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| self.spawn_error(&e))?;
        out.status
            .success()
            .then(|| parse_mcp_names(&String::from_utf8_lossy(&out.stdout)))
            .flatten()
            .ok_or_else(|| {
                "alash could not read codex's MCP server list (`codex mcp list --json`), so it \
                 cannot switch those servers off and will not run codex. Use `ask @claude`. \
                 Details: ask --doctor"
                    .to_string()
            })
    }

    /// What to pass on from the stderr of a call that succeeded.
    ///
    /// claude's stderr is quiet, so all of it is a warning. codex writes its banner, the
    /// prompt, any piped input and the answer there on every call: those are removed by
    /// exact match, since alash knows what it sent and received, and of the rest only
    /// lines that are warnings or errors are kept.
    fn warnings(self, stderr: &str, prompt: &str, answer: &str) -> String {
        let kept = match self {
            Harness::Claude => stderr.to_string(),
            Harness::Codex => {
                let mut rest = stderr.replace(prompt, "");
                if let (Some(open), Some(close)) = (rest.find("<stdin>\n"), rest.rfind("</stdin>"))
                {
                    if open < close {
                        rest.replace_range(open..close, "");
                    }
                }
                let answer = answer.trim();
                if !answer.is_empty() {
                    rest = rest.replace(answer, "");
                }
                rest.lines()
                    .filter(|l| is_notice(l))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };
        crate::widget::printable(&last_lines(&kept))
    }

    /// Run the harness and return its answer. `Err` is a complete message for the user.
    pub fn run(self, prompt: &str, one_turn: bool, stdin: Stdio) -> Result<Reply, String> {
        let missing = self.missing_features()?;
        if !missing.is_empty() {
            return Err(format!(
                "This codex has no feature named {}, so alash cannot switch its tools off and \
                 will not run it. Update alash, or use `ask @claude`. Details: ask --doctor",
                missing.join(", ")
            ));
        }
        let mcp_servers = self.mcp_servers()?;
        let limit = time_limit(one_turn)?;
        let mut cmd = Command::new(self.name());
        cmd.args(self.args(prompt, one_turn, &mcp_servers))
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let base_url = base_url();
        if let (Harness::Claude, Some(url)) = (self, &base_url) {
            cmd.env("ANTHROPIC_BASE_URL", url);
        }
        signals::watch();
        let mut child = cmd.spawn().map_err(|e| self.spawn_error(&e))?;
        let stdout = collect(child.stdout.take().expect("stdout is piped"));
        let stderr = collect(child.stderr.take().expect("stderr is piped"));
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(e) => return Err(format!("could not wait for {}: {e}", self.name())),
            }
            if signals::stopped() {
                stop(&mut child);
                return Err(format!("{} was stopped.", self.name()));
            }
            if started.elapsed() >= limit {
                stop(&mut child);
                // A harness that cannot reach its provider retries without end. What it
                // said so far is the only clue, so it goes first.
                return Err(format!(
                    "{}\n{} did not answer within {} s, so alash stopped it. Check its login \
                     and network with `ask --doctor --live`. ALASH_TIMEOUT=<seconds> changes \
                     the limit.",
                    last_lines(&stderr.text()),
                    self.name(),
                    limit.as_secs()
                )
                .trim_start()
                .to_string());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        // The readers end when the pipes close, normally at exit. The wait is bounded in
        // case something the harness started still holds a pipe open.
        let exited = Instant::now();
        while !(stdout.reader.is_finished() && stderr.reader.is_finished())
            && exited.elapsed() < Duration::from_secs(2)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let stdout = stdout.text();
        if status.success() {
            let warnings = self.warnings(&stderr.text(), prompt, &stdout);
            return Ok(Reply {
                text: stdout,
                warnings,
            });
        }
        let said = format!("{stdout}{}", stderr.text());
        let low = said.to_lowercase();
        let said = last_lines(&said);
        let hint = if let (Harness::Claude, Some(url)) = (self, &base_url) {
            if url_answers(url) {
                self.login_hint(&low)
            } else {
                Some(format!(
                    "ALASH_BASE_URL={url} is not answering. Start that proxy, or unset \
                     ALASH_BASE_URL to call Anthropic directly."
                ))
            }
        } else {
            self.login_hint(&low)
        };
        Err(format!(
            "{}\n{} failed ({}). {}",
            said.trim_end(),
            self.name(),
            status,
            hint.unwrap_or_else(|| "Run `ask --doctor` to check the setup.".into())
        ))
    }

    fn login_hint(self, low: &str) -> Option<String> {
        let logged_out = ["not logged in", "/login", "unauthorized", "401"]
            .iter()
            .any(|s| low.contains(s));
        logged_out.then(|| match self {
            Harness::Claude => "claude is not logged in: run `claude` and type /login.".into(),
            Harness::Codex => "codex is not logged in: run `codex login`, or check the model \
                               provider in ~/.codex/config.toml."
                .into(),
        })
    }

    fn spawn_error(self, e: &std::io::Error) -> String {
        if e.kind() == ErrorKind::NotFound {
            let other = match self {
                Harness::Claude => "codex",
                Harness::Codex => "claude",
            };
            format!(
                "{0} is not installed or not on the PATH. Install it and log in, or use \
                 `ask @{other}`.",
                self.name()
            )
        } else {
            format!("could not start {}: {e}", self.name())
        }
    }
}

/// Server names from the pretty-printed JSON array `codex mcp list --json` prints.
/// None when the output is not that shape or a name could not be passed safely as a
/// `-c mcp_servers.<name>.enabled=false` key: a server alash cannot name stays on.
// ponytail: line-based on codex's two-space pretty printing, no JSON parser. The count
// check below refuses on any other layout; parse properly if codex changes it.
fn parse_mcp_names(json: &str) -> Option<Vec<String>> {
    if !json.trim_start().starts_with('[') {
        return None;
    }
    let names: Vec<String> = json
        .lines()
        .filter_map(|l| l.strip_prefix("    \"name\": \"")?.strip_suffix("\","))
        .map(str::to_string)
        .collect();
    let objects = json.lines().filter(|l| *l == "  {").count();
    let safe = |n: &String| {
        !n.is_empty()
            && n.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    let complete = names.len() == objects && json.matches("\"transport\"").count() == objects;
    (complete && names.iter().all(safe)).then_some(names)
}

pub const KNOWN: &str = "Known harnesses: @claude, @codex.";

/// Split a leading `@harness` marker off a question. Without one the default applies.
pub fn split_marker(text: &str) -> Result<(Harness, &str), String> {
    let text = text.trim();
    let Some(marked) = text.strip_prefix('@') else {
        return Ok((Harness::default_from_env()?, text));
    };
    let (name, rest) = marked
        .split_once(char::is_whitespace)
        .unwrap_or((marked, ""));
    match Harness::from_name(name) {
        Some(h) => Ok((h, rest.trim_start())),
        None => Err(format!(
            "@{name} is not a known harness. {KNOWN} For a question that starts with @, use \
             `ask -- <question>`."
        )),
    }
}

pub fn base_url() -> Option<String> {
    std::env::var("ALASH_BASE_URL")
        .ok()
        .filter(|u| !u.is_empty())
}

/// `host:port` of a URL, with the scheme's default port when none is written.
fn host_port(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    if authority.is_empty() {
        return None;
    }
    let has_port = authority
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok());
    Some(if has_port {
        authority.to_string()
    } else {
        format!("{authority}:{}", if scheme == "https" { 443 } else { 80 })
    })
}

/// Whether something accepts a TCP connection at the URL. No HTTP request is made.
pub fn url_answers(url: &str) -> bool {
    host_port(url)
        .and_then(|hp| hp.to_socket_addrs().ok()?.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_port_adds_the_default_port() {
        assert_eq!(
            host_port("http://127.0.0.1:8787").unwrap(),
            "127.0.0.1:8787"
        );
        assert_eq!(host_port("http://localhost/v1").unwrap(), "localhost:80");
        assert_eq!(
            host_port("https://u:p@example.com/x").unwrap(),
            "example.com:443"
        );
        assert_eq!(host_port("http://[::1]").unwrap(), "[::1]:80");
        assert_eq!(host_port("http://[::1]:9000/").unwrap(), "[::1]:9000");
        assert_eq!(host_port("localhost:8787"), None);
    }

    #[test]
    fn mcp_names_are_read_or_the_list_is_refused() {
        let list = "[\n  {\n    \"name\": \"off-one\",\n    \"transport\": {\n      \"type\": \"stdio\"\n    }\n  },\n  {\n    \"name\": \"remote_x\",\n    \"transport\": {\n      \"name\": \"inner\"\n    }\n  }\n]\n";
        assert_eq!(parse_mcp_names(list).unwrap(), ["off-one", "remote_x"]);
        assert_eq!(parse_mcp_names("[]\n").unwrap(), Vec::<String>::new());
        assert_eq!(parse_mcp_names(""), None);
        assert_eq!(parse_mcp_names("error: no such command"), None);
        // One line, so no name is found for the object: refuse.
        assert_eq!(parse_mcp_names("[{\"name\":\"a\",\"transport\":{}}]"), None);
        // A name that would break out of the config key: refuse.
        assert_eq!(parse_mcp_names(&list.replace("off-one", "a.b=1")), None);
    }

    #[test]
    fn claude_warnings_are_passed_on_whole() {
        let w = Harness::Claude.warnings("Warning: this sign-in ends soon.\n", "q", "a");
        assert_eq!(w, "Warning: this sign-in ends soon.");
        assert_eq!(Harness::Claude.warnings("", "q", "a"), "");
        assert_eq!(
            Harness::Claude.warnings("bad\x1b[2Jtext\n", "q", "a"),
            "bad?[2Jtext"
        );
    }

    #[test]
    fn codex_warnings_are_picked_out_of_its_banner() {
        let prompt = "First line.\nwarning: a prompt line that starts like one.";
        let stderr = format!(
            "Reading additional input from stdin...\nOpenAI Codex v0.171.0\n--------\n\
             workdir: /tmp\nmodel: m\n--------\nuser\n{prompt}\n\n<stdin>\n\
             error: ENOSPC on device\nwarning: disk almost full\n</stdin>\n\
             2026-11-27T10:00:00.1Z  WARN codex_login: token expires in 2 days\n\
             warning: Falling back from WebSockets to HTTPS transport.\n\
             codex\nerror is not the answer\ntokens used\n4,732\n"
        );
        assert_eq!(
            Harness::Codex.warnings(&stderr, prompt, "error is not the answer\n"),
            "2026-11-27T10:00:00.1Z  WARN codex_login: token expires in 2 days\n\
             warning: Falling back from WebSockets to HTTPS transport."
        );
        let quiet = "OpenAI Codex v0.171.0\n--------\nuser\nhi\ncodex\nOk\ntokens used\n4\n";
        assert_eq!(Harness::Codex.warnings(quiet, "hi", "Ok\n"), "");
    }

    #[test]
    fn marker_picks_the_harness_only_when_it_leads() {
        assert_eq!(split_marker("@codex why").unwrap(), (Harness::Codex, "why"));
        assert_eq!(split_marker("@claude").unwrap(), (Harness::Claude, ""));
        assert_eq!(split_marker("codex is what?").unwrap().1, "codex is what?");
        assert!(split_marker("@gemini hi").is_err());
    }
}
