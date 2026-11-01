//! What leaves the app through each connection (area 15, "The personal-data
//! report"; area 11, "What a release shows"; chunk V5): the walk for what
//! reaches a model ([`super::models`]), which already follows a value
//! through variables, calls, events and text built from it, keeps each
//! holder of personal data at each place a value goes out.
//!
//! Those places, an [`Exit`] each, are every outside call a body can make
//! and the emails and replies Jadpo sends for it:
//!
//! - **A provider's operation**, `pay.customers.create(input)`, with what
//!   its input's type holds, a package's own marks among them.
//! - **A package's action**, `pay.start(input)`. Everything it is passed
//!   counts as leaving, whatever the action does with it, since a package
//!   action exists to call its API and nothing can check an installed
//!   one's use of a value more closely than that; what its own operations
//!   send is added under their names.
//! - **A file store's `put`**, and the file a writer's rows go into.
//! - **A model**, through the connection `ai:` names: each prompt built
//!   and each text embedded, whichever call then takes it.
//! - **An email**, through the connection `email:` names: each one
//!   emitted, and the address a magic link goes to.
//! - **A webhook's reply**, its failures' client fields too, which go back
//!   to the connection it came from.
//!
//! The rule is that nothing is reported as not reaching a connection when
//! it does. So what decided a value leaves with it ([`Left`]'s last part),
//! a parameter is followed to what every caller passes and counts for what
//! its own type holds, an event's part to what every `emit` put there and
//! to the event's own marks, a value
//! whose type says it is a marked field counts as that field however it
//! got there ([`Src::Declared`]), a provider's answer may hold whatever
//! its call was sent ([`Src::Answered`]), and a library function's result
//! whatever it was given. A cycle of calls is the summaries' fixpoint,
//! which only grows.
//!
//! The walk runs only for a body that type-checked: `jadpo check` writes
//! the report only when the whole project has no errors, since a body with
//! errors may have calls the walk can't see.
//!
//! Stores and caches aren't exits: what each keeps is the report's next
//! part (chunk P8), and a store's own connection is reached only by the
//! runtime.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_graph::body::{Body, Callee, OWN_CONNECTION, SiteId};
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::files::FilesOp;
use jadpo_std::http::PrincipalKind;

use super::models::{Held, event_part, holders};
use super::{Flow, Prov, Src, Summary, Walk, failed, model_payloads, model_summary, shallow};
use crate::ai_personal::{self, Holder, Holding};
use crate::data::Entities;
use crate::lookups::{Lookup, Name, NameView, file_at};
use crate::policy::proof::Prover;
use crate::types::{
    Callable, FieldsBase, Ty, TypeView, body_facts, callables_of, resolve, typed_body,
};

/// How a value leaves through a connection.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Through {
    /// A provider's operation, `customers.create`.
    Operation(String),
    /// A package's action, `start_subscription`.
    Action(String),
    /// A file put in a store, or a file a writer's rows go into.
    File,
    /// A prompt sent to a model, by its type's name.
    Prompt(String),
    /// A text sent for an embedding.
    Embedding,
    /// An email, by its type's name.
    Email(String),
    /// A webhook's reply, by the action that answers it.
    Reply(String),
}

impl Through {
    /// In a few words: `customers.create`, `prompt SummariseNotes`.
    pub fn named(&self) -> String {
        match self {
            Through::Operation(name) | Through::Action(name) => name.clone(),
            Through::File => "a file".to_string(),
            Through::Prompt(name) => format!("prompt {name}"),
            Through::Embedding => "an embedding".to_string(),
            Through::Email(name) => format!("email {name}"),
            Through::Reply(name) => format!("reply {name}"),
        }
    }

    /// Its kind, in the report's JSON twin.
    pub fn kind(&self) -> &'static str {
        match self {
            Through::Operation(_) => "operation",
            Through::Action(_) => "action",
            Through::File => "file",
            Through::Prompt(_) => "prompt",
            Through::Embedding => "embedding",
            Through::Email(_) => "email",
            Through::Reply(_) => "reply",
        }
    }
}

/// One way out of the app.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Exit {
    /// [`OWN_CONNECTION`] for a package's own operation, until a call
    /// names the connection.
    pub connection: String,
    pub through: Through,
}

