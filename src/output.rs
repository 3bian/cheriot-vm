// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::{AppError, Result};
use crate::cli::Format;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

fn without_nulls(value: Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::Object(object) => Some(Value::Object(
            object
                .into_iter()
                .filter_map(|(key, child)| Some((key, without_nulls(child)?)))
                .collect(),
        )),
        Value::Array(items) => Some(Value::Array(
            items.into_iter().filter_map(without_nulls).collect(),
        )),
        other => Some(other),
    }
}

pub(crate) fn toml(value: Value, name: &str) -> std::result::Result<String, String> {
    let document = match without_nulls(value).unwrap_or(Value::Object(Default::default())) {
        table @ Value::Object(_) => table,
        other => serde_json::json!({ name: other }),
    };
    toml::to_string_pretty(&document)
        .map(|text| text.trim_end().to_owned())
        .map_err(|error| error.to_string())
}

pub(crate) fn json_text_to_toml(text: &str, name: &str) -> std::result::Result<String, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    toml(value, name)
}

pub(crate) fn structured<T: Serialize>(
    value: &T,
    format: &Format,
    name: &str,
) -> Result<Option<String>> {
    Ok(match format {
        Format::Text => None,
        Format::Json => Some(serde_json::to_string_pretty(value)?),
        Format::Toml => Some(toml(serde_json::to_value(value)?, name).map_err(AppError::Message)?),
    })
}

pub(crate) const WIDTH: usize = 80;

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

pub(crate) fn wrap(text: &str, first: usize, rest: usize) -> String {
    let mut out = String::new();
    let mut column = first;
    let mut start_of_line = true;
    for word in text.split_whitespace() {
        let length = word.chars().count();
        if !start_of_line && column + 1 + length > WIDTH {
            out.push('\n');
            out.push_str(&" ".repeat(rest));
            column = rest;
            start_of_line = true;
        }
        if !start_of_line {
            out.push(' ');
            column += 1;
        }
        out.push_str(word);
        column += length;
        start_of_line = false;
    }
    out
}

pub(crate) fn line(text: &str) -> String {
    let text = shorten_paths(text);
    let indent = text.len() - text.trim_start().len();
    format!("{}{}", " ".repeat(indent), wrap(&text, indent, indent + 2))
}

pub(crate) fn path(path: &std::path::Path) -> String {
    let inside = |directory: Option<std::path::PathBuf>| {
        let directory = directory.filter(|directory| directory.parent().is_some())?;
        path.strip_prefix(directory).ok().map(|p| p.to_path_buf())
    };
    if let Some(relative) = inside(std::env::current_dir().ok()) {
        return if relative.as_os_str().is_empty() {
            ".".into()
        } else {
            relative.display().to_string()
        };
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    match inside(home.map(std::path::PathBuf::from)) {
        Some(relative) if !relative.as_os_str().is_empty() => {
            format!("~{}{}", std::path::MAIN_SEPARATOR, relative.display())
        }
        _ => path.display().to_string(),
    }
}

pub(crate) fn shorten_paths(text: &str) -> String {
    match std::env::current_dir() {
        Ok(directory) if directory.parent().is_some() => {
            let prefix = format!("{}{}", directory.display(), std::path::MAIN_SEPARATOR);
            text.replace(&prefix, "")
        }
        _ => text.to_owned(),
    }
}

pub(crate) fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let kept: String = text.chars().take(width.saturating_sub(3)).collect();
    format!("{kept}...")
}

