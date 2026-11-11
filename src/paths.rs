use std::path::{Path, PathBuf};

const FREE_PI_DIR_NAME: &str = ".free-pi";

pub fn home_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home);
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
        return PathBuf::from(profile);
    }
    let drive = std::env::var_os("HOMEDRIVE").unwrap_or_default();
    let path = std::env::var_os("HOMEPATH").unwrap_or_default();
    if !drive.is_empty() || !path.is_empty() {
        let mut result = PathBuf::from(drive);
        result.push(path);
        return result;
    }
    PathBuf::from(".")
}

pub fn free_pi_dir() -> PathBuf {
    home_dir().join(FREE_PI_DIR_NAME)
}

/// Fully separate from pi's own `~/.pi/agent`, so an existing pi install (and
/// its credentials/settings/sessions) is never touched.
pub fn agent_dir() -> PathBuf {
    free_pi_dir().join("agent")
}

pub fn credentials_path(agent_dir: &Path) -> PathBuf {
    agent_dir.join("credentials.json")
}

pub fn gravity_dir() -> PathBuf {
    free_pi_dir().join("gravity")
}

pub fn last_seen_version_path(agent_dir: &Path) -> PathBuf {
    agent_dir.join("last-seen-version")
}

/// Windows `canonicalize` returns a verbatim path (`\\?\C:\...`). pi names its
/// session folders from the normal path, so strip the prefix before encoding.
fn normalise_display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        text.into_owned()
    }
}

fn session_dir_name(cwd: &Path) -> String {
    let display = normalise_display_path(cwd);
    let stripped = display
        .strip_prefix('/')
        .or_else(|| display.strip_prefix('\\'))
        .unwrap_or(&display);
    format!("--{}--", stripped.replace(['/', '\\', ':'], "-"))
}

/// The per-working-directory session folder pi uses, reproduced exactly so
/// `--session` and resume output agree with pi's own layout.
pub fn session_dir(agent_dir: &Path, cwd: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    agent_dir.join("sessions").join(session_dir_name(&resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_dir_matches_pi_layout() {
        let dir = session_dir(Path::new("/tmp/agent"), Path::new("/home/me/project"));
        let text = dir.to_string_lossy().replace('\\', "/");
        assert!(text.ends_with("sessions/--home-me-project--"));
    }

    #[test]
    fn strips_windows_verbatim_prefix() {
        assert_eq!(
            session_dir_name(Path::new(r"\\?\C:\Users\me\project")),
            "--C--Users-me-project--"
        );
        assert_eq!(
            session_dir_name(Path::new(r"C:\Users\me\project")),
            "--C--Users-me-project--"
        );
        assert_eq!(
            session_dir_name(Path::new(r"\\?\UNC\server\share\project")),
            "---server-share-project--"
        );
    }
}
