//! Acceptance files, `acceptances/<stamp>_<purpose>.jadpo` (area 18,
//! "Acceptances"): one `acceptance { changes: { ... } }` per file, each
//! entry `subject: Kind.change` as the compiler's message gives it, with a
//! person's reason on the `///` line above.

use jadpo_db::record as format;
use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode};

/// The folder acceptances are kept in, at the root.
pub const FOLDER: &str = "acceptances";

/// The kinds of change an acceptance may list (area 18's table).
pub const KINDS: [&str; 8] = [
    "ApiChange",
    "DataChange",
    "DomainChange",
    "ErasureNotPossible",
    "NetworkChange",
    "PackageChange",
    "PolicyChange",
    "ThresholdChange",
];

/// One entry of an acceptance: what it allows and where it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// As [`super::compare::Need`] gives it: a dotted name, or a quoted one.
    pub subject: String,
    pub change: String,
    pub span: Span,
}

/// One acceptance file, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acceptance {
    pub path: String,
    /// Its `///` lines joined; `None` when there are none, or they are
    /// blank.
    pub reason: Option<String>,
    pub entries: Vec<Entry>,
    /// The `acceptance` keyword, or the file when there is none.
    pub span: Span,
    /// Why it isn't an acceptance (`acceptance.invalid`).
    pub problems: Vec<Diagnostic>,
}

/// Whether a project-relative path is an acceptance file.
pub fn is_acceptance(path: &str) -> bool {
    path.strip_prefix("acceptances/")
        .is_some_and(|name| !name.contains('/') && name.ends_with(".jadpo"))
}

/// Every acceptance file in the project, in path order.
pub fn acceptances(db: &dyn Db, project: Project) -> Vec<Acceptance> {
    let mut files: Vec<File> = project
        .files(db)
        .iter()
        .copied()
        .filter(|file| is_acceptance(file.path(db)))
        .collect();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    files.into_iter().map(|file| read(db, file)).collect()
}

fn span_of(path: &str, node: &jadpo_syntax::SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(path, range.start().into(), range.end().into())
}

fn token_span(path: &str, token: &jadpo_syntax::SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(path, range.start().into(), range.end().into())
}

/// The subject as an acceptance compares it: a dotted name as written,
/// without spaces, or a quoted one written again with Jadpo's escapes.
pub fn subject_of(key: &ast::MemberKey) -> String {
    match key.string_token() {
        Some(token) => format::quote(&crate::types::rules::unescape(token.text())),
        None => key
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect(),
    }
}

fn invalid(
    span: Span,
    message: impl Into<String>,
    label: &str,
    reason: &str,
    next: &str,
) -> Diagnostic {
    Diagnostic::new(&rule::ACCEPTANCE_INVALID, message)
        .at(span, label)
        .reason(reason)
        .next(next)
}

const SHAPE: &str = "An acceptance is one `acceptance { changes: { ... } }` per file, each entry `subject: Kind.change` copied from the compiler's message, with the reason on a `///` line above `acceptance`.";

