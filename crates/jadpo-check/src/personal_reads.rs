//! A marked field nothing reads (area 15, "What the compiler checks"):
//! personal data the app holds and never uses, `data.personal_unread`.
//!
//! A field counts as read when a query reads it (its `where:`, its order,
//! what it groups or adds up and what it gives back), an application's
//! action or function reads it off a record, a record goes out whole (it
//! is returned, or put in an object, an event or a failure), it is sent
//! through a connection, or the app needs it to work: a `search:` or a
//! `unique:` over it, a provider's erasure key, and the field that
//! identifies a caller. A test, a fixture or a seed doesn't count, and nor
//! does an export, which reads every field whether the app uses it or not:
//! a field kept for a legal reason is read by nothing else, which is what
//! a `decline` line says.
//!
//! Only `personal:` marks a field here. The check runs only on bodies and
//! queries that have no problems, since one that doesn't check may hold a
//! read nothing can see.

use std::collections::BTreeSet;

use jadpo_db::{Db, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::Kind;
use jadpo_graph::body::{Body, Expr, Node, SiteId, Stmt};

use crate::data::entity_read;
use crate::data::queries::{Condition, Element, Include, QueryModel, Source};
use crate::lookups::Name;
use crate::types::{callables_of, typed_body};

/// What the app's code and queries read.
#[derive(Default)]
struct Read {
    /// Each field read, by its entity and its name.
    fields: BTreeSet<(String, String)>,
    /// Each entity a whole record of goes out.
    whole: BTreeSet<String>,
}

impl Read {
    fn field(&mut self, entity: &str, field: &str) {
        self.fields.insert((entity.to_string(), field.to_string()));
    }

    /// Whether `entity`'s `field` is read, the entity by the name a table
    /// knows it by or its bare one.
    fn reads(&self, entity: &str, field: &str) -> bool {
        let bare = entity.rsplit('.').next().unwrap_or(entity);
        [entity, bare].iter().any(|name| {
            self.whole.contains(*name)
                || self.fields.contains(&(name.to_string(), field.to_string()))
        })
    }

    /// What `body` reads: each `.field` off an entity's record, and each
    /// record that goes out whole.
    fn body(&mut self, body: &Body) {
        for site in &body.sites {
            match &site.node {
                Node::Expr(Expr::Field { field, .. }) => {
                    if let Some((host, name)) = field.name().rsplit_once('.') {
                        self.field(host, name);
                        if let Some(bare) = host.rsplit('.').next() {
                            self.field(bare, name);
                        }
                    }
                }
                Node::Expr(Expr::Build { fields, .. } | Expr::Reject { fields, .. }) => {
                    for (_, value) in fields {
                        self.leaves(body, *value);
                    }
                }
                Node::Stmt(Stmt::Return { value } | Stmt::Emit { value }) => {
                    if let Some(value) = value {
                        self.leaves(body, *value);
                    }
                }
                _ => {}
            }
        }
    }

    /// The entities `value`'s type holds go out whole.
    fn leaves(&mut self, body: &Body, value: SiteId) {
        for key in body.site(value).ty.keys() {
            if key.kind() == Some(Kind::Entity) {
                self.whole.insert(key.name().to_string());
                if let Some(bare) = key.name().rsplit('.').next() {
                    self.whole.insert(bare.to_string());
                }
            }
        }
    }

    fn query(&mut self, query: &QueryModel) {
        let entity = &query.entity;
        self.element(entity, &query.element);
        self.conditions(entity, &query.filter);
        for order in &query.order {
            self.field(entity, &order.field);
        }
        if let Some(nearest) = &query.nearest {
            self.field(entity, &nearest.field);
        }
        if let Some(distance) = &query.distance {
            self.field(entity, &distance.field);
        }
        if let Some(geometry) = &query.geometry {
            self.field(entity, geometry);
        }
        if let Some(grouping) = &query.grouping {
            for (_, entry) in &grouping.entries {
                if let Some(field) = entry.field() {
                    self.field(entity, field);
                }
            }
        }
        for include in &query.includes {
            self.include(entity, include);
        }
    }

    fn include(&mut self, parent: &str, include: &Include) {
        self.element(&include.entity, &include.element);
        self.conditions(&include.entity, &include.filter);
        for order in &include.order {
            self.field(&include.entity, &order.field);
        }
        // What it follows is a reference, on one side or the other.
        self.field(parent, &include.via);
        self.field(&include.entity, &include.via);
        for inner in &include.includes {
            self.include(&include.entity, inner);
        }
    }

    fn element(&mut self, entity: &str, element: &Element) {
        if element.ty == entity || element.ty.rsplit('.').next() == entity.rsplit('.').next() {
            self.whole.insert(entity.to_string());
        }
        for (_, source) in &element.fields {
            if let Source::Column(field)
            | Source::Entry {
                field: Some(field), ..
            } = source
            {
                self.field(entity, field);
            }
        }
    }

    fn conditions(&mut self, entity: &str, conditions: &[Condition]) {
        for condition in conditions {
            self.condition(entity, condition);
        }
    }

    fn condition(&mut self, entity: &str, condition: &Condition) {
        match condition {
            Condition::Compare { field, .. }
            | Condition::IsNone { field, .. }
            | Condition::In { field, .. } => self.field(entity, field),
            Condition::And(parts) | Condition::Or(parts) => self.conditions(entity, parts),
            Condition::Not(inner)
            | Condition::Present {
                condition: inner, ..
            } => {
                self.condition(entity, inner);
            }
            Condition::Call { call, .. } => self.field(entity, &call.field),
            // Its words come from the entity's `search:`, counted there.
            Condition::Matches(_) => {}
        }
    }
}

/// What the app's own actions, functions and queries read, and what its
/// `search:`, `unique:`, identity, provider keys and connections use.
fn read(db: &dyn Db, project: Project) -> Option<Read> {
    let mut out = Read::default();
    for &file in project.files(db) {
        for callable in callables_of(db, file) {
            let checked = typed_body(db, project, callable);
            if !checked.problems.is_empty() {
                return None;
            }
            let kind = checked.key.as_ref().and_then(|key| key.kind());
            if !matches!(kind, Some(Kind::Action | Kind::Function)) {
                continue;
            }
            out.body(&checked.body);
        }
    }
    for query in &crate::data::queries::query_models(db, project).queries {
        out.query(query);
    }
    let model = crate::data::data_model(db, project);
    for entity in &model.entities {
        if let Some(search) = &entity.search {
            for field in &search.fields {
                out.field(&entity.name, field);
            }
        }
        for field in entity.fields.iter().filter(|field| field.unique) {
            out.field(&entity.name, &field.name);
        }
        for rule in &entity.unique {
            for field in &rule.fields {
                out.field(&entity.name, field);
            }
        }
    }
    for variant in &crate::identity::identity(db, project).variants {
        out.field(&variant.entity, &variant.subject);
    }
    for key in crate::provider_erasure::provider_keys(db, project) {
        out.field(&key.entity, &key.field);
    }
    for sent in crate::personal_data::sent(db, project) {
        if let crate::ai_personal::Holder::Field { host, field } = &sent.holder {
            out.field(host, field);
        }
    }
    Some(out)
}

/// `data.personal_unread`: each field marked `personal:` that nothing reads.
#[salsa::tracked(returns(ref))]
pub fn unread_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !crate::policy::policy_clean(db, project)
        || !crate::data::queries::queries_clean(db, project)
    {
        return out;
    }
    let Some(read) = read(db, project) else {
        return out;
    };
    let marks = crate::ai_personal::marks(db, project);
    for &file in project.files(db) {
        let mut seen = BTreeSet::new();
        for item in &item_tree(db, file).items {
            if item.kind != ItemKind::Entity || !seen.insert(item.name.as_str()) {
                continue;
            }
            let Some(entity_read) = entity_read(db, project, Name::new(db, item.name.clone()))
            else {
                continue;
            };
            let entity = &entity_read.model;
            if entity.path != *file.path(db) {
                continue;
            }
            for (at, field) in entity.fields.iter().enumerate() {
                let marked = marks
                    .fields
                    .get(&(entity.name.clone(), field.name.clone()))
                    .is_some_and(|mark| mark.category.is_some());
                if !marked || read.reads(&entity.name, &field.name) {
                    continue;
                }
                let Some(span) = entity_read.spans.fields.get(at) else {
                    continue;
                };
                let (name, field) = (&entity.name, &field.name);
                out.push(
                    Diagnostic::new(
                        &rule::DATA_PERSONAL_UNREAD,
                        format!("Nothing reads `{name}.{field}`, which is personal data"),
                    )
                    .at(span.clone(), "no query, output or action reads it")
                    .reason(format!(
                        "Personal data an app holds and never uses is data it shouldn't hold (data minimisation, GDPR Article 5). No query, output or action reads `{name}.{field}`; an export doesn't count, since it reads every field whether the app uses it or not."
                    ))
                    .next(format!(
                        "Read `{field}` where the app needs it, or remove it from `{name}`"
                    )),
                );
            }
        }
    }
    out
}
