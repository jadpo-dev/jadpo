//! The one file of values on a person's own machine (area 10, "Values on
//! your machine and in production"): `.env.local` in the project's root,
//! git-ignored, holding `NAME=value` lines. Jadpo defines no `.env`,
//! `.env.development` or layering; production values come from the
//! platform's environment, and the process's environment wins over this
//! file for a variable both set.
//!
//! The file is written the safe way: only if its place is an ordinary file
//! (never a link, which could point a secret somewhere else), through a
//! temporary file only its owner can read, renamed over the old one only if
//! the old one has not changed since it was read, and with `.env.local` in
//! `.gitignore` so a secret is not committed by accident.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use jadpo_diag::{Diagnostic, rule};

/// The file's name, in the project's root.
pub const FILE: &str = ".env.local";

/// One line of the file, kept as written so a rewrite changes only the line
/// it was asked to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Line {
    /// A blank line or a comment.
    Other(String),
    Entry {
        name: String,
        value: String,
        raw: String,
    },
}

/// `.env.local` as read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub path: PathBuf,
    /// The text read, to notice a change before replacing it.
    text: String,
    lines: Vec<Line>,
}

impl Local {
    /// The values the file sets, by variable.
    pub fn values(&self) -> BTreeMap<String, String> {
        self.lines
            .iter()
            .filter_map(|line| match line {
                Line::Entry { name, value, .. } => Some((name.clone(), value.clone())),
                Line::Other(_) => None,
            })
            .collect()
    }

    /// Sets `name` to `value`, in place if the file sets it and at the end
    /// otherwise.
    pub fn set(&mut self, name: &str, value: &str) {
        let raw = format!("{name}={}", quote(value));
        for line in &mut self.lines {
            if let Line::Entry {
                name: has,
                value: old,
                raw: old_raw,
            } = line
                && has == name
            {
                *old = value.to_string();
                *old_raw = raw;
                return;
            }
        }
        self.lines.push(Line::Entry {
            name: name.to_string(),
            value: value.to_string(),
            raw,
        });
    }

    /// The file's text as it would be written.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Other(text) | Line::Entry { raw: text, .. } => out.push_str(text),
            }
            out.push('\n');
        }
        out
    }
}

/// A value as `.env.local` writes it: bare when that reads back the same
/// everywhere, in double quotes otherwise.
fn quote(value: &str) -> String {
    let bare = !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '~' | '-'
                )
        });
    if bare {
        return value.to_string();
    }
    let mut out = String::from('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One value from after the `=`: double-quoted with escapes, single-quoted
/// as it is, or bare to the end of the line.
fn unquote(rest: &str) -> Result<String, &'static str> {
    let rest = rest.trim_start();
    match rest.chars().next() {
        Some('"') => {
            let mut out = String::new();
            let mut chars = rest[1..].chars();
            while let Some(c) = chars.next() {
                match c {
                    '"' => {
                        let after = chars.as_str().trim();
                        return if after.is_empty() || after.starts_with('#') {
                            Ok(out)
                        } else {
                            Err("has text after its closing quote")
                        };
                    }
                    '\\' => match chars.next() {
                        Some('n') => out.push('\n'),
                        Some('r') => out.push('\r'),
                        Some('t') => out.push('\t'),
                        Some('"') => out.push('"'),
                        Some('\\') => out.push('\\'),
                        _ => return Err("has an escape Jadpo doesn't know"),
                    },
                    c => out.push(c),
                }
            }
            Err("has an opening quote and no closing one")
        }
        Some('\'') => match rest[1..].split_once('\'') {
            Some((value, after)) if after.trim().is_empty() || after.trim().starts_with('#') => {
                Ok(value.to_string())
            }
            Some(_) => Err("has text after its closing quote"),
            None => Err("has an opening quote and no closing one"),
        },
        _ => Ok(rest.trim_end().to_string()),
    }
}

/// Whether `name` can be a variable name in the file.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Reads `.env.local` in `root`: an empty one when there is no file.
pub fn read(root: &Path) -> Result<Local, Box<Diagnostic>> {
    let path = root.join(FILE);
    let text = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| {
            Box::new(
                Diagnostic::new(
                    &rule::CONFIGURATION_LOCAL_INVALID_ENCODING,
                    format!("`{FILE}` isn't UTF-8 text"),
                )
                .reason("Local values are read as UTF-8 text, one `NAME=value` per line, so what is read is what was typed.")
                .next(format!("Open `{FILE}` in an editor and save it as UTF-8")),
            )
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Local {
                path,
                ..Local::default()
            });
        }
        Err(error) => {
            return Err(Box::new(
                Diagnostic::new(
                    &rule::CONFIGURATION_LOCAL_READ_FAILED,
                    format!("`{FILE}` couldn't be read"),
                )
                .reason(format!("Reading it failed: {error}."))
                .next(format!("Check that `{FILE}` is a file you can read")),
            ));
        }
    };
    parse(path, text)
}

