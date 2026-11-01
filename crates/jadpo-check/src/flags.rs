//! Area 16's feature flags: `flags { }` in `flags.jadpo` declares each
//! flag as `name: Type { default: ..., owner: "...", expires: "..." }`,
//! a `Bool` or an enum of plain variants, and actions, tests and a route's
//! `run:` read it as `flags.<name>`.
//!
//! Read once per project ([`flags`]) into a [`Flags`], which the bodies
//! (the type of `flags.<name>`), the routes, the graph and `jadpo-plan`
//! (the runtime's [`jadpo_std::flags::FlagsPlan`]) read. Where their state
//! is kept is `application { flags: { store: } }`, or the only database
//! ([`settings`]). A flag past its `expires:` date is `flag.expired`,
//! judged at the date the project is judged on (area 1, "Rules triggered
//! by time passing"), and never declinable ([`expired`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, Layout, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use crate::lookups::{Lookup, Name};
use crate::tree::{self, name_text, node_span, written};
use crate::types::{Const, Def, Prim, Ty, TypeRef, TypeView};

/// The word `flags { }` is declared with and its flags are read through:
/// `flags.<name>`.
pub const FLAGS: &str = "flags";

/// What a flag's block may hold.
const OPTIONS: [&str; 4] = ["default", "owner", "expires", "client"];

/// One flag, as `flags { }` declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flag {
    pub name: String,
    /// Its type as code reads it: `Bool`, or the enum.
    pub ty: Ty,
    /// The enum it is a variant of; `None` for a `Bool`.
    pub enum_type: Option<String>,
    /// Its values in order: `false` and `true`, or the enum's variants by
    /// name as it declares them.
    pub values: Vec<String>,
    /// Its `default:`, as a place in `values`.
    pub default: u32,
    pub owner: String,
    /// Its `expires:` date as written, `2027-01-31`, and in days since
    /// 1970-01-01.
    pub expires: String,
    pub expires_day: i32,
    /// Where its `expires:` date is written, which `flag.expired` points
    /// at.
    pub expires_at: Span,
    /// `client: true`: also answered by each API's `GET /flags`.
    pub client: bool,
}

impl Flag {
    /// Its value at place `at`, as source writes it: `true`, or
    /// `CheckoutLayout.compact`.
    pub fn written(&self, at: u32) -> String {
        let value = self.values.get(at as usize).map_or("", String::as_str);
        match &self.enum_type {
            Some(own) => format!("{own}.{value}"),
            None => value.to_string(),
        }
    }
}

/// Every flag of a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    /// Where `flags { }` is declared: its keyword.
    pub declared: Option<Span>,
    /// Each flag that read cleanly, in name order.
    pub flags: Vec<Flag>,
    /// Every flag name written, read cleanly or not, so a read of one that
    /// failed is not also reported as unknown.
    pub written: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

impl Flags {
    pub fn flag(&self, name: &str) -> Option<&Flag> {
        self.flags.iter().find(|flag| flag.name == name)
    }
}

/// The flags of `project`.
#[salsa::tracked(returns(ref))]
pub fn flags(db: &dyn Db, project: Project) -> Arc<Flags> {
    let lookup = Lookup::new(db, project);
    let mut out = Flags::default();
    let mut all = crate::config::singletons(db, project, FLAGS).into_iter();
    let Some(first) = all.next() else {
        return Arc::new(out);
    };
    out.declared = Some(first.keyword.clone());
    for other in all {
        out.problems.push(
            Diagnostic::new(
                &rule::FLAG_DECLARATION_INVALID,
                "`flags { }` is declared twice",
            )
            .at(other.keyword.clone(), "declared again")
            .also(first.keyword.clone(), "first declared here")
            .reason("Every flag is declared once, in the one `flags { }` in `flags.jadpo`, so one place lists every flag the app can switch.")
            .next("Move the flags this `flags` holds into the first, and remove it"),
        );
    }
    let path = first.path.clone();
    let mut seen: Vec<(String, Span)> = Vec::new();
    for item in first.list.iter().flat_map(|list| list.member_items()) {
        let MemberItem::Member(member) = item else {
            if let MemberItem::Elision(_) = item {
                continue;
            }
            out.problems.push(not_a_flag(&path, item.syntax()));
            continue;
        };
        let Some(key) = member.member_key() else {
            continue;
        };
        let mut parts = key.names();
        let (Some(name), None) = (parts.next(), parts.next()) else {
            out.problems.push(not_a_flag(&path, key.syntax()));
            continue;
        };
        let Some(text) = name_text(&name) else {
            continue;
        };
        let span = node_span(&path, name.syntax());
        if let Some((_, earlier)) = seen.iter().find(|(seen, _)| *seen == text) {
            out.problems.push(
                Diagnostic::new(
                    &rule::FLAG_DECLARATION_INVALID,
                    format!("`{text}` is declared twice"),
                )
                .at(span, "declared again")
                .also(earlier.clone(), "first declared here")
                .reason("Each flag has one declaration, so `flags.<name>` reads one flag.")
                .next(format!("Remove one `{text}`, or rename it")),
            );
            continue;
        }
        seen.push((text.clone(), span.clone()));
        out.written.push(text.clone());
        let mut reader = Reader {
            lookup: &lookup,
            path: &path,
            problems: &mut out.problems,
        };
        if let Some(flag) = reader.flag(&text, &span, &member) {
            out.flags.push(flag);
        }
    }
    out.flags.sort_by(|a, b| a.name.cmp(&b.name));
    Arc::new(out)
}

