//! Shared `--search` / `--limit` / `--json` handling for list commands.

use serde_json::Value;

use crate::cli::args::{self, flag, value};
use crate::error::{AidError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListOptions {
    pub json: bool,
    pub search: String,
    pub limit: Option<usize>,
}

const OPTIONS: [args::Opt; 3] = [
    flag("json", None),
    value("search", None),
    value("limit", None),
];

pub fn parse_list_options(argv: &[String], usage: &str) -> Result<ListOptions> {
    let parsed = args::parse(argv, &OPTIONS).map_err(|_| AidError::new(usage))?;
    if !parsed.positionals().is_empty() {
        return Err(AidError::new(usage));
    }

    let limit = match parsed.value("limit") {
        None => None,
        Some(raw) => {
            let digits_only = !raw.is_empty() && raw.chars().all(|ch| ch.is_ascii_digit());
            let parsed_limit = raw.parse::<usize>().ok().filter(|limit| *limit >= 1);
            match (digits_only, parsed_limit) {
                (true, Some(limit)) => Some(limit),
                _ => return Err(AidError::new("--limit must be a positive integer.")),
            }
        }
    };

    Ok(ListOptions {
        json: parsed.flag("json"),
        search: parsed
            .value("search")
            .map(|value| value.trim().to_lowercase())
            .unwrap_or_default(),
        limit,
    })
}

/// Filter `items` by a case-insensitive substring match over `searchable`.
pub fn filter_resources<T: Clone>(
    items: &[T],
    search: &str,
    limit: Option<usize>,
    searchable: impl Fn(&T) -> Vec<String>,
) -> Vec<T> {
    let needle = search.trim().to_lowercase();
    let filtered: Vec<T> = if needle.is_empty() {
        items.to_vec()
    } else {
        items
            .iter()
            .filter(|item| {
                searchable(item)
                    .iter()
                    .any(|value| value.to_lowercase().contains(&needle))
            })
            .cloned()
            .collect()
    };

    match limit {
        Some(limit) => filtered.into_iter().take(limit).collect(),
        None => filtered,
    }
}

/// Read a string array from a JSON value, ignoring non-string entries.
pub fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn parses_json_search_and_limit() {
        let options = parse_list_options(
            &args(&["--json", "--search", "App", "--limit", "5"]),
            "usage",
        )
        .unwrap();
        assert!(options.json);
        assert_eq!(options.search, "app");
        assert_eq!(options.limit, Some(5));
    }

    #[test]
    fn rejects_invalid_limits() {
        assert!(parse_list_options(&args(&["--limit", "0"]), "usage").is_err());
        assert!(parse_list_options(&args(&["--limit", "abc"]), "usage").is_err());
        assert!(parse_list_options(&args(&["--limit", "-1"]), "usage").is_err());
    }

    #[test]
    fn rejects_positionals_and_unknown_flags() {
        assert!(parse_list_options(&args(&["extra"]), "usage").is_err());
        assert!(parse_list_options(&args(&["--nope"]), "usage").is_err());
    }

    #[test]
    fn filtering_is_case_insensitive_and_limited() {
        let items = vec![
            "Alpha".to_string(),
            "beta".to_string(),
            "alphabet".to_string(),
        ];
        let filtered = filter_resources(&items, "ALPHA", None, |item| vec![item.clone()]);
        assert_eq!(filtered, vec!["Alpha".to_string(), "alphabet".to_string()]);

        let limited = filter_resources(&items, "", Some(2), |item| vec![item.clone()]);
        assert_eq!(limited.len(), 2);
    }
}
