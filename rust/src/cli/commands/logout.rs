//! `aid logout` — delete the stored session.

use crate::auth::token;
use crate::error::Result;
use crate::terminal::success;

pub fn run(_args: &[String]) -> Result<()> {
    token::clear()?;
    success("Signed out. Local tokens deleted.");
    Ok(())
}
