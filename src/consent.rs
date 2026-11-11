use std::io::{self, IsTerminal, Write};

pub const CONSENT_VERSION: &str = "4";

pub const CONSENT_TEXT: &str = r#"free-pi — before you start

  * Inference is free. Ads pay for it, and so does training.
  * Ad cards render in the terminal UI. Some ads come from a third-party ad
    provider (Gravity), which receives excerpts of your recent conversation
    to pick a relevant ad.
  * Gravity also keeps a session and visitor ID on your machine (in
    ~/.free-pi) and receives usage events from free-pi to measure and
    attribute ads.
  * Your coding sessions (prompts, code, files, and model responses) are saved
    as traces and used to train models. We may share or sell that data to
    third parties. Once you accept, there is no opt-out, except where the law
    gives you one.
  * We do our best to remove personal information from traces before we store
    them. We cannot promise it is perfectly clean.
  * free-pi is in alpha and testing. There are no warranties or guarantees,
    expressed or implied, and no guarantee of uptime.
  * If GitHub email capture is enabled, we collect your GitHub email and may
    use it to contact you about the service or product.
  * free-pi-cli checks the server for the minimum supported version and asks
    you to update if this build is too old.

By accepting you agree to the Terms of Service (https://freepi.ai/terms)
and the Privacy Policy (https://freepi.ai/privacy).

Decline and nothing is sent to the free-pi server."#;

pub const REPROMPT_CONSENT_TEXT: &str = r#"free-pi — before you start

  * Inference is free. Ads pay for it, and so does training.
  * Ad cards render in the terminal UI. Some ads come from a third-party ad
    provider (Gravity), which receives excerpts of your recent conversation
    to pick a relevant ad.
  * Gravity also keeps a session and visitor ID on your machine (in
    ~/.free-pi) and receives usage events from free-pi to measure and
    attribute ads.
  * Your coding sessions (prompts, code, files, and model responses) are saved
    as traces and used to train models. We may share or sell that data to
    third parties. Once you accept, there is no opt-out, except where the law
    gives you one.
  * We do our best to remove personal information from traces before we store
    them. We cannot promise it is perfectly clean.
  * free-pi is in alpha and testing. There are no warranties or guarantees,
    expressed or implied, and no guarantee of uptime.
  * If GitHub email capture is enabled, we collect your GitHub email and may
    use it to contact you about the service or product.
  * free-pi-cli checks the server for the minimum supported version and asks
    you to update if this build is too old.

By accepting you agree to the Terms of Service (https://freepi.ai/terms)
and the Privacy Policy (https://freepi.ai/privacy).

Decline and free-pi exits; you will be asked again next time."#;

pub const NON_INTERACTIVE_CONSENT_TEXT: &str =
    "free-pi first-run consent needs an interactive terminal (stdin is not a TTY). \
     Run `bunx free-pi-cli` in a terminal once to review and accept; \
     after that, non-interactive runs work.";

pub const CONSENT_PROMPT: &str = "Accept? [y/N] ";

pub fn parse_consent_answer(answer: &str) -> bool {
    let trimmed = answer.trim();
    trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes")
}

/// The consent gate. On non-TTY stdin this declines immediately instead of
/// hanging; consent to ads + training must be a deliberate interactive act.
pub fn prompt_consent(text: &str) -> bool {
    if !io::stdin().is_terminal() {
        println!("{NON_INTERACTIVE_CONSENT_TEXT}");
        return false;
    }
    println!("{text}");
    print!("{CONSENT_PROMPT}");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    parse_consent_answer(&answer)
}

pub const INTRO_TEXT: &str = r#"Welcome to free-pi — a free coding agent in your terminal.

  • It's the pi coding agent, free to use. Ads in the terminal pay for the
    inference — ad content never reaches the model.
  • Tell it what to build, fix, or explain, like any coding assistant.
  • One session per account at a time. If you close the terminal, relaunch
    with "bunx free-pi-cli" — or resume where you left off with
    "bunx free-pi-cli --session <id>" (the id prints when you exit).

Set FREEPI_NO_INTRO=1 to skip this next time."#;

pub fn show_intro() {
    let skip = std::env::var("FREEPI_NO_INTRO")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !skip {
        println!("\n{INTRO_TEXT}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_answers() {
        assert!(parse_consent_answer("y"));
        assert!(parse_consent_answer(" YES \n"));
        assert!(!parse_consent_answer("n"));
        assert!(!parse_consent_answer(""));
        assert!(!parse_consent_answer("yep"));
    }
}
