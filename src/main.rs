mod auth;
mod browser;
mod cli;
mod consent;
mod credentials;
mod paths;
mod pi_launch;
mod runtime;
mod version;
mod version_check;

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use auth::{AuthClient, TokenCheck};
use cli::Command;
use consent::{CONSENT_TEXT, CONSENT_VERSION, REPROMPT_CONSENT_TEXT};
use pi_launch::LaunchOptions;
use version::CLI_VERSION;
use version_check::Action;

fn main() {
    let code = match real_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    };
    std::process::exit(code);
}

fn real_main() -> Result<i32> {
    if let Some(ready) = std::env::var_os("FREEPI_RELAUNCH_READY") {
        let _ = fs::write(ready, "");
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&argv) {
        Command::Version => {
            println!("{}", cli::version_line());
            return Ok(0);
        }
        Command::Help => {
            println!("{}", cli::HELP_TEXT);
            return Ok(0);
        }
        Command::Unknown(arg) => {
            eprintln!(
                "free-pi-cli: unknown argument \"{arg}\"\n\n{}",
                cli::HELP_TEXT
            );
            return Ok(2);
        }
        Command::Logout => return run_logout(),
        Command::Mcp(args) => {
            let agent_dir = paths::agent_dir();
            return pi_launch::run_mcp(&args, &agent_dir);
        }
        Command::Run => {}
    }

    run_agent(&argv)
}

fn run_logout() -> Result<i32> {
    let credentials_path = paths::credentials_path(&paths::agent_dir());
    let had_token = credentials::load_jwt(&credentials_path).is_some();
    credentials::clear_jwt(&credentials_path)?;
    if had_token {
        println!(
            "Logged out: removed {}. The next run signs in again.",
            credentials_path.display()
        );
    } else {
        println!("Already logged out — no stored sign-in token.");
    }
    Ok(0)
}

fn base_url() -> String {
    std::env::var("FREEPI_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| version::DEFAULT_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn run_agent(argv: &[String]) -> Result<i32> {
    let session = match cli::parse_session_arg(argv) {
        Ok(session) => session,
        Err(message) => {
            println!("{message}");
            return Ok(1);
        }
    };

    let agent_dir = paths::agent_dir();
    let credentials_path = paths::credentials_path(&agent_dir);
    let base = base_url();
    let auth = AuthClient::new(&base);

    let mut jwt = credentials::load_jwt(&credentials_path);
    let mut consent_current = false;

    if let Some(token) = jwt.clone() {
        match auth.check_token(&token) {
            Ok(TokenCheck::Unauthorized) => {
                credentials::clear_jwt(&credentials_path)?;
                jwt = None;
            }
            Ok(check) => {
                consent_current = auth::consent_is_current(&check);
                if auth::needs_reprompt(&check) {
                    if !consent::prompt_consent(REPROMPT_CONSENT_TEXT) {
                        println!(
                            "Declined. free-pi needs you to accept the updated terms to continue; you will be asked again next time."
                        );
                        return Ok(1);
                    }
                    consent_current = true;
                    if let Err(err) = auth.submit_user_consent(&token, CONSENT_VERSION) {
                        // A dead JWT sends the user through a fresh login;
                        // anything else just means consent was not recorded
                        // and we ask again next startup.
                        if err.to_string() == "unauthorized" {
                            credentials::clear_jwt(&credentials_path)?;
                            jwt = None;
                        }
                    }
                }
            }
            // Network blips and 5xx responses are not a cue to re-login; let
            // pi surface the real problem on first use.
            Err(_) => {}
        }
    }

    if jwt.is_none() {
        if !consent::prompt_consent(CONSENT_TEXT) {
            println!("Declined. Exiting without contacting the free-pi server.");
            return Ok(1);
        }
        consent::show_intro();
        jwt = Some(login_flow(&auth, &credentials_path)?);
        consent_current = true;
    }
    let jwt = jwt.expect("jwt is set by the first-run login or an existing credential");

    // Privacy invariant: no server call happens before the consent gate above.
    let version = version_check::check_client_version(&base, CLI_VERSION);
    if version.action == Action::Block {
        println!("free-pi-cli is below the required minimum version; update with: bunx free-pi-cli@latest");
        return Ok(1);
    }
    let update_latest = if version.action == Action::Notice {
        version.latest.clone()
    } else {
        None
    };

    runtime::record_last_seen_version(&agent_dir);

    if let Some(notice) = &version.notice {
        println!("{notice}");
    }

    let models = runtime::effective_models(version.models.as_ref(), version.model.as_deref());
    runtime::write_runtime(
        &agent_dir,
        &base,
        &models,
        version.fusion.as_ref(),
        std::env::var_os("FREEPI_KEEP_ENV_PROVIDERS").is_some(),
    )
    .context("could not prepare the free-pi agent directory")?;

    let result = pi_launch::launch_pi(LaunchOptions {
        base_url: &base,
        jwt: &jwt,
        agent_dir: &agent_dir,
        session: session.as_deref(),
        models: &models,
        fusion: version.fusion.as_ref(),
        update_latest: update_latest.as_deref(),
        consent_current,
    })?;

    println!(
        "\nfree-pi session ended. Start it again anytime:  bunx free-pi-cli --session {}",
        result.session_id
    );
    println!("(or install once for a shorter command:  bun add -g free-pi-cli  →  free-pi-cli)\n");

    Ok(0)
}

fn login_flow(auth: &AuthClient, credentials_path: &Path) -> Result<String> {
    let device = auth.start_device_flow()?;
    println!(
        "\nOpen {} and enter code: {}\n",
        device.verification_uri, device.user_code
    );
    browser::open_browser(&device.verification_uri);
    auth.submit_consent(&device.session_id, CONSENT_VERSION)?;
    let token = auth.poll_for_token(&device.session_id, device.interval)?;
    credentials::save_jwt(credentials_path, &token)?;
    Ok(token)
}
