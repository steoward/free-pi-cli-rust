use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::paths;
use crate::runtime::cli_tools_allowlist;
use crate::version::{CLI_VERSION, PINNED_PI_VERSION};
use crate::version_check::{CatalogModel, FusionIds};

pub struct LaunchOptions<'a> {
    pub base_url: &'a str,
    pub jwt: &'a str,
    pub agent_dir: &'a Path,
    pub session: Option<&'a str>,
    pub models: &'a [CatalogModel],
    pub fusion: Option<&'a FusionIds>,
    pub update_latest: Option<&'a str>,
    /// True once the user accepted the consent version that discloses the
    /// Gravity pixel. The extension only starts the pixel when this is set.
    pub consent_current: bool,
}

pub struct LaunchResult {
    pub session_id: String,
}

/// The JS runtime that runs pi. Prefer Bun (the supported `bunx` path), then
/// Node >= 22 for users who only have npm installed.
fn resolve_js_runtime() -> Result<std::ffi::OsString> {
    if let Some(runtime) = std::env::var_os("FREEPI_JS_RUNTIME").filter(|v| !v.is_empty()) {
        return Ok(runtime);
    }
    for candidate in ["bun", "node"] {
        if let Some(path) = find_on_path(candidate) {
            return Ok(path.into_os_string());
        }
    }
    bail!("free-pi-cli needs Bun or Node.js 22.19+ on PATH to run the pi agent")
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_string())
            .split(';')
            .map(|ext| ext.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path) {
        for ext in &extensions {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
    }
    None
}

fn pi_cli_candidates() -> Vec<PathBuf> {
    let relative = Path::new("@earendil-works")
        .join("pi-coding-agent")
        .join("dist")
        .join("bundle")
        .join("cli.js");
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("node_modules").join(&relative));
            candidates.push(
                dir.join("..")
                    .join("lib")
                    .join("node_modules")
                    .join(&relative),
            );
            candidates.push(dir.join("..").join("node_modules").join(&relative));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("node_modules").join(&relative));
    }
    candidates
}

fn resolve_pi_cli() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("FREEPI_PI_CLI") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(package_dir) = std::env::var_os("FREEPI_PI_PACKAGE") {
        let path = PathBuf::from(package_dir)
            .join("dist")
            .join("bundle")
            .join("cli.js");
        if path.is_file() {
            return Some(path);
        }
    }
    pi_cli_candidates().into_iter().find(|path| path.is_file())
}

fn pi_command(runtime: &std::ffi::OsStr) -> Result<(Command, bool)> {
    if let Some(cli) = resolve_pi_cli() {
        let mut command = Command::new(runtime);
        command.arg(cli);
        return Ok((command, false));
    }

    // Direct binary runs (cargo run, a platform package without the npm shim)
    // can still work through bunx. Never fetch pi with npm: bunx is the
    // supported launcher for this build.
    let runtime_name = Path::new(runtime)
        .file_stem()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if runtime_name == "bun" || runtime_name == "bun.exe" {
        let mut command = Command::new(runtime);
        command.arg("x").arg("--bun").arg(format!(
            "@earendil-works/pi-coding-agent@{PINNED_PI_VERSION}"
        ));
        return Ok((command, true));
    }
    bail!(
        "the bundled pi agent was not found. Reinstall free-pi-cli, or set FREEPI_PI_CLI to the path of pi's dist/bundle/cli.js"
    )
}

fn read_session_id(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    let header: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    header
        .get("id")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn session_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect()
}

fn find_session(dir: &Path, id: &str) -> Option<String> {
    session_files(dir)
        .into_iter()
        .find_map(|path| read_session_id(&path).filter(|found| found == id))
}

fn newest_session(dir: &Path) -> Option<String> {
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for path in session_files(dir) {
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Some(id) = read_session_id(&path) else {
            continue;
        };
        if best.as_ref().map_or(true, |(time, _)| modified >= *time) {
            best = Some((modified, id));
        }
    }
    best.map(|(_, id)| id)
}

