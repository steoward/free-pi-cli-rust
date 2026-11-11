use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use crate::version::CLI_VERSION;
use crate::version_check::{CatalogModel, FusionIds};

pub const PROVIDER_NAME: &str = "free-pi";
pub const DEFAULT_MODEL_ID: &str = "deepseek-v4-flash";
pub const FUSION_PICKER_ID: &str = "🧨 fusion";
pub const TODO_TOOL_NAME: &str = "todo";

pub const SAFE_TOOLS: &[&str] = &["read", "bash", "edit", "write", "grep", "find", "ls"];
pub const CUSTOM_TOOLS: &[&str] = &[
    "free_pi_usage",
    "free_pi_buy_credits",
    "free_pi_docs",
    "web_search",
    "docs_lookup",
    "generate_image",
    TODO_TOOL_NAME,
];
pub const EXTRA_TOOLS: &[&str] = &["codemode", "tool_search"];
const MCP_TOOLS: &[&str] = &[
    "mcp__*",
    "list_mcp_resources",
    "list_mcp_resource_templates",
    "read_mcp_resource",
];

const APPEND_SYSTEM: &str = include_str!("../assets/APPEND_SYSTEM.md");
const EXTENSION_BUNDLE: &str = include_str!("../assets/free-pi-extension.js");
const LONG_JOBS_SKILL: &str = include_str!("../assets/long-jobs-SKILL.md");

pub fn allowed_tools() -> Vec<&'static str> {
    SAFE_TOOLS
        .iter()
        .chain(CUSTOM_TOOLS)
        .chain(EXTRA_TOOLS)
        .copied()
        .collect()
}

/// The strict `--tools` allowlist: the closed set free-pi ever legitimately
/// runs, plus MCP tool patterns for servers the user configured themselves.
pub fn cli_tools_allowlist() -> String {
    allowed_tools()
        .into_iter()
        .chain(MCP_TOOLS.iter().copied())
        .collect::<Vec<_>>()
        .join(",")
}

/// pi appends `/chat/completions` to the provider base URL, so it must end in
/// `/v1` to reach the free-pi proxy's `/v1/chat/completions` route.
pub fn completions_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

pub fn effective_models(
    models: Option<&Vec<CatalogModel>>,
    fallback_model: Option<&str>,
) -> Vec<CatalogModel> {
    if let Some(models) = models.filter(|models| !models.is_empty()) {
        return models.clone();
    }
    let id = fallback_model.unwrap_or(DEFAULT_MODEL_ID);
    vec![CatalogModel {
        id: id.to_string(),
        name: id.to_string(),
    }]
}

fn models_json(base_url: &str, models: &[CatalogModel]) -> Value {
    let provider_models: Vec<Value> = models
        .iter()
        .map(|model| {
            json!({
                "id": model.id,
                "name": model.name,
                "reasoning": false,
                "input": ["text"],
                "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
                "contextWindow": 1_048_576,
                "maxTokens": 8192
            })
        })
        .collect();

    json!({
        "$schema": "https://pi.dev/schemas/models.schema.json",
        "providers": {
            PROVIDER_NAME: {
                "name": PROVIDER_NAME,
                "baseUrl": completions_base_url(base_url),
                "api": "openai-completions",
                "apiKey": "$FREEPI_JWT",
                "headers": { "x-client-version": CLI_VERSION },
                "models": provider_models
            }
        }
    })
}

fn settings_json(models: &[CatalogModel], keep_env_providers: bool) -> Value {
    let mut settings = Map::new();
    settings.insert("defaultProvider".into(), json!(PROVIDER_NAME));
    settings.insert("defaultModel".into(), json!(models[0].id));
    settings.insert("defaultTools".into(), json!(allowed_tools()));
    settings.insert("packages".into(), json!([]));
    settings.insert("quietStartup".into(), json!(true));
    settings.insert("defaultThinkingLevel".into(), json!("off"));
    settings.insert("hideThinkingBlock".into(), json!(true));
    settings.insert("tuiMode".into(), json!("fullscreen"));
    settings.insert("theme".into(), json!("light/dark"));
    settings.insert("enableInstallTelemetry".into(), json!(false));
    settings.insert("defaultProjectTrust".into(), json!("ask"));
    if !keep_env_providers {
        let mut scoped = vec![format!("{PROVIDER_NAME}/{FUSION_PICKER_ID}")];
        scoped.extend(
            models
                .iter()
                .map(|model| format!("{PROVIDER_NAME}/{}", model.id)),
        );
        settings.insert("enabledModels".into(), json!(scoped));
    }
    Value::Object(settings)
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("could not write {}", path.display()))
}

