//! Prompts as files (area 21, "Prompts as files"): a prompt is a `type X =
//! Prompt { ... }` in a domain's `prompts/` folder (or `shared/prompts/`;
//! anywhere in loose files), its words in `<name>.prompt` beside it, in two
//! sections, `# Instructions` and `# Request`.
//!
//! [`analysis`] reads every prompt once: its fields and how each is
//! written (a closed value in its text form, free text or JSON as a data
//! block, or the conversation's turns), its file, the problems between the
//! two, and the entry the runtime renders it from, with its digest. The
//! body check reads only a value's type to know `prompts.render` takes it,
//! so editing a prompt's words checks no body again.

use jadpo_db::{Base, Db, ItemKind, Layout, PROMPT_EXTENSION, Project, Symbol, symbols};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::prompt::{self, Form, Piece, Problem, PromptEntry, PromptField, PromptPlan, SectionKind};
use jadpo_syntax::ast::{AstNode, Decl};
use sha2::{Digest, Sha256};

use crate::case::to_snake;
use crate::lookups::{Lookup, Name, file_at};
use crate::tree::{self, node_span};
use crate::types::{Def, FieldDef, Prim, Ty, TypeView};

/// The namespace `prompts.render` is called through.
pub const PROMPTS: &str = "prompts";

/// The standard type a conversation's messages are.
const CHAT_MESSAGE: &str = jadpo_std::standard::ai::CHAT_MESSAGE;

/// How a prompt's field is sent.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sent {
    /// Where its placeholder stands.
    Written(Form),
    /// Its messages are the turns before the request.
    Conversation,
    /// It can't be sent; reported at the field.
    Refused,
}

/// One prompt, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptFacts {
    /// The type's name, `AnswerQuestion`.
    pub type_name: String,
    /// Its name in lower_snake_case, `answer_question`: how the manifest,
    /// `jadpo ai show` and a fake name it.
    pub key: String,
    /// The `.jadpo` file declaring it.
    pub path: String,
    /// Where its name is written.
    pub name_span: Span,
    /// Its `.prompt` file's path.
    pub text_path: String,
    /// What the runtime renders it from, when it and its file check.
    pub entry: Option<PromptEntry>,
    /// The first 8 hex characters of a SHA-256 over its two files, when
    /// its text can be read.
    pub digest: Option<String>,
    /// Who owns it: its domain, `shared/` or the loose files.
    pub ownership: jadpo_db::Ownership,
    /// Its `///` description.
    pub description: Option<String>,
}

/// Every prompt in the project and what is wrong with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    pub prompts: Vec<PromptFacts>,
    pub problems: Vec<Diagnostic>,
}

impl Analysis {
    /// The prompt called `key` (`answer_question`).
    pub fn prompt(&self, key: &str) -> Option<&PromptFacts> {
        self.prompts.iter().find(|facts| facts.key == key)
    }
}

/// Whether a symbol is a prompt's type.
fn is_prompt(symbol: &Symbol) -> bool {
    symbol.kind == ItemKind::Type(Base::Prompt)
        && !symbol.test
        && symbol.library.is_none()
        && symbol.provided.is_none()
}

/// The folder a path is in, with its `/`, or nothing at the root.
fn folder_of(path: &str) -> &str {
    path.rfind('/')
        .and_then(|at| path.get(..=at))
        .unwrap_or_default()
}

/// The `.prompt` file beside a prompt type's file: named after the type.
pub fn text_path(type_path: &str, type_name: &str) -> String {
    format!(
        "{}{}{PROMPT_EXTENSION}",
        folder_of(type_path),
        to_snake(type_name)
    )
}

