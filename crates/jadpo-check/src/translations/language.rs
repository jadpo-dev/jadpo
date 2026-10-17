//! A language file (area 10, "Translations as messages"): one
//! `translations { }` in a file named after its locale, whose members word
//! a message, an enum or an email subject. This reads the file by itself, as
//! the members it writes and the strings they hold; what they are
//! compared with is [`super::declared`] and [`super::check`]'s.

use jadpo_db::{Db, File, ItemKind, Project, item_tree, location};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::locale_ordinal;
use jadpo_std::translation::{FieldUse, Template, TemplateError, parse_template};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use crate::tree::{self, node_span};

/// A string in a language file, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// The string with its escapes read.
    pub text: String,
    /// Where it is, quotes included.
    pub span: Span,
    /// For each byte of `text`, the file offset it was read from, and one
    /// more for the end.
    offsets: Vec<u32>,
    /// The wording, or what is wrong with it.
    pub template: Result<(Template, Vec<FieldUse>), TemplateError>,
}

impl Written {
    /// The span in the file of a byte range of the text.
    pub fn range(&self, start: usize, end: usize) -> Span {
        let last = self.offsets.len().saturating_sub(1);
        let from = self.offsets.get(start.min(last)).copied().unwrap_or(0);
        let to = self.offsets.get(end.min(last)).copied().unwrap_or(from);
        Span::new(&self.span.path, from, to.max(from + 1))
    }
}

/// What a member holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// `name: "wording"`.
    Text(Written),
    /// `name: { value: "wording" ... }`: a wording for each value of an
    /// enum, or each plural form.
    Variants(Vec<Variant>),
    /// Neither; reported when the file was read.
    Invalid,
}

/// One entry of a member's block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    pub key_span: Span,
    /// `None` when the value isn't a string, which has been reported.
    pub written: Option<Written>,
}

/// One member of a language file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub key_span: Span,
    /// The whole member, its name and its wording.
    pub span: Span,
    pub body: Body,
}

/// One owner's group in the root language file: `jadpo: { }` holds Jadpo's
/// own messages (area 10, "Jadpo's and packages' text").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub key_span: Span,
    pub members: Vec<Member>,
    /// The group's closing brace: where a member is added.
    pub close: Option<u32>,
}

/// A language file, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageFile {
    pub path: String,
    /// The file name without `.jadpo`: the locale it is for, in the
    /// spelling `Locale.<tag>` uses.
    pub tag: String,
    /// That locale's place among `Locale`'s variants, when the name is one.
    pub locale: Option<u32>,
    /// The `translations` keyword.
    pub keyword: Span,
    pub members: Vec<Member>,
    /// For the root file: the owners' groups, where a folder's file has
    /// members.
    pub groups: Vec<Group>,
    /// The closing brace of `translations { }`: where a member or a group
    /// is added.
    pub close: Option<u32>,
    /// Whether this is the root `translations/` folder's file.
    pub root: bool,
    /// What is wrong with the file by itself.
    pub problems: Vec<Diagnostic>,
}

impl LanguageFile {
    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|member| member.name == name)
    }

    /// The group an owner's wording is in.
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.name == name)
    }
}

impl Group {
    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|member| member.name == name)
    }
}

/// A member's wording as one string, for what a change to it changes.
pub fn flatten(body: &Body) -> String {
    match body {
        Body::Text(written) => written.text.clone(),
        Body::Variants(variants) => variants
            .iter()
            .map(|variant| {
                format!(
                    "{}={}",
                    variant.name,
                    variant.written.as_ref().map_or("", |written| &written.text)
                )
            })
            .collect::<Vec<_>>()
            .join("\u{1f}"),
        Body::Invalid => String::new(),
    }
}

/// Whether `file` holds a language file's `translations { }`.
pub fn is_language_file(db: &dyn Db, file: File) -> bool {
    item_tree(db, file)
        .items
        .iter()
        .any(|item| item.kind == ItemKind::Singleton && item.name == "translations")
}