fn not_a_flag(path: &str, node: &jadpo_syntax::SyntaxNode) -> Diagnostic {
    let text = written(node);
    Diagnostic::new(
        &rule::FLAG_DECLARATION_INVALID,
        format!("`{text}` isn't a flag"),
    )
    .at(node_span(path, node), "not `name: Type { }`")
    .reason("`flags { }` holds flags, each written `name: Bool { }` or with an enum as its type, its block giving `default:`, `owner:` and `expires:`.")
    .next("Write the flag as `name: Bool { }`, its block giving `default:`, `owner:` and `expires:`, or remove this line")
}

/// The type of flag `name`, so a body or a route that reads it depends on
/// that flag alone: `Ty::Error` for a flag written but not read cleanly,
/// whose mistake is already reported, and `None` for no flag.
#[salsa::tracked(returns(ref))]
pub fn flag_type<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<Ty> {
    let flags = flags(db, project);
    let text = name.text(db);
    match flags.flag(text) {
        Some(flag) => Some(flag.ty.clone()),
        None => flags.written.iter().any(|w| w == text).then_some(Ty::Error),
    }
}

struct Reader<'a, 'l> {
    lookup: &'a Lookup<'l>,
    path: &'a str,
    problems: &'a mut Vec<Diagnostic>,
}

