use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::consent::CONSENT_VERSION;

#[derive(Debug, Clone, Deserialize)]
pub struct GithubDeviceResponse {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenCheck {
    Unauthorized,
    /// Old server, a 5xx, or a body without `consent_version`: keep going and
    /// ask again next startup.
    NoConsentField,
    /// `Some(version)` when the server recorded one; `None` for an explicit null.
    Consent(Option<String>),
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: String,
}

pub struct AuthClient {
    base_url: String,
    agent: ureq::Agent,
}

impl AuthClient {
    pub fn new(base_url: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent: config.into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<HttpResponse> {
        let mut res = self
            .agent
            .post(&self.url(path))
            .header("content-type", "application/json")
            .send(body.to_string())
            .with_context(|| format!("network request to {path} failed"))?;
        let status = res.status().as_u16();
        let text = res
            .body_mut()
            .read_to_string()
            .with_context(|| format!("could not read response body from {path}"))?;
        Ok(HttpResponse { status, body: text })
    }

    /// Bounded retry on network failures only; HTTP responses are returned
    /// as-is. One dropped packet must not kill first-run before the device
    /// code even prints.
    fn post_json_retry(
        &self,
        path: &str,
        body: &serde_json::Value,
        attempts: u32,
    ) -> Result<HttpResponse> {
        let mut last_error = None;
        for attempt in 1..=attempts {
            match self.post_json(path, body) {
                Ok(response) => return Ok(response),
                Err(err) => {
                    last_error = Some(err);
                    if attempt < attempts {
                        std::thread::sleep(Duration::from_millis(1_500 * u64::from(attempt)));
                    }
                }
            }
        }
        let detail = last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown error".to_string());
        bail!("network unreachable after {attempts} attempts ({detail})")
    }

    pub fn start_device_flow(&self) -> Result<GithubDeviceResponse> {
        let response = self.post_json_retry("/auth/github/device", &serde_json::json!({}), 4)?;
        if response.status != 200 {
            bail!("device flow start failed: HTTP {}", response.status);
        }
        serde_json::from_str(&response.body)
            .context("device flow start returned an unexpected body")
    }

    pub fn submit_consent(&self, session_id: &str, consent_version: &str) -> Result<()> {
        let response = self.post_json_retry(
            "/auth/consent",
            &serde_json::json!({
                "session_id": session_id,
                "consent_version": consent_version,
            }),
            4,
        )?;
        if response.status != 200 {
            bail!("consent submission failed: HTTP {}", response.status);
        }
        Ok(())
    }

    /// Poll /auth/token until a JWT is issued, tolerating a bounded run of
    /// network failures while the user is off in a browser.
    pub fn poll_for_token(&self, session_id: &str, interval: u64) -> Result<String> {
        let mut consecutive_network_failures = 0u32;
        loop {
            let response = match self.post_json(
                "/auth/token",
                &serde_json::json!({ "session_id": session_id }),
            ) {
                Ok(response) => {
                    consecutive_network_failures = 0;
                    response
                }
                Err(err) => {
                    consecutive_network_failures += 1;
                    if consecutive_network_failures >= 6 {
                        bail!(
                            "token poll failed: network unreachable after {consecutive_network_failures} attempts ({err})"
                        );
                    }
                    std::thread::sleep(Duration::from_secs(interval.max(5)));
                    continue;
                }
            };

            // consent_required arrives as a 403, so report the actionable
            // message instead of a bare HTTP code.
            if response.status == 403 {
                if let Ok(body) = serde_json::from_str::<serde_json::Value>(&response.body) {
                    if body.get("code").and_then(|v| v.as_str()) == Some("consent_required") {
                        bail!("consent_required: server did not record consent before polling");
                    }
                }
            }
            if !(200..300).contains(&response.status) {
                bail!("token poll failed: HTTP {}", response.status);
            }

            let parsed: serde_json::Value = serde_json::from_str(&response.body)
                .context("token poll returned an unexpected body")?;
            if let Some(token) = parsed.get("token").and_then(|v| v.as_str()) {
                if !token.is_empty() {
                    return Ok(token.to_string());
                }
            }
            if parsed.get("code").and_then(|v| v.as_str()) == Some("consent_required") {
                bail!("consent_required: server did not record consent before polling");
            }
            let retry_after = parsed
                .get("retry_after")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            std::thread::sleep(Duration::from_secs(interval.max(retry_after)));
        }
    }

    /// GET /me with the stored JWT.
    pub fn check_token(&self, jwt: &str) -> Result<TokenCheck> {
        let mut res = self
            .agent
            .get(&self.url("/me"))
            .header("authorization", format!("Bearer {jwt}"))
            .call()
            .context("could not reach /me")?;
        let status = res.status().as_u16();
        if status == 401 {
            return Ok(TokenCheck::Unauthorized);
        }
        if status != 200 {
            return Ok(TokenCheck::NoConsentField);
        }
        let body: serde_json::Value = match res.body_mut().read_to_string() {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => return Ok(TokenCheck::NoConsentField),
            },
            Err(_) => return Ok(TokenCheck::NoConsentField),
        };
        match body.get("consent_version") {
            None => Ok(TokenCheck::NoConsentField),
            Some(serde_json::Value::Null) => Ok(TokenCheck::Consent(None)),
            Some(serde_json::Value::String(value)) => Ok(TokenCheck::Consent(Some(value.clone()))),
            Some(_) => Ok(TokenCheck::NoConsentField),
        }
    }

    /// POST /me/consent: record that an existing user accepted `consent_version`.
    pub fn submit_user_consent(&self, jwt: &str, consent_version: &str) -> Result<()> {
        let res = self
            .agent
            .post(&self.url("/me/consent"))
            .header("authorization", format!("Bearer {jwt}"))
            .header("content-type", "application/json")
            .send(serde_json::json!({ "consent_version": consent_version }).to_string())
            .context("could not reach /me/consent")?;
        let status = res.status().as_u16();
        if status == 401 {
            bail!("unauthorized");
        }
        if !(200..300).contains(&status) {
            bail!("consent submission failed: HTTP {status}");
        }
        Ok(())
    }
}

pub fn needs_reprompt(check: &TokenCheck) -> bool {
    match check {
        TokenCheck::Consent(Some(version)) => version != CONSENT_VERSION,
        TokenCheck::Consent(None) => true,
        _ => false,
    }
}

pub fn consent_is_current(check: &TokenCheck) -> bool {
    matches!(check, TokenCheck::Consent(Some(version)) if version == CONSENT_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reprompt_rules_match_upstream() {
        assert!(needs_reprompt(&TokenCheck::Consent(None)));
        assert!(needs_reprompt(&TokenCheck::Consent(Some("3".into()))));
        assert!(!needs_reprompt(&TokenCheck::Consent(Some("4".into()))));
        assert!(!needs_reprompt(&TokenCheck::NoConsentField));
        assert!(!needs_reprompt(&TokenCheck::Unauthorized));
        assert!(consent_is_current(&TokenCheck::Consent(Some("4".into()))));
        assert!(!consent_is_current(&TokenCheck::Consent(None)));
    }
}