/// Materializes the isolated pi agent directory: provider catalog, scope,
/// defaults, the bundled extension, the agent rules, and the long-jobs skill.
pub fn write_runtime(
    agent_dir: &Path,
    base_url: &str,
    models: &[CatalogModel],
    _fusion: Option<&FusionIds>,
    keep_env_providers: bool,
) -> Result<()> {
    fs::create_dir_all(agent_dir)
        .with_context(|| format!("could not create {}", agent_dir.display()))?;

    write_file(
        &agent_dir.join("models.json"),
        &serde_json::to_string_pretty(&models_json(base_url, models))?,
    )?;
    write_file(
        &agent_dir.join("settings.json"),
        &serde_json::to_string_pretty(&settings_json(models, keep_env_providers))?,
    )?;
    write_file(&agent_dir.join("APPEND_SYSTEM.md"), APPEND_SYSTEM)?;
    write_file(
        &agent_dir.join("skills").join("long-jobs").join("SKILL.md"),
        LONG_JOBS_SKILL,
    )?;

    let extensions_dir = agent_dir.join("extensions");
    // Remove an old TS build so pi does not load the same extension twice.
    let _ = fs::remove_file(extensions_dir.join("free-pi.ts"));
    write_file(&extensions_dir.join("free-pi.js"), EXTENSION_BUNDLE)?;
    Ok(())
}

/// First run seeds the marker silently (a new user gets the onboarding intro,
/// not a "what's new"). Later upgrades would print bundled highlights; this
/// rewrite ships no changelog data, so it only advances the marker.
pub fn record_last_seen_version(agent_dir: &Path) {
    let path = crate::paths::last_seen_version_path(agent_dir);
    let _ = write_file(&path, CLI_VERSION);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Vec<CatalogModel> {
        vec![
            CatalogModel {
                id: "deepseek-v4-flash".into(),
                name: "DeepSeek V4 Flash".into(),
            },
            CatalogModel {
                id: "deepseek-v4-pro".into(),
                name: "DeepSeek V4 Pro".into(),
            },
        ]
    }

    #[test]
    fn appends_v1_once() {
        assert_eq!(
            completions_base_url("https://api.freepi.ai"),
            "https://api.freepi.ai/v1"
        );
        assert_eq!(
            completions_base_url("https://api.freepi.ai/"),
            "https://api.freepi.ai/v1"
        );
        assert_eq!(
            completions_base_url("https://api.freepi.ai/v1/"),
            "https://api.freepi.ai/v1"
        );
    }

    #[test]
    fn falls_back_to_the_single_model() {
        assert_eq!(
            effective_models(None, Some("particle"))[0].id,
            "particle".to_string()
        );
        assert_eq!(
            effective_models(Some(&vec![]), None)[0].id,
            DEFAULT_MODEL_ID
        );
    }

    #[test]
    fn models_json_contains_catalog_and_env_key() {
        let json = models_json("https://api.freepi.ai", &catalog());
        let provider = &json["providers"][PROVIDER_NAME];
        assert_eq!(provider["baseUrl"], "https://api.freepi.ai/v1");
        assert_eq!(provider["apiKey"], "$FREEPI_JWT");
        assert_eq!(provider["models"][0]["id"], "deepseek-v4-flash");
        assert_eq!(provider["headers"]["x-client-version"], CLI_VERSION);
    }

    #[test]
    fn settings_scope_fusion_first_and_can_be_opted_out() {
        let json = settings_json(&catalog(), false);
        assert_eq!(json["defaultModel"], "deepseek-v4-flash");
        assert_eq!(json["enabledModels"][0], "free-pi/🧨 fusion");
        assert_eq!(json["enabledModels"][1], "free-pi/deepseek-v4-flash");
        assert_eq!(json["packages"], json!([]));
        assert_eq!(json["defaultProjectTrust"], "ask");
        // Matches the upstream TypeScript release's defaults.
        assert_eq!(json["tuiMode"], "fullscreen");
        assert_eq!(json["theme"], "light/dark");
        assert_eq!(json["defaultThinkingLevel"], "off");
        assert_eq!(json["hideThinkingBlock"], json!(true));
        assert_eq!(json["quietStartup"], json!(true));
        assert_eq!(json["enableInstallTelemetry"], json!(false));

        let kept = settings_json(&catalog(), true);
        assert!(kept.get("enabledModels").is_none());
    }
}