impl Reader<'_, '_> {
    /// One flag; `None` when it has a problem, which is reported.
    fn flag(&mut self, name: &str, span: &Span, member: &ast::Member) -> Option<Flag> {
        let path = self.path;
        let Some(value) = member.value() else {
            self.problems.push(
                Diagnostic::new(
                    &rule::FLAG_DECLARATION_INVALID,
                    format!("`{name}:` has no type"),
                )
                .at(span.clone(), "no type")
                .reason("A flag is a `Bool`, or an enum when it has more than two values: `name: Bool { }`.")
                .next(format!("Write `{name}: Bool {{ ... }}`, or an enum of its values")),
            );
            return None;
        };
        let (value, options) = match &value {
            Expr::RecordExpr(record) => match record.expr() {
                Some(inner) => (inner, record.member_list()),
                None => (value.clone(), None),
            },
            other => (other.clone(), member.block()),
        };
        let type_span = node_span(path, value.syntax());
        let ty_text = written(value.syntax());
        let ty = match self.lookup.resolve(&TypeRef::from_expr(&value)) {
            // An unknown type name is reported by resolution.
            Ty::Error => return None,
            ty => ty,
        };
        let mut ok = true;
        let values = match self.values(&ty) {
            Ok(values) => Some(values),
            Err(why) => {
                ok = false;
                self.problems.push(
                    Diagnostic::new(
                        &rule::FLAG_DECLARATION_INVALID,
                        format!("`{name}` can't be a `{ty_text}`"),
                    )
                    .at(type_span, "not a flag's type")
                    .reason(format!(
                        "A flag is switched between a few values while the app runs, so it is a `Bool`, or an enum whose variants carry no data. {why}"
                    ))
                    .next(format!("Make `{name}` a `Bool`, or an enum of its values")),
                );
                None
            }
        };
        let mut default: Option<(Span, ast::Member)> = None;
        let mut owner: Option<String> = None;
        let mut expires: Option<(String, i32, Span)> = None;
        let mut client = false;
        let mut seen: Vec<(String, Span)> = Vec::new();
        for (key, option) in tree::members(options) {
            let Some(word) = name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            if !OPTIONS.contains(&word.as_str()) {
                ok = false;
                self.unknown_option(name, &word, &key_span);
                continue;
            }
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == word) {
                ok = false;
                self.problems.push(
                    Diagnostic::new(
                        &rule::FLAG_DECLARATION_INVALID,
                        format!("`{word}:` is given twice on `{name}`"),
                    )
                    .at(key_span, "given again")
                    .also(first.clone(), "first given here")
                    .reason("A flag says each of `default:`, `owner:` and `expires:` once, so there is one answer to read.")
                    .next(format!("Remove one `{word}:`")),
                );
                continue;
            }
            seen.push((word.clone(), key_span.clone()));
            let at = option
                .value()
                .map_or_else(|| key_span.clone(), |v| node_span(path, v.syntax()));
            match word.as_str() {
                "default" => default = Some((key_span, option)),
                "owner" => match option.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Str(text)) if !text.trim().is_empty() => owner = Some(text),
                    _ => {
                        ok = false;
                        self.problems.push(
                            Diagnostic::new(
                                &rule::FLAG_DECLARATION_INVALID,
                                format!("`owner:` on `{name}` isn't who owns it, in quotes"),
                            )
                            .at(at, "not text")
                            .reason("`owner:` names who decides when the flag is removed, as text in quotes; the error a flag past its date gives names them.")
                            .next("Write who owns the flag in quotes after `owner:`"),
                        );
                    }
                },
                "expires" => {
                    let text = match option.value().map(|v| Const::from_expr(&v)) {
                        Some(Const::Str(text)) => Some(text),
                        _ => None,
                    };
                    match text.as_deref().map(|t| (t, jadpo_std::date_number(t))) {
                        Some((text, Some(day))) => expires = Some((text.to_string(), day, at)),
                        _ => {
                            ok = false;
                            let why = match text.as_deref().map(jadpo_std::check_calendar_date) {
                                Some(Err(why)) => format!("The value {why}."),
                                _ => "It isn't written in quotes.".to_string(),
                            };
                            self.problems.push(
                                Diagnostic::new(
                                    &rule::FLAG_DECLARATION_INVALID,
                                    format!("`expires:` on `{name}` isn't a date"),
                                )
                                .at(at, "not a date")
                                .reason(format!(
                                    "`expires:` is the date the flag should be gone by, written in quotes, `\"2027-01-31\"`. {why}"
                                ))
                                .next("Write the date in quotes, such as `expires: \"2027-01-31\"`"),
                            );
                        }
                    }
                }
                // `client: true` puts it in each API's `GET /flags`.
                _ => match option.value().map(|v| Const::from_expr(&v)) {
                    Some(Const::Bool(on)) => client = on,
                    _ => {
                        ok = false;
                        self.problems.push(
                            Diagnostic::new(
                                &rule::FLAG_DECLARATION_INVALID,
                                format!("`client:` on `{name}` isn't `true` or `false`"),
                            )
                            .at(at, "not a `Bool`")
                            .reason("`client: true` also answers the flag, worked out for the caller, from each API's generated `GET /flags`; `false`, or no `client:`, keeps it on the server.")
                            .next("Write `client: true` or `client: false`"),
                        );
                    }
                },
            }
        }
        let missing: Vec<&str> = [
            ("default", default.is_none()),
            (
                "owner",
                owner.is_none() && !seen.iter().any(|(w, _)| w == "owner"),
            ),
            (
                "expires",
                expires.is_none() && !seen.iter().any(|(w, _)| w == "expires"),
            ),
        ]
        .into_iter()
        .filter(|(_, missing)| *missing)
        .map(|(word, _)| word)
        .collect();
        if !missing.is_empty() {
            ok = false;
            let shown: Vec<String> = missing.iter().map(|word| format!("`{word}:`")).collect();
            let (none, listed) = (crate::or_list(&shown), crate::and_list(&shown));
            self.problems.push(
                Diagnostic::new(
                    &rule::FLAG_DECLARATION_INVALID,
                    format!("`{name}` has no {none}"),
                )
                .at(span.clone(), "missing what a flag says")
                .reason("Every flag says its `default:`, the value everyone gets until it is changed; its `owner:`, who decides when it goes; and `expires:`, the date it should be gone by, after which `jadpo check` fails until it is removed or the date moved on.")
                .next(format!("Add {listed} to `{name}`")),
            );
        }
        let values = values?;
        let enum_type = match self.lookup.structure(&ty) {
            Ty::Named(own) if self.lookup.base_prim(&ty) != Some(Prim::Bool) => Some(own),
            _ => None,
        };
        let shape = Flag {
            name: name.to_string(),
            ty: ty.clone(),
            enum_type,
            values,
            default: 0,
            owner: owner.clone().unwrap_or_default(),
            expires: String::new(),
            expires_day: 0,
            expires_at: span.clone(),
            client,
        };
        let default = match default {
            Some((key_span, option)) => match value_at(
                &shape,
                path,
                option.value().as_ref(),
                &key_span,
                &rule::FLAG_DECLARATION_INVALID,
                ("default", "default"),
            ) {
                Ok(at) => Some(at),
                Err(problem) => {
                    self.problems.push(*problem);
                    None
                }
            },
            None => None,
        };
        let (Some(default), Some(owner), Some((expires, expires_day, expires_at)), true) =
            (default, owner, expires, ok)
        else {
            return None;
        };
        Some(Flag {
            default,
            owner,
            expires,
            expires_day,
            expires_at,
            ..shape
        })
    }

    /// The values a flag of type `ty` takes, or why it can't be one.
    fn values(&self, ty: &Ty) -> Result<Vec<String>, String> {
        if self.lookup.base_prim(ty) == Some(Prim::Bool) {
            return Ok(jadpo_std::flags::BOOL_VALUES
                .iter()
                .map(|value| value.to_string())
                .collect());
        }
        match self.lookup.structure(ty) {
            Ty::Named(own) => match self.lookup.def(&own) {
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                    Ok(variants.iter().map(|v| v.name.clone()).collect())
                }
                Some(Def::Enum { .. }) => {
                    Err("Its variants carry data, which no setting of a flag gives.".to_string())
                }
                _ => Err(String::new()),
            },
            Ty::Nullable(_) => {
                Err("A flag always has a value, its `default:` until it is changed.".to_string())
            }
            _ => Err(String::new()),
        }
    }

    fn unknown_option(&mut self, flag: &str, word: &str, span: &Span) {
        let near = nearest(word, OPTIONS);
        let problem = Diagnostic::new(
            &rule::FLAG_DECLARATION_INVALID,
            format!("`{word}` isn't an option a flag takes"),
        )
        .at(span.clone(), "unknown option")
        .reason(format!(
            "`{flag}` can say `default:`, `owner:`, `expires:` and `client:`.{}",
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        ));
        self.problems.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{word}`"))
                .edit(span.clone(), near),
            None => problem.next(format!("Remove `{word}`")),
        });
    }
}

/// The place among `flag`'s values that `value` names, written in source:
/// `true` or `false` for a `Bool` flag, `Type.variant` for an enum's. A
/// value that names none is `refused`, at `key` when there is no value.
pub(crate) fn value_at(
    flag: &Flag,
    path: &str,
    value: Option<&ast::Expr>,
    key: &Span,
    refused: &'static jadpo_diag::Rule,
    (word, written_as): (&str, &str),
) -> Result<u32, Box<Diagnostic>> {
    let at = value.map_or_else(|| key.clone(), |v| node_span(path, v.syntax()));
    let shown = value.map(|v| written(v.syntax())).unwrap_or_default();
    let name = &flag.name;
    let found = match (value.map(Const::from_expr), &flag.enum_type) {
        (Some(Const::Bool(on)), None) => flag.values.iter().position(|v| *v == on.to_string()),
        (Some(Const::Path(names)), Some(own)) => match names.as_slice() {
            [host, variant] if host == own => flag.values.iter().position(|v| v == variant),
            _ => None,
        },
        _ => None,
    };
    if let Some(found) = found.and_then(|found| u32::try_from(found).ok()) {
        return Ok(found);
    }
    let (reason, next) = match &flag.enum_type {
        None => (
            format!("`{name}` is a `Bool`, so its {word} is `true` or `false`."),
            format!("Write `{written_as}: false` or `{written_as}: true`"),
        ),
        Some(own) => {
            let variant = match value.map(Const::from_expr) {
                Some(Const::Path(names)) => names.last().cloned().unwrap_or_default(),
                _ => shown.clone(),
            };
            let near = nearest(&variant, flag.values.iter().map(String::as_str));
            let first = flag.values.first().map_or("value", String::as_str);
            (
                format!(
                    "`{name}` is a `{own}`, so its {word} is one of its variants, {}.{}",
                    crate::or_list(
                        &flag
                            .values
                            .iter()
                            .map(|v| format!("`{own}.{v}`"))
                            .collect::<Vec<_>>()
                    ),
                    near.map_or_else(String::new, |near| format!(" Did you mean `{own}.{near}`?"))
                ),
                format!("Write `{written_as}: {own}.{}`", near.unwrap_or(first)),
            )
        }
    };
    Err(Box::new(
        Diagnostic::new(
            refused,
            if value.is_some() {
                format!("`{shown}` isn't a value of `{name}`")
            } else {
                format!("`{written_as}:` on `{name}` has no value")
            },
        )
        .at(at, "not one of its values")
        .reason(reason)
        .next(next),
    ))
}

/// `flags.<name>` that names no flag, at `span`: `in_test` for a test's
/// `flags:`, which names the flag alone.
pub fn unknown(db: &dyn Db, project: Project, name: &str, span: Span, in_test: bool) -> Diagnostic {
    let flags = flags(db, project);
    let near = nearest(name, flags.written.iter().map(String::as_str));
    let shown = if in_test {
        name.to_string()
    } else {
        format!("flags.{name}")
    };
    let problem = Diagnostic::new(&rule::FLAG_UNKNOWN, format!("`{shown}` isn't a flag"))
        .at(span.clone(), "no such flag")
        .reason(match (&flags.declared, near) {
            (None, _) => "The project declares no `flags { }`, so it has no flags; they are declared in `flags.jadpo`.".to_string(),
            (Some(_), near) => format!(
                "`flags {{ }}` declares no flag called `{name}`.{}",
                near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
            ),
        });
    match near {
        Some(near) => {
            let fixed = if in_test {
                near.to_string()
            } else {
                format!("flags.{near}")
            };
            problem
                .next(format!("Use `{fixed}` instead of `{shown}`"))
                .edit(span, fixed)
        }
        None if in_test => problem.next(format!(
            "Declare `{name}: Bool {{ ... }}` in `flags.jadpo`, or remove `{name}:` from `flags:`"
        )),
        None => problem.next(format!(
            "Declare `{name}: Bool {{ ... }}` in `flags.jadpo`, or remove the read"
        )),
    }
}

/// `flags.<name>` read where a value can't change between calls, at
/// `span`, by `who` (`a function`).
pub fn read_context(written: &str, who: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::FLAG_READ_CONTEXT,
        format!("{} can't read `{written}`", capital(who)),
    )
    .at(span, "reads a flag")
    .reason(format!(
        "A flag's value is worked out for the caller while the app runs, and {who} gives the same result for the same arguments, so it can't read one; an action reads it and passes its value in as a parameter."
    ))
    .next(format!("Read `{written}` in the action that calls this, and pass its value as a parameter"))
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// --- where their state is kept -----------------------------------------------

/// `application { flags: { store: } }`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlagsRead {
    /// The store `jadpo_flags` is kept in: the one named, or the only
    /// database. `None` with several databases and none named, or none.
    pub store: Option<String>,
    pub problems: Vec<Diagnostic>,
}

/// The project's `flags:` settings.
#[salsa::tracked(returns(ref))]
pub fn settings(db: &dyn Db, project: Project) -> Arc<FlagsRead> {
    let mut out = FlagsRead::default();
    let connections = crate::data::connections(db, project);
    let databases = connections.databases();
    if let [only] = databases.as_slice() {
        out.store = Some(only.clone());
    }
    let Some(file) = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE)
    else {
        return Arc::new(out);
    };
    let Some(source) = tree::source(db, *file) else {
        return Arc::new(out);
    };
    let path = file.path(db);
    for decl in source.decls() {
        let ast::Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
            continue;
        }
        for (key, member) in tree::members(singleton.member_list()) {
            if name_text(&key).as_deref() != Some(FLAGS) {
                continue;
            }
            let block = member.block();
            if block.is_none() {
                out.problems.push(
                    Diagnostic::new(&rule::FLAG_STORE_INVALID, "`flags:` holds no block")
                        .at(node_span(path, member.syntax()), "not a block")
                        .reason("`flags: { store: <connection> }` names the database every flag's state is kept in.")
                        .next("Write `flags: { store: main }`, naming a database connection"),
                );
            }
            for (setting, entry) in tree::members(block) {
                let name = name_text(&setting).unwrap_or_default();
                if name != "store" {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::FLAG_STORE_INVALID,
                            format!("`{name}` isn't a flags setting"),
                        )
                        .at(node_span(path, setting.syntax()), "not a setting")
                        .reason("`flags:` holds `store:`, the database every flag's state is kept in; the flags themselves are declared in `flags.jadpo`.")
                        .next(format!("Remove `{name}:`")),
                    );
                    continue;
                }
                let written = entry
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                if connections.kind(&written) == Some(crate::data::ConnectionKind::Postgres) {
                    out.store = Some(written);
                    continue;
                }
                let span = entry.value().map_or_else(
                    || node_span(path, entry.syntax()),
                    |v| node_span(path, v.syntax()),
                );
                let mut problem = Diagnostic::new(
                    &rule::FLAG_STORE_INVALID,
                    format!("`store: {written}` isn't a database"),
                )
                .at(span.clone(), "not a database")
                .reason(format!(
                    "Every flag's state is kept in a database, so a change reaches each server; this application's databases are {}.",
                    database_list(&databases)
                ));
                problem = match nearest(&written, databases.iter().map(String::as_str)) {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{written}`"))
                        .edit(span, near),
                    None => problem.next("Name a database connection"),
                };
                out.problems.push(problem);
            }
        }
    }
    Arc::new(out)
}

