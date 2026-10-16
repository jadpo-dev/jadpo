//! Following values through actions (area 9, "Field restrictions"):
//! "passing a value through a variable or another type does not escape the
//! field's rules; the compiler tracks where it came from."
//!
//! Each action's body is walked for where every value may have come from
//! ([`Prov`]): a record its body loaded, created or changed, with how the
//! caller reached it ([`Reached`]); a field of one; the caller itself, a
//! `Principal` parameter or part of one; or one of its parameters, which
//! whoever calls it passes. Two checks read it:
//!
//! - **Outputs** (`policy.output_field_unproved`). Each `Output` the body
//!   builds carries only fields every caller who reached their record may
//!   read. A parameter that reaches an output is checked where the action
//!   is called, against what the caller passes.
//! - **Creates** (`policy.role_field_create_forbidden`). A create whose
//!   entity's `create:` names only user fields sets one of them from the
//!   caller: set from the request, it could only ever succeed for the
//!   caller anyway.
//!
//! The walk follows explicit copies: assignments, fields, objects, lists,
//! `match` and the collection operations. Which records a condition lets
//! through is not a copy, and a function or library call carries all of
//! its arguments into its result. A local holds everything ever assigned
//! to it, worked out until nothing changes.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{ArmBody, Body, Callee, Expr, LocalId, Node, SiteId, Stmt};
use jadpo_graph::{Key, Kind, TypeRef};
use jadpo_std::Op;
use jadpo_std::http::PrincipalKind;
use jadpo_std::read::Cardinality;
use jadpo_std::write::WriteOp;

use super::proof::{Fact, Prover, Reached, Way};
use super::{ActionWho, Effect, Grant};
use crate::data::Entities;
use crate::data::queries::{Element, Include, IncludeKind, Source};
use crate::lookups::{Lookup, Name};
use crate::types::{Def, FieldsBase, TypeView, callables_of, typed_body};

/// Where part of a value may have come from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Src {
    /// The caller: a `Principal` value, or a part of one.
    Caller(PrincipalKind),
    /// A record of `entity`, which the caller reached as `reached`: each
    /// of its fields is that record's.
    Record { entity: String, reached: Reached },
    /// A field of a record of `entity`.
    Field {
        entity: String,
        field: String,
        reached: Reached,
    },
    /// The parameter at this position.
    Param(usize),
}

/// Where a value may have come from: as a whole, and by field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Prov {
    whole: Vec<Src>,
    fields: BTreeMap<String, Prov>,
}

impl Prov {
    fn of(src: Src) -> Prov {
        Prov {
            whole: vec![src],
            fields: BTreeMap::new(),
        }
    }

    fn add(&mut self, src: Src) {
        if !self.whole.contains(&src) {
            self.whole.push(src);
        }
    }

    fn union(&mut self, other: &Prov) {
        for src in &other.whole {
            self.add(src.clone());
        }
        for (name, prov) in &other.fields {
            self.fields.entry(name.clone()).or_default().union(prov);
        }
    }

    /// The field called `name`'s.
    fn field(&self, name: &str) -> Prov {
        let mut out = self.fields.get(name).cloned().unwrap_or_default();
        for src in &self.whole {
            match src {
                Src::Record { entity, reached } => out.add(Src::Field {
                    entity: entity.clone(),
                    field: name.to_string(),
                    reached: reached.clone(),
                }),
                other => out.add(other.clone()),
            }
        }
        out
    }

    /// Every source of every part.
    fn flat(&self) -> Vec<Src> {
        let mut out = self.whole.clone();
        for prov in self.fields.values() {
            for src in prov.flat() {
                if !out.contains(&src) {
                    out.push(src);
                }
            }
        }
        out
    }

    /// With each parameter replaced by what `args` say was passed.
    fn substitute(&self, args: &[Prov]) -> Prov {
        let mut out = Prov::default();
        for src in &self.whole {
            match src {
                Src::Param(at) => {
                    if let Some(arg) = args.get(*at) {
                        out.union(arg);
                    }
                }
                other => out.add(other.clone()),
            }
        }
        for (name, prov) in &self.fields {
            out.fields
                .entry(name.clone())
                .or_default()
                .union(&prov.substitute(args));
        }
        out
    }
}