impl Exit {
    /// This exit, gone through `connection` when it is a package's own.
    fn on(&self, connection: &str) -> Exit {
        Exit {
            connection: if self.connection == OWN_CONNECTION {
                connection.to_string()
            } else {
                self.connection.clone()
            },
            through: self.through.clone(),
        }
    }
}

/// A holder of personal data leaving by an exit, and whether it only
/// decided what was sent, rather than being in it.
pub type Left = (Exit, Holder, bool);

impl Walk<'_> {
    /// `prov` leaves by `exit`, with what `declared` holds: each holder of
    /// personal data in it, or that decided it, and each parameter's part
    /// for the callers to follow.
    fn sends(&mut self, flow: &Flow<'_>, exit: &Exit, prov: &Prov, declared: &[Ty]) {
        if !(self.personal && self.report) {
            return;
        }
        let mut params = Vec::new();
        let mut held = holders(flow, prov, |at, path| params.push((at, path)));
        for src in prov.flat() {
            hold_for_exits(flow, src, false, &mut held, &mut params);
        }
        for (at, path) in params {
            self.left_params.insert((exit.clone(), at, path));
        }
        let marks = ai_personal::marks(flow.db, flow.project);
        for ty in declared {
            let mut found = BTreeMap::new();
            ai_personal::deep(&flow.lookup, marks, ty, &mut found, &mut BTreeSet::new());
            for (holder, holding) in found {
                held.hold(holder, &holding, false);
            }
        }
        held.fold_types();
        for (holder, (holding, decided)) in held.0 {
            if !holding.categories.is_empty() {
                self.left.insert((exit.clone(), holder, decided));
            }
        }
    }

    /// A value at `site` whose type is a marked field's counts as that
    /// field, wherever it came from.
    pub(super) fn typed_as_field(&mut self, flow: &Flow<'_>, site: SiteId, prov: &mut Prov) {
        let mut ty = self.value_type(site);
        while let TypeRef::Nullable(inner) | TypeRef::List(inner) | TypeRef::Set(inner) = ty {
            ty = inner;
        }
        let TypeRef::Field(key) = ty else {
            return;
        };
        let Some((host, field)) = key.name().rsplit_once('.') else {
            return;
        };
        self.read_marked(flow, host, field, prov);
    }

    /// `host`'s field `field` is read into `out`: when it is personal data
    /// it counts as that field wherever the value goes out.
    pub(super) fn read_marked(&mut self, flow: &Flow<'_>, host: &str, field: &str, out: &mut Prov) {
        let marks = ai_personal::marks(flow.db, flow.project);
        let (host, field) = field_origin(flow, host, field);
        if marks
            .mark(&host, &field)
            .is_some_and(|mark| mark.category.is_some())
        {
            out.add(Src::Declared { host, field });
        }
    }

    /// The outside call of `callee` with `args`, each holding `provs`:
    /// what it sends, and what decided that it runs.
    pub(super) fn called_out(
        &mut self,
        flow: &Flow<'_>,
        callee: &Callee,
        args: &[Option<SiteId>],
        provs: &[Prov],
    ) {
        if !self.report {
            return;
        }
        let mut all = Prov::default();
        for prov in provs {
            all.union(prov);
        }
        let all = self.with_pc(all);
        let declared = || -> Vec<Ty> {
            args.iter()
                .flatten()
                .filter_map(|at| ty_of(self.value_type(*at)))
                .collect()
        };
        match callee {
            Callee::Provider {
                connection,
                operation,
            } => {
                let declared = declared();
                self.sends(
                    flow,
                    &Exit {
                        connection: connection.clone(),
                        through: Through::Operation(operation.clone()),
                    },
                    &all,
                    &declared,
                );
            }
            Callee::PackageAction { connection, action } => {
                let declared = declared();
                let name = action.name().rsplit('.').next().unwrap_or_default();
                self.sends(
                    flow,
                    &Exit {
                        connection: connection.clone(),
                        through: Through::Action(name.to_string()),
                    },
                    &all,
                    &declared,
                );
                // What its own operations send, under their names.
                let summary = flow.summary(action);
                for (exit, holder, decided) in &summary.left {
                    self.left
                        .insert((exit.on(connection), holder.clone(), *decided));
                }
                for (exit, at, path) in &summary.left_params {
                    let prov = provs.get(*at).map(|p| p.at(path)).unwrap_or_default();
                    let prov = self.with_pc(prov);
                    self.sends(flow, &exit.on(connection), &prov, &[]);
                }
            }
            Callee::Files {
                connection,
                op: FilesOp::Put,
            } => {
                let declared = declared();
                self.sends(
                    flow,
                    &Exit {
                        connection: connection.clone(),
                        through: Through::File,
                    },
                    &all,
                    &declared,
                );
            }
            Callee::Embed { .. } => {
                if let Some(connection) = &crate::ai::settings(flow.db, flow.project).connection {
                    self.sends(
                        flow,
                        &Exit {
                            connection: connection.clone(),
                            through: Through::Embedding,
                        },
                        &all,
                        &[],
                    );
                }
            }
            _ => {}
        }
    }

    /// A prompt `shown` built with `prov` in a placeholder of type
    /// `declared`: it goes to the model `ai:` names.
    pub(super) fn prompted_out(
        &mut self,
        flow: &Flow<'_>,
        shown: &str,
        prov: &Prov,
        declared: Option<&Ty>,
    ) {
        if let Some(connection) = &crate::ai::settings(flow.db, flow.project).connection {
            let declared: Vec<Ty> = declared.cloned().into_iter().collect();
            self.sends(
                flow,
                &Exit {
                    connection: connection.clone(),
                    through: Through::Prompt(shown.to_string()),
                },
                prov,
                &declared,
            );
        }
    }

    /// An `emit` of `event` holding `prov`: an email goes out through the
    /// connection `email:` names.
    pub(super) fn emailed(&mut self, flow: &Flow<'_>, event: &Key, prov: &Prov) {
        if flow.lookup.base(event.name()) != Some(FieldsBase::EmailMessage) {
            return;
        }
        if let Some(connection) = &crate::emails::settings(flow.db, flow.project).connection {
            let prov = self.with_pc(prov.clone());
            let name = event.name().rsplit('.').next().unwrap_or_default();
            self.sends(
                flow,
                &Exit {
                    connection: connection.clone(),
                    through: Through::Email(name.to_string()),
                },
                &prov,
                &[Ty::Named(event.name().to_string())],
            );
        }
    }

    /// A file holding `rows`, written to the store `connection`.
    pub(super) fn filed(&mut self, flow: &Flow<'_>, connection: &str, rows: &Prov) {
        self.sends(
            flow,
            &Exit {
                connection: connection.to_string(),
                through: Through::File,
            },
            rows,
            &[],
        );
    }

    /// A reply action's failure holding `prov` in a client field: it goes
    /// back where the webhook came from, as its reply does.
    pub(super) fn held_in_reply(&mut self, flow: &Flow<'_>, prov: &Prov) {
        if !self.reply {
            return;
        }
        let bare = self.name.rsplit('.').next().unwrap_or_default().to_string();
        for route in &crate::webhooks::webhooks(flow.db, flow.project).routes {
            if route
                .replies
                .iter()
                .any(|(_, action)| action.name() == self.name)
            {
                self.sends(
                    flow,
                    &Exit {
                        connection: route.connection.clone(),
                        through: Through::Reply(bare.clone()),
                    },
                    prov,
                    &[],
                );
            }
        }
    }

    /// A call of something with `summary`, passed `provs`: what it sends,
    /// in this body's terms.
    pub(super) fn passed_out(&mut self, flow: &Flow<'_>, summary: &Summary, provs: &[Prov]) {
        if !self.report {
            return;
        }
        // What decided that the call runs decides what it sends.
        let decided = self.with_pc(Prov::default());
        for (exit, holder, only_decided) in &summary.left {
            self.left
                .insert((exit.clone(), holder.clone(), *only_decided));
            self.sends(flow, exit, &decided, &[]);
        }
        for (exit, at, path) in &summary.left_params {
            let prov = provs.get(*at).map(|p| p.at(path)).unwrap_or_default();
            let prov = self.with_pc(prov);
            self.sends(flow, exit, &prov, &[]);
        }
    }
}

