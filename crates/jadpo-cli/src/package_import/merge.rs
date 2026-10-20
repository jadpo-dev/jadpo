//! Importing again merges rather than overwrites (area 11, "An importer";
//! chunk 7.2): corrections made to an imported file survive.
//!
//! A file is merged one declaration at a time and, inside one, one member
//! at a time: a declaration or member the description has and the file
//! doesn't is added; one both have is kept as the file has it, since a
//! person or an agent may have corrected it (an operation's `changes:`, a
//! field's type); one the file has and the description doesn't is kept,
//! and listed, for a person to decide. Only `provider.jadpo`'s `imported:`,
//! which records what was read, is always the description's.

use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

/// A file merged with what an import made of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Merged {
    pub text: String,
    /// What the import added: `todos.archive`, `TodoView.note`.
    pub added: Vec<String>,
    /// What the file has that the description doesn't.
    pub kept: Vec<String>,
}

/// One declaration of a file: its key, its member list's items by key,
/// and where it and they are.
struct Declared {
    key: String,
    /// From the end of what comes before it, so its `///` lines come too.
    text: String,
    members: Vec<Member>,
    /// Where its member list's `}` is.
    close: Option<usize>,
}

struct Member {
    key: String,
    text: String,
    /// Its own range, without what comes before it.
    range: (usize, usize),
}

/// `existing` merged with `imported`; `None` when `existing` doesn't read,
/// so it is left alone.
pub fn merge(existing: &str, imported: &str) -> Option<Merged> {
    let before = declarations(existing)?;
    let after = declarations(imported)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut out = Merged::default();
    let mut appended = String::new();
    for new in &after {
        let Some(old) = before.iter().find(|old| old.key == new.key) else {
            appended.push_str(&new.text);
            appended.push('\n');
            out.added.push(shown(&new.key));
            continue;
        };
        let Some(close) = old.close else {
            continue;
        };
        let mut inserted = String::new();
        for member in &new.members {
            match old.members.iter().find(|known| known.key == member.key) {
                None => {
                    inserted.push_str(&member.text);
                    inserted.push('\n');
                    out.added
                        .push(format!("{}.{}", shown(&new.key), member.key));
                }
                // What was read is always the description's.
                Some(known) if new.key == "provider" && member.key == "imported" => {
                    let own = imported_text(imported, member);
                    edits.push((known.range.0, known.range.1, own));
                }
                Some(_) => {}
            }
        }
        if !inserted.is_empty() {
            let after_newline = existing[..close].ends_with('\n');
            let lead = if after_newline { "" } else { "\n" };
            edits.push((close, close, format!("{lead}{inserted}")));
        }
        for member in &old.members {
            if !new.members.iter().any(|found| found.key == member.key) {
                out.kept.push(format!("{}.{}", shown(&old.key), member.key));
            }
        }
    }
    for old in &before {
        if !after.iter().any(|new| new.key == old.key) {
            out.kept.push(shown(&old.key));
        }
    }
    let mut text = existing.to_string();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    for (start, end, insert) in edits {
        text.replace_range(start..end, &insert);
    }
    if !appended.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
        text.push_str(&appended);
    }
    out.text = jadpo_fmt::format_file(&text).unwrap_or(text);
    Some(out)
}

/// A member's own text, without what comes before it.
fn imported_text(text: &str, member: &Member) -> String {
    text[member.range.0..member.range.1].to_string()
}

/// `type TodoView` as `TodoView`, `resource todos` as `todos`.
fn shown(key: &str) -> String {
    key.split_once(' ')
        .map_or(key, |(_, name)| name)
        .to_string()
}

fn declarations(text: &str) -> Option<Vec<Declared>> {
    let parse = jadpo_syntax::parse_file(text);
    if parse.has_errors() {
        return None;
    }
    let file = ast::SourceFile::cast(parse.syntax())?;
    let mut out = Vec::new();
    let mut from = 0usize;
    for decl in file.decls() {
        let node = decl.syntax().clone();
        let end = usize::from(node.text_range().end());
        let declared_text = text[from..end].trim_start_matches('\n').to_string();
        from = end;
        let Some(key) = key_of(&decl) else {
            continue;
        };
        let list = node.children().find_map(ast::MemberList::cast);
        let (members, close) = match &list {
            Some(list) => (members(text, list), close_of(list.syntax())),
            None => (Vec::new(), None),
        };
        out.push(Declared {
            key,
            text: declared_text,
            members,
            close,
        });
    }
    Some(out)
}