/// The language file `file` holds, read.
#[salsa::tracked(returns(ref))]
pub fn language_file(db: &dyn Db, project: Project, file: File) -> Option<LanguageFile> {
    let path = file.path(db).clone();
    let items = &item_tree(db, file).items;
    let item = items
        .iter()
        .find(|item| item.kind == ItemKind::Singleton && item.name == "translations")?;
    let source = tree::source(db, file)?;
    let Some(ast::Decl::SingletonDecl(decl)) = tree::decl_at(&source, item.index) else {
        return None;
    };
    let place = location(db, project, file);
    let root = place.place == jadpo_db::Place::RootTranslations;
    let tag = place.stem.clone();
    let keyword = decl.keyword().map_or_else(
        || node_span(&path, decl.syntax()),
        |token| {
            Span::new(
                &path,
                u32::from(token.text_range().start()),
                u32::from(token.text_range().end()),
            )
        },
    );
    let mut out = LanguageFile {
        locale: locale_ordinal(&tag),
        tag,
        keyword,
        members: Vec::new(),
        groups: Vec::new(),
        close: decl.member_list().and_then(|list| closing_brace(&list)),
        root,
        problems: Vec::new(),
        path,
    };
    let Some(list) = decl.member_list() else {
        return Some(out);
    };
    for item in list.member_items() {
        match item {
            MemberItem::Member(member) if out.root => read_group(&mut out, &member),
            MemberItem::Member(member) => read_member(&mut out, &member),
            MemberItem::BareMember(bare) => {
                let span = node_span(&out.path, bare.syntax());
                out.problems.push(shape(
                    &span,
                    "no wording",
                    "A member of `translations { }` is a name and its wording, `notice_text: \"...\"`.",
                    "Write the wording after a colon",
                ));
            }
            MemberItem::Elision(_) => {}
            other => {
                let span = node_span(&out.path, other.syntax());
                out.problems.push(shape(
                    &span,
                    "not a member",
                    "`translations { }` holds one member for each message, enum or email it words, and nothing else.",
                    "Remove it, or move it out of the language file",
                ));
            }
        }
    }
    Some(out)
}

/// A node's span without the comments around it, so replacing the member
/// keeps them.
fn core_span(path: &str, node: &jadpo_syntax::SyntaxNode) -> Span {
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let first = tokens.next();
    let last = tokens.last().or_else(|| first.clone());
    match (first, last) {
        (Some(first), Some(last)) => Span::new(
            path,
            u32::from(first.text_range().start()),
            u32::from(last.text_range().end()),
        ),
        _ => node_span(path, node),
    }
}

/// The offset of a block's closing brace.
fn closing_brace(list: &ast::MemberList) -> Option<u32> {
    list.syntax()
        .children_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == jadpo_syntax::SyntaxKind::R_BRACE)
        .last()
        .map(|token| u32::from(token.text_range().start()))
}

fn shape(span: &Span, label: &str, reason: &str, next: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TRANSLATION_WORDING_SHAPE,
        "A member of `translations { }` isn't a name with its wording",
    )
    .at(span.clone(), label)
    .reason(reason)
    .next(next)
}