fn database_list(databases: &[String]) -> String {
    if databases.is_empty() {
        return "none".to_string();
    }
    crate::and_list(
        &databases
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>(),
    )
}

/// The store the flags' state is kept in, when the project declares any
/// flag and one is named or is the only database. `flags:` still being
/// written asks for no table yet.
pub fn store(db: &dyn Db, project: Project) -> Option<String> {
    flags(db, project).declared.as_ref()?;
    let settings = settings(db, project);
    if !settings.problems.is_empty() {
        return None;
    }
    settings.store.clone()
}

/// The problems of `flags { }`, of where their state is kept, and the
/// flags past their date.
#[salsa::tracked(returns(ref))]
pub fn problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let model = flags(db, project);
    let mut out = model.problems.clone();
    out.extend(settings(db, project).problems.iter().cloned());
    // A loose project, one file of tests, keeps no state: its flags read
    // their `default:`, or a test's `flags:`.
    if let Some(keyword) = &model.declared
        && project.layout(db) == Layout::Project
        && settings(db, project).store.is_none()
        && settings(db, project).problems.is_empty()
    {
        let databases = crate::data::connections(db, project).databases();
        let (message, reason, next) = match databases.first() {
            Some(first) => (
                "The flags have no database named to keep their state in".to_string(),
                format!(
                    "Each flag's current value, and who gets which, is kept in a database so a change reaches every server while the app runs; this application has {}, and `app.jadpo` names none with `flags: {{ store: }}`.",
                    database_list(&databases)
                ),
                format!("Add `flags: {{ store: {first} }}` to `application {{ }}` in `app.jadpo`"),
            ),
            None => (
                "The flags have no database to keep their state in".to_string(),
                "Each flag's current value, and who gets which, is kept in a database so a change reaches every server while the app runs, and this application has none.".to_string(),
                "Add a PostgreSQL connection under `connections:` in `app.jadpo`".to_string(),
            ),
        };
        out.push(
            Diagnostic::new(&rule::FLAG_STORE_REQUIRED, message)
                .at(keyword.clone(), "no store for their state")
                .reason(reason)
                .next(next),
        );
    }
    out.extend(expired(db, project).iter().cloned());
    out.extend(untested(db, project).iter().cloned());
    out
}