/// What a caller of an action learns of it: where its result may have
/// come from, in terms of its parameters, and which parameters reach an
/// output it builds.
#[derive(Clone, Debug, Default)]
struct Summary {
    returns: Prov,
    sends: BTreeSet<usize>,
}

/// One action to walk: its file and body.
struct Action {
    path: String,
    name: String,
    body: Arc<Body>,
}

/// Every action's problems with what its values carry, by file.
#[salsa::tracked(returns(ref))]
pub fn flow_report(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<Diagnostic>> {
    if !super::policy_clean(db, project) {
        return BTreeMap::new();
    }
    // Nothing to follow without a field's own policy or a create only
    // user fields may make.
    let access = super::access(db, project);
    let needed = access.entities.values().any(|policy| {
        let create = policy.grants(Effect::Create);
        !policy.fields.is_empty()
            || (!create.is_empty()
                && create
                    .iter()
                    .all(|grant| matches!(grant, Grant::Caller { .. })))
    });
    if !needed {
        return BTreeMap::new();
    }
    let mut actions: BTreeMap<Key, Action> = BTreeMap::new();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            let checked = typed_body(db, project, callable);
            let Some(key) = checked
                .key
                .as_ref()
                .filter(|key| key.kind() == Some(Kind::Action))
            else {
                continue;
            };
            actions.insert(
                key.clone(),
                Action {
                    path: file.path(db).clone(),
                    name: key.name().to_string(),
                    body: Arc::clone(&checked.body),
                },
            );
        }
    }
    let mut flow = Flow {
        db,
        project,
        prover: Prover::new(db, project),
        lookup: Lookup::new(db, project),
        entities: Entities::new(db, project),
        actions: &actions,
        summaries: BTreeMap::new(),
        walking: BTreeSet::new(),
        out: BTreeMap::new(),
    };
    let keys: Vec<Key> = actions.keys().cloned().collect();
    for key in keys {
        flow.summary(&key);
    }
    let mut out = flow.out;
    for problems in out.values_mut() {
        problems.dedup();
    }
    out
}

/// The flow problems in the file at `path`.
pub fn flow_problems(db: &dyn Db, project: Project, path: &str) -> Vec<Diagnostic> {
    flow_report(db, project)
        .get(path)
        .cloned()
        .unwrap_or_default()
}

struct Flow<'a> {
    db: &'a dyn Db,
    project: Project,
    prover: Prover<'a>,
    lookup: Lookup<'a>,
    entities: Entities<'a>,
    actions: &'a BTreeMap<Key, Action>,
    summaries: BTreeMap<Key, Summary>,
    /// The actions being walked, so a call back into one reads as nothing
    /// rather than going round.
    walking: BTreeSet<Key>,
    out: BTreeMap<String, Vec<Diagnostic>>,
}