/// A member of the root file: an owner's group of wordings.
fn read_group(out: &mut LanguageFile, member: &ast::Member) {
    let path = out.path.clone();
    let Some(key) = member.member_key() else {
        return;
    };
    let key_span = node_span(&path, key.syntax());
    let mut names = key.names();
    let (Some(first), None) = (names.next(), names.next()) else {
        out.problems.push(shape(
            &key_span,
            "a dotted name",
            "The root file groups by whose text it is: `jadpo: { }` for Jadpo's own messages, and a library package's name for a package's. A group is one name.",
            "Write the group's name alone",
        ));
        return;
    };
    let Some(name) = tree::name_text(&first) else {
        return;
    };
    let key_span = node_span(&path, first.syntax());
    let Some(block) = member.block().filter(|_| member.value().is_none()) else {
        out.problems.push(shape(
            &key_span,
            "not in a group",
            "The root file words text the app did not write, and groups it by whose text it is: `jadpo: { name: \"...\" }` for Jadpo's own messages, and a library package's name for a package's.",
            &format!("Move `{name}` into `jadpo: {{ }}`, or the package's group"),
        ));
        return;
    };
    let mut scratch = LanguageFile {
        path: path.clone(),
        tag: out.tag.clone(),
        locale: out.locale,
        keyword: out.keyword.clone(),
        members: Vec::new(),
        groups: Vec::new(),
        close: None,
        root: false,
        problems: Vec::new(),
    };
    for item in block.member_items() {
        match item {
            MemberItem::Member(inner) => read_member(&mut scratch, &inner),
            MemberItem::BareMember(bare) => scratch.problems.push(shape(
                &node_span(&path, bare.syntax()),
                "no wording",
                "A member of a group is a name and its wording, `notice_text: \"...\"`.",
                "Write the wording after a colon",
            )),
            MemberItem::Elision(_) => {}
            other => scratch.problems.push(shape(
                &node_span(&path, other.syntax()),
                "not a member",
                "A group holds one member for each message it words, and nothing else.",
                "Remove it",
            )),
        }
    }
    out.problems.append(&mut scratch.problems);
    out.groups.push(Group {
        name,
        key_span,
        members: scratch.members,
        close: closing_brace(&block),
    });
}

fn read_member(out: &mut LanguageFile, member: &ast::Member) {
    let path = out.path.clone();
    let Some(key) = member.member_key() else {
        return;
    };
    let key_span = node_span(&path, key.syntax());
    let mut names = key.names();
    let (Some(first), None) = (names.next(), names.next()) else {
        out.problems.push(shape(
            &key_span,
            "a dotted name",
            "A language file has no grouping: the domain is the group, so a member is one name. (Only the root `translations/` folder's files group, by whose text it is.)",
            "Write the member's name alone",
        ));
        return;
    };
    let Some(name) = tree::name_text(&first) else {
        return;
    };
    let key_span = node_span(&path, first.syntax());
    let body = match (member.value(), member.block()) {
        (Some(value), None) => match written(&path, &value) {
            Some(text) => Body::Text(text),
            None => {
                out.problems.push(shape(
                    &node_span(&path, value.syntax()),
                    "not a string",
                    "A wording is a plain string, `\"...\"`, or a block of strings with one for each value of an enum or each plural form.",
                    "Write the wording as a string",
                ));
                Body::Invalid
            }
        },
        (None, Some(block)) => Body::Variants(read_variants(out, &name, &block)),
        (Some(value), Some(_)) => {
            out.problems.push(shape(
                &node_span(&path, value.syntax()),
                "a string and a block",
                "A member is one wording or a block of wordings for the values of an enum or the plural forms, never both.",
                "Keep the string, or the block",
            ));
            Body::Invalid
        }
        (None, None) => {
            out.problems.push(shape(
                &key_span,
                "no wording",
                "A member is a name and its wording, `notice_text: \"...\"`.",
                "Write the wording after the colon",
            ));
            Body::Invalid
        }
    };
    out.members.push(Member {
        name,
        key_span,
        span: core_span(&path, member.syntax()),
        body,
    });
}

