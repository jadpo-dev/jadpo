//! Declining a judgement call (area 1, "Declining a judgement call";
//! question 11 there, decided 2026-10-17). Every finding is an error, and a
//! declinable one may be answered by a `decline <rule>: "<reason>"` line
//! directly above the declaration or member it is on, instead of a fix.
//!
//! [`answer`] runs last in [`crate::check`]: it drops each finding a line
//! declines, and reports the lines themselves: one naming a rule that
//! can't be declined (`decline.not_declinable`), one with no reason
//! (`decline.reason_required`), and one whose rule no longer fires there
//! (`decline.unused`). The commands (`jadpo declines list` and `clear`)
//! and the editor's "Decline with a reason" and "Remove decline" read and
//! write the lines through [`declines`], [`removal`] and [`insertion`], so
//! all of them agree on what a decline is and where it goes.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, Project};
use jadpo_diag::{CATALOGUE, Diagnostic, Edit, Span, nearest, rule};
use jadpo_syntax::SyntaxKind::{self, *};
use jadpo_syntax::ast::{self, AstNode};
use jadpo_syntax::{SyntaxElement, SyntaxNode};

use crate::types::rules::unescape;

/// One `decline` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decline {
    /// The project-relative file.
    pub path: String,
    /// The rule as written, `library.similar_exists`; empty when the line
    /// names none (a syntax error).
    pub rule: String,
    /// The reason, quotes and escapes removed; `None` when the line has
    /// none.
    pub reason: Option<String>,
    /// The one-based line it is on.
    pub line: u32,
    /// The line from `decline` to its reason.
    pub span: Span,
    /// Where the rule is written, or the `decline` keyword without one.
    pub rule_span: Span,
    /// The range of the declaration or member it stands above.
    pub owner: (u32, u32),
    /// What it declines on: the declaration's or member's name, when it has
    /// one (`cap_total`, `rounding.capped` for a library's).
    pub subject: Option<String>,
}

/// Which declines a command acts on: every one, or those of one rule,
/// under one path, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub rule: Option<String>,
    /// A project-relative file or folder; `None` or `.` for all.
    pub path: Option<String>,
}

impl Filter {
    pub fn matches(&self, decline: &Decline) -> bool {
        let rule = self.rule.as_ref().is_none_or(|rule| *rule == decline.rule);
        let path = match self.path.as_deref() {
            None | Some("" | ".") => true,
            Some(path) => {
                let path = path.trim_end_matches('/');
                decline.path == path || decline.path.starts_with(&format!("{path}/"))
            }
        };
        rule && path
    }
}

/// Every declinable rule's id, sorted.
pub fn declinable_rules() -> Vec<&'static str> {
    CATALOGUE
        .iter()
        .filter(|rule| rule.declinable)
        .map(|rule| rule.id)
        .collect()
}

/// Every decline in `file`, in source order.
pub fn declines_in(db: &dyn Db, file: File) -> Vec<Decline> {
    let parse = jadpo_db::parse(db, file);
    let root = parse.syntax();
    let path = file.path(db);
    let text = file.text(db);
    root.descendants()
        .filter(|node| node.kind() == DECLINE)
        .filter_map(ast::Decline::cast)
        .map(|decline| read(&decline, path, text))
        .collect()
}

/// Every decline in the project, by file and position.
pub fn declines(db: &dyn Db, project: Project) -> Vec<Decline> {
    let mut files: Vec<File> = project.files(db).clone();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    files
        .into_iter()
        .flat_map(|file| declines_in(db, file))
        .collect()
}

fn read(decline: &ast::Decline, path: &str, text: &str) -> Decline {
    let node = decline.syntax();
    let range = node.text_range();
    let keyword = decline.decline_kw_token();
    let start: u32 = keyword
        .as_ref()
        .map_or(range.start(), |token| token.text_range().start())
        .into();
    let end: u32 = range.end().into();
    let rule_node = decline.decline_rule();
    let rule = rule_node
        .as_ref()
        .map(ast::DeclineRule::id)
        .unwrap_or_default();
    let rule_span = match (&rule_node, &keyword) {
        (Some(rule), _) => {
            let r = rule.syntax().text_range();
            Span::new(path, r.start().into(), r.end().into())
        }
        (None, Some(keyword)) => {
            let r = keyword.text_range();
            Span::new(path, r.start().into(), r.end().into())
        }
        (None, None) => Span::new(path, start, end),
    };
    let owner = node.parent();
    Decline {
        path: path.to_string(),
        rule,
        reason: decline.reason().map(|token| unescape(token.text())),
        line: line_of(text, start),
        span: Span::new(path, start, end),
        rule_span,
        owner: owner.as_ref().map_or((start, end), |owner| {
            let r = owner.text_range();
            (r.start().into(), r.end().into())
        }),
        subject: owner.as_ref().and_then(subject),
    }
}

fn line_of(text: &str, offset: u32) -> u32 {
    let end = (offset as usize).min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}

