//! Terminal output primitives: palette, text wrapping and layout blocks.
//!
//! These are behavioural ports of `src/ui/{colors,format,banner}.js` from v1 so
//! that v2 renders the same blocks: status lines, key/value fields, responsive
//! tables and the command list.

use std::io::{IsTerminal, Write};
use std::sync::OnceLock;

const RESET: &str = "\x1b[0m";

fn colors_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        if std::env::var("TERM")
            .map(|term| term == "dumb")
            .unwrap_or(false)
        {
            return false;
        }
        std::io::stdout().is_terminal()
    })
}

fn paint(prefix: &str, value: &str) -> String {
    if colors_enabled() {
        format!("{prefix}{value}{RESET}")
    } else {
        value.to_string()
    }
}

pub fn bold(value: &str) -> String {
    paint("\x1b[38;2;244;244;245m\x1b[1m", value)
}

pub fn dim(value: &str) -> String {
    paint("\x1b[38;2;161;161;170m", value)
}

pub fn green(value: &str) -> String {
    paint("\x1b[38;2;115;190;145m", value)
}

pub fn red(value: &str) -> String {
    paint("\x1b[38;2;225;90;90m", value)
}

pub fn yellow(value: &str) -> String {
    paint("\x1b[38;2;220;185;105m", value)
}

/// Terminal width in columns, defaulting to 80 like v1.
pub fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(width, _)| width.0 as usize)
        .filter(|width| *width > 0)
        .map(|width| width.max(8))
        .unwrap_or(80)
}