fn read(db: &dyn Db, file: File) -> Acceptance {
    let path = file.path(db).clone();
    let parse = jadpo_db::parse(db, file);
    let root = parse.syntax();
    let whole = Span::new(&path, 0, 0);
    let mut out = Acceptance {
        path: path.clone(),
        reason: None,
        entries: Vec::new(),
        span: whole.clone(),
        problems: Vec::new(),
    };
    // A syntax error says what is wrong already.
    if !parse.errors().is_empty() {
        return out;
    }
    let decls: Vec<jadpo_syntax::SyntaxNode> = root
        .children()
        .filter(|node| node.kind() != SyntaxKind::USE_DECL)
        .collect();
    let singleton = decls
        .iter()
        .find_map(|node| ast::SingletonDecl::cast(node.clone()))
        .filter(|decl| {
            decl.keyword()
                .is_some_and(|word| word.text() == "acceptance")
        });
    let Some(decl) = singleton else {
        out.problems.push(invalid(
            whole,
            format!("`{path}` holds no acceptance"),
            "not an acceptance",
            SHAPE,
            "Write `acceptance { changes: { ... } }`, or run `jadpo acceptance new <purpose>` to draft one",
        ));
        return out;
    };
    let keyword = decl.keyword().expect("checked above");
    out.span = token_span(&path, &keyword);
    for other in decls.iter().filter(|node| *node != decl.syntax()) {
        out.problems.push(invalid(
            span_of(&path, other),
            format!("`{path}` holds more than its acceptance"),
            "not part of an acceptance",
            SHAPE,
            "Remove it: an acceptance file holds its `acceptance` alone",
        ));
    }
    let docs: Vec<String> = ast::doc_comments(decl.syntax())
        .iter()
        .map(|token| token.text().trim_start_matches('/').trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if !docs.is_empty() {
        out.reason = Some(docs.join(" "));
    }
    let Some(list) = decl.member_list() else {
        return out;
    };
    let mut changes = None;
    for item in list.syntax().children() {
        let member = ast::Member::cast(item.clone());
        let key = member.as_ref().and_then(ast::Member::member_key);
        match (
            &member,
            key.as_ref().map(|key| key.syntax().text().to_string()),
        ) {
            (Some(member), Some(word)) if word == "changes" => match member.block() {
                Some(block) => changes = Some(block),
                None => out.problems.push(invalid(
                    span_of(&path, member.syntax()),
                    "`changes:` isn't a list of changes",
                    "expected `changes: { ... }`",
                    SHAPE,
                    "Write each change on its own line inside `changes: { ... }`",
                )),
            },
            _ => out.problems.push(invalid(
                span_of(&path, &item),
                "An acceptance holds only `changes:`",
                "not part of an acceptance",
                SHAPE,
                "Remove it, and put the reason on a `///` line above `acceptance`",
            )),
        }
    }
    let Some(changes) = changes else {
        if out.problems.is_empty() {
            out.problems.push(invalid(
                out.span.clone(),
                "This acceptance lists no change",
                "no `changes:`",
                SHAPE,
                "Run `jadpo acceptance new <purpose>`, which lists every change not yet accepted",
            ));
        }
        return out;
    };
    for item in changes.syntax().children() {
        let entry = ast::Member::cast(item.clone()).and_then(|member| {
            let key = member.member_key()?;
            let value = member.value()?;
            let change: String = value
                .syntax()
                .text()
                .to_string()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            Some((subject_of(&key), change, member))
        });
        match entry {
            Some((subject, change, member))
                if change
                    .split_once('.')
                    .is_some_and(|(kind, word)| KINDS.contains(&kind) && is_word(word))
                    && subject != "\"\"" =>
            {
                out.entries.push(Entry {
                    subject,
                    change,
                    span: span_of(&path, member.syntax()),
                });
            }
            _ => out.problems.push(invalid(
                span_of(&path, &item),
                "This isn't a change an acceptance can list",
                "expected `subject: Kind.change`",
                &format!(
                    "Each entry names what changed and how, as the compiler's message gives it, such as `main.TodoView.priority: ApiChange.output_field_removed`; the kinds are {}.",
                    crate::and_list(&KINDS.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>())
                ),
                "Copy the entry from `jadpo check`'s message, or run `jadpo acceptance new <purpose>`",
            )),
        }
    }
    if out.entries.is_empty() && out.problems.is_empty() {
        out.problems.push(invalid(
            out.span.clone(),
            "This acceptance lists no change",
            "nothing listed",
            SHAPE,
            "Run `jadpo acceptance new <purpose>`, which lists every change not yet accepted",
        ));
    }
    out
}

fn is_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `syntax.unexpected_token` for a quoted key anywhere but an acceptance's
/// `changes:` (Richard, 2026-10-30): elsewhere a member's name is a name.
pub fn quoted_keys(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let allowed = is_acceptance(path);
    let root = jadpo_db::parse(db, file).syntax();
    root.descendants()
        .filter_map(ast::MemberKey::cast)
        .filter_map(|key| key.string_token())
        .filter(|token| {
            // In an acceptance, only an entry of `changes:` may be quoted.
            !allowed
                || !token
                    .parent_ancestors()
                    .filter(|node| node.kind() == SyntaxKind::MEMBER)
                    .nth(1)
                    .and_then(ast::Member::cast)
                    .and_then(|member| member.member_key())
                    .is_some_and(|key| key.syntax().text() == "changes")
        })
        .map(|token| {
            Diagnostic::new(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected a member, found a string",
            )
            .at(token_span(path, &token), "expected `name: value`")
            .reason("Inside braces, each line holds one member, written `name: value`; only an acceptance's `changes:` names a subject in quotes.")
            .next("Write this line as `name: value`")
        })
        .collect()
}