/// The name of the declaration or member `node`, with its library's for a
/// library's member.
fn subject(node: &SyntaxNode) -> Option<String> {
    let name = node.children().find_map(|child| match child.kind() {
        NAME => Some(child.text().to_string()),
        MEMBER_KEY => Some(child.text().to_string()),
        _ => None,
    })?;
    let library = node
        .ancestors()
        .skip(1)
        .find(|ancestor| ancestor.kind() == LIBRARY_DECL)
        .and_then(|library| ast::LibraryDecl::cast(library)?.name())
        .map(|name| name.syntax().text().to_string());
    Some(match library {
        Some(library) => format!("{library}.{name}"),
        None => name,
    })
}

/// Whether a `decline` line may stand above a node of this kind.
fn is_owner(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        TYPE_DECL
            | FUNCTION_DECL
            | CONSTANT_DECL
            | ACTION_DECL
            | QUERY_DECL
            | FIXTURE_DECL
            | ENTITY_DECL
            | LIBRARY_DECL
            | ROUTES_DECL
            | SCHEDULE_DECL
            | SUBSCRIBER_DECL
            | TEST_DECL
            | SEED_DECL
            | PACKAGE_DECL
            | RESOURCE_DECL
            | PROVIDE_DECL
            | SINGLETON_DECL
            | MEMBER
            | BARE_MEMBER
            | ROUTE_ENTRY
    )
}

/// The declaration or member a finding at `offset` is on: the innermost
/// one around it.
fn owner_at(root: &SyntaxNode, offset: u32) -> Option<SyntaxNode> {
    let token = root.token_at_offset(offset.into()).right_biased()?;
    token.parent_ancestors().find(|node| is_owner(node.kind()))
}

/// The findings `found` leaves once each declined one is dropped, with
/// the problems of the `decline` lines themselves.
/// `complete` says every file parsed, so a rule that didn't fire wasn't
/// stopped by a syntax error.
pub(crate) fn answer(
    db: &dyn Db,
    project: Project,
    found: Vec<Diagnostic>,
    complete: bool,
) -> Vec<Diagnostic> {
    let all = declines(db, project);
    if all.is_empty() {
        return found;
    }
    let roots: BTreeMap<String, SyntaxNode> = project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), jadpo_db::parse(db, *file).syntax()))
        .collect();
    let texts: BTreeMap<String, String> = jadpo_db::source_texts(db, project);
    let mut used = vec![false; all.len()];
    let mut out = Vec::new();
    for diagnostic in found {
        let declined = diagnostic.rule.declinable
            && diagnostic.primary.as_ref().is_some_and(|label| {
                let Some(owner) = roots
                    .get(&label.span.path)
                    .and_then(|root| owner_at(root, label.span.start))
                else {
                    return false;
                };
                let range = owner.text_range();
                let owner = (u32::from(range.start()), u32::from(range.end()));
                let found = all.iter().enumerate().find(|(i, decline)| {
                    !used[*i]
                        && decline.path == label.span.path
                        && decline.owner == owner
                        && decline.rule == diagnostic.rule.id
                });
                match found {
                    Some((i, _)) => {
                        used[i] = true;
                        true
                    }
                    None => false,
                }
            });
        if !declined {
            out.push(diagnostic);
        }
    }
    let mut problems = Vec::new();
    let mut unused = Vec::new();
    for (i, decline) in all.iter().enumerate() {
        if decline.rule.is_empty() {
            // A syntax error says so.
            continue;
        }
        match jadpo_diag::find(&decline.rule) {
            Some(found) if found.declinable => {
                if !used[i] {
                    let text = texts.get(&decline.path).map_or("", String::as_str);
                    unused.push(unused_decline(decline, text));
                }
            }
            Some(found) => problems.push(not_declinable(decline, found.id)),
            None => problems.push(unknown_rule(decline)),
        }
        if decline
            .reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
        {
            problems.push(reason_required(decline));
        }
    }
    // A rule that didn't fire may only have been stopped by another error,
    // so a decline is called unused only when everything else is answered.
    let settled = complete
        && out
            .iter()
            .chain(&problems)
            .all(|d| d.rule.declinable || d.rule.id.starts_with("decline."));
    out.extend(problems);
    if settled {
        out.extend(unused);
    }
    out
}

/// The line `decline` is on, whole, with its line break: what removing it
/// deletes.
pub fn removal(text: &str, decline: &Decline) -> Edit {
    let start = text[..decline.span.start as usize]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let before = &text[start..decline.span.start as usize];
    let start = if before.trim().is_empty() {
        start
    } else {
        decline.span.start as usize
    };
    let end = text[decline.span.end as usize..]
        .find('\n')
        .map_or(text.len(), |i| decline.span.end as usize + i + 1);
    Edit {
        span: Span::new(&decline.path, start as u32, end as u32),
        replacement: String::new(),
    }
}