/// How far a field's type is followed to the field it is one of.
const DEPTH: usize = 8;

/// The field `host`'s `field` is a copy of: itself when it has a mark of its
/// own or a type of its own, or the one its type names, as an output picking
/// `User.notes` does.
pub(super) fn field_origin(flow: &Flow<'_>, host: &str, field: &str) -> (String, String) {
    let marks = ai_personal::marks(flow.db, flow.project);
    let mut at = (host.to_string(), field.to_string());
    for _ in 0..DEPTH {
        if marks.mark(&at.0, &at.1).is_some() {
            break;
        }
        let Some(def) = flow.lookup.field(&at.0, std::slice::from_ref(&at.1)) else {
            break;
        };
        let mut ty = &def.declared;
        while let Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) = ty {
            ty = inner;
        }
        match ty {
            Ty::Field(next, path) => match path.as_slice() {
                [only] => at = (next.clone(), only.clone()),
                _ => break,
            },
            _ => break,
        }
    }
    at
}

/// What `src` holds that only what leaves through a connection counts: a
/// marked field the value's type says it is, and what a provider's answer
/// may hold ([`Src::Declared`], [`Src::Answered`]).
fn hold_for_exits(
    flow: &Flow<'_>,
    src: Src,
    decided: bool,
    held: &mut Held,
    params: &mut Vec<(usize, Vec<String>)>,
) {
    match shallow(src) {
        Src::Decided(inner) => hold_for_exits(flow, *inner, true, held, params),
        Src::Declared { host, field } => {
            let marks = ai_personal::marks(flow.db, flow.project);
            if let Some(holding) = ai_personal::field_holding(&flow.lookup, marks, &host, &field) {
                held.hold(Holder::Field { host, field }, &holding, decided);
            }
        }
        Src::Answered(inner) => {
            let answered = holders(flow, &Prov::of((*inner).clone()), |at, path| {
                params.push((at, path));
            });
            for (holder, (holding, only_decided)) in answered.0 {
                held.hold(holder, &holding, decided || only_decided);
            }
            hold_for_exits(flow, *inner, decided, held, params);
        }
        _ => {}
    }
}

