//! A small, strict argument parser.
//!
//! v1 used Node's `util.parseArgs` in strict mode: unknown options are errors,
//! `--opt value`, `--opt=value`, `-o value` and `-o=value` are accepted, flags
//! take no value and repeated options accumulate only when declared as multiple.
//! This module reproduces that behaviour without a CLI framework, which keeps the
//! binary small and the error messages identical to v1's usage strings.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Boolean switch (`--force`, `-f`).
    Flag,
    /// Single value; a later occurrence replaces an earlier one.
    Value,
    /// Repeatable value (`--scope a --scope b`).
    Multi,
}

#[derive(Debug, Clone, Copy)]
pub struct Opt {
    pub long: &'static str,
    pub short: Option<char>,
    pub kind: Kind,
}

pub const fn flag(long: &'static str, short: Option<char>) -> Opt {
    Opt {
        long,
        short,
        kind: Kind::Flag,
    }
}

pub const fn value(long: &'static str, short: Option<char>) -> Opt {
    Opt {
        long,
        short,
        kind: Kind::Value,
    }
}

pub const fn multi(long: &'static str, short: Option<char>) -> Opt {
    Opt {
        long,
        short,
        kind: Kind::Multi,
    }
}

#[derive(Debug, Default)]
pub struct Parsed {
    flags: BTreeMap<&'static str, bool>,
    values: BTreeMap<&'static str, Vec<String>>,
    positionals: Vec<String>,
}

impl Parsed {
    pub fn flag(&self, long: &str) -> bool {
        self.flags.get(long).copied().unwrap_or(false)
    }

    pub fn value(&self, long: &str) -> Option<&str> {
        self.values
            .get(long)
            .and_then(|values| values.last())
            .map(String::as_str)
    }

    pub fn values(&self, long: &str) -> Vec<String> {
        self.values.get(long).cloned().unwrap_or_default()
    }

    pub fn positionals(&self) -> &[String] {
        &self.positionals
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    message: String,
}

fn error(message: impl Into<String>) -> ParseError {
    ParseError {
        message: message.into(),
    }
}

/// Parse `args` against `options`.
pub fn parse(args: &[String], options: &[Opt]) -> Result<Parsed, ParseError> {
    let mut parsed = Parsed::default();
    let mut index = 0;
    let mut only_positionals = false;

    while index < args.len() {
        let token = &args[index];
        index += 1;

        if only_positionals {
            parsed.positionals.push(token.clone());
            continue;
        }

        if token == "--" {
            only_positionals = true;
            continue;
        }

        if let Some(long) = token.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (long, None),
            };
            let option = options
                .iter()
                .find(|option| option.long == name)
                .ok_or_else(|| error(format!("Unknown option: --{name}")))?;
            apply(&mut parsed, option, inline, args, &mut index)?;
            continue;
        }

        if token.len() > 1 && token.starts_with('-') {
            let mut chars = token[1..].chars();
            let short = chars.next().expect("checked length");
            let rest: String = chars.collect();
            let option = options
                .iter()
                .find(|option| option.short == Some(short))
                .ok_or_else(|| error(format!("Unknown option: -{short}")))?;

            let inline = if rest.is_empty() {
                None
            } else {
                Some(rest.strip_prefix('=').unwrap_or(&rest).to_string())
            };
            apply(&mut parsed, option, inline, args, &mut index)?;
            continue;
        }

        parsed.positionals.push(token.clone());
    }

    Ok(parsed)
}

fn apply(
    parsed: &mut Parsed,
    option: &Opt,
    inline: Option<String>,
    args: &[String],
    index: &mut usize,
) -> Result<(), ParseError> {
    match option.kind {
        Kind::Flag => {
            if inline.is_some() {
                return Err(error(format!(
                    "Option --{} does not take a value",
                    option.long
                )));
            }
            parsed.flags.insert(option.long, true);
            Ok(())
        }
        Kind::Value | Kind::Multi => {
            let value = match inline {
                Some(value) => value,
                None => {
                    let next = args.get(*index).cloned().ok_or_else(|| {
                        error(format!("Option --{} requires a value", option.long))
                    })?;
                    *index += 1;
                    next
                }
            };
            let entry = parsed.values.entry(option.long).or_default();
            if option.kind == Kind::Value {
                entry.clear();
            }
            entry.push(value);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTIONS: [Opt; 5] = [
        flag("force", Some('f')),
        value("name", Some('n')),
        multi("scope", Some('s')),
        flag("dry-run", None),
        value("redirect", Some('r')),
    ];

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn long_and_short_values_are_equivalent() {
        let long = parse(&args(&["--name", "app"]), &OPTIONS).unwrap();
        assert_eq!(long.value("name"), Some("app"));
        let short = parse(&args(&["-n", "app"]), &OPTIONS).unwrap();
        assert_eq!(short.value("name"), Some("app"));
        let inline = parse(&args(&["--name=app"]), &OPTIONS).unwrap();
        assert_eq!(inline.value("name"), Some("app"));
        let attached = parse(&args(&["-napp"]), &OPTIONS).unwrap();
        assert_eq!(attached.value("name"), Some("app"));
    }

    #[test]
    fn flags_accumulate_as_booleans() {
        let parsed = parse(&args(&["--force", "--dry-run"]), &OPTIONS).unwrap();
        assert!(parsed.flag("force"));
        assert!(parsed.flag("dry-run"));
        assert!(!parsed.flag("missing"));
    }

    #[test]
    fn repeated_values_keep_the_last_and_multi_keeps_all() {
        let parsed = parse(&args(&["-n", "a", "-n", "b"]), &OPTIONS).unwrap();
        assert_eq!(parsed.value("name"), Some("b"));
        let parsed = parse(&args(&["-s", "openid", "-s", "email"]), &OPTIONS).unwrap();
        assert_eq!(parsed.values("scope"), vec!["openid", "email"]);
    }

    #[test]
    fn positionals_are_collected_in_order() {
        let parsed = parse(&args(&["app-1", "app-2", "--force"]), &OPTIONS).unwrap();
        assert_eq!(parsed.positionals(), ["app-1", "app-2"]);
    }

    #[test]
    fn double_dash_stops_option_parsing() {
        let parsed = parse(&args(&["--", "--force"]), &OPTIONS).unwrap();
        assert!(!parsed.flag("force"));
        assert_eq!(parsed.positionals(), ["--force"]);
    }

    #[test]
    fn unknown_options_are_errors() {
        assert!(parse(&args(&["--nope"]), &OPTIONS).is_err());
        assert!(parse(&args(&["-z"]), &OPTIONS).is_err());
    }

    #[test]
    fn missing_values_are_errors() {
        assert!(parse(&args(&["--name"]), &OPTIONS).is_err());
    }

    #[test]
    fn flags_reject_values() {
        assert!(parse(&args(&["--force=1"]), &OPTIONS).is_err());
    }
}