/// `text` with the `declines` in it removed.
pub fn without(text: &str, declines: &[&Decline]) -> String {
    let mut edits: Vec<Edit> = declines.iter().map(|d| removal(text, d)).collect();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.span.start));
    let mut out = text.to_string();
    for edit in edits {
        out.replace_range(edit.span.start as usize..edit.span.end as usize, "");
    }
    out
}

/// The line declining `diagnostic`'s rule on the declaration or member it
/// is on, inserted below any declines it already has, with an empty
/// reason for the author to write (`decline.reason_required` asks for it
/// until they do). `None` for a rule that can't be declined.
pub fn insertion(db: &dyn Db, project: Project, diagnostic: &Diagnostic) -> Option<Edit> {
    if !diagnostic.rule.declinable {
        return None;
    }
    let label = diagnostic.primary.as_ref()?;
    let file = project
        .files(db)
        .iter()
        .copied()
        .find(|file| file.path(db) == &label.span.path)?;
    let root = jadpo_db::parse(db, file).syntax();
    let owner = owner_at(&root, label.span.start)?;
    let text = file.text(db);
    // The first token after the owner's `decline` lines and comments.
    let first = owner
        .children_with_tokens()
        .filter(|element| element.kind() != DECLINE && !element.kind().is_trivia())
        .find_map(|element: SyntaxElement| match element {
            SyntaxElement::Token(token) => Some(token.text_range().start()),
            SyntaxElement::Node(node) => node
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| !t.kind().is_trivia())
                .map(|t| t.text_range().start()),
        })?;
    let first = u32::from(first) as usize;
    let line_start = text[..first].rfind('\n').map_or(0, |i| i + 1);
    let indent: String = text[line_start..first]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect();
    Some(Edit {
        span: Span::new(&label.span.path, line_start as u32, line_start as u32),
        replacement: format!("{indent}decline {}: \"\"\n", diagnostic.rule.id),
    })
}

fn place(decline: &Decline) -> String {
    match &decline.subject {
        Some(subject) => format!("`{subject}`"),
        None => "the declaration below it".to_string(),
    }
}

fn unused_decline(decline: &Decline, text: &str) -> Diagnostic {
    let removed = removal(text, decline);
    let rule = &decline.rule;
    Diagnostic::new(
        &rule::DECLINE_UNUSED,
        format!("Nothing to decline: `{rule}` doesn't fire here"),
    )
    .at(decline.rule_span.clone(), "no such finding here")
    .reason(format!(
        "This line declines `{rule}` on {}, but `jadpo check` finds no `{rule}` there now, so the decline has outlived its reason.",
        place(decline)
    ))
    .next("Remove the `decline` line")
    .edit(removed.span, removed.replacement)
}

fn declinable_list() -> String {
    crate::and_list(
        &declinable_rules()
            .iter()
            .map(|id| format!("`{id}`"))
            .collect::<Vec<_>>(),
    )
}

fn not_declinable(decline: &Decline, id: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::DECLINE_NOT_DECLINABLE,
        format!("`{id}` can't be declined"),
    )
    .at(decline.rule_span.clone(), "a plain mistake")
    .reason(format!(
        "Only judgement calls can be declined: {}. `{id}` reports a mistake, which only a fix answers.",
        declinable_list()
    ))
    .next(format!(
        "Remove the `decline` line, and fix what `{id}` reports if it fires"
    ))
}

fn unknown_rule(decline: &Decline) -> Diagnostic {
    let written = &decline.rule;
    let ids = CATALOGUE.iter().map(|rule| rule.id);
    let diagnostic = Diagnostic::new(
        &rule::DECLINE_NOT_DECLINABLE,
        format!("`{written}` isn't a rule"),
    )
    .at(decline.rule_span.clone(), "no rule with this id");
    match nearest(written, ids).filter(|near| jadpo_diag::find(near).is_some_and(|r| r.declinable)) {
        Some(near) => diagnostic
            .reason(format!(
                "Jadpo has no rule `{written}`, so there is nothing to decline. Did you mean `{near}`?"
            ))
            .next(format!("Use `{near}` instead of `{written}`"))
            .edit(decline.rule_span.clone(), near),
        None => diagnostic
            .reason(format!(
                "Jadpo has no rule `{written}`, so there is nothing to decline. The rules that can be declined are {}.",
                declinable_list()
            ))
            .next("Name the rule as its error's `rule:` line gives it, or remove the `decline` line"),
    }
}

fn reason_required(decline: &Decline) -> Diagnostic {
    let rule = if decline.rule.is_empty() {
        "<rule>"
    } else {
        &decline.rule
    };
    Diagnostic::new(&rule::DECLINE_REASON_REQUIRED, "This decline gives no reason")
        .at(decline.span.clone(), "reason needed")
        .reason(
            "A decline is reviewed like the code it sits on: its reason says why the finding is right to leave, so a reviewer can agree or not.",
        )
        .next(format!(
            "Write why in the quotes: `decline {rule}: \"<why>\"`"
        ))
}
