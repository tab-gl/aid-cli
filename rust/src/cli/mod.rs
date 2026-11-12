//! Command-line surface: chaining, aliases, help and dispatch.

pub mod args;
pub mod commands;

use crate::error::{AidError, Result};
use crate::terminal::{self, command_list};

/// Binary aliases accepted as the first token of a command chain.
pub const ALIASES: [&str; 3] = ["aid", "ace-id", "aidc"];

/// CLI version reported by `--version`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Run every command in `argv`, stopping at the first failure.
///
/// Returns the process exit code: 0 on success, 1 on the first error.
pub fn run(argv: &[String]) -> i32 {
    for chain in split_command_chain(argv) {
        if let Err(err) = run_command(&chain) {
            terminal::failure(err.message());
            return 1;
        }
    }
    0
}

/// Split on the `;`, `;;` and `\;` chain separators used by v1.
pub fn split_command_chain(argv: &[String]) -> Vec<Vec<String>> {
    let mut chains: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for token in argv {
        if token == ";" || token == ";;" || token == "\\;" {
            if !current.is_empty() {
                chains.push(strip_cli_prefix(&current));
            }
            current.clear();
            continue;
        }
        current.push(token.clone());
    }
    if !current.is_empty() {
        chains.push(strip_cli_prefix(&current));
    }
    if chains.is_empty() {
        chains.push(Vec::new());
    }
    chains
}

fn strip_cli_prefix(argv: &[String]) -> Vec<String> {
    if argv
        .first()
        .map(|token| ALIASES.contains(&token.as_str()))
        .unwrap_or(false)
    {
        argv[1..].to_vec()
    } else {
        argv.to_vec()
    }
}

/// Normalise compatibility shorthands (`init`, `app`, bare `list`, ...).
pub fn normalize(command: &str, args: &[String]) -> (&'static str, Vec<String>) {
    match command {
        "init" => ("setup", args.to_vec()),
        "app" => ("apps", args.to_vec()),
        "list" | "create" | "get" | "update" | "delete" => {
            let mut rest = Vec::new();
            let mut iter = args.iter();
            if let Some(first) = iter.next() {
                if first != "app" {
                    rest.push(first.clone());
                }
            }
            rest.extend(iter.cloned());
            let sub = match command {
                "list" => "list",
                "create" => "create",
                "get" => "get",
                "update" => "update",
                _ => "delete",
            };
            let mut argv = vec![sub.to_string()];
            argv.extend(rest);
            ("apps", argv)
        }
        "setup" => ("setup", args.to_vec()),
        "login" => ("login", args.to_vec()),
        "logout" => ("logout", args.to_vec()),
        "whoami" => ("whoami", args.to_vec()),
        "apps" => ("apps", args.to_vec()),
        "clients" => ("clients", args.to_vec()),
        "link" => ("link", args.to_vec()),
        "sync" => ("sync", args.to_vec()),
        "test-auth" => ("test-auth", args.to_vec()),
        _ => ("", args.to_vec()),
    }
}

fn run_command(argv: &[String]) -> Result<()> {
    let Some(command) = argv.first() else {
        print_help();
        return Ok(());
    };

    if matches!(command.as_str(), "help" | "h" | "--help" | "-h") {
        print_help();
        return Ok(());
    }
    if matches!(command.as_str(), "version" | "-v" | "--version") {
        println!("ace-id {}", version());
        return Ok(());
    }

    let args = &argv[1..];
    let (name, args) = normalize(command, args);

    match name {
        "setup" => commands::setup::run(&args),
        "login" => commands::login::run(&args),
        "logout" => commands::logout::run(&args),
        "whoami" => commands::whoami::run(&args),
        "test-auth" => commands::test_auth::run(&args),
        "apps" => commands::apps::run(&args),
        "clients" => commands::clients::run(&args),
        "link" => commands::link::run(&args),
        "sync" => commands::sync::run(&args),
        "" => Err(AidError::new(format!("Unknown command: {command}"))),
        _ => Err(AidError::new(format!("Unknown command: {command}"))),
    }
}