/// Every prompt and the problems in them and their files.
#[salsa::tracked(returns(ref))]
pub fn analysis(db: &dyn Db, project: Project) -> Analysis {
    let mut out = Analysis::default();
    let mut found: Vec<&Symbol> = symbols(db, project)
        .declarations
        .values()
        .flatten()
        .filter(|symbol| is_prompt(symbol))
        .collect();
    found.sort_by(|a, b| (&a.path, a.index).cmp(&(&b.path, b.index)));
    let records = project.records(db);
    let text_of = |path: &str| {
        records
            .iter()
            .find(|record| record.path == path)
            .map(|record| record.text.as_ref().map(String::as_str))
    };
    let mut claimed: Vec<String> = Vec::new();
    for symbol in found {
        // A name declared twice is the naming rules' to report; the first
        // is kept.
        let key = to_snake(&symbol.name);
        if out.prompts.iter().any(|seen| seen.key == key) {
            continue;
        }
        let Some(file) = file_at(db, project, Name::new(db, symbol.path.clone())) else {
            continue;
        };
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let Some(Decl::TypeDecl(decl)) = tree::decl_at(&source, symbol.index) else {
            continue;
        };
        let name_span = decl.name().map_or_else(
            || node_span(&symbol.path, decl.syntax()),
            |name| node_span(&symbol.path, name.syntax()),
        );
        let field_spans: Vec<(String, Span)> = tree::members(decl.member_list())
            .into_iter()
            .filter_map(|(name, _)| {
                Some((tree::name_text(&name)?, node_span(&symbol.path, name.syntax())))
            })
            .collect();
        let lookup = Lookup::within(db, project, &symbol.ownership);
        let canonical = lookup.canonical_name(&symbol.name);
        let fields: Vec<FieldDef> = match lookup.def(&canonical) {
            Some(Def::Fields { fields, .. }) => fields.clone(),
            _ => Vec::new(),
        };
        let path = text_path(&symbol.path, &symbol.name);
        claimed.push(path.clone());
        let mut reading = Reading {
            type_name: &symbol.name,
            text_path: &path,
            name_span: name_span.clone(),
            field_spans: &field_spans,
            problems: Vec::new(),
        };
        let sent = reading.fields(&lookup, &fields);
        let digest = match text_of(&path) {
            Some(Ok(text)) => Some(digest(file.text(db), text)),
            _ => None,
        };
        let entry = match text_of(&path) {
            Some(Ok(text)) => reading.text(text, &sent).map(|(instructions, request)| {
                let conversation = sent.iter().find_map(|(name, how)| {
                    (*how == Sent::Conversation).then(|| name.clone())
                });
                PromptEntry {
                    key: key.clone(),
                    type_name: canonical.clone(),
                    digest: digest.clone().unwrap_or_default(),
                    instructions,
                    request,
                    fields: sent
                        .iter()
                        .filter_map(|(name, how)| match how {
                            Sent::Written(form) => Some(PromptField {
                                name: name.clone(),
                                form: *form,
                            }),
                            _ => None,
                        })
                        .collect(),
                    conversation,
                }
            }),
            // A file that can't be read is the loader's to report.
            Some(Err(_)) => None,
            None => {
                reading.problems.push(no_file(&symbol.name, &path, name_span.clone()));
                None
            }
        };
        let entry = entry.filter(|_| reading.problems.is_empty());
        out.problems.append(&mut reading.problems);
        out.prompts.push(PromptFacts {
            type_name: canonical,
            key,
            path: symbol.path.clone(),
            name_span,
            text_path: path,
            entry,
            digest,
            ownership: symbol.ownership.clone(),
            description: crate::graph::text::doc(decl.syntax()),
        });
    }
    // A prompt's text with no type beside it.
    let loose = project.layout(db) == Layout::Loose;
    for record in records {
        let path = &record.path;
        let is_text = if loose {
            path.ends_with(PROMPT_EXTENSION)
        } else {
            jadpo_db::is_prompt_text(path)
        };
        if is_text && !claimed.contains(path) {
            out.problems.push(no_type(path, record.text.as_deref().unwrap_or("")));
        }
    }
    out.prompts.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// The problems in the project's prompts.
pub fn prompt_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &analysis(db, project).problems
}

