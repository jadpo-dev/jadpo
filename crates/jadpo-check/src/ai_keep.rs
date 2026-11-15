//! `ai: { keep_content: }` (area 21, "Prompts and answers in logs"; chunk
//! K6): the prompts whose calls are kept in production, sealed as
//! `health` data.
//!
//! - **`prompts:`** lists one or more of the application's prompts by
//!   name, `<prompt>`, each once.
//! - **`keep:`** is required: how long each call is kept, a duration
//!   written out from a day to ten years, `30d`.
//! - **`store:`** names the database the calls are kept in; left out, it
//!   is `main` when that is a database, or the application's only one. It
//!   must be the store of everyone who signs in, so their erasure deletes
//!   what was kept for them in the same statement as their sessions.
//!
//! Every mistake in it is `ai.settings_invalid`, but a streamed prompt,
//! whose answer the call never holds whole, which is
//! `toolchain.not_built_yet`; keeping anything needs the application's
//! field key (`data.field_encryption_key_required`).

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode};

use crate::data::ConnectionKind;
use crate::tree::{self, name_text, node_span};
use crate::types::Const;

/// The keys `keep_content:` takes.
const KEYS: [&str; 3] = ["prompts", "store", "keep"];

const REASON: &str = "`keep_content:` keeps the calls of the prompts it lists in production, sealed with the application's field key as `health` data: `prompts: [<prompt>]`, the prompts by name; `keep:`, required, how long each call is kept, written out from `1d` to `3650d`; and `store:`, the database they are kept in.";

/// `ai: { keep_content: }`, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeepRead {
    /// Each prompt it lists, by name, with where the list is written.
    pub prompts: Vec<String>,
    pub prompts_at: Option<Span>,
    /// The database the calls are kept in, once it is known.
    pub store: Option<String>,
    /// `keep:`, once it reads.
    pub keep_ms: Option<u64>,
    /// Where `keep_content:` is written.
    pub at: Span,
}

impl KeepRead {
    /// Whether every part of it read, so calls can be kept.
    pub fn complete(&self) -> bool {
        !self.prompts.is_empty() && self.store.is_some() && self.keep_ms.is_some()
    }
}

fn invalid(title: impl Into<String>, span: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::AI_SETTINGS_INVALID, title)
        .at(span, label.to_string())
        .reason(REASON)
}

