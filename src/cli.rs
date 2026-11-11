// Pure argv parsing for free-pi-cli's local commands. Anything unrecognized
// (including trailing args after a valid command) is an error, so a typo like
// `free-pi-cli loguot` can never silently start a billed agent session.
use crate::version::CLI_VERSION;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run,
    Help,
    Version,
    Logout,
    Mcp(Vec<String>),
    Unknown(String),
}

pub const HELP_TEXT: &str = r#"free-pi — free AI coding in your terminal; ads pay for the inference.

Usage:
  bunx free-pi-cli                 start the coding agent (first run: consent + GitHub sign-in)
  bunx free-pi-cli --session <id>  resume a previous session
  bunx free-pi-cli logout          remove the stored sign-in token; the next run signs in again
  bunx free-pi-cli mcp <command>   add, remove, list, or sign in to your own MCP servers
                                   (free-pi-cli mcp --help for details)
  bunx free-pi-cli --version       print the CLI version
  bunx free-pi-cli --help          show this help

Environment:
  FREEPI_BASE_URL            override the backend base URL (default https://api.freepi.ai)
  FREEPI_NO_GRAVITY_PIXEL=1  do not run Gravity's ad-measurement pixel
  FREEPI_KEEP_ENV_PROVIDERS  show every provider your environment can authenticate, not just free-pi
  FREEPI_CLI_BIN             override the native binary the npm shim runs
  FREEPI_PI_CLI              override pi's dist/bundle/cli.js path
"#;

pub fn version_line() -> String {
    format!("free-pi-cli {CLI_VERSION}")
}

/// `argv` is already `std::env::args().skip(1)`.
pub fn parse(argv: &[String]) -> Command {
    let Some(first) = argv.first() else {
        return Command::Run;
    };
    if first == "mcp" {
        return Command::Mcp(argv[1..].to_vec());
    }
    if first == "--session" {
        return if argv.len() > 2 {
            Command::Unknown(argv[2].clone())
        } else {
            Command::Run
        };
    }

    let kind = match first.as_str() {
        "--help" | "-h" | "help" => Command::Help,
        "--version" | "-v" => Command::Version,
        "logout" => Command::Logout,
        other => return Command::Unknown(other.to_string()),
    };
    if argv.len() > 1 {
        return Command::Unknown(argv[1].clone());
    }
    kind
}

/// Parses and validates the single `--session <id>` resume flag.
pub fn parse_session_arg(argv: &[String]) -> Result<Option<String>, String> {
    let Some(index) = argv.iter().position(|arg| arg == "--session") else {
        return Ok(None);
    };
    let Some(value) = argv.get(index + 1) else {
        return Err("Missing value for --session".to_string());
    };
    if value.starts_with("--") {
        return Err("Missing value for --session".to_string());
    }
    validate_session_id(value)?;
    Ok(Some(value.clone()))
}

/// Mirrors pi's SessionManager.assertValidSessionId.
pub fn validate_session_id(id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && id.chars().last().is_some_and(|c| c.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Invalid --session value: Session id must be non-empty, contain only alphanumeric characters, '-', '_', and '.', and start and end with an alphanumeric character (got {id:?})"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_local_commands() {
        assert_eq!(parse(&args(&[])), Command::Run);
        assert_eq!(parse(&args(&["help"])), Command::Help);
        assert_eq!(parse(&args(&["-h"])), Command::Help);
        assert_eq!(parse(&args(&["--version"])), Command::Version);
        assert_eq!(parse(&args(&["logout"])), Command::Logout);
        assert_eq!(
            parse(&args(&["mcp", "add", "x"])),
            Command::Mcp(args(&["add", "x"]))
        );
    }

    #[test]
    fn rejects_unknown_and_trailing_args() {
        assert_eq!(parse(&args(&["loguot"])), Command::Unknown("loguot".into()));
        assert_eq!(
            parse(&args(&["--help", "extra"])),
            Command::Unknown("extra".into())
        );
        assert_eq!(
            parse(&args(&["--session", "abc", "extra"])),
            Command::Unknown("extra".into())
        );
    }

    #[test]
    fn session_values() {
        assert_eq!(
            parse_session_arg(&args(&["--session", "abc-123"])),
            Ok(Some("abc-123".into()))
        );
        assert_eq!(
            parse_session_arg(&args(&["--session"])),
            Err("Missing value for --session".into())
        );
        assert!(parse_session_arg(&args(&["--session", "-abc"])).is_err());
        assert!(parse_session_arg(&args(&["--session", "abc-"])).is_err());
        assert!(parse_session_arg(&args(&["--session", "a b"])).is_err());
    }
}