/// The plan the runtime renders prompts from: every prompt that checks.
pub fn plan(db: &dyn Db, project: Project) -> PromptPlan {
    PromptPlan {
        entries: analysis(db, project)
            .prompts
            .iter()
            .filter_map(|facts| facts.entry.clone())
            .collect(),
    }
}

/// The first 8 hex characters of a SHA-256 over a prompt's two files, its
/// type's then its text's, each after its length in bytes as 8 bytes,
/// little-endian, so text moved from one to the other changes it.
pub fn digest(type_text: &str, prompt_text: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [type_text, prompt_text] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `name` is a prompt's type, as a reader of `ty` sees it, and
/// which prompt: its key.
pub fn prompt_of(table: &dyn TypeView, ty: &Ty) -> Option<String> {
    let Ty::Named(name) = ty else {
        return None;
    };
    match table.def(name)? {
        Def::Fields {
            base: crate::types::FieldsBase::Prompt,
            ..
        } => {
            let own = name.rsplit('.').next().unwrap_or(name);
            Some(to_snake(own))
        }
        _ => None,
    }
}

/// One prompt being read, with the problems found so far.
struct Reading<'a> {
    type_name: &'a str,
    text_path: &'a str,
    name_span: Span,
    field_spans: &'a [(String, Span)],
    problems: Vec<Diagnostic>,
}

