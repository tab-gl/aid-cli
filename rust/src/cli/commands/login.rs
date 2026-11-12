//! `aid login` — Authorization Code + PKCE against the Ace ID issuer.

use crate::auth::oauth::{exchange_code, merge_tokens, open_browser, CallbackServer};
use crate::auth::pkce::{challenge_for, generate_state, generate_verifier};
use crate::auth::token;
use crate::config::Config;
use crate::error::Result;
use crate::terminal::info;
use crate::terminal::spinner::Spinner;

pub fn run(_args: &[String]) -> Result<()> {
    let config = Config::from_env()?;

    let verifier = generate_verifier()?;
    let challenge = challenge_for(&verifier);
    let state = generate_state()?;

    let authorize_url = crate::auth::oauth::authorize_url(&config, &state, &challenge)?;

    // Bind the loopback listener before printing the URL so the browser cannot
    // arrive before the CLI is listening.
    let callback = CallbackServer::bind(&config)?;

    info("Opening browser to sign in…");
    info("If it doesn't open automatically, visit:");
    println!("  {authorize_url}\n");

    open_browser(&authorize_url);

    let spinner = Spinner::start("Waiting for you to authorize in the browser…");
    let code = match callback.wait_for_code(&config, &state) {
        Ok(code) => code,
        Err(err) => {
            spinner.fail();
            return Err(err);
        }
    };

    spinner.update("Exchanging authorization code for tokens…");
    let response = match exchange_code(&config, &code, &verifier) {
        Ok(response) => response,
        Err(err) => {
            spinner.fail();
            return Err(err);
        }
    };

    let tokens = merge_tokens(None, &response, &config.issuer);
    if let Err(err) = token::save(tokens) {
        spinner.fail();
        return Err(err);
    }

    spinner.succeed("Signed in. Tokens stored.");
    Ok(())
}