impl Flow<'_> {
    /// The summary of the action `key`, walking it the first time.
    fn summary(&mut self, key: &Key) -> Summary {
        if let Some(done) = self.summaries.get(key) {
            return done.clone();
        }
        if !self.walking.insert(key.clone()) {
            return Summary::default();
        }
        let Some(action) = self.actions.get(key) else {
            return Summary::default();
        };
        let body = Arc::clone(&action.body);
        let path = action.path.clone();
        let name = action.name.clone();
        let mut walk = Walk {
            locals: vec![Prov::default(); body.locals.len()],
            body: &body,
            path,
            name: name.clone(),
            line: self.prover.access.root.action(&name).cloned(),
            gate: None,
            returns: Prov::default(),
            sends: BTreeSet::new(),
            report: false,
            problems: Vec::new(),
        };
        walk.gate = walk
            .line
            .as_ref()
            .and_then(|line| gate_way(self.prover.access, line));
        for (at, param) in body.params.iter().enumerate() {
            let ty = &body.local(*param).ty;
            let start = match caller_kind(ty) {
                Some(kind) => Prov::of(Src::Caller(kind)),
                None => Prov::of(Src::Param(at)),
            };
            walk.locals[param.0 as usize] = start;
        }
        // Each local holds everything ever assigned to it: walk until
        // nothing changes, then once more to report.
        for _ in 0..=body.locals.len().max(1) {
            let before = walk.locals.clone();
            walk.returns = Prov::default();
            walk.block(self, &body.block);
            if walk.locals == before {
                break;
            }
        }
        walk.report = true;
        walk.returns = Prov::default();
        walk.sends.clear();
        walk.block(self, &body.block);
        let summary = Summary {
            returns: walk.returns,
            sends: walk.sends,
        };
        let problems = walk.problems;
        let at = self
            .actions
            .get(key)
            .map(|a| a.path.clone())
            .unwrap_or_default();
        self.out.entry(at).or_default().extend(problems);
        self.walking.remove(key);
        self.summaries.insert(key.clone(), summary.clone());
        summary
    }
}

/// The kind of caller a parameter's type is: `Principal.user`.
fn caller_kind(ty: &TypeRef) -> Option<PrincipalKind> {
    match ty {
        TypeRef::Nullable(inner) => caller_kind(inner),
        TypeRef::Std(name, args) if args.is_empty() => {
            [PrincipalKind::User, PrincipalKind::Service]
                .into_iter()
                .find(|kind| kind.type_name() == name)
        }
        _ => None,
    }
}

/// What a listed action's line checks as the action starts, when it
/// depends on no record (area 9, "Actions": anyone signed in, or roles
/// held across the whole application): every caller passed one of its
/// entries.
fn gate_way(access: &super::Access, line: &super::ActionGrants) -> Option<Way> {
    let mut way = Vec::new();
    for who in &line.who {
        match who {
            ActionWho::Signed => way.push(vec![Fact::Grant(Grant::Signed)]),
            ActionWho::System => way.push(vec![Fact::Grant(Grant::System)]),
            ActionWho::Role { membership, role }
                if access
                    .membership(membership)
                    .is_some_and(|held| held.scope.is_none()) =>
            {
                way.push(vec![Fact::Grant(Grant::Role {
                    membership: membership.clone(),
                    role: role.clone(),
                    via: None,
                })]);
            }
            ActionWho::Role { .. } | ActionWho::Field { .. } | ActionWho::Public => return None,
        }
    }
    Some(way)
}

/// The last part of a field's key: `budget` of `Project.budget`.
fn field_name(key: &Key) -> String {
    key.name()
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_string()
}

struct Walk<'b> {
    body: &'b Body,
    path: String,
    name: String,
    /// Its line under root `policy { actions: }`, when listed.
    line: Option<super::ActionGrants>,
    /// What that line checks as it starts, when it depends on no record.
    gate: Option<Way>,
    locals: Vec<Prov>,
    returns: Prov,
    sends: BTreeSet<usize>,
    /// The last walk, which reports.
    report: bool,
    problems: Vec<Diagnostic>,
}