impl Reading<'_> {
    fn field_span(&self, name: &str) -> Span {
        self.field_spans
            .iter()
            .find(|(seen, _)| seen == name)
            .map_or_else(|| self.name_span.clone(), |(_, span)| span.clone())
    }

    fn file_name(&self) -> &str {
        self.text_path.rsplit('/').next().unwrap_or(self.text_path)
    }

    /// How each field is sent, reporting those that can't be.
    fn fields(&mut self, lookup: &Lookup<'_>, fields: &[FieldDef]) -> Vec<(String, Sent)> {
        let mut out: Vec<(String, Sent)> = Vec::new();
        let mut conversation: Option<String> = None;
        for field in fields {
            let how = match sent(lookup, &field.declared) {
                Ok(Sent::Conversation) => {
                    if let Some(first) = &conversation {
                        self.problems.push(second_conversation(
                            self.type_name,
                            first,
                            &field.name,
                            self.field_span(first),
                            self.field_span(&field.name),
                        ));
                        Sent::Refused
                    } else {
                        conversation = Some(field.name.clone());
                        Sent::Conversation
                    }
                }
                Ok(how) => how,
                Err(why) => {
                    self.problems.push(refused(
                        self.type_name,
                        &field.name,
                        &field.declared,
                        why,
                        self.field_span(&field.name),
                    ));
                    Sent::Refused
                }
            };
            out.push((field.name.clone(), how));
        }
        out
    }

    /// The file's two sections, checked against the fields.
    fn text(&mut self, text: &str, sent: &[(String, Sent)]) -> Option<(Vec<Piece>, Vec<Piece>)> {
        let file = prompt::parse(text);
        let before = self.problems.len();
        let mut unknown = false;
        let mut structure = false;
        // One error for a file with neither section.
        let both_missing = SectionKind::ALL
            .iter()
            .all(|kind| file.problems.contains(&Problem::Missing(*kind)));
        for problem in &file.problems {
            match problem {
                Problem::Unclosed { .. } | Problem::Lone { .. } => {
                    unknown = true;
                    self.problems.push(brace(self.text_path, text, problem));
                }
                Problem::Missing(SectionKind::Request) if both_missing => {}
                _ => {
                    structure = true;
                    self.problems.push(section(self.text_path, text, &file, problem, both_missing));
                }
            }
        }
        let mut used: Vec<&str> = Vec::new();
        for kind in SectionKind::ALL {
            let Some(section) = file.section(kind) else {
                continue;
            };
            for (name, start, end) in section.fields() {
                let span = Span::new(self.text_path, start, end);
                match sent.iter().find(|(field, _)| field == name).map(|(_, how)| how) {
                    None => {
                        unknown = true;
                        self.problems.push(self.unknown(name, span, sent));
                    }
                    Some(Sent::Conversation) => {
                        used.push(name);
                        self.problems.push(self.conversation_written(name, span));
                    }
                    Some(Sent::Refused) => used.push(name),
                    Some(Sent::Written(form)) => {
                        used.push(name);
                        if kind == SectionKind::Instructions && form.is_free() {
                            self.problems.push(self.free_text(name, *form, span));
                        }
                    }
                }
            }
        }
        // A field no placeholder writes, once every placeholder names a
        // field and the file has both its sections: otherwise a misspelt
        // or missing placeholder is the one mistake.
        if !unknown && !structure {
            for (name, how) in sent {
                if matches!(how, Sent::Written(_)) && !used.contains(&name.as_str()) {
                    let diagnostic = self.unused(name);
                    self.problems.push(diagnostic);
                }
            }
        }
        if self.problems.len() > before {
            return None;
        }
        Some((
            file.instructions.map(|s| s.pieces).unwrap_or_default(),
            file.request.map(|s| s.pieces).unwrap_or_default(),
        ))
    }

    fn unknown(&self, name: &str, span: Span, sent: &[(String, Sent)]) -> Diagnostic {
        let written: Vec<&str> = sent
            .iter()
            .filter(|(_, how)| matches!(how, Sent::Written(_)))
            .map(|(name, _)| name.as_str())
            .collect();
        let placeholders: Vec<String> = written.iter().map(|name| format!("`{{{name}}}`")).collect();
        let may = if placeholders.is_empty() {
            "it has no field its text can write".to_string()
        } else {
            format!("its text may write {}", crate::and_list(&placeholders))
        };
        let nearest = jadpo_diag::nearest(name.trim(), written.iter().copied());
        let next = match nearest {
            Some(near) => format!(
                "Write `{{{near}}}`, or add the field to `{}`",
                self.type_name
            ),
            None => format!(
                "Remove `{{{name}}}`, or add the field to `{}`",
                self.type_name
            ),
        };
        Diagnostic::new(
            &rule::PROMPT_PLACEHOLDER_UNKNOWN,
            format!("`{{{name}}}` isn't a field of `{}`", self.type_name),
        )
        .at(span, "no such field")
        .reason(format!(
            "A prompt's placeholders are its type's fields, written `{{field}}`, as a translation's are. `{}` has no field `{name}`; {may}.",
            self.type_name
        ))
        .next(next)
    }

    fn unused(&self, name: &str) -> Diagnostic {
        Diagnostic::new(
            &rule::PROMPT_PLACEHOLDER_UNUSED,
            format!(
                "`{}.{name}` is written nowhere in `{}`",
                self.type_name,
                self.file_name()
            ),
        )
        .at(self.field_span(name), "never written")
        .reason(format!(
            "Each field of a prompt is a placeholder its text writes, so a model is sent only what a reviewer reads in `{}`.",
            self.file_name()
        ))
        .next(format!(
            "Write `{{{name}}}` in `{}`, or remove the field",
            self.file_name()
        ))
    }

    fn free_text(&self, name: &str, form: Form, span: Span) -> Diagnostic {
        let what = match form {
            Form::Json => "an object or a list, sent as JSON,",
            _ => "free text,",
        };
        Diagnostic::new(
            &rule::PROMPT_FREE_TEXT_IN_INSTRUCTIONS,
            format!(
                "`{{{name}}}` is free text in the instructions of `{}`",
                self.type_name
            ),
        )
        .at(span, "free text")
        .reason(format!(
            "`{name}` is {what} which can carry instructions of its own. Free text is only ever sent as data, under `# Request`, inside a block tagged `<{name}>` that it can't close, so it can't pass itself off as the application's instructions. A closed value (an enum, a number, a date, a `Locale`) may be written in the instructions."
        ))
        .next(format!("Move `{{{name}}}` under `# Request`"))
    }

    fn conversation_written(&self, name: &str, span: Span) -> Diagnostic {
        Diagnostic::new(
            &rule::PROMPT_PLACEHOLDER_TYPE,
            format!(
                "`{{{name}}}` is the conversation of `{}`, written in its text",
                self.type_name
            ),
        )
        .at(span, "the conversation")
        .reason(format!(
            "A `List<ChatMessage>` field is the conversation: its messages are sent as the turns before the request, in order, never as text. `{name}` is sent that way without a placeholder."
        ))
        .next(format!("Remove `{{{name}}}`"))
    }
}