/// Each flag past its `expires:` date at the date the project is judged
/// on (area 1, "Rules triggered by time passing"): today for `jadpo
/// check`, the commit's for `jadpo build`. Never declinable: moving the
/// date on is the reviewed choice.
#[salsa::tracked(returns(ref))]
pub fn expired(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let Some(today) = project.judged_on(db) else {
        return Vec::new();
    };
    flags(db, project)
        .flags
        .iter()
        .filter(|flag| flag.expires_day < today)
        .map(|flag| {
            let name = &flag.name;
            Diagnostic::new(
                &rule::FLAG_EXPIRED,
                format!("`{name}` expired on {}", flag.expires),
            )
            .at(flag.expires_at.clone(), "past its date")
            .reason(format!(
                "`{name}` is owned by \"{}\", and was to be gone by {}. A flag left in place keeps both of its branches in the code; removing it keeps one, and moving `expires:` on is the reviewed choice to keep it longer, so this can't be declined.",
                flag.owner,
                flag.expires,
            ))
            .next(format!(
                "Ask \"{}\" whether to remove `{name}` or move its `expires:` date on",
                flag.owner
            ))
        })
        .collect()
}

/// What a test's `flags:` gives (area 16, "What the compiler checks"):
/// each flag it names with the place of its value, and the problems.
pub(crate) fn test_flags(
    db: &dyn Db,
    project: Project,
    path: &str,
    block: Option<ast::MemberList>,
    out: &mut Vec<(String, u32)>,
    problems: &mut Vec<Diagnostic>,
) {
    let model = flags(db, project);
    let mut seen: Vec<(String, Span)> = Vec::new();
    for (name_node, entry) in tree::members(block) {
        let Some(name) = name_text(&name_node) else {
            continue;
        };
        let span = node_span(path, name_node.syntax());
        if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == name) {
            problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_TEST_SETTING_DUPLICATE,
                    format!("`{name}` is given twice in `flags:`"),
                )
                .at(span, "given again")
                .also(first.clone(), "first given here")
                .reason("A test says each flag's value once, so there is one answer to read.")
                .next(format!("Remove one `{name}:`")),
            );
            continue;
        }
        seen.push((name.clone(), span.clone()));
        let Some(flag) = model.flag(&name) else {
            if !model.written.contains(&name) {
                problems.push(unknown(db, project, &name, span, true));
            }
            continue;
        };
        match value_at(
            flag,
            path,
            entry.value().as_ref(),
            &span,
            &rule::TYPE_SETTING_VALUE,
            ("value", &name),
        ) {
            Ok(at) => out.push((name, at)),
            Err(problem) => problems.push(*problem),
        }
    }
}