pub(crate) fn columns<S: AsRef<str>>(names: &[S]) -> String {
    let widest = names
        .iter()
        .map(|name| name.as_ref().chars().count())
        .max()
        .unwrap_or(0);
    let per_row = ((WIDTH - 2 + 2) / (widest + 2)).clamp(1, names.len().max(1));
    let rows = names.len().div_ceil(per_row);
    let mut out = String::new();
    for row in 0..rows {
        let mut line = String::from(" ");
        for column in 0..per_row {
            let Some(name) = names.get(column * rows + row) else {
                continue;
            };
            line.push_str(&format!(" {:<widest$} ", name.as_ref()));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

pub(crate) fn choices<S: AsRef<str>>(title: &str, names: &[S]) -> String {
    if names.is_empty() {
        return format!("\n\n{title}: none");
    }
    format!("\n\n{title}:\n{}", columns(names).trim_end())
}

pub(crate) fn message(text: &str) -> String {
    const PREFIX: usize = "cheriot: ".len();
    let text = shorten_paths(text);
    let mut out = String::new();
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if line.starts_with(' ') {
            out.push_str(line);
        } else if index == 0 {
            out.push_str(&wrap(line, PREFIX, 2));
        } else {
            out.push_str(&wrap(line, 0, 2));
        }
    }
    out
}

static COLOUR_STDOUT: AtomicBool = AtomicBool::new(false);
static COLOUR_STDERR: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_colour(choice: Option<&crate::cli::Color>) {
    use crate::cli::Color;
    use std::io::IsTerminal;
    let global = crate::global::value("color").ok().flatten();
    let choice = match (choice, global.as_deref()) {
        (Some(choice), _) => choice.clone(),
        (None, Some("always")) => Color::Always,
        (None, Some("never")) => Color::Never,
        (None, _) => Color::Auto,
    };
    let allowed = std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
        && std::env::var_os("TERM").is_none_or(|term| term != "dumb");
    let (out, err) = match choice {
        Color::Always => (true, true),
        Color::Never => (false, false),
        Color::Auto => (
            allowed && std::io::stdout().is_terminal(),
            allowed && std::io::stderr().is_terminal(),
        ),
    };
    COLOUR_STDOUT.store(out, Ordering::Relaxed);
    COLOUR_STDERR.store(err, Ordering::Relaxed);
}

fn paint(text: &str, code: &str, enabled: bool) -> String {
    if enabled && !text.is_empty() {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_owned()
    }
}

fn stdout_colour() -> bool {
    COLOUR_STDOUT.load(Ordering::Relaxed)
}

pub(crate) fn bold(text: &str) -> String {
    paint(text, "1", stdout_colour())
}

pub(crate) fn dim(text: &str) -> String {
    paint(text, "2", stdout_colour())
}

pub(crate) fn error(text: &str) {
    eprintln!("{} {}", error_prefix(), message(text));
}

pub(crate) fn error_prefix() -> String {
    paint("cheriot:", "1;31", COLOUR_STDERR.load(Ordering::Relaxed))
}

fn status_code(word: &str) -> Option<&'static str> {
    match word {
        "ok" | "valid" | "supported" | "passed" => Some("32"),
        "missing" | "invalid" | "not supported" | "failed" | "error" => Some("31"),
        "skipped" | "unknown" | "third-party" | "unresolved" => Some("33"),
        _ => None,
    }
}

pub(crate) fn status(word: &str) -> String {
    match status_code(word) {
        Some(code) => paint(word, code, stdout_colour()),
        None => word.to_owned(),
    }
}

pub(crate) fn heading(text: &mut String, title: &str) {
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&bold(title));
    text.push('\n');
}

pub(crate) fn fields(text: &mut String, pairs: &[(&str, String)]) {
    let width = pairs.iter().map(|(key, _)| key.len()).max().unwrap_or(0) + 1;
    let indent = 2 + width + 2;
    for (key, value) in pairs {
        let label = format!("{key}:");
        let mut lines = wrap_words(value, WIDTH.saturating_sub(indent).max(20)).into_iter();
        text.push_str(&format!(
            "  {}  {}\n",
            dim(&format!("{label:<width$}")),
            status(&lines.next().unwrap_or_default())
        ));
        for line in lines {
            text.push_str(&format!("{}{line}\n", " ".repeat(indent)));
        }
    }
}

pub(crate) fn lines<I: IntoIterator<Item = S>, S: AsRef<str>>(text: &mut String, items: I) {
    for item in items {
        for (index, line) in item.as_ref().lines().enumerate() {
            if line.is_empty() {
                text.push('\n');
            } else if index == 0 {
                text.push_str(&format!("  {}\n", wrap(line, 2, 4)));
            } else if line.starts_with(' ') {
                text.push_str(&format!("    {line}\n"));
            } else {
                text.push_str(&format!("    {}\n", wrap(line, 4, 4)));
            }
        }
    }
}