/// Why a field can't be sent to a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    Secret(&'static str),
    Binary,
    Optional,
}

/// How a field of type `ty` is sent, or why it can't be.
fn sent(lookup: &Lookup<'_>, ty: &Ty) -> Result<Sent, Why> {
    if let Some(secret) = crate::types::secrets::held_secret(lookup, ty, &mut Vec::new()) {
        return Err(Why::Secret(secret));
    }
    if matches!(ty, Ty::Nullable(_)) || lookup.is_nullable(ty) {
        return Err(Why::Optional);
    }
    if binary(lookup, ty, 0) {
        return Err(Why::Binary);
    }
    let structure = lookup.structure(ty);
    if let Ty::List(item) = &structure
        && matches!(item.as_ref(), Ty::Named(name) | Ty::Opaque(name, _) if name == CHAT_MESSAGE)
    {
        return Ok(Sent::Conversation);
    }
    if closed(lookup, ty) {
        return Ok(Sent::Written(Form::Closed));
    }
    match lookup.representation(ty) {
        // Text, and the types held as text (an email address, a URL,
        // `Markdown`), written as it is.
        Some(Prim::Text | Prim::PresentationText | Prim::Email | Prim::Url | Prim::Uuid) => {
            Ok(Sent::Written(Form::Text))
        }
        _ => Ok(Sent::Written(Form::Json)),
    }
}

/// Whether a value of `ty` is closed: it can't carry an instruction, so it
/// may be written in the instructions in its text form. An enum whose
/// values carry no data, a whole number or a decimal, a `Bool`, a date or
/// an instant (area 21: "an enum, a number, a date, a `Locale`, a
/// `Country`").
fn closed(lookup: &Lookup<'_>, ty: &Ty) -> bool {
    if let Ty::Named(name) = lookup.structure(ty)
        && let Some(Def::Enum { variants }) = lookup.def(&name)
    {
        return variants.iter().all(|variant| variant.fields.is_none());
    }
    matches!(
        lookup.representation(ty),
        Some(
            Prim::Bool
                | Prim::Int
                | Prim::Decimal
                | Prim::CalendarDate
                | Prim::Instant
                | Prim::ZonedTime
        )
    )
}

/// Whether a value of `ty` is or holds bytes or a file, which a prompt
/// can't send as text.
fn binary(lookup: &Lookup<'_>, ty: &Ty, depth: u32) -> bool {
    if depth > 8 {
        return false;
    }
    if lookup.base_prim(ty) == Some(Prim::Bytes) {
        return true;
    }
    match lookup.structure(ty) {
        Ty::Opaque(name, _) => name == "File",
        Ty::List(item) | Ty::Set(item) | Ty::Nullable(item) => binary(lookup, &item, depth + 1),
        Ty::Map(key, value) => {
            binary(lookup, &key, depth + 1) || binary(lookup, &value, depth + 1)
        }
        _ => false,
    }
}

