use std::process::{Command, Stdio};

/// Only http(s) URLs are ever handed to a platform opener: a `file:` or custom
/// scheme is a launch primitive, not a link. Control characters are rejected.
pub fn safe_browser_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control()) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

/// Best-effort, detached browser open. Printing the URL is the primary path
/// (SSH sessions and containers have no browser), so all failures are silent.
pub fn open_browser(url: &str) {
    let Some(safe) = safe_browser_url(url) else {
        return;
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &safe]);
        cmd
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg(&safe);
        cmd
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(&safe);
        cmd
    };

    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_schemes() {
        assert!(safe_browser_url("https://github.com/login/device").is_some());
        assert!(safe_browser_url("http://127.0.0.1:8080/x").is_some());
        assert!(safe_browser_url("file:///etc/passwd").is_none());
        assert!(safe_browser_url("javascript:alert(1)").is_none());
        assert!(safe_browser_url("https://example.com/\nrm").is_none());
    }
}