pub(crate) fn table<const N: usize>(text: &mut String, headers: &[&str; N], rows: &[[String; N]]) {
    let mut widths = headers.map(str::len);
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    let last_start = 2 + widths[..N - 1].iter().map(|width| width + 2).sum::<usize>();
    let last_room = WIDTH.saturating_sub(last_start).max(16);
    let mut line = |cells: [&str; N], header: bool| {
        let mut rendered = String::from(" ");
        let mut continuation = Vec::new();
        for (index, (cell, width)) in cells.iter().zip(widths).enumerate() {
            if index + 1 == N {
                let mut parts = wrap_words(cell, last_room).into_iter();
                let first = parts.next().unwrap_or_default();
                let first = if header { dim(&first) } else { status(&first) };
                rendered.push_str(&format!(" {first}"));
                continuation.extend(parts);
            } else {
                let padding = width - cell.chars().count();
                let cell = if header { dim(cell) } else { status(cell) };
                rendered.push_str(&format!(" {cell}{} ", " ".repeat(padding)));
            }
        }
        text.push_str(rendered.trim_end());
        text.push('\n');
        for part in continuation {
            text.push_str(&format!("{}{part}\n", " ".repeat(last_start)));
        }
    };
    line(*headers, true);
    for row in rows {
        line(std::array::from_fn(|index| row[index].as_str()), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_wraps_at_spaces_and_no_line_passes_the_width() {
        let text = "word ".repeat(40);
        let wrapped = wrap(&text, 9, 2);
        let lines: Vec<&str> = wrapped.lines().collect();
        assert!(lines.len() > 1);
        assert!(lines[0].len() + 9 <= WIDTH);
        for line in &lines[1..] {
            assert!(line.starts_with("  word"), "{line:?}");
            assert!(line.len() <= WIDTH, "{line:?}");
        }
        let path = "x".repeat(WIDTH + 10);
        assert_eq!(wrap(&format!("see {path} now"), 0, 2).lines().count(), 3);
    }

    #[test]
    fn messages_keep_blocks_that_are_already_laid_out() {
        let text = format!("{}\n\nChoices:\n  one  two", "word ".repeat(30));
        let message = message(&text);
        assert!(message.ends_with("\n\nChoices:\n  one  two"), "{message}");
        assert!(message.lines().all(|line| line.len() <= WIDTH), "{message}");
    }

    #[test]
    fn names_fill_columns_downwards_within_the_width() {
        let names: Vec<String> = (0..23)
            .map(|index| format!("project-number-{index:02}"))
            .collect();
        let listed = columns(&names);
        assert!(listed.lines().all(|line| line.len() <= WIDTH), "{listed}");
        let first: Vec<&str> = listed.lines().next().unwrap().split_whitespace().collect();
        assert!(first.len() > 1, "{listed}");
        assert_eq!(first[0], "project-number-00");
        assert!(
            listed
                .lines()
                .nth(1)
                .unwrap()
                .trim_start()
                .starts_with("project-number-01")
        );
        assert_eq!(listed.split_whitespace().count(), names.len());
        assert!(choices("Projects", &names).starts_with("\n\nProjects:\n  project-number-00"));
        assert_eq!(choices::<&str>("Projects", &[]), "\n\nProjects: none");
    }

    #[test]
    fn field_values_and_last_table_cells_continue_beneath_themselves() {
        let mut text = String::new();
        fields(&mut text, &[("Key", "value ".repeat(30))]);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() > 1);
        assert!(lines[1].starts_with("        value"), "{text}");
        assert!(lines.iter().all(|line| line.len() <= WIDTH), "{text}");

        let mut text = String::new();
        table(
            &mut text,
            &["NAME", "NOTE"],
            &[["a".into(), "note ".repeat(30)]],
        );
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() > 2, "{text}");
        assert!(lines[2].starts_with("        note"), "{text}");
        assert!(lines.iter().all(|line| line.len() <= WIDTH), "{text}");
    }

    #[test]
    fn clipped_names_say_that_they_were_cut() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("a-very-long-symbol-name", 10), "a-very-...");
    }
}