impl Walk<'_> {
    fn local(&mut self, id: Option<LocalId>, prov: &Prov) {
        if let Some(id) = id
            && let Some(held) = self.locals.get_mut(id.0 as usize)
        {
            held.union(prov);
        }
    }

    fn span(&self, site: SiteId) -> Span {
        let at = self.body.site(site);
        Span::new(&self.path, at.start, at.end)
    }

    fn block(&mut self, flow: &mut Flow<'_>, sites: &[SiteId]) {
        for &site in sites {
            self.statement(flow, site);
        }
    }

    fn statement(&mut self, flow: &mut Flow<'_>, site: SiteId) {
        match &self.body.site(site).node {
            Node::Stmt(stmt) => match stmt {
                Stmt::Var { local, value } | Stmt::Assign { local, value } => {
                    if let Some(value) = value {
                        let prov = self.eval(flow, *value);
                        self.local(*local, &prov);
                    }
                }
                Stmt::Return { value } => {
                    if let Some(value) = value {
                        let prov = self.eval(flow, *value);
                        self.returns.union(&prov);
                    }
                }
                Stmt::If {
                    arms, otherwise, ..
                } => {
                    for arm in arms {
                        if let Some(condition) = arm.condition {
                            self.eval(flow, condition);
                        }
                        self.block(flow, &arm.then);
                    }
                    self.block(flow, otherwise);
                }
                Stmt::For { item, list, body } => {
                    if let Some(list) = list {
                        let prov = self.eval(flow, *list);
                        self.local(*item, &prov);
                    }
                    self.block(flow, body);
                }
                Stmt::Emit { value } | Stmt::Assert { value, .. } | Stmt::Expr { value } => {
                    if let Some(value) = value {
                        self.eval(flow, *value);
                    }
                }
            },
            Node::Expr(_) => {
                self.eval(flow, site);
            }
        }
    }

    fn eval(&mut self, flow: &mut Flow<'_>, site: SiteId) -> Prov {
        let node = self.body.site(site).node.clone();
        let Node::Expr(expr) = node else {
            self.statement(flow, site);
            return Prov::default();
        };
        match expr {
            Expr::Literal(_) | Expr::Stored { .. } | Expr::Unchecked | Expr::Propagate { .. } => {
                Prov::default()
            }
            Expr::Local(id) => self.locals.get(id.0 as usize).cloned().unwrap_or_default(),
            Expr::Field { receiver, field } => self.eval(flow, receiver).field(&field_name(&field)),
            Expr::Call { callee, args, .. } => self.call(flow, &callee, &args),
            Expr::Build { callee, fields, .. } => {
                let mut out = Prov::default();
                let mut given = Vec::new();
                for (key, value) in &fields {
                    let prov = self.eval(flow, *value);
                    given.push((field_name(key), *value, prov.clone()));
                    out.fields.entry(field_name(key)).or_default().union(&prov);
                }
                if let Callee::Object(key) = &callee
                    && self.report
                    && is_output(&flow.lookup, key.name())
                {
                    for (name, value, prov) in given {
                        self.output(flow, key.name(), &name, value, &prov);
                    }
                }
                out
            }
            Expr::List { items } => {
                let mut out = Prov::default();
                for item in items {
                    out.union(&self.eval(flow, item));
                }
                out
            }
            Expr::Binary { lhs, rhs, .. } => {
                let mut out = self.eval(flow, lhs);
                out.union(&self.eval(flow, rhs));
                out
            }
            Expr::Unary { operand, .. } => self.eval(flow, operand),
            Expr::Match { subject, arms, .. } => {
                let subject = match subject {
                    Some(subject) => self.eval(flow, subject),
                    None => Prov::default(),
                };
                let mut out = Prov::default();
                for arm in &arms {
                    self.local(arm.binding, &subject);
                    match &arm.body {
                        ArmBody::Value(value) => out.union(&self.eval(flow, *value)),
                        ArmBody::Block(sites) => {
                            self.block(flow, sites);
                            if let Some(last) = sites.last()
                                && matches!(self.body.site(*last).node, Node::Expr(_))
                            {
                                out.union(&self.eval(flow, *last));
                            }
                        }
                        ArmBody::Empty => {}
                    }
                }
                out
            }
            Expr::Attempt { inner, arms } => {
                for arm in &arms {
                    self.failure(flow, &arm.failure, &arm.fields);
                }
                match inner {
                    Some(inner) => self.eval(flow, inner),
                    None => Prov::default(),
                }
            }
            Expr::Reject { failure, fields } => {
                match &failure {
                    Some(failure) => self.failure(flow, failure, &fields),
                    None => {
                        for (_, value) in fields {
                            self.eval(flow, value);
                        }
                    }
                }
                Prov::default()
            }
            Expr::Write {
                entity,
                op,
                transition,
                target,
                set,
                patch,
                when_supplied,
                conflicts,
                ..
            } => {
                for arm in &conflicts {
                    self.failure(flow, &arm.failure, &arm.fields);
                }
                let target = target.map(|target| self.eval(flow, target));
                let mut values: Vec<(String, SiteId, Prov)> = Vec::new();
                for (key, value) in &set {
                    let prov = self.eval(flow, *value);
                    values.push((field_name(key), *value, prov));
                }
                if let Some(patch) = patch {
                    self.eval(flow, patch);
                }
                for (_, fields) in &when_supplied {
                    for (_, value) in fields {
                        self.eval(flow, *value);
                    }
                }
                let entity = entity.name().to_string();
                let set_names: Vec<String> = values.iter().map(|(n, _, _)| n.clone()).collect();
                match op {
                    WriteOp::Create => {
                        if self.report && self.line.is_none() {
                            self.create(flow, &entity, &values);
                        }
                        let reached = self.written(flow, &entity, Effect::Create, &set_names, None);
                        Prov::of(Src::Record { entity, reached })
                    }
                    WriteOp::Update => {
                        let reached = self.written(
                            flow,
                            &entity,
                            Effect::Update,
                            &set_names,
                            target.as_ref(),
                        );
                        if let Some(transition) = &transition {
                            self.condition(flow, site, &entity, transition, &reached);
                        }
                        Prov::of(Src::Record { entity, reached })
                    }
                    WriteOp::Delete => {
                        if let Some(transition) = &transition {
                            let reached =
                                self.written(flow, &entity, Effect::Delete, &[], target.as_ref());
                            self.condition(flow, site, &entity, transition, &reached);
                        }
                        Prov::default()
                    }
                }
            }
            Expr::Request { .. } => Prov::default(),
            Expr::Inline { body, .. } => match body {
                Some(body) => self.eval(flow, body),
                None => Prov::default(),
            },
        }
    }

    /// How a caller reached the record a write gives back: the grants its
    /// statement checked (the entity's, or the action's line), each
    /// restricted field it sets, and, changing a record loaded earlier,
    /// how they reached that.
    fn written(
        &self,
        flow: &Flow<'_>,
        entity: &str,
        effect: Effect,
        set: &[String],
        target: Option<&Prov>,
    ) -> Reached {
        let prover = &flow.prover;
        let grants = match (&self.line, flow.entities.get(entity)) {
            (Some(line), Some(model)) => line.on(model, &prover.access.memberships),
            _ => prover.grants(entity, effect),
        };
        let mut reached = Reached::by(prover.way(entity, &grants));
        if let Some(policy) = prover.access.entity(entity) {
            for name in set {
                if let Some(grants) = policy.field_grants(name, effect) {
                    reached = reached.and(Reached::by(prover.way(entity, grants)));
                }
            }
        }
        if let Some(target) = target {
            for src in &target.whole {
                if let Src::Record {
                    entity: loaded,
                    reached: before,
                } = src
                    && loaded == entity
                {
                    reached = reached.and(before.clone());
                }
            }
        }
        self.gated(reached)
    }

    /// A transition's `when:` tests fields of the record it changes, and
    /// refusing it shows what they hold, so every caller who may run it
    /// may read each one (area 9, "Restricted fields in conditions and
    /// orders").
    fn condition(
        &mut self,
        flow: &Flow<'_>,
        site: SiteId,
        entity: &str,
        transition: &Key,
        reached: &Reached,
    ) {
        if !self.report {
            return;
        }
        let name = transition.name().to_string();
        let Some((_, short)) = name.split_once('.') else {
            return;
        };
        let Some(when) = flow
            .entities
            .get(entity)
            .and_then(|model| model.lifecycle.as_ref())
            .and_then(|lifecycle| lifecycle.transition(short))
            .and_then(|transition| transition.when.as_ref())
        else {
            return;
        };
        let mut fields = BTreeSet::new();
        when.fields(&mut fields);
        let Some(field) = fields
            .into_iter()
            .find(|field| !flow.prover.field_proved(entity, field, reached))
        else {
            return;
        };
        let who = flow.prover.counterexample(reached, |facts| {
            flow.prover.field_readable(entity, &field, facts)
        });
        let readers = flow.prover.readers(entity, &field);
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` runs `{name}`, whose `when:` tests `{entity}.{field}`, which not every caller may read",
                    self.name
                ),
            )
            .at(self.span(site), format!("tests `{field}`"))
            .reason(format!(
                "Anyone `{}`'s grants let in may run `{name}`{among} and {readers} `{field}`. Whether its `when:` refuses the record shows what `{field}` holds, so a write's condition tests only fields every caller may read.",
                self.name
            ))
            .next(format!(
                "Run `{name}` from an action only callers who may read `{field}` reach, or give `{field}`'s `read:` everyone who may change the `{entity}`"
            )),
        );
    }

    /// With what the action's line checks as it starts.
    fn gated(&self, reached: Reached) -> Reached {
        match &self.gate {
            Some(way) => reached.and(Reached::by(way.clone())),
            None => reached,
        }
    }

    fn call(&mut self, flow: &mut Flow<'_>, callee: &Callee, args: &[Option<SiteId>]) -> Prov {
        // An inline function's parameter holds the list it is given.
        let first = match args.first() {
            Some(Some(first)) => self.eval(flow, *first),
            _ => Prov::default(),
        };
        let mut provs: Vec<Prov> = vec![first.clone()];
        for arg in args.iter().skip(1) {
            let prov = match arg {
                Some(arg) => {
                    if let Node::Expr(Expr::Inline { param, .. }) = &self.body.site(*arg).node {
                        self.local(*param, &first);
                    }
                    self.eval(flow, *arg)
                }
                None => Prov::default(),
            };
            provs.push(prov);
        }
        let all = || {
            let mut out = Prov::default();
            for prov in &provs {
                out.union(prov);
            }
            out
        };
        match callee {
            Callee::Entity { entity, operation } if operation == "get" => {
                let entity = entity.name().to_string();
                let reached = self.gated(flow.prover.read(&entity));
                Prov::of(Src::Record { entity, reached })
            }
            Callee::Query(key) => self.query(flow, key),
            Callee::Action(key) => {
                let summary = flow.summary(key);
                if self.report {
                    for at in &summary.sends {
                        let Some(Some(arg)) = args.get(*at) else {
                            continue;
                        };
                        let prov = provs.get(*at).cloned().unwrap_or_default();
                        self.passed(flow, key.name(), *arg, &prov);
                    }
                } else {
                    for at in &summary.sends {
                        if let Some(prov) = provs.get(*at) {
                            for src in prov.flat() {
                                if let Src::Param(param) = src {
                                    self.sends.insert(param);
                                }
                            }
                        }
                    }
                }
                summary.returns.substitute(&provs)
            }
            Callee::Caller(kind) => Prov::of(Src::Caller(*kind)),
            Callee::Std(Op::CollectionCount) => Prov::default(),
            Callee::Std(Op::CollectionFilter) => first,
            Callee::Std(Op::CollectionMap) => provs.get(1).cloned().unwrap_or_default(),
            Callee::Session(_) | Callee::Unknown => Prov::default(),
            _ => all(),
        }
    }

    /// What a query's result carries: an `Output` was proved where the
    /// query is declared; records and other objects are their entity's
    /// fields, read through its `read:`.
    fn query(&self, flow: &Flow<'_>, key: &Key) -> Prov {
        let Some(model) = crate::data::queries::query_model(
            flow.db,
            flow.project,
            Name::new(flow.db, key.name().to_string()),
        ) else {
            return Prov::default();
        };
        if is_output(&flow.lookup, &model.element.ty) {
            return Prov::default();
        }
        let reached = self.gated(flow.prover.read(&model.entity));
        let element = self.element(
            flow,
            &model.entity,
            &model.element,
            &model.includes,
            &reached,
        );
        if model.cardinality == Cardinality::Page {
            let mut page = Prov::default();
            page.fields.insert("items".to_string(), element);
            return page;
        }
        element
    }

    fn element(
        &self,
        flow: &Flow<'_>,
        entity: &str,
        element: &Element,
        includes: &[Include],
        reached: &Reached,
    ) -> Prov {
        if element.ty == entity {
            return Prov::of(Src::Record {
                entity: entity.to_string(),
                reached: reached.clone(),
            });
        }
        let mut out = Prov::default();
        for (name, source) in &element.fields {
            let prov = match source {
                Source::Column(field) => Prov::of(Src::Field {
                    entity: entity.to_string(),
                    field: field.clone(),
                    reached: reached.clone(),
                }),
                Source::Include(at) => {
                    let Some(inner) = includes.get(*at) else {
                        continue;
                    };
                    let next = match inner.kind {
                        IncludeKind::Parent => {
                            flow.prover
                                .parent(entity, &inner.via, &inner.entity, reached)
                        }
                        IncludeKind::Child | IncludeKind::Children => {
                            self.gated(flow.prover.read(&inner.entity))
                        }
                    };
                    self.element(flow, &inner.entity, &inner.element, &inner.includes, &next)
                }
            };
            out.fields.insert(name.clone(), prov);
        }
        out
    }

    /// The fields among `prov`'s sources that a caller who reached their
    /// record isn't proved to read, with how they reached it.
    fn unproved(&self, flow: &Flow<'_>, prov: &Prov) -> Vec<(String, String, Reached)> {
        let mut out = Vec::new();
        for src in prov.flat() {
            match src {
                Src::Field {
                    entity,
                    field,
                    reached,
                } => {
                    if !flow.prover.field_proved(&entity, &field, &reached) {
                        out.push((entity, field, reached));
                    }
                }
                Src::Record { entity, reached } => {
                    let Some(policy) = flow.prover.access.entity(&entity) else {
                        continue;
                    };
                    for own in &policy.fields {
                        if !flow.prover.field_proved(&entity, &own.field, &reached) {
                            out.push((entity.clone(), own.field.clone(), reached.clone()));
                        }
                    }
                }
                Src::Caller(_) | Src::Param(_) => {}
            }
        }
        out
    }

    /// A failure given `fields`: its client fields are sent to the caller,
    /// as an output's are.
    fn failure(&mut self, flow: &mut Flow<'_>, failure: &Key, fields: &[(Key, SiteId)]) {
        for (key, value) in fields {
            let prov = self.eval(flow, *value);
            if self.report && client_field(flow, key) {
                self.output(flow, failure.name(), &field_name(key), *value, &prov);
            }
        }
    }

    /// An `Output`'s or a failure's client field `field` built from
    /// `value`.
    fn output(&mut self, flow: &Flow<'_>, output: &str, field: &str, value: SiteId, prov: &Prov) {
        for src in prov.flat() {
            if let Src::Param(at) = src {
                self.sends.insert(at);
            }
        }
        let Some((entity, from, reached)) = self.unproved(flow, prov).into_iter().next() else {
            return;
        };
        let who = flow.prover.counterexample(&reached, |facts| {
            flow.prover.field_readable(&entity, &from, facts)
        });
        let readers = flow.prover.readers(&entity, &from);
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        let reason = format!(
            "`{}` reaches the `{entity}` it comes from for anyone its grants let in{among} and {readers} `{from}`. What a caller is sent has one shape for everyone, and a value passed through a variable or another type keeps its field's rules.",
            self.name
        );
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{output}.{field}` carries `{entity}.{from}`, which not every caller of `{}` may read",
                    self.name
                ),
            )
            .at(self.span(value), format!("from `{entity}.{from}`"))
            .reason(reason)
            .next(format!("Leave `{field}` out of `{output}`")),
        );
    }

    /// A call passing `value` to the action `callee`, whose parameter
    /// reaches an output it builds.
    fn passed(&mut self, flow: &Flow<'_>, callee: &str, value: SiteId, prov: &Prov) {
        for src in prov.flat() {
            if let Src::Param(at) = src {
                self.sends.insert(at);
            }
        }
        let Some((entity, from, reached)) = self.unproved(flow, prov).into_iter().next() else {
            return;
        };
        let who = flow.prover.counterexample(&reached, |facts| {
            flow.prover.field_readable(&entity, &from, facts)
        });
        let readers = flow.prover.readers(&entity, &from);
        let among = who
            .map(|who| format!(", {who} among them,"))
            .unwrap_or_default();
        let reason = format!(
            "`{callee}` puts what it is given here in the output it returns, and `{}` reaches the `{entity}` it comes from for anyone its grants let in{among} while {readers} `{from}`. A value passed to another action keeps its field's rules.",
            self.name
        );
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_OUTPUT_FIELD_UNPROVED,
                format!(
                    "`{}` passes `{entity}.{from}` to `{callee}`, which sends it, though not every caller may read it",
                    self.name
                ),
            )
            .at(self.span(value), format!("from `{entity}.{from}`"))
            .reason(reason)
            .next(format!("Pass `{callee}` what every caller may read, or leave `{from}` out of its output")),
        );
    }

    /// `policy.role_field_create_forbidden`: a create whose entity's
    /// `create:` names only user fields, none of them set from the caller.
    fn create(&mut self, flow: &Flow<'_>, entity: &str, values: &[(String, SiteId, Prov)]) {
        let grants = flow.prover.grants(entity, Effect::Create);
        let callers: Vec<(&String, PrincipalKind)> = grants
            .iter()
            .filter_map(|grant| match grant {
                Grant::Caller { field, kind } => Some((field, *kind)),
                _ => None,
            })
            .collect();
        if callers.is_empty() || callers.len() != grants.len() {
            return;
        }
        let set: Vec<&(String, SiteId, Prov)> = values
            .iter()
            .filter(|(name, _, _)| callers.iter().any(|(field, _)| *field == name))
            .collect();
        if set.is_empty() {
            return;
        }
        let from_caller = set.iter().any(|(name, _, prov)| {
            let kind = callers
                .iter()
                .find(|(field, _)| *field == name)
                .map(|(_, kind)| *kind);
            prov.flat()
                .iter()
                .any(|src| matches!(src, Src::Caller(k) if Some(*k) == kind))
        });
        if from_caller {
            return;
        }
        let (field, value, _) = set[0];
        let kind = callers
            .iter()
            .find(|(f, _)| *f == field)
            .map_or(PrincipalKind::User, |(_, kind)| *kind);
        let listed: Vec<String> = callers.iter().map(|(f, _)| format!("`{f}`")).collect();
        let only = if listed.len() == 1 {
            "only "
        } else {
            "only the user fields "
        };
        self.problems.push(
            Diagnostic::new(
                &rule::POLICY_ROLE_FIELD_CREATE_FORBIDDEN,
                format!(
                    "`{}` sets `{entity}.{field}` from the request, not the caller",
                    self.name
                ),
            )
            .at(self.span(*value), "not the caller")
            .reason(format!(
                "`{entity}`'s `create:` names {only}{}, so a `{entity}` is only ever created by the caller it points at. Set from the request, the create would be refused for every value but the caller's own, so it is set from the caller: the `id` of a `{}` the action takes.",
                crate::and_list(&listed),
                kind.type_name()
            ))
            .next(format!(
                "Take the caller as a parameter typed `{}`, and set `{field}` to its `id`",
                kind.type_name()
            )),
        );
    }
}

/// Whether the failure field `key` is one of its `client:` fields, which
/// the caller is sent.
fn client_field(flow: &Flow<'_>, key: &Key) -> bool {
    let graph = crate::graph::graph(flow.db, flow.project);
    graph.get(key).is_some_and(|node| {
        matches!(
            node.detail,
            jadpo_graph::Detail::Field {
                section: Some(jadpo_graph::Section::Client),
                ..
            }
        )
    })
}

/// Whether the type called `name` is an `Output`.
fn is_output(lookup: &Lookup<'_>, name: &str) -> bool {
    matches!(
        lookup.def(name),
        Some(Def::Fields {
            base: FieldsBase::Output,
            ..
        })
    )
}
