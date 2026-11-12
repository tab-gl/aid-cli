//! Interactive prompts and terminal capability checks.

use std::io::{BufRead, IsTerminal, Write};

use crate::error::{AidError, Result};

/// True when both stdin and stdout are terminals, matching v1's interactive
/// guard (`process.stdin.isTTY && process.stdout.isTTY`).
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn read_line() -> Result<String> {
    let mut line = String::new();
    let read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|err| AidError::new(format!("Could not read terminal input: {err}")))?;
    if read == 0 {
        return Err(AidError::new(
            "No input received from the terminal. Run this command interactively.",
        ));
    }
    Ok(line.trim().to_string())
}

/// Prompt with a default value shown in brackets. An empty answer keeps it.
pub fn prompt(label: &str, fallback: &str) -> Result<String> {
    print!("  {label} [{fallback}]: ");
    std::io::stdout().flush().ok();
    let answer = read_line()?;
    if answer.is_empty() {
        Ok(fallback.to_string())
    } else {
        Ok(answer)
    }
}

/// Ask a yes/no question; anything other than `y`/`yes` is treated as no.
pub fn confirm(message: &str) -> Result<bool> {
    print!("{message}");
    std::io::stdout().flush().ok();
    let answer = read_line()?;
    Ok(matches!(answer.to_lowercase().as_str(), "y" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_tty_is_not_interactive() {
        // The test harness runs without a terminal attached.
        assert!(!is_interactive());
    }
}
