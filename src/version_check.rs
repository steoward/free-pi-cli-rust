use std::cmp::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FusionIds {
    pub planner: String,
    pub executor: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Ok,
    Notice,
    Block,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionCheck {
    pub action: Action,
    pub latest: Option<String>,
    pub model: Option<String>,
    pub models: Option<Vec<CatalogModel>>,
    pub notice: Option<String>,
    pub fusion: Option<FusionIds>,
}

#[derive(Debug, Deserialize)]
struct ClientVersionResponse {
    min: String,
    latest: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    models: Option<Vec<CatalogModel>>,
    #[serde(default)]
    notice: Option<String>,
    #[serde(default)]
    fusion: Option<FusionIds>,
}

/// Compares `X.Y.Z`-shaped versions on their first three numeric parts.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let parse = |value: &str| -> Option<[u64; 3]> {
        let parts: Vec<&str> = value.split('.').collect();
        if parts.len() < 3 {
            return None;
        }
        Some([
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ])
    };
    Some(parse(a)?.cmp(&parse(b)?))
}

/// Offline-safe by construction: every failure mode resolves to `Ok` and never
/// blocks startup.
pub fn check_client_version(base_url: &str, cli_version: &str) -> VersionCheck {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .http_status_as_error(false)
        .build();
    let agent: ureq::Agent = config.into();
    let url = format!("{}/client-version", base_url.trim_end_matches('/'));

    let mut response = match agent.get(&url).call() {
        Ok(response) => response,
        Err(_) => return VersionCheck::default(),
    };
    if response.status().as_u16() != 200 {
        return VersionCheck::default();
    }
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(_) => return VersionCheck::default(),
    };
    let parsed: ClientVersionResponse = match serde_json::from_str(&body) {
        Ok(parsed) => parsed,
        Err(_) => return VersionCheck::default(),
    };

    let action = match compare_versions(cli_version, &parsed.min) {
        Some(Ordering::Less) => Action::Block,
        _ => match compare_versions(cli_version, &parsed.latest) {
            Some(Ordering::Less) => Action::Notice,
            _ => Action::Ok,
        },
    };

    VersionCheck {
        action,
        latest: Some(parsed.latest),
        model: parsed.model,
        models: parsed.models,
        notice: parsed.notice,
        fusion: parsed.fusion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numeric_versions() {
        assert_eq!(compare_versions("0.7.0", "0.6.18"), Some(Ordering::Greater));
        assert_eq!(compare_versions("0.7.0", "0.7.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("0.7.0", "0.8.0"), Some(Ordering::Less));
        assert_eq!(compare_versions("1.0", "1.0.0"), None);
        assert_eq!(compare_versions("x.y.z", "1.0.0"), None);
    }
}