/// What each part of a parameter of `callable` that leaves says it holds:
/// its declared type's marks and standard types. So a parameter that comes
/// from a request, whose caller the walk never sees, counts for what it is
/// typed as, however it was passed on.
fn parameters_out(
    flow: &Flow<'_>,
    callable: Callable<'_>,
    summary: &Summary,
    out: &mut BTreeSet<Left>,
) {
    if summary.left_params.is_empty() {
        return;
    }
    let body = &typed_body(flow.db, flow.project, callable).body;
    let marks = ai_personal::marks(flow.db, flow.project);
    for (exit, at, path) in &summary.left_params {
        let Some(param) = body.params.get(*at) else {
            continue;
        };
        let Some(ty) = ty_of(&body.local(*param).ty) else {
            continue;
        };
        let ty = part_of(flow, ty, path);
        let mut found = BTreeMap::new();
        ai_personal::deep(&flow.lookup, marks, &ty, &mut found, &mut BTreeSet::new());
        for (holder, holding) in found {
            if !holding.categories.is_empty() {
                out.insert((exit.clone(), holder, false));
            }
        }
    }
}

/// The type of the part `path` of a value of `ty`; `ty` itself when it can't
/// be told, which holds everything the part does.
fn part_of(flow: &Flow<'_>, ty: Ty, path: &[String]) -> Ty {
    if path.is_empty() {
        return ty;
    }
    let mut host = &ty;
    while let Ty::Nullable(inner) | Ty::List(inner) | Ty::Set(inner) = host {
        host = inner;
    }
    let found = match host {
        Ty::Named(host) => flow
            .lookup
            .field(host, path)
            .map(|def| def.declared.clone()),
        _ => None,
    };
    found.unwrap_or(ty)
}

/// `ty` as the type tables write it, for [`ai_personal::deep`].
fn ty_of(ty: &TypeRef) -> Option<Ty> {
    Some(match ty {
        TypeRef::List(inner) => Ty::List(Box::new(ty_of(inner)?)),
        TypeRef::Set(inner) => Ty::Set(Box::new(ty_of(inner)?)),
        TypeRef::Nullable(inner) => Ty::Nullable(Box::new(ty_of(inner)?)),
        TypeRef::Map(key, value) => Ty::Map(Box::new(ty_of(key)?), Box::new(ty_of(value)?)),
        TypeRef::Std(name, args) => Ty::Opaque(
            name.clone(),
            args.iter().map(ty_of).collect::<Option<Vec<Ty>>>()?,
        ),
        TypeRef::Node(key) => Ty::Named(key.name().to_string()),
        TypeRef::Field(key) => {
            let (host, field) = key.name().rsplit_once('.')?;
            Ty::Field(host.to_string(), vec![field.to_string()])
        }
        _ => return None,
    })
}

