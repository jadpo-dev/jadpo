//! Polls (area 11, "Polls"; area 12, "Events from outside"; chunk 7.6c):
//! polling turns an API without webhooks into events.
//!
//! A provider package's `polls:` in `provider.jadpo` names each poll and
//! gives what it calls, `call:`, an operation of the package that takes
//! no input and changes nothing; how often, `every:`, a minute or more;
//! and what it emits, `emits:`, a `public` event of the package with no
//! `wire_name:` (that marks one its provider sends), each of whose fields
//! is a field of the operation's answer of the same type. The event is the
//! package's own, as one its action emits is, and carries only what
//! matters: an answer whose other fields changed (when it was fetched)
//! changes nothing.
//!
//! [`declared`] reads and checks every package's polls; [`polls`] gives
//! each connection's that run: those whose event the app subscribes to,
//! at the connection's interval (`polls: { latest: 6h }`) or the
//! package's, kept where the package's webhooks would be.

use std::collections::BTreeMap;

use jadpo_db::{Db, ItemKind, Ownership, PackageHome, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Key;
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::lookups::{Lookup, NameView};
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, FieldsBase, Ty, TypeView, WrittenCoding, type_table};

/// The keys each poll takes, each required.
pub const POLL_KEYS: [&str; 3] = ["call", "every", "emits"];

/// The shortest interval a poll runs at: the app's tick runs once a
/// minute.
pub const LEAST_EVERY_MS: u64 = 60_000;

/// One poll a provider package declares, read and checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeclaredPoll {
    /// Its name in `polls:`, `latest`.
    pub name: String,
    /// The operation it calls, `prices.latest`.
    pub call: String,
    /// `every:`, in milliseconds.
    pub every_ms: u64,
    /// The event it emits: its key, and its name in the package.
    pub event: Key,
    pub event_name: String,
    /// The event's fields, each taken from the answer's of its name.
    pub fields: Vec<String>,
}

/// Every provider package's polls, by its folder, and their problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declared {
    pub of: BTreeMap<String, Vec<DeclaredPoll>>,
    pub problems: Vec<Diagnostic>,
}

/// One connection's poll that runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PollModel {
    pub connection: String,
    pub name: String,
    pub call: String,
    /// The connection's interval, or the package's.
    pub every_ms: u64,
    pub event: Key,
    pub fields: Vec<String>,
    /// Where its events, and what it last saw, are kept.
    pub store: String,
}

/// The polls that run, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Polls {
    pub polls: Vec<PollModel>,
    pub problems: Vec<Diagnostic>,
}

/// The names of the events each package's polls emit, as written, by the
/// package's folder: read from `provider.jadpo` alone, so a package that
/// receives webhooks tells its own events from its provider's without
/// checking the polls (chunk 7.6b's rule for what its actions emit).
#[salsa::tracked(returns(ref))]
pub fn emitted(
    db: &dyn Db,
    project: Project,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    let mut out: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for (home, _, decl) in providers(db, project) {
        for (_, member) in polls_of(&decl) {
            for (key, inner) in tree::members(member.block()) {
                if name_text(&key).as_deref() == Some("emits")
                    && let Some(value) = inner.value()
                {
                    out.entry(home.folder.clone())
                        .or_default()
                        .insert(tree::written(value.syntax()));
                }
            }
        }
    }
    out
}

/// Each provider package's `provider { }`, with its home and path.
fn providers(db: &dyn Db, project: Project) -> Vec<(PackageHome, String, ast::SingletonDecl)> {
    let mut out = Vec::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let Ownership::Package(home) = &location.ownership else {
            continue;
        };
        let path = file.path(db).clone();
        if path != format!("{}/{}", home.folder, jadpo_db::PROVIDER_FILE) {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let found = source.decls().find_map(|decl| match decl {
            Decl::SingletonDecl(decl)
                if decl
                    .keyword()
                    .is_some_and(|keyword| keyword.text() == "provider") =>
            {
                Some(decl)
            }
            _ => None,
        });
        if let Some(decl) = found {
            out.push((home.clone(), path, decl));
        }
    }
    out
}

/// The entries of `provider { }`'s `polls:`, each a name and its member.
fn polls_of(decl: &ast::SingletonDecl) -> Vec<(ast::Name, ast::Member)> {
    tree::members(decl.member_list())
        .into_iter()
        .filter(|(key, _)| name_text(key).as_deref() == Some("polls"))
        .flat_map(|(_, member)| tree::members(member.block()))
        .collect()
}