fn key_of(decl: &Decl) -> Option<String> {
    let name = |node: &SyntaxNode| {
        node.children()
            .find_map(ast::Name::cast)
            .and_then(|name| name.ident_token())
            .map(|token| token.text().to_string())
    };
    match decl {
        Decl::TypeDecl(decl) => Some(format!("type {}", name(decl.syntax())?)),
        Decl::ResourceDecl(decl) => Some(format!("resource {}", name(decl.syntax())?)),
        Decl::PackageDecl(_) => Some("package".to_string()),
        Decl::SingletonDecl(decl) => decl.keyword().map(|token| token.text().to_string()),
        _ => None,
    }
}

fn members(text: &str, list: &ast::MemberList) -> Vec<Member> {
    let mut out = Vec::new();
    let mut from = list
        .syntax()
        .first_token()
        .map_or(0, |token| usize::from(token.text_range().end()));
    for item in list.member_items() {
        let range = item.syntax().text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        let key = match &item {
            MemberItem::Member(member) => member.member_key().map(|key| {
                key.names()
                    .filter_map(|name| name.ident_token())
                    .map(|token| token.text().to_string())
                    .collect::<Vec<_>>()
                    .join(".")
            }),
            MemberItem::BareMember(member) => member
                .syntax()
                .children()
                .find_map(ast::Name::cast)
                .and_then(|name| name.ident_token())
                .map(|token| token.text().to_string()),
            _ => None,
        };
        let with_docs = text[from..end].trim_start_matches(['\n', ' ']).to_string();
        from = end;
        if let Some(key) = key {
            out.push(Member {
                key,
                text: format!("    {with_docs}"),
                range: (start, end),
            });
        }
    }
    out
}

/// Where a member list's closing `}` starts.
fn close_of(list: &SyntaxNode) -> Option<usize> {
    list.children_with_tokens()
        .filter_map(|element| element.into_token())
        .last()
        .filter(|token| token.kind() == SyntaxKind::R_BRACE)
        .map(|token| usize::from(token.text_range().start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrections_survive_and_new_members_are_added() {
        let existing = "/// A todo.\npublic type TodoView = Object {\n    id: Uuid\n    title: Text {\n        max_length: 100\n    }\n    owner: Text\n}\n";
        let imported = "/// A todo.\npublic type TodoView = Object {\n    id: Uuid\n    title: Text {\n        max_length: 200\n    }\n    /// When it was done.\n    done_at: Instant?\n}\n\npublic type Tag = Object {\n    name: Text\n}\n";
        let merged = merge(existing, imported).expect("both read");
        assert_eq!(
            merged.text,
            "/// A todo.\npublic type TodoView = Object {\n    id: Uuid\n    title: Text {\n        max_length: 100\n    }\n    owner: Text\n    /// When it was done.\n    done_at: Instant?\n}\n\npublic type Tag = Object {\n    name: Text\n}\n"
        );
        assert_eq!(merged.added, vec!["TodoView.done_at", "Tag"]);
        assert_eq!(merged.kept, vec!["TodoView.owner"]);
    }

    #[test]
    fn what_was_read_is_always_the_descriptions() {
        let existing = "provider {\n    protocol: Protocol.json_http\n    credential: CredentialScheme.basic\n    imported: {\n        file: \"a.json\"\n        digest: \"sha256:1\"\n    }\n}\n";
        let imported = "provider {\n    protocol: Protocol.json_http\n    credential: CredentialScheme.bearer\n    imported: {\n        file: \"a.json\"\n        digest: \"sha256:2\"\n    }\n}\n";
        let merged = merge(existing, imported).expect("both read");
        assert!(
            merged.text.contains("CredentialScheme.basic"),
            "{}",
            merged.text
        );
        assert!(merged.text.contains("sha256:2"), "{}", merged.text);
        assert!(merged.added.is_empty());
    }
}
