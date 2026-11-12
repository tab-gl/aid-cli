//! Ace ID CLI v2 — a Rust implementation of the Ace Identity Console.
//!
//! The binary keeps the v1 command surface (`aid`, `ace-id`, `aidc`), the v1
//! configuration formats and the v1 user-facing messages, while removing the
//! Node.js runtime requirement.

use ace_id_cli::cli;

fn main() {
    // Match v1 (and every Unix CLI): a closed pipe (`aid --help | head`) ends the
    // process quietly instead of panicking on a broken stdout.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = cli::run(&argv);
    if code != 0 {
        std::process::exit(code);
    }
}