/// Render the top-level help screen (identical structure to v1).
pub fn print_help() {
    // `withScreen` in v1 clears the visible screen only on a terminal.
    terminal::clear_screen();
    terminal::show_banner(version());

    println!("  {}", terminal::dim("USAGE"));
    println!();
    println!("    aid <command> [options]");
    println!("    aid <command> [options] \\; aid <command> [options]");
    println!();
    println!("  {}", terminal::dim("COMMANDS"));
    command_list(&[
        (
            "setup",
            "Set up a project; --recover cleans interrupted setup (--force confirms)",
        ),
        ("init", "Alias for setup"),
        ("login", "Sign in via Ace ID"),
        ("logout", "Sign out"),
        ("whoami [--json]", "Show signed-in account"),
        (
            "test-auth [--json]",
            "Verify the current session; supports JSON output",
        ),
        ("apps", "Manage applications"),
        (
            "link",
            "Link an application to this folder and write .aid.json",
        ),
        (
            "sync [--dry-run]",
            "Preview or apply remote metadata changes for this folder",
        ),
        ("clients", "Manage OAuth clients"),
        ("version", "Show CLI version"),
        ("help", "Show this message"),
    ]);
    println!();
    println!("  {}", terminal::dim("SHORT FLAGS"));
    command_list(&[
        ("-h", "Alias for --help"),
        ("-i", "Alias for --interactive / --id by command"),
        ("-n", "Alias for --name"),
        ("-r", "Alias for --redirect"),
        ("-s", "Alias for --scope"),
        ("-d", "Alias for --description"),
        ("-c", "Alias for --client"),
        ("-p", "Alias for --public"),
        ("-f", "Alias for --force"),
        ("-v", "Alias for --version"),
    ]);
    println!();
    println!("  {}", terminal::dim("APPS SUBCOMMANDS"));
    command_list(&[
        ("apps list", "List applications"),
        ("apps create --name <name>", "Create an application"),
        ("apps get <id>", "Show an application"),
        ("apps update <id> --name <name>", "Update application name"),
        ("apps delete <id>...", "Delete one or more applications"),
        ("app <subcommand>", "Compatibility alias for apps"),
        (
            "create app ...",
            "Compatibility shorthand for apps create ...",
        ),
    ]);
    println!();
    println!("  {}", terminal::dim("PROJECT LINKING"));
    command_list(&[
        (
            "link <app_id>",
            "Link an existing app to the current folder",
        ),
        (
            "link <app_id> --client <client_id>",
            "Link an app and record an OAuth client",
        ),
        ("sync [--dry-run]", "Preview or apply metadata changes"),
        ("sync <app_id>", "Replace the current link with an app"),
    ]);
    println!();
    println!("  {}", terminal::dim("CLIENTS SUBCOMMANDS"));
    command_list(&[
        (
            "clients list",
            "List clients with app ownership and link diagnostics",
        ),
        (
            "clients create --name <n> --redirect <uri>",
            "Create a client",
        ),
        ("clients get <client_id>", "Show one client"),
        ("clients delete <client_id>", "Delete a client"),
        (
            "clients rotate-secret <client_id>",
            "Rotate a client secret",
        ),
    ]);
    println!();
    println!("  {}", terminal::dim("COMMAND CHAINING"));
    command_list(&[
        (
            "aid setup -i \\; aid create app -n my-app",
            "Run full commands sequentially",
        ),
        (
            "aid login \\; aid apps list",
            "Stop immediately if a command fails",
        ),
    ]);
    println!();
    println!("  {}", terminal::dim("ALIASES"));
    command_list(&[
        ("aid", "Canonical command"),
        ("ace-id", "Compatibility alias"),
        ("aidc", "Compatibility alias"),
        ("app", "Compatibility alias for apps"),
    ]);
    println!();
    println!("  {}", terminal::dim("SUPPORT"));
    command_list(&[
        ("Docs", "https://docs.ace-base.cc"),
        ("Support", "https://bsky.app/profile/ace-base.cc"),
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn chains_split_on_separators() {
        let chains = split_command_chain(&args(&["login", ";", "apps", "list"]));
        assert_eq!(chains.len(), 2);
        assert_eq!(chains[0], args(&["login"]));
        assert_eq!(chains[1], args(&["apps", "list"]));
    }

    #[test]
    fn escaped_and_double_separators_also_split() {
        let chains = split_command_chain(&args(&["a", "\\;", "b", ";;", "c"]));
        assert_eq!(chains.len(), 3);
    }

    #[test]
    fn empty_input_yields_one_empty_chain() {
        assert_eq!(split_command_chain(&[]).len(), 1);
        assert!(split_command_chain(&[])[0].is_empty());
    }

    #[test]
    fn binary_aliases_are_stripped() {
        let chains = split_command_chain(&args(&["ace-id", "login"]));
        assert_eq!(chains[0], args(&["login"]));
    }

    #[test]
    fn compatibility_shorthands_normalise() {
        assert_eq!(normalize("init", &[]).0, "setup");
        assert_eq!(normalize("app", &args(&["list"])).0, "apps");
        let (name, args_out) = normalize("list", &[]);
        assert_eq!(name, "apps");
        assert_eq!(args_out, args(&["list"]));
        let (name, args_out) = normalize("create", &args(&["app", "-n", "x"]));
        assert_eq!(name, "apps");
        assert_eq!(args_out, args(&["create", "-n", "x"]));
    }

    #[test]
    fn unknown_commands_are_reported() {
        let code = run(&args(&["definitely-not-a-command"]));
        assert_eq!(code, 1);
    }

    #[test]
    fn version_prints_the_crate_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