/// Every provider package's polls, read and checked.
#[salsa::tracked(returns(ref))]
pub fn declared(db: &dyn Db, project: Project) -> Declared {
    let mut out = Declared::default();
    for (home, path, decl) in providers(db, project) {
        let written = tree::members(decl.member_list())
            .into_iter()
            .find(|(key, _)| name_text(key).as_deref() == Some("polls"));
        let Some((_, member)) = written else {
            continue;
        };
        if member.block().is_none() {
            out.problems.push(
                Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, "`polls:` isn't a block")
                    .at(node_span(&path, member.syntax()), "not a block")
                    .reason(REASON)
                    .next("Write `polls: { <name>: { call: <resource>.<operation>  every: 1d  emits: <Event> } }`"),
            );
            continue;
        }
        let reader = Reader {
            db,
            project,
            home: &home,
            path: &path,
        };
        for (name, entry) in polls_of(&decl) {
            let Some(name) = name_text(&name) else {
                continue;
            };
            if let Some(poll) = reader.poll(&name, &entry, &mut out.problems) {
                out.of.entry(home.folder.clone()).or_default().push(poll);
            }
        }
    }
    out
}

const REASON: &str = "Each poll names the operation the runtime calls, `call:`, which takes nothing and changes nothing; how often, `every:`, a minute or more; and the package's public event its answer is made into, `emits:`, kept only when it changed.";

/// Reads one package's polls.
struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    home: &'a PackageHome,
    path: &'a str,
}

