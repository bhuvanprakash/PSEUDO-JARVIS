use regex::Regex;

pub fn is_command_blocked(command: &str) -> Option<&'static str> {
    let blocked = [
        (r"(?i)rm\s+-rf\s+/", "recursive delete of root"),
        (r"(?i):\(\)\s*\{\s*:\|\s*&\s*\}\s*;", "fork bomb"),
        (r"(?i)mkfs\.", "filesystem format"),
    ];
    for (pattern, reason) in blocked {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(command) {
                return Some(reason);
            }
        }
    }
    if command.contains("sudo") {
        return Some("sudo requires elevated flag and confirmation");
    }
    None
}

pub fn redact_secrets(input: &str) -> String {
    let patterns = [
        (r"(?i)(api[_-]?key|token|password|secret)\s*[:=]\s*\S+", "$1=[REDACTED]"),
        (r"sk-[a-zA-Z0-9]{20,}", "[REDACTED_OPENAI_KEY]"),
        (r"ghp_[a-zA-Z0-9]{20,}", "[REDACTED_GITHUB_TOKEN]"),
    ];
    let mut output = input.to_string();
    for (pattern, replacement) in patterns {
        if let Ok(re) = Regex::new(pattern) {
            output = re.replace_all(&output, replacement).to_string();
        }
    }
    output
}