/// The callable a package's own action or function `key` names, by the
/// name the tables know it by, `AcmePay.start`.
pub(super) fn package_callable<'db>(
    db: &'db dyn Db,
    project: Project,
    key: &Key,
) -> Option<Callable<'db>> {
    let lookup = Lookup::new(db, project);
    let own = key.name().rsplit('.').next()?;
    lookup
        .facts(own)
        .declarations
        .iter()
        .filter(|symbol| {
            symbol.ownership.package().is_some()
                && symbol.library.is_none()
                && symbol.canonical == key.name()
        })
        .filter_map(|symbol| {
            let file = file_at(db, project, Name::new(db, symbol.path.clone()))?;
            Some(Callable::new(db, file, symbol.index, None))
        })
        .rfind(|callable| body_facts(db, project, *callable).key.as_ref() == Some(key))
}

fn flow_of<'a>(db: &'a dyn Db, project: Project) -> Flow<'a> {
    Flow {
        db,
        project,
        prover: Prover::new(db, project),
        lookup: Lookup::new(db, project),
        entities: Entities::new(db, project),
        personal: true,
    }
}

/// Everything that leaves the app through its connections.
#[salsa::tracked(returns(ref))]
pub fn left(db: &dyn Db, project: Project) -> BTreeSet<Left> {
    let flow = flow_of(db, project);
    let mut out = BTreeSet::new();
    // Every action and function of the app's, each with what it calls. A
    // package's are read where the app calls them, or under its
    // subscribers below.
    for &file in project.files(db) {
        if file.path(db).ends_with(".test.jadpo")
            || jadpo_db::location(db, project, file)
                .ownership
                .package()
                .is_some()
        {
            continue;
        }
        for callable in callables_of(db, file) {
            let Some(key) =
                body_facts(db, project, callable).key_of(&[Kind::Action, Kind::Function])
            else {
                continue;
            };
            if resolve(db, project, key) != Some(callable) {
                continue;
            }
            let summary = model_summary(db, project, callable);
            out.extend(summary.left.iter().cloned());
            parameters_out(&flow, callable, summary, &mut out);
        }
    }
    let connections = &crate::connections::package_connections(db, project).connections;
    let payloads = model_payloads(db, project);
    let body = Body::default();
    // What each subscriber passes from its event, and each of a package's
    // own subscribers, which hears the webhooks of each connection to it.
    for model in crate::events::subscribers(db, project).iter() {
        let summary = flow.summary(&model.run);
        let payload = payloads.get(&model.event).cloned().unwrap_or_default();
        let mut walk = Walk::blank(&body, model.path.clone(), model.name.clone());
        walk.report = true;
        walk.personal = true;
        for (exit, at, path) in &summary.left_params {
            let Some(arg) = model.args.get(*at) else {
                continue;
            };
            let mut full = arg.clone();
            full.extend(path.iter().cloned());
            let part = event_part(&flow, &payload, model.event.name(), &full);
            walk.sends(&flow, exit, &part, &[]);
        }
        let mut sent = walk.left;
        sent.extend(summary.left.iter().cloned());
        for (exit, holder, decided) in sent {
            if exit.connection != OWN_CONNECTION {
                out.insert((exit, holder, decided));
                continue;
            }
            for connection in connections
                .iter()
                .filter(|connection| Some(&connection.folder) == model.package.as_ref())
            {
                out.insert((exit.on(&connection.name), holder.clone(), decided));
            }
        }
    }
    // A webhook's reply goes back where it came from.
    for route in &crate::webhooks::webhooks(db, project).routes {
        for (event, action) in &route.replies {
            let summary = flow.summary(action);
            let payload = payloads.get(event).cloned().unwrap_or_default();
            let provs = [event_part(&flow, &payload, event.name(), &[])];
            let mut walk = Walk::blank(&body, String::new(), action.name().to_string());
            walk.report = true;
            walk.personal = true;
            let name = action.name().rsplit('.').next().unwrap_or_default();
            let exit = Exit {
                connection: route.connection.clone(),
                through: Through::Reply(name.to_string()),
            };
            let mut shown = summary.returns.substitute(&provs);
            for src in super::decide(&failed(summary, &provs)) {
                shown.add(src);
            }
            walk.sends(&flow, &exit, &shown, &[]);
            out.extend(walk.left);
        }
    }
    // The address a magic link goes to.
    let identity = crate::identity::identity(db, project);
    if let (Some(link), Some(user)) = (&identity.magic_link, identity.variant(PrincipalKind::User))
    {
        let holder = Holder::Field {
            host: user.entity.clone(),
            field: link.address.clone(),
        };
        let marks = ai_personal::marks(db, project);
        let marked = ai_personal::field_holding(&flow.lookup, marks, &user.entity, &link.address)
            .is_some_and(|holding: Holding| !holding.categories.is_empty());
        match &link.send {
            // Its own action sends it, given the address in its request.
            Some(_) => {
                let (action, _) = crate::identity::sign_in_send_action(db, project);
                if let (Some(action), true) = (action, marked) {
                    let key = flow.lookup.registry().key(Kind::Action, action, None);
                    let summary = flow.summary(&key);
                    let mut request = Prov::default();
                    request
                        .fields
                        .entry("address".to_string())
                        .or_default()
                        .add(Src::Marked {
                            host: user.entity.clone(),
                            field: link.address.clone(),
                        });
                    let mut walk = Walk::blank(&body, String::new(), key.name().to_string());
                    walk.report = true;
                    walk.personal = true;
                    walk.passed_out(&flow, summary, &[request]);
                    out.extend(walk.left);
                }
            }
            None => {
                if let (true, Some(connection)) =
                    (marked, &crate::emails::settings(db, project).connection)
                {
                    out.insert((
                        Exit {
                            connection: connection.clone(),
                            through: Through::Email("MagicLinkEmail".to_string()),
                        },
                        holder,
                        false,
                    ));
                }
            }
        }
    }
    out
}