fn refused(type_name: &str, field: &str, declared: &Ty, why: Why, span: Span) -> Diagnostic {
    let ty = declared.to_string();
    let (message, label, reason, next) = match why {
        Why::Secret(secret) => (
            format!("`{type_name}.{field}` holds a `{secret}`"),
            "a secret",
            format!(
                "`{secret}` is a secret value: it is never sent, logged, stored or shown, and a model's provider keeps what it is sent."
            ),
            format!("Remove `{field}`"),
        ),
        Why::Binary => (
            format!("`{type_name}.{field}` is `{ty}`, which a prompt can't send"),
            "not text",
            "A prompt is text. Images, audio and files are sent by a provider package's own operations, not in a prompt.".to_string(),
            format!("Remove `{field}`, or send what it holds as text"),
        ),
        Why::Optional => (
            format!("`{type_name}.{field}` may be `none`"),
            "optional",
            "Every placeholder of a prompt is written each time it is sent, and its text has no conditions, so a field can't be missing.".to_string(),
            format!(
                "Make `{field}` required, and send an empty text or another prompt where there is nothing"
            ),
        ),
    };
    Diagnostic::new(&rule::PROMPT_PLACEHOLDER_TYPE, message)
        .at(span, label)
        .reason(reason)
        .next(next)
}

fn second_conversation(
    type_name: &str,
    first: &str,
    second: &str,
    first_at: Span,
    at: Span,
) -> Diagnostic {
    Diagnostic::new(
        &rule::PROMPT_PLACEHOLDER_TYPE,
        format!("`{type_name}` has a second conversation, `{second}`"),
    )
    .at(at, "a second conversation")
    .also(first_at, "the first")
    .reason(format!(
        "A `List<ChatMessage>` field is the conversation: its messages are sent as the turns before the request, in order. A prompt has at most one, and `{first}` is it."
    ))
    .next(format!("Join `{second}` and `{first}` into one `List<ChatMessage>`"))
}

fn no_file(type_name: &str, path: &str, at: Span) -> Diagnostic {
    let file = path.rsplit('/').next().unwrap_or(path);
    Diagnostic::new(
        &rule::PROMPT_SECTION_MISSING,
        format!("`{type_name}` has no `{file}`"),
    )
    .at(at, "no text")
    .reason(format!(
        "A prompt's words live beside its type, in `{file}`: `# Instructions`, the fixed text a reviewer reads, then `# Request`, what is asked each time."
    ))
    .next(format!(
        "Create `{path}` with an `# Instructions` and a `# Request` section"
    ))
}

fn no_type(path: &str, text: &str) -> Diagnostic {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.strip_suffix(PROMPT_EXTENSION).unwrap_or(file);
    let type_name = crate::case::to_camel(stem);
    let first = text.lines().next().map_or(0, str::len);
    Diagnostic::new(
        &rule::PROMPT_SECTION_MISSING,
        format!("`{file}` belongs to no prompt"),
    )
    .at(
        Span::new(path, 0, u32::try_from(first).unwrap_or(0)),
        "no type beside it",
    )
    .reason(format!(
        "A prompt's text sits beside its `type X = Prompt {{ ... }}`, named after it, so nothing sends `{file}` until a `{type_name}` is declared in `{}{stem}.jadpo`.",
        folder_of(path)
    ))
    .next(format!(
        "Declare `type {type_name} = Prompt {{ ... }}` beside it, or remove the file"
    ))
}

/// The line a byte offset is on, and that line's start and end.
fn line_at(text: &str, at: u32) -> (u32, u32) {
    let at = (at as usize).min(text.len());
    let start = text.get(..at).and_then(|before| before.rfind('\n')).map_or(0, |n| n + 1);
    let end = text
        .get(at..)
        .and_then(|after| after.find('\n'))
        .map_or(text.len(), |n| at + n);
    (
        u32::try_from(start).unwrap_or(0),
        u32::try_from(end).unwrap_or(0),
    )
}