impl Reader<'_> {
    fn invalid(&self, span: Span, message: String, label: &str) -> Diagnostic {
        Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message).at(span, label.to_string())
    }

    fn value_span(&self, member: &ast::Member) -> Span {
        member.value().map_or_else(
            || node_span(self.path, member.syntax()),
            |value| node_span(self.path, value.syntax()),
        )
    }

    /// Poll `name`, from `member`.
    fn poll(
        &self,
        name: &str,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) -> Option<DeclaredPoll> {
        let span = node_span(self.path, member.syntax());
        let Some(block) = member.block() else {
            out.push(
                self.invalid(span, format!("Poll `{name}` isn't a block"), "not a block")
                    .reason(REASON)
                    .next(format!(
                        "Write `{name}: {{ call: <resource>.<operation>  every: 1d  emits: <Event> }}`"
                    )),
            );
            return None;
        };
        let mut found: BTreeMap<String, ast::Member> = BTreeMap::new();
        let before = out.len();
        for (key, inner) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            let key_span = node_span(self.path, key.syntax());
            if !POLL_KEYS.contains(&written.as_str()) {
                let near = nearest(&written, POLL_KEYS.iter().copied());
                let problem = self
                    .invalid(
                        key_span.clone(),
                        format!("Poll `{name}` has no key `{written}:`"),
                        "not a poll key",
                    )
                    .reason(REASON);
                out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}:` instead of `{written}:`"))
                        .edit(key_span, near),
                    None => problem.next(format!("Remove `{written}:`")),
                });
                continue;
            }
            found.insert(written, inner);
        }
        for key in POLL_KEYS {
            if !found.contains_key(key) {
                out.push(
                    self.invalid(
                        span.clone(),
                        format!("Poll `{name}` doesn't give `{key}:`"),
                        &format!("no `{key}:`"),
                    )
                    .reason(REASON)
                    .next(match key {
                        "call" => {
                            "Add `call:` with the operation it calls, `<resource>.<operation>`"
                        }
                        "every" => "Add `every:` with how often it runs, such as `every: 1d`",
                        _ => "Add `emits:` with the package's public event its answer is made into",
                    }),
                );
            }
        }
        if out.len() > before {
            return None;
        }
        let call = self.call(name, &found["call"], out);
        let every_ms = self.every(name, &found["every"], out);
        let (operation, output) = call?;
        let (event, event_name, fields) =
            self.emits(name, &found["emits"], (&operation, &output), out)?;
        Some(DeclaredPoll {
            name: name.to_string(),
            call: operation,
            every_ms: every_ms?,
            event,
            event_name,
            fields,
        })
    }

    /// `call:`: an operation of the package that takes nothing, changes
    /// nothing, and answers with an object its types read. Its name, and
    /// its answer's type as the checker knows it.
    fn call(
        &self,
        poll: &str,
        member: &ast::Member,
        out: &mut Vec<Diagnostic>,
    ) -> Option<(String, String)> {
        let span = self.value_span(member);
        let written = member
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let operations = crate::providers::operations(self.db, self.project);
        let found = written
            .split_once('.')
            .and_then(|(resource, name)| operations.find(&self.home.folder, resource, name));
        let Some(operation) = found else {
            // An installed package's resource no connection reaches isn't
            // compiled (chunk 7.1c2): its poll is checked once it is.
            let waiting = written.split_once('.').is_some_and(|(resource, _)| {
                let file = format!("{}/resources/{resource}.jadpo", self.home.folder);
                self.home.installed
                    && self
                        .project
                        .lazy(self.db)
                        .iter()
                        .any(|lazy| *lazy.path(self.db) == file)
            });
            if waiting {
                return None;
            }
            let known: Vec<String> = operations
                .of(&self.home.folder)
                .map(crate::providers::Operation::full_name)
                .collect();
            let problem = self
                .invalid(
                    span.clone(),
                    format!(
                        "Poll `{poll}` calls `{written}`, which isn't one of the package's operations"
                    ),
                    "no such operation",
                )
                .reason(REASON);
            out.push(match nearest(&written, known.iter().map(String::as_str)) {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{written}`"))
                    .edit(span, near),
                None => {
                    problem.next("Call one of the package's operations, `<resource>.<operation>`")
                }
            });
            return None;
        };
        let refused = |why: &str, label: &str, next: &str| {
            self.invalid(
                span.clone(),
                format!("Poll `{poll}` calls `{written}`, {why}"),
                label,
            )
            .reason("A poll calls the same operation every time, with nothing from the app: it reads a feed that doesn't depend on the app's data (rates, a price list), changes nothing at the provider, and answers with an object the package's types read.")
            .next(next.to_string())
        };
        if operation.input.is_some() {
            out.push(refused(
                "which takes an input",
                "takes an input",
                "Call an operation with no `input:`; checking each of the app's own records is the app's schedule calling the package",
            ));
            return None;
        }
        if operation.changes {
            out.push(refused(
                "which changes something",
                "changes something",
                "Call an operation that only reads",
            ));
            return None;
        }
        let table = type_table(self.db, self.project);
        let output = match &operation.output {
            Ty::Named(name) if table.base(name) == Some(FieldsBase::Object) => name.clone(),
            _ => {
                out.push(refused(
                    "which gives no object",
                    "gives no object",
                    "Call an operation whose `output:` is an `Object` of the package",
                ));
                return None;
            }
        };
        let functions = operation.decode.is_some()
            || table
                .fields(&output)
                .unwrap_or_default()
                .iter()
                .any(|field| matches!(field.coding, Some(WrittenCoding::Functions { .. })));
        if functions {
            out.push(refused(
                "whose answer is read by the package's own functions",
                "read by a function",
                "Call an operation whose answer its types read, with built-in encodings (`wire:`)",
            ));
            return None;
        }
        Some((written, output))
    }

    /// `every:`: a duration of a minute or more.
    fn every(&self, poll: &str, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<u64> {
        let millis = match member.value().as_ref().map(Const::from_expr) {
            Some(Const::Duration(text)) => jadpo_std::Dur::parse(&text)
                .and_then(|dur| u64::try_from(dur.millis).ok())
                .filter(|millis| *millis >= LEAST_EVERY_MS),
            _ => None,
        };
        if millis.is_none() {
            out.push(
                self.invalid(
                    self.value_span(member),
                    format!("Poll `{poll}`'s `every:` must be a duration of at least a minute"),
                    "not a minute or more",
                )
                .reason("A poll is called by the app's tick, which runs once a minute, so it runs at most once a minute; a connection may call it less often.")
                .next("Write how often the feed changes, such as `every: 1d`, a minute at least"),
            );
        }
        millis
    }

    /// `emits:`: a public event of the package without `wire_name:`, each
    /// of whose fields is a field of `call`'s answer of the same type.
    fn emits(
        &self,
        poll: &str,
        member: &ast::Member,
        (call, output): (&str, &str),
        out: &mut Vec<Diagnostic>,
    ) -> Option<(Key, String, Vec<String>)> {
        let span = self.value_span(member);
        let written = member
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let ownership = Ownership::Package(self.home.clone());
        let lookup = Lookup::within(self.db, self.project, &ownership);
        let table = type_table(self.db, self.project);
        let symbol = lookup
            .declared(&written)
            .iter()
            .find(|symbol| {
                matches!(symbol.kind, ItemKind::Type(_))
                    && symbol.ownership == ownership
                    && table.base(&symbol.canonical) == Some(FieldsBase::Event)
            })
            .cloned();
        let Some(symbol) = symbol else {
            out.push(
                self.invalid(
                    span,
                    format!("Poll `{poll}` emits `{written}`, which isn't one of the package's events"),
                    "not an event of the package",
                )
                .reason(REASON)
                .next("Declare the event as `public type <Name> = Event { ... }` in the package's `events/`, and name it in `emits:`"),
            );
            return None;
        };
        let refused = |why: &str, label: &str, reason: &str, next: &str| {
            self.invalid(
                span.clone(),
                format!("Poll `{poll}` emits `{written}`, {why}"),
                label,
            )
            .reason(reason.to_string())
            .next(next.to_string())
        };
        if !symbol.public {
            out.push(refused(
                "which isn't public",
                "not public",
                "A poll runs only while the app subscribes to its event, and the app subscribes only to a package's public events.",
                &format!("Declare `public type {written} = Event {{ ... }}`"),
            ));
            return None;
        }
        if gives_wire_name(self.db, self.project, &symbol.path, &written) {
            out.push(refused(
                "which gives `wire_name:`",
                "a webhook's event",
                "`wire_name:` marks an event the provider sends in a webhook; the event a poll emits is the package's own, made from the answer.",
                "Remove `wire_name:` from the event, or emit an event of the package's own",
            ));
            return None;
        }
        let answer = table.fields(output).unwrap_or_default();
        let mut fields = Vec::new();
        for field in table.fields(&symbol.canonical).unwrap_or_default() {
            let label = match answer.iter().find(|given| given.name == field.name) {
                None => format!("`{}` isn't in the answer", field.name),
                Some(given)
                    if given.declared != field.declared || given.optional != field.optional =>
                {
                    format!("`{}` isn't the answer's type", field.name)
                }
                Some(_) => {
                    fields.push(field.name.clone());
                    continue;
                }
            };
            out.push(
                self.invalid(
                    span,
                    format!("Poll `{poll}` can't make `{written}` from `{call}`'s answer"),
                    &label,
                )
                .reason(format!(
                    "A poll's event is made from its answer, each of the event's fields taken from the answer's field of its name and type; only those fields count when the runtime asks whether the answer changed. `{call}` answers with `{output}`."
                ))
                .next(format!(
                    "Give `{written}` only fields `{output}` has, with the same types"
                )),
            );
            return None;
        }
        let key = crate::graph::declaration_key(&lookup, &symbol.canonical)?;
        Some((key, written, fields))
    }
}