/// Reads `keep_content:` from `entry`, written at `at` in `path`.
pub(crate) fn read(
    db: &dyn Db,
    project: Project,
    path: &str,
    at: Span,
    entry: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> KeepRead {
    let mut read = KeepRead {
        prompts: Vec::new(),
        prompts_at: None,
        store: None,
        keep_ms: None,
        at: at.clone(),
    };
    let Some(block) = entry.block() else {
        out.push(
            invalid(
                "`keep_content:` holds no block",
                node_span(path, entry.syntax()),
                "not a block",
            )
            .next("Write `keep_content: { prompts: [<prompt>]  keep: 30d }`"),
        );
        return read;
    };
    let mut given: Vec<String> = Vec::new();
    let mut store: Option<(String, Span)> = None;
    for (name, member) in tree::members(Some(block)) {
        let word = name_text(&name).unwrap_or_default();
        let span = node_span(path, name.syntax());
        if !KEYS.contains(&word.as_str()) {
            let mut problem = invalid(
                format!("`{word}` isn't one of `keep_content:`'s settings"),
                span.clone(),
                "not a setting",
            );
            problem = match nearest(&word, KEYS.iter().copied()) {
                Some(near) => problem
                    .next(format!("Use `{near}:` instead of `{word}:`"))
                    .edit(span, near),
                None => problem.next(format!("Remove `{word}:`")),
            };
            out.push(problem);
            continue;
        }
        if given.contains(&word) {
            out.push(
                invalid(
                    format!("`keep_content:` gives `{word}:` twice"),
                    span,
                    "given again",
                )
                .next(format!("Remove the second `{word}:`")),
            );
            continue;
        }
        given.push(word.clone());
        let value_span = member.value().map_or_else(
            || node_span(path, member.syntax()),
            |value| node_span(path, value.syntax()),
        );
        match word.as_str() {
            "prompts" => {
                read.prompts_at = Some(value_span.clone());
                read.prompts = prompts(&member, value_span, out);
            }
            "store" => {
                let written = member
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                store = Some((written, value_span));
            }
            _ => {
                let written = member
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                let millis = jadpo_std::Dur::parse(&written)
                    .and_then(|dur| u64::try_from(dur.millis).ok())
                    .filter(|millis| {
                        (jadpo_std::ai::KEEP_LEAST_MS..=jadpo_std::ai::KEEP_MOST_MS)
                            .contains(millis)
                    });
                match millis {
                    Some(millis) => read.keep_ms = Some(millis),
                    None => out.push(
                        invalid(
                            "`keep:` must be a duration from `1d` to `3650d`",
                            value_span,
                            "not a period",
                        )
                        .next("Write how long each call is kept, such as `keep: 30d`"),
                    ),
                }
            }
        }
    }
    for (needed, next) in [
        (
            "prompts",
            "Add `prompts: [<prompt>]`, the prompts whose calls are kept",
        ),
        ("keep", "Add `keep: 30d`, how long each call is kept"),
    ] {
        if !given.iter().any(|word| word == needed) {
            out.push(
                invalid(
                    format!("`keep_content:` has no `{needed}:`"),
                    at.clone(),
                    "incomplete",
                )
                .next(next),
            );
        }
    }
    read.store = kept_in(db, project, store, at, out);
    read
}

/// `prompts: [<prompt>, ...]`: at least one, each once.
fn prompts(member: &ast::Member, span: Span, out: &mut Vec<Diagnostic>) -> Vec<String> {
    let Some(Const::List(items)) = member.value().map(|value| Const::from_expr(&value)) else {
        out.push(
            invalid("`prompts:` must be a list of prompts", span, "not a list")
                .next("Write `prompts: [<prompt>]`"),
        );
        return Vec::new();
    };
    if items.is_empty() {
        out.push(
            invalid("`prompts:` lists no prompt", span, "empty")
                .next("List a prompt whose calls are kept, or remove `keep_content:`"),
        );
        return Vec::new();
    }
    let mut read: Vec<String> = Vec::new();
    for item in &items {
        match item {
            Const::Path(names) if names.len() == 1 => {
                let name = names[0].clone();
                if read.contains(&name) {
                    out.push(
                        invalid(
                            format!("`prompts:` lists `{name}` twice"),
                            span.clone(),
                            "listed again",
                        )
                        .next(format!("Remove the second `{name}`")),
                    );
                    continue;
                }
                read.push(name);
            }
            other => out.push(
                invalid(
                    format!("{} isn't a prompt's name", other.describe()),
                    span.clone(),
                    "not a prompt",
                )
                .next("Name each prompt as its file is named"),
            ),
        }
    }
    read
}

/// The database the calls are kept in: the one `store:` names, `main`
/// when that is a database, or the only one.
fn kept_in(
    db: &dyn Db,
    project: Project,
    named: Option<(String, Span)>,
    head: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<String> {
    let connections = crate::data::connections(db, project);
    let databases = connections.databases();
    let listed = crate::and_list(
        &databases
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>(),
    );
    if let Some((written, span)) = named {
        if connections.kind(&written) == Some(ConnectionKind::Postgres) {
            return Some(written);
        }
        out.push(
            invalid(
                format!("`store: {written}` isn't a database"),
                span,
                "not a database",
            )
            .next(format!("Name one of the application's databases, {listed}")),
        );
        return None;
    }
    if databases.iter().any(|name| name == crate::data::MAIN) {
        return Some(crate::data::MAIN.to_string());
    }
    if let [only] = databases.as_slice() {
        return Some(only.clone());
    }
    out.push(
        invalid(
            "`keep_content:` doesn't say which database keeps the calls",
            head,
            "no `store:`",
        )
        .next(match databases.first() {
            Some(first) => format!("Add `store: {first}` to `keep_content:`"),
            None => "Add a PostgreSQL connection under `connections:` in `app.jadpo`".to_string(),
        }),
    );
    None
}

/// What else is wrong with `keep_content:`, once it reads: a prompt the
/// application doesn't have, or one it streams; a store that isn't where
/// everyone who signs in is kept; and no field key to seal the calls with.
#[salsa::tracked(returns(ref))]
pub fn keep_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let settings = crate::ai::settings(db, project);
    let Some(keep) = &settings.keep else {
        return out;
    };
    let analysis = crate::prompts::analysis(db, project);
    let streamed = crate::ai::streamed_prompts(db, project);
    let listed = keep.prompts_at.clone().unwrap_or_else(|| keep.at.clone());
    for prompt in &keep.prompts {
        if analysis.prompt(prompt).is_none() {
            let known: Vec<&str> = analysis
                .prompts
                .iter()
                .map(|facts| facts.key.as_str())
                .collect();
            let mut problem = invalid(
                format!("`{prompt}` isn't one of the application's prompts"),
                listed.clone(),
                "no such prompt",
            );
            problem = match nearest(prompt, known.iter().copied()) {
                Some(near) => problem.next(format!("Use `{near}` instead of `{prompt}`")),
                None => problem.next("Name a prompt by its file's name"),
            };
            out.push(problem);
            continue;
        }
        if streamed.contains(prompt) {
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Keeping a streamed prompt's calls isn't built yet: `{prompt}` is streamed"),
                )
                .at(listed.clone(), "decided, not built")
                .reason("Area 21 keeps a listed prompt's messages and its answer; a streamed answer is read by the route after its action has ended, so this compiler can't keep it whole yet.")
                .next(format!("Leave `{prompt}` out of `keep_content:` for now")),
            );
        }
    }
    if let Some(store) = &keep.store {
        let identity = crate::identity::identity(db, project);
        for variant in &identity.variants {
            if variant.store != *store {
                out.push(
                    invalid(
                        format!("`keep_content:` keeps calls in `{store}`, but `{}` is in `{}`", variant.entity, variant.store),
                        keep.at.clone(),
                        "not where callers are",
                    )
                    .reason(format!(
                        "A kept call is erased with the person it was made for, in the same statement as their sessions, so it is kept where `{}` records are: in `{}`.",
                        variant.entity, variant.store
                    ))
                    .next(format!("Write `store: {}` in `keep_content:`", variant.store)),
                );
                break;
            }
        }
    }
    let key = crate::data::sealed::field_key(db, project);
    if !key.written {
        out.push(
            Diagnostic::new(
                &rule::DATA_FIELD_ENCRYPTION_KEY_REQUIRED,
                "`ai: { keep_content: }` keeps calls sealed, but `application { }` names no key to seal them with",
            )
            .at(keep.at.clone(), "kept sealed")
            .reason("The prompts and answers `keep_content:` keeps are sealed with the application's one key, which `application { field_encryption_key: }` names, as an encrypted field is. Jadpo won't make up a secret for you: losing this key loses every call kept with it for good, so it is a new value each environment has to be given and keep safe.")
            .next("Add `field_encryption_key: config.field_encryption_key` to `application { }` in `app.jadpo`, declare `field_encryption_key: EncryptionKey` in `config.jadpo`, and run `jadpo config set field_encryption_key --generate`"),
        );
    }
    out
}