fn read_variants(out: &mut LanguageFile, host: &str, block: &ast::MemberList) -> Vec<Variant> {
    let path = out.path.clone();
    let mut variants: Vec<Variant> = Vec::new();
    for item in block.member_items() {
        match item {
            MemberItem::Member(entry) => {
                let Some(key) = entry.member_key() else {
                    continue;
                };
                let mut names = key.names();
                let (Some(first), None) = (names.next(), names.next()) else {
                    out.problems.push(shape(
                        &node_span(&path, key.syntax()),
                        "a dotted name",
                        "Inside a member there is only a name for each value of the enum or each plural form.",
                        "Write the value's name alone",
                    ));
                    continue;
                };
                let Some(name) = tree::name_text(&first) else {
                    continue;
                };
                let key_span = node_span(&path, first.syntax());
                match (entry.value(), entry.block()) {
                    (Some(value), None) => {
                        let text = written(&path, &value);
                        if text.is_none() {
                            out.problems.push(shape(
                                &node_span(&path, value.syntax()),
                                "not a string",
                                "A wording is a plain string, `\"...\"`.",
                                "Write the wording as a string",
                            ));
                        }
                        variants.push(Variant {
                            name,
                            key_span,
                            written: text,
                        });
                    }
                    (_, Some(inner)) => {
                        out.problems.push(shape(
                            &node_span(&path, inner.syntax()),
                            "nested again",
                            &format!(
                                "`{host}` holds one level of wordings, one for each value of an enum or each plural form; nesting means variants and plural forms only."
                            ),
                            "Write each value's wording as a string",
                        ));
                        variants.push(Variant {
                            name,
                            key_span,
                            written: None,
                        });
                    }
                    (None, None) => {
                        out.problems.push(shape(
                            &key_span,
                            "no wording",
                            "Each value of the enum or plural form is a name and its wording.",
                            "Write the wording after the colon",
                        ));
                        variants.push(Variant {
                            name,
                            key_span,
                            written: None,
                        });
                    }
                }
            }
            MemberItem::BareMember(bare) => out.problems.push(shape(
                &node_span(&path, bare.syntax()),
                "no wording",
                "Each value of the enum or plural form is a name and its wording.",
                "Write the wording after a colon",
            )),
            _ => {}
        }
    }
    variants
}

/// A string literal, read as a wording.
fn written(path: &str, expr: &Expr) -> Option<Written> {
    let Expr::Literal(literal) = expr else {
        return None;
    };
    let token = literal.value()?;
    if token.kind() != jadpo_syntax::SyntaxKind::STRING {
        return None;
    }
    let start = u32::from(token.text_range().start());
    let end = u32::from(token.text_range().end());
    let (text, offsets) = decode(token.text(), start);
    let template = parse_template(&text);
    Some(Written {
        template,
        text,
        span: Span::new(path, start, end),
        offsets,
    })
}

/// The text of a string token between its quotes with its escapes read, and
/// the file offset each byte of it came from (one more at the end).
fn decode(raw: &str, base: u32) -> (String, Vec<u32>) {
    let closed = raw.len() >= 2 && raw.ends_with('"');
    let inner_end = if closed { raw.len() - 1 } else { raw.len() };
    let inner = raw.get(1..inner_end).unwrap_or("");
    let mut text = String::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut chars = inner.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let from = base + 1 + at as u32;
        let decoded = if c == '\\' {
            match chars.next() {
                Some((_, 'n')) => '\n',
                Some((_, 'r')) => '\r',
                Some((_, 't')) => '\t',
                Some((_, other)) => other,
                None => break,
            }
        } else {
            c
        };
        text.push(decoded);
        offsets.extend(std::iter::repeat_n(from, decoded.len_utf8()));
    }
    offsets.push(base + 1 + inner.len() as u32);
    (text, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_keep_their_offsets() {
        let (text, offsets) = decode(r#""a\"b\n{c}""#, 10);
        assert_eq!(text, "a\"b\n{c}");
        // `a` at 11, the quote's backslash at 12, `b` at 14, the newline's
        // backslash at 15, `{` at 17.
        assert_eq!(offsets[0], 11);
        assert_eq!(offsets[1], 12);
        assert_eq!(offsets[2], 14);
        assert_eq!(offsets[3], 15);
        assert_eq!(offsets[4], 17);
        assert_eq!(*offsets.last().unwrap_or(&0), 20);
    }
}