// --- both values tested (chunk B10) ------------------------------------------

/// One read of a flag the tests must cover: where it is, the flag, and the
/// values the tests that reach it give.
struct ReadSite {
    flag: String,
    span: Span,
    covered: BTreeSet<u32>,
    reached: usize,
}

/// Where every read of a declared flag is, outside tests, and what reaches
/// each: each test, by its file and place, with the reads it reaches.
struct Reach {
    reads: BTreeMap<(String, u32), ReadSite>,
    tests: Vec<TestReach>,
}

/// One test and the reads of flags it reaches.
pub(crate) struct TestReach {
    pub path: String,
    pub index: u32,
    /// The value its `flags:` gives each flag it names.
    pub given: Vec<(String, u32)>,
    /// The flags whose reads it reaches, by name.
    pub flags: BTreeSet<String>,
    /// Where each read it reaches is, by file and offset.
    pub reads: BTreeSet<(String, u32)>,
    /// Whether its settings have a problem, so what it gives is unknown.
    pub unclear: bool,
}

fn reach(db: &dyn Db, project: Project) -> Reach {
    use jadpo_graph::Kind;
    use jadpo_graph::body::{Expr as BodyExpr, Node};

    use crate::types::{Fact, body_facts, callables_of, resolve, typed_body};

    let model = flags(db, project);
    let mut reads: BTreeMap<(String, u32), ReadSite> = BTreeMap::new();
    let mut tests = Vec::new();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            if body_facts(db, project, callable)
                .key_of(&[Kind::Test])
                .is_some()
            {
                tests.push(callable);
                continue;
            }
            let path = file.path(db).clone();
            for site in &typed_body(db, project, callable).body.sites {
                if let Node::Expr(BodyExpr::Flag { flag }) = &site.node
                    && model.flag(flag.name()).is_some()
                {
                    reads.insert(
                        (path.clone(), site.start),
                        ReadSite {
                            flag: flag.name().to_string(),
                            span: Span::new(&path, site.start, site.end),
                            covered: BTreeSet::new(),
                            reached: 0,
                        },
                    );
                }
            }
        }
    }
    // A route's `run:` reads, at the route.
    let mut route_reads: BTreeMap<jadpo_graph::Key, Vec<(String, u32)>> = BTreeMap::new();
    for (key, flag, span) in crate::routes::flag_arguments(db, project) {
        if model.flag(&flag).is_none() {
            continue;
        }
        let at = (span.path.clone(), span.start);
        route_reads.entry(key).or_default().push(at.clone());
        reads.insert(
            at,
            ReadSite {
                flag,
                span,
                covered: BTreeSet::new(),
                reached: 0,
            },
        );
    }
    let graph = crate::graph::graph(db, project);
    let mut out = Vec::new();
    for test in tests {
        let mut reached: BTreeSet<(String, u32)> = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut queue = vec![test];
        while let Some(current) = queue.pop() {
            if !visited.insert((
                current.file(db).path(db).clone(),
                current.index(db),
                current.function(db),
            )) {
                continue;
            }
            let path = current.file(db).path(db).clone();
            let checked = typed_body(db, project, current);
            for site in &checked.body.sites {
                if let Node::Expr(BodyExpr::Flag { .. }) = &site.node {
                    reached.insert((path.clone(), site.start));
                }
            }
            for route in checked.body.requests() {
                if let Some(at) = route_reads.get(route) {
                    reached.extend(at.iter().cloned());
                }
                if let Some(id) = graph.find(route) {
                    for to in graph.outgoing(id, jadpo_graph::EdgeKind::Runs) {
                        if let Some(next) = resolve(db, project, &graph.node(to).key) {
                            queue.push(next);
                        }
                    }
                }
            }
            for fact in &body_facts(db, project, current).facts {
                if let Fact::Action(key) | Fact::Fixture(key) | Fact::Function(key) = fact
                    && let Some(next) = resolve(db, project, key)
                {
                    queue.push(next);
                }
            }
        }
        let reached: BTreeSet<(String, u32)> = reached
            .into_iter()
            .filter(|at| reads.contains_key(at))
            .collect();
        let settings = crate::config::test_settings(db, project, test);
        out.push(TestReach {
            path: test.file(db).path(db).clone(),
            index: test.index(db),
            unclear: !settings.problems.is_empty(),
            given: settings.flags.clone(),
            flags: reached
                .iter()
                .filter_map(|at| reads.get(at).map(|read| read.flag.clone()))
                .collect(),
            reads: reached,
        });
    }
    Reach { reads, tests: out }
}