fn strip_ansi(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// Display width of a single char: 0 for combining/joiner marks, 2 for wide
/// East Asian and emoji code points, 1 otherwise.
fn char_width(ch: char) -> usize {
    let code = ch as u32;
    if matches!(code, 0x200d | 0xfe0e | 0xfe0f) || (0x0300..=0x036f).contains(&code) {
        return 0;
    }
    if (0x1100..=0x11ff).contains(&code)
        || (0x2e80..=0xa4cf).contains(&code)
        || (0xac00..=0xd7af).contains(&code)
        || (0xf900..=0xfaff).contains(&code)
        || (0xfe10..=0xfe6f).contains(&code)
        || (0xff00..=0xff60).contains(&code)
        || (0xffe0..=0xffe6).contains(&code)
        || (0x1f000..=0x1faff).contains(&code)
        || (0x20000..=0x3ffff).contains(&code)
    {
        return 2;
    }
    1
}

pub fn visible_len(value: &str) -> usize {
    strip_ansi(value).chars().map(char_width).sum()
}

pub fn pad_display(value: &str, width: usize) -> String {
    let mut out = value.to_string();
    let padding = width.saturating_sub(visible_len(value));
    out.push_str(&" ".repeat(padding));
    out
}

pub fn truncate(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if visible_len(value) <= width {
        return value.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let size = char_width(ch);
        if used + size > width - 1 {
            break;
        }
        out.push(ch);
        used += size;
    }
    out.push('…');
    out
}

fn split_long_word(word: &str, width: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for ch in word.chars() {
        let size = char_width(ch);
        if !line.is_empty() && used + size > width {
            parts.push(std::mem::take(&mut line));
            used = 0;
        }
        line.push(ch);
        used += size;
    }
    if !line.is_empty() {
        parts.push(line);
    }
    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

/// Word-wrap `value` to `width` display columns, preserving blank lines.
pub fn wrap_text(value: &str, width: usize) -> Vec<String> {
    if width < 1 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();

    for paragraph in value.split('\n') {
        let paragraph = paragraph.trim_end_matches('\r');
        let mut line = String::new();
        let mut line_width = 0usize;

        for word in paragraph.split_whitespace() {
            let word_width = visible_len(word);
            if word_width > width {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let mut pieces = split_long_word(word, width);
                let last = pieces.pop().unwrap_or_default();
                lines.extend(pieces);
                line_width = visible_len(&last);
                line = last;
            } else if line.is_empty() {
                line = word.to_string();
                line_width = word_width;
            } else if line_width + 1 + word_width <= width {
                line.push(' ');
                line.push_str(word);
                line_width += 1 + word_width;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
                line_width = word_width;
            }
        }

        if !line.is_empty() {
            lines.push(line);
        } else if paragraph.is_empty() {
            lines.push(String::new());
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/* ── Status lines ───────────────────────── */

pub fn success(message: &str) {
    println!("\n{} {}\n", green("✓"), message);
}

pub fn failure(message: &str) {
    eprintln!("\n{} {}\n", red("✗"), message);
}

pub fn warn(message: &str) {
    eprintln!("{} {}", yellow("!"), message);
}

pub fn info(message: &str) {
    let width = terminal_width();
    for line in wrap_text(message, width.saturating_sub(2)) {
        println!("  {line}");
    }
}

/* ── Headings ───────────────────────────── */

pub fn heading(text: &str) {
    println!("\n{}\n", dim(&text.to_uppercase()));
}

/* ── Key / value block ──────────────────── */

pub fn fields(rows: &[(&str, Option<String>)]) {
    let visible: Vec<(&str, &str)> = rows
        .iter()
        .filter_map(|(label, value)| value.as_deref().map(|value| (*label, value)))
        .collect();
    if visible.is_empty() {
        return;
    }

    let width = terminal_width();
    let max_label = visible
        .iter()
        .map(|(label, _)| visible_len(label))
        .max()
        .unwrap_or(0);
    let label_width = 24.min(1.max(max_label.min(width / 3)));
    let value_width = 1.max(width.saturating_sub(label_width + 6));

    println!();
    for (label, value) in visible {
        let label_size = visible_len(label);
        if label_size > label_width {
            for line in wrap_text(label, label_width) {
                println!("  {}", dim(&line));
            }
            for line in wrap_text(value, 1.max(width.saturating_sub(4))) {
                println!("    {}", bold(&line));
            }
            continue;
        }

        if visible_len(value) <= value_width && !value.contains('\n') {
            println!(
                "  {}  {}",
                dim(&pad_display(label, label_width)),
                bold(value)
            );
            continue;
        }

        println!("  {}", dim(label));
        for line in wrap_text(value, value_width) {
            println!("    {}", bold(&line));
        }
    }
    println!();
}

/* ── Responsive table ───────────────────── */

fn allocate_column_widths(natural: &[usize], headers: &[&str], available: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = natural
        .iter()
        .enumerate()
        .map(|(index, natural)| (*natural).min(6.max(18.min(visible_len(headers[index])))))
        .collect();
    let mut remaining = available as isize - widths.iter().sum::<usize>() as isize;

    while remaining > 0 {
        let mut target: Option<usize> = None;
        let mut largest_ratio = 0f64;
        for index in 0..widths.len() {
            if widths[index] >= natural[index] {
                continue;
            }
            let ratio = natural[index] as f64 / 1f64.max(widths[index] as f64);
            if ratio > largest_ratio {
                target = Some(index);
                largest_ratio = ratio;
            }
        }
        match target {
            Some(index) => {
                widths[index] += 1;
                remaining -= 1;
            }
            None => break,
        }
    }

    widths
}

fn render_wide_table(headers: &[&str], rows: &[Vec<String>], natural: &[usize], width: usize) {
    let gap = "  ";
    let available = 1.max(width.saturating_sub(3 + gap.len() * (headers.len().saturating_sub(1))));
    let widths = if natural.iter().sum::<usize>() <= available {
        natural.to_vec()
    } else {
        allocate_column_widths(natural, headers, available)
    };

    let header_lines: Vec<Vec<String>> = headers
        .iter()
        .enumerate()
        .map(|(index, header)| wrap_text(header, widths[index]))
        .collect();
    let header_height = header_lines.iter().map(Vec::len).max().unwrap_or(1);
    for line in 0..header_height {
        let cells: Vec<String> = header_lines
            .iter()
            .enumerate()
            .map(|(index, lines)| {
                dim(&pad_display(
                    lines.get(line).map(String::as_str).unwrap_or(""),
                    widths[index],
                ))
            })
            .collect();
        println!("  {}", cells.join(gap));
    }
    let rule: Vec<String> = widths.iter().map(|size| "─".repeat(*size)).collect();
    println!("  {}", rule.join("──"));

    for row in rows {
        let cells: Vec<Vec<String>> = row
            .iter()
            .enumerate()
            .map(|(index, value)| wrap_text(value, widths[index]))
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for line in 0..height {
            let rendered: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(index, lines)| {
                    pad_display(
                        lines.get(line).map(String::as_str).unwrap_or(""),
                        widths[index],
                    )
                })
                .collect();
            println!("  {}", rendered.join(gap));
        }
    }
}

/// Render a table, falling back to a stacked layout on narrow terminals.
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() {
        return;
    }
    if rows.is_empty() {
        println!("\n{}\n", dim("(empty)"));
        return;
    }

    let natural: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            rows.iter()
                .map(|row| visible_len(row.get(index).map(String::as_str).unwrap_or("")))
                .max()
                .unwrap_or(0)
                .max(visible_len(header))
        })
        .collect();
    let width = terminal_width();
    let minimum_table_width = 2 + headers.len() * 6 + headers.len().saturating_sub(1) * 2;

    println!();
    if width >= 72 && width >= minimum_table_width {
        render_wide_table(headers, rows, &natural, width);
    } else {
        let primary_width = 1.max(width.saturating_sub(4));
        for row in rows {
            let primary = row.first().cloned().unwrap_or_default();
            for title in wrap_text(&primary, primary_width) {
                println!("  {}", bold(&title));
            }
            for (index, header) in headers.iter().enumerate().skip(1) {
                let label = format!("{header}:");
                for label_line in wrap_text(&label, 1.max(width.saturating_sub(4))) {
                    println!("    {}", dim(&label_line));
                }
                let value = row.get(index).cloned().unwrap_or_default();
                for line in wrap_text(&value, 1.max(width.saturating_sub(6))) {
                    println!("      {line}");
                }
            }
            println!();
        }
    }
    println!();
}

/* ── Usage ──────────────────────────────── */

pub fn usage(text: &str) {
    println!();
    for line in text.trim().split('\n') {
        for wrapped in wrap_text(line, 1.max(terminal_width().saturating_sub(2))) {
            println!("  {wrapped}");
        }
    }
    println!();
}

/* ── Responsive command list ───────────── */

pub fn command_list(rows: &[(&str, &str)]) {
    if rows.is_empty() {
        return;
    }

    let width = terminal_width();
    let indent = "    ";
    let gap = "  ";
    let command_width = rows
        .iter()
        .map(|(command, _)| visible_len(command))
        .max()
        .unwrap_or(0);
    let align_columns = width >= 72
        && command_width <= 30
        && width.saturating_sub(indent.len() + command_width + gap.len()) >= 24;

    println!();

    for (command, description) in rows {
        if align_columns {
            let description_width =
                1.max(width.saturating_sub(indent.len() + command_width + gap.len() + 1));
            let description_lines = wrap_text(description, description_width);
            println!(
                "{indent}{}{gap}{}",
                bold(&pad_display(command, command_width)),
                description_lines.first().map(String::as_str).unwrap_or("")
            );
            let continuation = format!("{indent}{}{gap}", " ".repeat(command_width));
            for line in description_lines.iter().skip(1) {
                println!("{continuation}{line}");
            }
            continue;
        }

        if visible_len(&format!("{indent}{command}{gap}{description}")) <= width {
            println!("{indent}{}{gap}{description}", bold(command));
            continue;
        }

        let safe_command = truncate(command, 1.max(width.saturating_sub(indent.len())));
        println!("{indent}{}", bold(&safe_command));
        let description_indent = format!("{indent}  ");
        for line in wrap_text(
            description,
            1.max(width.saturating_sub(description_indent.len())),
        ) {
            println!("{description_indent}{line}");
        }
    }
    println!();
}

/* ── Machine-readable output ───────────── */

pub fn json_output(value: &serde_json::Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(_) => println!("null"),
    }
}

