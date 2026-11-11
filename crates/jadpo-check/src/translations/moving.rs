//! Moving a message to `shared/translations/` (area 1, "Sharing and
//! promotion"; area 10, "Where translations live"): the edits, beside the
//! move of its type, that take its wording with it. In each of its
//! domain's language files the member leaves, with the comment lines
//! above it, for the same locale's file in `shared/translations/`, made
//! when there is none; the lines of the domain's `sources.jadpo_record`
//! that name it move to the shared folder's record, so a translation keeps
//! what it was made from and who accepted it; and every call of it, in
//! code, tests and email templates, names `shared` where it named the
//! domain.

use jadpo_db::{Db, Project};
use jadpo_diag::Span;
use jadpo_syntax::ast::AstNode;

use super::declared::{SHARED, language_files};
use super::language::language_file;
use super::sources::{self, record_path};

/// The edits that move the wording and the calls of message `key`, worded
/// in `domain`, to `shared/translations/`.
pub(crate) fn wording_and_calls(
    db: &dyn Db,
    project: Project,
    domain: &str,
    key: &str,
) -> Vec<(Span, String)> {
    let mut edits = wording(db, project, domain, key);
    edits.extend(record(db, project, domain, key));
    edits.extend(calls(db, project, domain, key));
    edits
}

/// The member `key` out of each of `domain`'s language files and into the
/// shared file of the same locale.
fn wording(db: &dyn Db, project: Project, domain: &str, key: &str) -> Vec<(Span, String)> {
    let shared: Vec<_> = language_files(db, project, SHARED)
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref().map(|l| (file, l)))
        .collect();
    let mut edits = Vec::new();
    for file in language_files(db, project, domain) {
        let Some(language) = language_file(db, project, file) else {
            continue;
        };
        let Some(member) = language.member(key) else {
            continue;
        };
        let text = file.text(db);
        let (start, end) = whole_lines(text, member.span.start, member.span.end);
        let mut moved = text[start as usize..end as usize].to_string();
        if !moved.ends_with('\n') {
            moved.push('\n');
        }
        edits.push((Span::new(file.path(db).clone(), start, end), String::new()));
        let into = shared
            .iter()
            .find(|(_, other)| other.tag == language.tag)
            .and_then(|(file, other)| Some((*file, other.close?)));
        match into {
            Some((target, close)) => {
                let target_text = target.text(db);
                let line = line_start(target_text, close);
                // On a line of its own before the closing brace, or after
                // the opening one where the block is written `{}`.
                if target_text[line as usize..close as usize].trim().is_empty() {
                    edits.push((Span::new(target.path(db).clone(), line, line), moved));
                } else {
                    edits.push((
                        Span::new(target.path(db).clone(), close, close),
                        format!("\n{moved}"),
                    ));
                }
            }
            None => edits.push((
                Span::new(
                    format!("{SHARED}/translations/{}.jadpo", language.tag),
                    0,
                    0,
                ),
                format!("translations {{\n{moved}}}\n"),
            )),
        }
    }
    edits
}

/// The record lines naming `key` out of `domain`'s `sources.jadpo_record`
/// and into the shared folder's, each record written whole.
fn record(db: &dyn Db, project: Project, domain: &str, key: &str) -> Vec<(Span, String)> {
    let text_of = |path: &str| {
        project
            .records(db)
            .iter()
            .find(|record| record.path == path)
            .and_then(|record| record.text.as_ref().ok())
    };
    let from = record_path(domain);
    let Some(entries) = text_of(&from).and_then(|text| sources::parse(text).ok()) else {
        return Vec::new();
    };
    let (moved, kept): (Vec<_>, Vec<_>) =
        entries.into_iter().partition(|entry| entry.member == key);
    if moved.is_empty() {
        return Vec::new();
    }
    let to = record_path(SHARED);
    let existing = text_of(&to);
    let mut shared = match existing.map(|text| sources::parse(text)) {
        Some(Ok(entries)) => entries,
        // A shared record Jadpo can't read is its own error; it is left as
        // it is, and the moved lines are left out rather than lost in it.
        Some(Err(_)) => return Vec::new(),
        None => Vec::new(),
    };
    shared.retain(|entry| entry.member != key);
    shared.extend(moved);
    let length = |text: Option<&String>| text.map_or(0, |text| text.len() as u32);
    vec![
        (
            Span::new(from.clone(), 0, length(text_of(&from))),
            sources::render(&kept),
        ),
        (Span::new(to, 0, length(existing)), sources::render(&shared)),
    ]
}

/// Each `translations.<domain>.<key>` the project writes, in its code and
/// tests and in its email templates, naming `shared` instead.
fn calls(db: &dyn Db, project: Project, domain: &str, key: &str) -> Vec<(Span, String)> {
    let mut edits = Vec::new();
    for &file in project.files(db) {
        let Some(source) = crate::tree::source(db, file) else {
            continue;
        };
        let tokens: Vec<_> = source
            .syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia())
            .collect();
        for window in tokens.windows(5) {
            let words: Vec<&str> = window.iter().map(|token| token.text()).collect();
            if words == [super::TRANSLATIONS, ".", domain, ".", key] {
                let range = window[2].text_range();
                edits.push((
                    Span::new(
                        file.path(db).clone(),
                        u32::from(range.start()),
                        u32::from(range.end()),
                    ),
                    SHARED.to_string(),
                ));
            }
        }
    }
    let written = format!("{}.{domain}.{key}", super::TRANSLATIONS);
    for record in project.records(db) {
        if !jadpo_db::is_email_template(&record.path) {
            continue;
        }
        let Ok(text) = &record.text else {
            continue;
        };
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let mut from = 0;
        while let Some(at) = text[from..].find(&written).map(|at| at + from) {
            let before = text[..at].chars().next_back();
            let after = text[at + written.len()..].chars().next();
            if !before.is_some_and(|c| word(c) || c == '.') && !after.is_some_and(word) {
                let start = at + super::TRANSLATIONS.len() + 1;
                edits.push((
                    Span::new(
                        record.path.clone(),
                        start as u32,
                        (start + domain.len()) as u32,
                    ),
                    SHARED.to_string(),
                ));
            }
            from = at + written.len();
        }
    }
    edits
}

/// The start of the line holding `at`.
fn line_start(text: &str, at: u32) -> u32 {
    text[..at as usize].rfind('\n').map_or(0, |i| i as u32 + 1)
}

/// The lines `start..end` is on, from the start of its first, taking the
/// comment lines right above it, to the end of its last, its newline
/// included.
fn whole_lines(text: &str, start: u32, end: u32) -> (u32, u32) {
    let mut first = line_start(text, start);
    while first > 0 {
        let above = line_start(text, first - 1);
        if text[above as usize..first as usize]
            .trim_start()
            .starts_with("//")
        {
            first = above;
        } else {
            break;
        }
    }
    let last = text[end as usize..]
        .find('\n')
        .map_or(text.len(), |i| end as usize + i + 1);
    (first, last as u32)
}
