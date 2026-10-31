//! A disk store's settings (area 16, "The disk driver"; chunk B2): each
//! connection with `driver: Driver.disk` gives `path:`, the folder its
//! files are kept in, and may give `free_space_floor:`, the share of the
//! disk below which a put answers `storage_full`, 5% when left out.
//!
//! - `path:` is an absolute folder written out plainly, with no `.` or
//!   `..` in it, outside the project's own folder, so users' files are
//!   never swept into git, a build or the source a server is given
//!   (`connection.disk_path_invalid`).
//! - `free_space_floor:` is a percentage from 0% up to, but not, 100%,
//!   to the ten-thousandth of a percent (`connection.value_invalid`).
//! - Nothing else is a disk store's key (`connection.key_invalid`).

use std::path::{Component, Path};

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::AstNode;

use crate::data::ConnectionKind;
use crate::tree::{self, name_text, node_span};
use crate::types::Const;

/// `free_space_floor:` written as a percentage, `5%` or `2.5%`, in
/// millionths of the disk (`jadpo_std::files::WHOLE_DISK`): `None` for a
/// percentage that is negative, 100% or more, or finer than a
/// ten-thousandth of a percent.
pub fn parse_floor(written: &str) -> Option<u32> {
    let number = written.strip_suffix('%')?;
    let (whole, places) = number.split_once('.').unwrap_or((number, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(places) || places.len() > 4 {
        return None;
    }
    if number.contains('.') && places.is_empty() {
        return None;
    }
    let whole: u32 = whole.parse().ok().filter(|whole| *whole < 100)?;
    let places: u32 = format!("{places:0<4}").parse().ok()?;
    Some(whole * 10_000 + places)
}

/// Every problem with the app's disk stores' settings.
#[salsa::tracked(returns(ref))]
pub fn disk_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let folder = project.folder(db);
    for found in crate::config::singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != ConnectionKind::Files {
                    continue;
                }
                for (setting, option) in tree::members(entry.block()) {
                    let word = name_text(&setting).unwrap_or_default();
                    let span = option.value().map_or_else(
                        || node_span(&found.path, option.syntax()),
                        |value| node_span(&found.path, value.syntax()),
                    );
                    let value = option.value().as_ref().map(Const::from_expr);
                    match word.as_str() {
                        "driver" => {}
                        "path" => out.extend(path_problem(&connection, value, span, folder)),
                        "free_space_floor" => {
                            let floor = match &value {
                                Some(Const::Percentage(written)) => parse_floor(written),
                                _ => None,
                            };
                            if floor.is_none() {
                                out.push(
                                    Diagnostic::new(
                                        &rule::CONNECTION_VALUE_INVALID,
                                        format!("`{connection}`'s `free_space_floor:` must be a percentage below 100%"),
                                    )
                                    .at(span, "not a floor")
                                    .reason("`free_space_floor:` is the share of the disk kept free: below it, a put answers `storage_full`, so imports and the database aren't starved of space. It is a percentage from `0%` up to, but not, `100%`, `5%` when left out.")
                                    .next("Write `free_space_floor: 5%`, or remove it"),
                                );
                            }
                        }
                        other => out.push(
                            Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!("A connection with `driver: Driver.disk` takes no `{other}:`"),
                            )
                            .at(node_span(&found.path, setting.syntax()), "not a disk setting")
                            .reason("A disk store gives `path:`, the folder its files are kept in, and may give `free_space_floor:`, the share of the disk kept free, 5% when left out.")
                            .next(format!("Remove `{other}:`")),
                        ),
                    }
                }
            }
        }
    }
    out
}

/// What is wrong with `connection`'s `path:`, written as `value`, against
/// the project's own `folder` (empty where it isn't known).
fn path_problem(
    connection: &str,
    value: Option<Const>,
    span: Span,
    folder: &str,
) -> Option<Diagnostic> {
    let invalid = |title: String, label: &str| {
        Diagnostic::new(&rule::CONNECTION_DISK_PATH_INVALID, title).at(span.clone(), label)
    };
    let Some(Const::Str(written)) = value else {
        return Some(
            Diagnostic::new(
                &rule::CONNECTION_VALUE_INVALID,
                format!("`{connection}`'s `path:` must be a folder written out"),
            )
            .at(span.clone(), "not a folder")
            .reason("`path:` is the folder on the server a disk store keeps its files in, written out in full: `path: \"/var/lib/acme/uploads\"`.")
            .next("Write `path: \"/var/lib/<app>/<store>\"`"),
        );
    };
    let next = format!(
        "Write the folder in full, outside the project, such as `path: \"/var/lib/<app>/{connection}\"`"
    );
    let path = Path::new(&written);
    if written.chars().any(char::is_control) || !path.is_absolute() {
        return Some(
            invalid(format!("`{connection}`'s `path:` must be an absolute folder"), "a relative path")
                .reason("A disk store keeps its files at `path:` on the server that runs the app. A relative path depends on where the server starts, often the project's own folder, where git and `jadpo build` would sweep users' files up with the source.")
                .next(next),
        );
    }
    if path
        .components()
        .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Some(
            invalid(format!("`{connection}`'s `path:` must name its folder without `.` or `..`"), "holds `.` or `..`")
                .reason("`path:` names a disk store's folder as it is, so which folder it is, and whether it is inside the project, can be told from the text alone.")
                .next(next),
        );
    }
    let folder = Path::new(folder);
    if folder.is_absolute() {
        let resolved = resolved(path);
        if let Ok(inside) = resolved.strip_prefix(folder) {
            let first = inside
                .components()
                .next()
                .map(|part| format!("inside `{}/`", part.as_os_str().to_string_lossy()))
                .unwrap_or_else(|| "the project's own folder".to_string());
            return Some(
                invalid(format!("`{connection}`'s `path:` is inside the project"), &first)
                    .reason("A disk store's files are your users' data. Inside the project, among its source or in `build/`, git would commit them, `jadpo build` and `jadpo dev` could write over them, and every copy of the source would carry them.")
                    .next(next),
            );
        }
    }
    None
}

/// `path` with the links of the part of it that exists resolved, so a
/// link into the project is seen as the project; the rest is kept as
/// written.
fn resolved(path: &Path) -> std::path::PathBuf {
    let mut existing = path;
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = std::fs::canonicalize(existing) {
            let mut out = real;
            out.extend(rest.iter().rev());
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_floor;

    /// A floor is a percentage below 100%, to a ten-thousandth of one.
    #[test]
    fn a_floor_is_a_percentage_below_a_hundred() {
        assert_eq!(parse_floor("5%"), Some(50_000));
        assert_eq!(parse_floor("0%"), Some(0));
        assert_eq!(parse_floor("2.5%"), Some(25_000));
        assert_eq!(parse_floor("99.9999%"), Some(999_999));
        for refused in ["100%", "-1%", "5", "5.%", ".5%", "1.00001%", "150%", "5mb"] {
            assert_eq!(parse_floor(refused), None, "{refused}");
        }
    }
}