/// What the package in `folder` sends through its own operations, whoever
/// calls it: each operation's input as its type declares it, what the
/// package's actions and subscribers send, each marked field of its own
/// types and each standard type that is personal data, as `jadpo package
/// check` shows it (area 11, "What a release shows"). An app's own fields
/// reach it from outside, so the app's report lists those.
#[salsa::tracked(returns(ref))]
pub fn package_left<'db>(db: &'db dyn Db, project: Project, folder: Name<'db>) -> BTreeSet<Left> {
    let folder = folder.text(db);
    let flow = flow_of(db, project);
    let marks = ai_personal::marks(db, project);
    let mut out = BTreeSet::new();
    let prefix = format!("{folder}/");
    for &file in project.files(db) {
        if !file.path(db).starts_with(&prefix) || file.path(db).ends_with(".test.jadpo") {
            continue;
        }
        for callable in callables_of(db, file) {
            let Some(key) =
                body_facts(db, project, callable).key_of(&[Kind::Action, Kind::Function])
            else {
                continue;
            };
            if super::callable_of(db, project, key, true) != Some(callable) {
                continue;
            }
            let summary = model_summary(db, project, callable);
            out.extend(summary.left.iter().cloned());
            parameters_out(&flow, callable, summary, &mut out);
        }
    }
    let payloads = model_payloads(db, project);
    let body = Body::default();
    for model in crate::events::subscribers(db, project)
        .iter()
        .filter(|model| model.package.as_deref() == Some(folder.as_str()))
    {
        let summary = flow.summary(&model.run);
        let payload = payloads.get(&model.event).cloned().unwrap_or_default();
        let mut walk = Walk::blank(&body, model.path.clone(), model.name.clone());
        walk.report = true;
        walk.personal = true;
        for (exit, at, path) in &summary.left_params {
            let Some(arg) = model.args.get(*at) else {
                continue;
            };
            let mut full = arg.clone();
            full.extend(path.iter().cloned());
            let part = event_part(&flow, &payload, model.event.name(), &full);
            walk.sends(&flow, exit, &part, &[]);
        }
        out.extend(walk.left);
    }
    for operation in crate::providers::operations(db, project).of(folder) {
        let Some((input, _)) = &operation.input else {
            continue;
        };
        let mut found = BTreeMap::new();
        ai_personal::deep(&flow.lookup, marks, input, &mut found, &mut BTreeSet::new());
        for (holder, holding) in found {
            if !holding.categories.is_empty() {
                out.insert((
                    Exit {
                        connection: OWN_CONNECTION.to_string(),
                        through: Through::Operation(operation.full_name()),
                    },
                    holder,
                    false,
                ));
            }
        }
    }
    out
}