fn brace(path: &str, _text: &str, problem: &Problem) -> Diagnostic {
    let file = path.rsplit('/').next().unwrap_or(path);
    let (start, end, message, label, next) = match problem {
        Problem::Lone { start, end } => (
            *start,
            *end,
            format!("A `}}` on its own in `{file}`"),
            "not doubled",
            "Write `}}` for a brace, or `{field}` for a field",
        ),
        Problem::Unclosed { start, end } => (
            *start,
            *end,
            format!("A `{{` in `{file}` that never closes"),
            "no `}` on this line",
            "Write `{{` for a brace, or close the placeholder with `}`",
        ),
        _ => (0, 0, String::new(), "", ""),
    };
    Diagnostic::new(&rule::PROMPT_PLACEHOLDER_UNKNOWN, message)
        .at(Span::new(path, start, end), label)
        .reason("In a prompt's text `{field}` writes a field of its type, and a brace is written doubled: `{{` writes `{` and `}}` writes `}`, as in a translation.")
        .next(next)
}

fn section(
    path: &str,
    text: &str,
    file: &prompt::PromptFile,
    problem: &Problem,
    both_missing: bool,
) -> Diagnostic {
    let name = path.rsplit('/').next().unwrap_or(path);
    let reason = "A prompt's file is its two sections, each once: `# Instructions`, the fixed text sent as the system message, then `# Request`, what is asked each time. Every line is in one of them, so a reviewer reads exactly what a model is sent.";
    let heading = |kind: SectionKind| {
        file.section(kind)
            .map(|section| Span::new(path, section.heading.0, section.heading.1))
    };
    let first_line = || {
        let (start, end) = line_at(text, 0);
        Span::new(path, start, end)
    };
    let (message, at, label, next) = match problem {
        Problem::Missing(_) if both_missing => (
            format!("`{name}` has neither `# Instructions` nor `# Request`"),
            first_line(),
            "no sections",
            "Start the file with `# Instructions` and its text, then `# Request` and its text".to_string(),
        ),
        Problem::Missing(kind) => {
            let other = match kind {
                SectionKind::Instructions => SectionKind::Request,
                SectionKind::Request => SectionKind::Instructions,
            };
            let at = heading(other).unwrap_or_else(first_line);
            (
                format!("`{name}` has no `{}`", kind.heading()),
                at,
                "its only section",
                match kind {
                    SectionKind::Instructions => {
                        "Add `# Instructions` and the fixed text above `# Request`".to_string()
                    }
                    SectionKind::Request => {
                        "Add `# Request` and what is asked each time after the instructions"
                            .to_string()
                    }
                },
            )
        }
        Problem::Before { start, end } => (
            format!("`{name}` has text before `# Instructions`"),
            Span::new(path, *start, *end),
            "in no section",
            "Move the text into a section, or remove it".to_string(),
        ),
        Problem::OutOfOrder { start, end } => (
            format!("`# Request` comes before `# Instructions` in `{name}`"),
            Span::new(path, *start, *end),
            "written first",
            "Move `# Instructions` and its text above `# Request`".to_string(),
        ),
        Problem::Repeated { kind, start, end } => (
            format!("`{}` is written twice in `{name}`", kind.heading()),
            Span::new(path, *start, *end),
            "a second time",
            format!("Join the two into one `{}`", kind.heading()),
        ),
        Problem::Empty { kind, start, end } => (
            format!("`{}` in `{name}` has no text", kind.heading()),
            Span::new(path, *start, *end),
            "empty",
            "Write the section's text under its heading".to_string(),
        ),
        Problem::Unclosed { .. } | Problem::Lone { .. } => (
            String::new(),
            first_line(),
            "",
            String::new(),
        ),
    };
    Diagnostic::new(&rule::PROMPT_SECTION_MISSING, message)
        .at(at, label)
        .reason(reason)
        .next(next)
}