/// Each test, by its file and place, with the flags whose reads it
/// reaches and the values its `flags:` gives: what `jadpo flags remove`
/// reads to know which tests tested a branch that goes.
pub(crate) fn tests_reaching(db: &dyn Db, project: Project) -> Vec<TestReach> {
    reach(db, project).tests
}

/// Every read of a flag that the tests reaching it don't give every value
/// (area 16, "What the compiler checks"): `flag.value_untested`, at the
/// read. A test reaches what its body calls, through the actions,
/// fixtures and functions it reaches, and the routes it sends requests
/// through, their `run:` arguments and what they run. It gives each flag
/// its `flags:` value, or the `default:`. A test's own reads aren't
/// counted, and the rule can't be declined.
#[salsa::tracked(returns(ref))]
pub fn untested(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let model = flags(db, project);
    if model.flags.is_empty() {
        return Vec::new();
    }
    let Reach { mut reads, tests } = reach(db, project);
    // A test whose `flags:` doesn't read cleanly gives values no one can
    // know, and its mistake is reported already.
    if tests.iter().any(|test| test.unclear) {
        return Vec::new();
    }
    for test in &tests {
        for at in &test.reads {
            let Some(read) = reads.get_mut(at) else {
                continue;
            };
            let value = test
                .given
                .iter()
                .find(|(flag, _)| *flag == read.flag)
                .map(|(_, at)| *at)
                .or_else(|| model.flag(&read.flag).map(|flag| flag.default));
            if let Some(value) = value {
                read.covered.insert(value);
                read.reached += 1;
            }
        }
    }
    let mut out = Vec::new();
    for read in reads.values() {
        let Some(flag) = model.flag(&read.flag) else {
            continue;
        };
        let missing: Vec<u32> = (0..flag.values.len() as u32)
            .filter(|at| !read.covered.contains(at))
            .collect();
        let Some(&first) = missing.first() else {
            continue;
        };
        let name = &flag.name;
        let shown: Vec<String> = missing
            .iter()
            .map(|at| format!("`{}`", flag.written(*at)))
            .collect();
        let given: Vec<String> = read
            .covered
            .iter()
            .map(|at| format!("`{}`", flag.written(*at)))
            .collect();
        let (message, reached) = if read.reached == 0 {
            (
                format!("No test reaches this read of `flags.{name}`"),
                "No test reaches it.".to_string(),
            )
        } else {
            (
                format!(
                    "No test that reaches `flags.{name}` gives it {}",
                    crate::or_list(&shown)
                ),
                format!(
                    "The tests that reach it give it only {}.",
                    crate::and_list(&given)
                ),
            )
        };
        let next = if first == flag.default {
            format!(
                "Add a test that reaches this read and leaves `{name}` at its default, `{}`",
                flag.written(first)
            )
        } else {
            format!(
                "Add a test that reaches this read with `flags: {{ {name}: {} }}`",
                flag.written(first)
            )
        };
        out.push(
            Diagnostic::new(&rule::FLAG_VALUE_UNTESTED, message)
                .at(read.span.clone(), "not tested with every value")
                .reason(format!(
                    "Each read of a flag is tested with every value it can have, so each branch is known to work before the flag is switched. {reached} A test that doesn't name the flag in its `flags:` gives it the `default:`, `{}`.",
                    flag.written(flag.default)
                ))
                .next(next),
        );
    }
    out
}
