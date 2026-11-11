use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct StoredCredentials {
    token: String,
}

pub fn load_jwt(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed: StoredCredentials = serde_json::from_str(&raw).ok()?;
    if parsed.token.is_empty() {
        None
    } else {
        Some(parsed.token)
    }
}

pub fn save_jwt(path: &Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    let body = serde_json::to_vec(&StoredCredentials {
        token: token.to_string(),
    })?;
    fs::write(path, body).with_context(|| format!("could not write {}", path.display()))?;
    restrict_permissions(path);
    Ok(())
}

pub fn clear_jwt(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("could not remove {}", path.display())),
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_token() {
        let dir = std::env::temp_dir().join(format!("freepi-creds-{}", uuid::Uuid::now_v7()));
        let path = dir.join("credentials.json");
        assert_eq!(load_jwt(&path), None);
        save_jwt(&path, "abc.def.ghi").unwrap();
        assert_eq!(load_jwt(&path).as_deref(), Some("abc.def.ghi"));
        clear_jwt(&path).unwrap();
        assert_eq!(load_jwt(&path), None);
        clear_jwt(&path).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}