pub fn launch_pi(options: LaunchOptions<'_>) -> Result<LaunchResult> {
    let cwd = std::env::current_dir().context("could not determine the working directory")?;
    let session_dir = paths::session_dir(options.agent_dir, &cwd);
    fs::create_dir_all(&session_dir)
        .with_context(|| format!("could not create {}", session_dir.display()))?;

    let (requested_session, generated) = match options.session {
        Some(id) => match find_session(&session_dir, id) {
            Some(full) => (Some(full), None),
            None => bail!("No session found matching '{id}' in this directory"),
        },
        None => {
            let id = uuid::Uuid::now_v7().to_string();
            (None, Some(id))
        }
    };
    let session_id = requested_session
        .clone()
        .or_else(|| generated.clone())
        .expect("a session id is always chosen");

    let runtime = resolve_js_runtime()?;
    let (mut command, is_bunx_fallback) = pi_command(&runtime)?;

    command
        .arg("--session-dir")
        .arg(&session_dir)
        .arg("--tools")
        .arg(cli_tools_allowlist());
    if let Some(id) = &requested_session {
        command.arg("--session").arg(id);
    } else if let Some(id) = &generated {
        command.arg("--session-id").arg(id);
    }
    let _ = is_bunx_fallback;

    let models_json = serde_json::to_string(options.models)?;
    let fusion_json = options
        .fusion
        .map(serde_json::to_string)
        .transpose()?
        .unwrap_or_default();

    command
        .env("PI_CODING_AGENT_DIR", options.agent_dir)
        .env("PI_SKIP_VERSION_CHECK", "1")
        .env("PI_TELEMETRY", "0")
        .env("FREEPI_JWT", options.jwt)
        .env("FREEPI_BASE_URL", options.base_url)
        .env("FREEPI_MODELS_JSON", models_json)
        .env("FREEPI_FUSION_JSON", fusion_json)
        .env("FREEPI_SESSION_ID", &session_id)
        .env("FREEPI_CLI_VERSION", CLI_VERSION)
        .env("FREEPI_GRAVITY_DIR", paths::gravity_dir())
        .env(
            "FREEPI_GRAVITY_ENABLED",
            if options.consent_current { "1" } else { "0" },
        )
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(latest) = options.update_latest {
        command.env("FREEPI_UPDATE_LATEST", latest);
    }

    // pi's TUI reports its own failures; free-pi still exits cleanly and
    // prints the resume line, matching the SDK build's behavior.
    let _status = command.status().context("could not start the pi agent")?;

    let final_session = newest_session(&session_dir).unwrap_or(session_id);
    Ok(LaunchResult {
        session_id: final_session,
    })
}

pub fn brand_mcp_line(line: &str) -> String {
    line.replace("pi mcp", "free-pi-cli mcp")
        .replace("~/.pi/agent", "~/.free-pi/agent")
        .replace("Start pi", "Start free-pi-cli")
}

/// `free-pi-cli mcp ...` runs pi's own MCP command against free-pi's agent dir.
pub fn run_mcp(args: &[String], agent_dir: &Path) -> Result<i32> {
    let runtime = resolve_js_runtime()?;
    let (mut command, _) = pi_command(&runtime)?;
    command
        .arg("mcp")
        .args(args)
        .env("PI_CODING_AGENT_DIR", agent_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().context("could not start pi mcp")?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    std::thread::scope(|scope| {
        if let Some(stdout) = stdout {
            scope.spawn(move || stream_branded(stdout, false));
        }
        if let Some(stderr) = stderr {
            scope.spawn(move || stream_branded(stderr, true));
        }
    });

    let status = child.wait().context("could not wait for pi mcp")?;
    Ok(status.code().unwrap_or(1))
}

fn stream_branded<R: std::io::Read>(reader: R, is_stderr: bool) {
    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else {
            break;
        };
        let branded = brand_mcp_line(&line);
        if is_stderr {
            let _ = writeln!(std::io::stderr(), "{branded}");
        } else {
            let _ = writeln!(std::io::stdout(), "{branded}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebrands_mcp_output() {
        assert_eq!(
            brand_mcp_line("pi mcp add github"),
            "free-pi-cli mcp add github"
        );
        assert_eq!(
            brand_mcp_line("Saved to ~/.pi/agent/mcp.json"),
            "Saved to ~/.free-pi/agent/mcp.json"
        );
        assert_eq!(
            brand_mcp_line("Start pi in the project"),
            "Start free-pi-cli in the project"
        );
    }

    #[test]
    fn reads_session_header_id() {
        let dir = std::env::temp_dir().join(format!("freepi-session-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("2026_session.jsonl");
        fs::write(
            &path,
            r#"{"type":"session","version":3,"id":"019abc-def","cwd":"/tmp"}"#,
        )
        .unwrap();
        assert_eq!(
            find_session(&dir, "019abc-def").as_deref(),
            Some("019abc-def")
        );
        assert_eq!(find_session(&dir, "other"), None);
        assert_eq!(newest_session(&dir).as_deref(), Some("019abc-def"));
        let _ = fs::remove_dir_all(dir);
    }
}