/* ── Banner ─────────────────────────────── */

/// Half-block wordmark, identical to the v1 banner.
pub fn show_banner(version: &str) {
    const WORDMARK: [&str; 2] = ["   ▄▀█ █ █▀▄ █▀▀", "   █▀█ █ █▄▀ █▄▄"];
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let _ = writeln!(handle);
    for line in WORDMARK {
        let _ = writeln!(handle, "{}", bold(line));
    }
    let _ = writeln!(handle, "   {}  {}", dim("CLI"), dim(&format!("v{version}")));
    let _ = writeln!(handle);
    let _ = handle.flush();
}

/* ── Screen helpers ─────────────────────── */

/// Clear the visible screen on the normal buffer (scrollback is preserved).
pub fn clear_screen() {
    if !std::io::stdout().is_terminal() {
        return;
    }
    let mut handle = std::io::stdout();
    let _ = write!(handle, "\x1b[H\x1b[2J");
    let _ = handle.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_len_ignores_ansi_sequences() {
        assert_eq!(visible_len("\x1b[1mabc\x1b[0m"), 3);
        assert_eq!(visible_len("abc"), 3);
    }

    #[test]
    fn wide_characters_count_as_two_columns() {
        assert_eq!(visible_len("日本"), 4);
    }

    #[test]
    fn wrap_breaks_on_word_boundaries() {
        assert_eq!(
            wrap_text("the quick brown fox", 9),
            vec!["the quick", "brown fox"]
        );
    }

    #[test]
    fn wrap_splits_words_longer_than_the_width() {
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_preserves_blank_lines() {
        assert_eq!(wrap_text("a\n\nb", 10), vec!["a", "", "b"]);
    }

    #[test]
    fn truncate_appends_ellipsis() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
        assert_eq!(truncate("abc", 1), "…");
    }

    #[test]
    fn pad_display_uses_visible_width() {
        assert_eq!(pad_display("ab", 4), "ab  ");
        assert_eq!(visible_len(&pad_display("日本", 4)), 4);
    }
}