/// Reads `text` as the contents of `.env.local`.
pub fn parse(path: PathBuf, text: String) -> Result<Local, Box<Diagnostic>> {
    let mut lines = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            lines.push(Line::Other(raw.to_string()));
            continue;
        }
        let syntax = |what: &str| {
            Box::new(
                Diagnostic::new(
                    &rule::CONFIGURATION_LOCAL_SYNTAX,
                    format!("`{FILE}` line {number} {what}"),
                )
                .reason("Each line is blank, a `#` comment, or `NAME=value`; a value with spaces or other unusual characters goes in double quotes.")
                .next(format!("Fix line {number} of `{FILE}`")),
            )
        };
        let Some((name, rest)) = trimmed.split_once('=') else {
            return Err(syntax("isn't `NAME=value`"));
        };
        let name = name.trim();
        if !is_name(name) {
            return Err(syntax("doesn't start with a variable name"));
        }
        let value = unquote(rest).map_err(|what| syntax(what))?;
        if let Some(first) = seen.insert(name.to_string(), number) {
            return Err(syntax(&format!(
                "sets `{name}`, which line {first} already sets"
            )));
        }
        lines.push(Line::Entry {
            name: name.to_string(),
            value,
            raw: raw.to_string(),
        });
    }
    Ok(Local { path, text, lines })
}

/// Writes `local` back, if the file is still as it was read. Returns
/// whether `.gitignore` had to be given the file's name.
pub fn write(root: &Path, local: &Local) -> Result<bool, Box<Diagnostic>> {
    let path = root.join(FILE);
    let failed = |what: &str, error: std::io::Error| {
        Box::new(
            Diagnostic::new(
                &rule::CONFIGURATION_LOCAL_WRITE_FAILED,
                format!("`{FILE}` couldn't be saved"),
            )
            .reason(format!("{what}: {error}."))
            .next("Check that the project's folder is writable, then run the command again"),
        )
    };
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if !meta.is_file() => {
            return Err(Box::new(
                Diagnostic::new(
                    &rule::CONFIGURATION_LOCAL_UNSAFE_TARGET,
                    format!("`{FILE}` isn't an ordinary file"),
                )
                .reason("It is a link or a folder, and a secret written through a link could end up somewhere that isn't git-ignored, so Jadpo writes only to an ordinary file.")
                .next(format!("Remove `{FILE}`, and run the command again")),
            ));
        }
        Ok(_) | Err(_) => {}
    }
    let temporary = root.join(format!("{FILE}.jadpo-new"));
    let _ = std::fs::remove_file(&temporary);
    let mut file = open_private(&temporary).map_err(|error| failed("Creating it failed", error))?;
    if let Err(error) = file
        .write_all(local.render().as_bytes())
        .and_then(|()| file.sync_all())
    {
        let _ = std::fs::remove_file(&temporary);
        return Err(failed("Writing it failed", error));
    }
    drop(file);
    // Only replace what was read: a save made since is not overwritten.
    let now = std::fs::read(&path).unwrap_or_default();
    if now != local.text.as_bytes() {
        let _ = std::fs::remove_file(&temporary);
        return Err(Box::new(
            Diagnostic::new(
                &rule::CONFIGURATION_LOCAL_CHANGED,
                format!("`{FILE}` changed while it was being updated"),
            )
            .reason("Something saved the file after it was read, and replacing it now would lose that change.")
            .next("Run the command again"),
        ));
    }
    if let Err(error) = std::fs::rename(&temporary, &path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(failed("Replacing it failed", error));
    }
    Ok(ignore(root))
}

/// A new file only its owner can read.
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Makes sure `.gitignore` in `root` names `.env.local`; whether it had to
/// add it. A file that can't be written is left alone.
fn ignore(root: &Path) -> bool {
    let path = root.join(".gitignore");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let ignored = text.lines().any(|line| {
        let line = line.trim();
        matches!(
            line,
            ".env.local" | "/.env.local" | ".env.*" | ".env*" | "*.local"
        )
    });
    if ignored {
        return false;
    }
    let mut added = text;
    if !added.is_empty() && !added.ends_with('\n') {
        added.push('\n');
    }
    added.push_str(FILE);
    added.push('\n');
    std::fs::write(&path, added).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_text(text: &str) -> Result<Local, Box<Diagnostic>> {
        parse(PathBuf::from(FILE), text.to_string())
    }

    #[test]
    fn lines_read_as_names_and_values() {
        let local =
            read_text("# local\nMAIL_SENDER=a@example.test\n\nNAME=\"two words\"\nQ='x y'\n")
                .expect("reads");
        let values = local.values();
        assert_eq!(values["MAIL_SENDER"], "a@example.test");
        assert_eq!(values["NAME"], "two words");
        assert_eq!(values["Q"], "x y");
    }

    #[test]
    fn a_value_survives_being_written_and_read() {
        for value in [
            "plain",
            "with space",
            "quote\"s",
            "back\\slash",
            "a=b#c",
            "line\nbreak",
            "",
        ] {
            let mut local = Local::default();
            local.set("KEY", value);
            let back = read_text(&local.render()).expect("reads").values();
            assert_eq!(back["KEY"], value, "{value:?}");
        }
    }

    #[test]
    fn setting_changes_one_line_and_keeps_the_rest() {
        let mut local = read_text("# keep\nA=1\nB=2\n").expect("reads");
        local.set("A", "9");
        local.set("C", "3");
        assert_eq!(local.render(), "# keep\nA=9\nB=2\nC=3\n");
    }

    #[test]
    fn a_broken_line_names_its_number_not_its_value() {
        let error = read_text("A=1\nnot a line with an equals\n").expect_err("syntax");
        assert!(error.message.contains("line 2"));
        let error = read_text("A=1\nA=2\n").expect_err("twice");
        assert!(error.message.contains("line 2") && error.message.contains("`A`"));
        let error = read_text("S=\"abc\" tail\n").expect_err("after the quote");
        assert!(!error.message.contains("abc"));
    }
}