/// Whether event `name`, declared in the file at `path`, gives
/// `wire_name:`.
fn gives_wire_name(db: &dyn Db, project: Project, path: &str, name: &str) -> bool {
    let Some(file) = project
        .files(db)
        .iter()
        .copied()
        .find(|file| file.path(db) == path)
    else {
        return false;
    };
    let Some(source) = tree::source(db, file) else {
        return false;
    };
    source.decls().any(|decl| match decl {
        Decl::TypeDecl(decl) => {
            decl.name().as_ref().and_then(name_text).as_deref() == Some(name)
                && tree::members(decl.member_list())
                    .iter()
                    .any(|(key, _)| name_text(key).as_deref() == Some(crate::types::WIRE_NAME))
        }
        _ => false,
    })
}

/// Each connection's polls that run: those whose event an app's
/// subscriber listens for, at the connection's interval or the package's,
/// kept in the store the package's webhooks would be; with the problems
/// found (no such store).
#[salsa::tracked(returns(ref))]
pub fn polls(db: &dyn Db, project: Project) -> Polls {
    let mut out = Polls::default();
    let declared = declared(db, project);
    if declared.of.is_empty() {
        return out;
    }
    let subscribers = crate::events::subscribers(db, project);
    let store = crate::webhooks::store(db, project);
    for connection in &crate::connections::package_connections(db, project).connections {
        let Some(polls) = declared.of.get(&connection.folder) else {
            continue;
        };
        for poll in polls {
            let heard = subscribers
                .iter()
                .any(|subscriber| subscriber.package.is_none() && subscriber.event == poll.event);
            if !heard {
                continue;
            }
            let Some(store) = store.clone() else {
                let (path, start, end) = &connection.name_at;
                let databases = crate::data::connections(db, project).databases();
                let named = databases.first().cloned().unwrap_or_else(|| "main".into());
                out.problems.push(
                    Diagnostic::new(
                        &rule::EVENT_STORE_REQUIRED,
                        format!(
                            "`{}` polls `{}` but has no store to keep its events in",
                            connection.name, poll.name
                        ),
                    )
                    .at(Span::new(path.clone(), *start, *end), "no outbox")
                    .reason("Each change a poll sees is kept in the outbox until the app's subscribers have it, with what the poll last saw beside it: in the store `events: { store: }` names, or the only store that holds the app's records.")
                    .next(format!(
                        "Add `events: {{ store: {named} }}` to `application {{ }}` in `app.jadpo`, naming a database that holds the app's records"
                    )),
                );
                continue;
            };
            let every_ms = connection
                .polls
                .iter()
                .find(|(name, _)| *name == poll.name)
                .map_or(poll.every_ms, |(_, every)| *every);
            out.polls.push(PollModel {
                connection: connection.name.clone(),
                name: poll.name.clone(),
                call: poll.call.clone(),
                every_ms,
                event: poll.event.clone(),
                fields: poll.fields.clone(),
                store,
            });
        }
    }
    out
}
