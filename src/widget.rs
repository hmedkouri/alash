//! What the key binding may put on the command line. Nothing here runs anything.

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// One line that looks like a command: place it for the user to read and run.
    Place(String),
    /// Anything else: show it, leave the command line alone.
    Message(String),
}

/// Drop code-fence lines and wrapping backticks; replace control characters, which could
/// otherwise drive the terminal when the text is displayed.
pub fn clean(raw: &str) -> String {
    let body = raw
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim();
    let unwrapped = body
        .strip_prefix('`')
        .and_then(|b| b.strip_suffix('`'))
        .filter(|b| !b.contains(['`', '\n']))
        .unwrap_or(body);
    printable(unwrapped.trim())
}

/// Text from another program, made safe to show: control characters other than newline
/// are replaced, so it cannot drive the terminal.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() && c != '\n' { '?' } else { c })
        .collect()
}

pub fn decide(raw: &str) -> Decision {
    let text = clean(raw);
    if text.is_empty() {
        Decision::Message("The model returned nothing.".into())
    } else if text.contains('\n') || text.ends_with('?') {
        Decision::Message(text)
    } else {
        Decision::Place(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(s: &str) -> Decision {
        Decision::Place(s.into())
    }

    #[test]
    fn one_line_is_placed() {
        assert_eq!(decide("ls -la\n"), place("ls -la"));
        assert_eq!(decide("```zsh\nls -la\n```\n"), place("ls -la"));
        assert_eq!(decide("`ls -la`"), place("ls -la"));
        assert_eq!(decide("echo `date`"), place("echo `date`"));
        assert_eq!(decide("`a` && `b`"), place("`a` && `b`"));
    }

    #[test]
    fn prose_and_questions_are_shown_not_placed() {
        assert!(matches!(decide("Which directory?"), Decision::Message(_)));
        assert!(matches!(decide("cd /tmp\nrm -rf x"), Decision::Message(_)));
        assert!(matches!(decide("```\n```"), Decision::Message(_)));
        assert!(matches!(decide(""), Decision::Message(_)));
    }

    #[test]
    fn control_characters_never_reach_the_terminal() {
        assert_eq!(decide("ls\x1b[2J -la"), place("ls?[2J -la"));
        assert_eq!(decide("ls\r"), place("ls"));
        assert_eq!(decide("a\rb"), place("a?b"));
    }
}
