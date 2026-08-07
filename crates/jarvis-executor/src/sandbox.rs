use std::sync::OnceLock;

use regex::Regex;

fn blocked_patterns() -> &'static Vec<(Regex, &'static str)> {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            (r"(?i)rm\s+-rf\s+/", "recursive delete of root"),
            (r"(?i):\(\)\s*\{\s*:\|\s*&\s*\}\s*;", "fork bomb"),
            (r"(?i)mkfs\.", "filesystem format"),
        ]
        .iter()
        .filter_map(|(pattern, reason)| Regex::new(pattern).ok().map(|re| (re, *reason)))
        .collect()
    })
}

fn redact_patterns() -> &'static Vec<(Regex, &'static str)> {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            (
                r"(?i)(api[_-]?key|token|password|secret)\s*[:=]\s*\S+",
                "$1=[REDACTED]",
            ),
            (r"sk-[a-zA-Z0-9]{20,}", "[REDACTED_OPENAI_KEY]"),
            (r"ghp_[a-zA-Z0-9]{20,}", "[REDACTED_GITHUB_TOKEN]"),
        ]
        .iter()
        .filter_map(|(pattern, replacement)| {
            Regex::new(pattern).ok().map(|re| (re, *replacement))
        })
        .collect()
    })
}

pub fn is_command_blocked(command: &str) -> Option<&'static str> {
    for (re, reason) in blocked_patterns() {
        if re.is_match(command) {
            return Some(reason);
        }
    }
    if command.contains("sudo") {
        return Some("sudo requires elevated flag and confirmation");
    }
    None
}

pub fn redact_secrets(input: &str) -> String {
    let mut output = input.to_string();
    for (re, replacement) in redact_patterns() {
        output = re.replace_all(&output, *replacement).to_string();
    }
    output
}
