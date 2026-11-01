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

/// What a flag's block may hold. `client:` is B10's.
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
                // `client: true` comes with `GET /flags`.
                _ => {
                    ok = false;
                    self.problems.push(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            format!("`client:` on `{name}` isn't built yet"),
                        )
                        .at(key_span, "not built yet")
                        .reason("A flag returned to browsers and apps through a generated `GET /flags` route isn't built yet: accepted, no client could read it.")
                        .next(format!("Remove `client:` from `{name}`, and pass its value in what a route answers")),
                    );
                }
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
        // `client:` isn't built yet, so it is never suggested.
        let near = nearest(word, ["default", "owner", "expires"]);
        let problem = Diagnostic::new(
            &rule::FLAG_DECLARATION_INVALID,
            format!("`{word}` isn't an option a flag takes"),
        )
        .at(span.clone(), "unknown option")
        .reason(format!(
            "`{flag}` can say `default:`, `owner:` and `expires:`.{}",
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
