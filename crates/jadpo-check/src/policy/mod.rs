//! Area 9's policy: who may do what to which records.
//!
//! - An entity's `policy:` lists, for each of `create`, `read`, `update`
//!   and `delete`, who may do it: a field of the record that points at the
//!   caller (`owner_id`), a membership role (`CompanyRole.editor`, or
//!   `TeamRole.lead(from_team_id)` naming the reference it applies
//!   through), or `Access.authenticated`, `Access.public` and
//!   `Access.system`. Anything not listed is refused.
//! - A membership entity's `membership:` says where a role is held
//!   (`scope:`, a reference or `MembershipScope.application`), who holds it
//!   (`member:`) and which field holds it (`role:`).
//! - Root `policy { }` names the tenant (`tenant: Company`), which every
//!   entity's grants are limited to unless it says `tenant:
//!   Tenancy.shared`, and the actions that take their permission from a
//!   list of their own (`actions:`).
//!
//! Each entity's policy and membership are read by name, so an edit reads
//! again only the entity it is in; [`access`] gathers them, resolved, for
//! the plan, which compiles them into every statement (`jadpo-plan`), and
//! for the permissions report ([`report`]). The checks read them by name
//! instead, through [`Policies`], so each depends only on the entities it
//! proves against (chunk 4.11). [`reach`] checks the routes: every create,
//! read, update and delete a route reaches needs a grant, and one anyone
//! may call needs `Access.public`.
//!
//! - A field's own `policy:` narrows its entity's grants for `create`,
//!   `read` and `update`, or says `FieldAccess.server_only` for a field no
//!   client may read or write ([`FieldPolicy`]); it can only take away
//!   (`policy.field_widens_entity`).
//!
//! [`proof`] decides whether every caller who reached a record may read a
//! field of it, or the parent a query includes; [`outputs`] checks each
//! query's output and includes with it, and [`flow`] follows values
//! through actions' bodies into the outputs they build and the user
//! fields their creates set.
//!
//! Built in chunks 4.4 and 4.4b; OAuth scopes (`scopes:`) come after the
//! first build.

pub mod flow;
pub mod outputs;
pub mod proof;
pub mod reach;
pub mod report;

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Ownership, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::PrincipalKind;
use jadpo_std::{Access as AccessWord, FieldAccess, MembershipScope, Tenancy as TenancyWord};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use crate::data::{Entities, EntityModel};
use crate::lookups::{Lookup, Name, NameView};
use crate::tree::{self, node_span};
use crate::types::{Def, TypeView};

/// What a grant lets someone do to an entity's records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Effect {
    Create,
    Read,
    Update,
    Delete,
}

impl Effect {
    pub const ALL: [Effect; 4] = [Effect::Create, Effect::Read, Effect::Update, Effect::Delete];

    /// As a `policy:` names its list: `create`.
    pub fn word(self) -> &'static str {
        match self {
            Effect::Create => "create",
            Effect::Read => "read",
            Effect::Update => "update",
            Effect::Delete => "delete",
        }
    }

    pub fn from_word(word: &str) -> Option<Effect> {
        Effect::ALL.into_iter().find(|effect| effect.word() == word)
    }
}

/// Who one entry of a grant list lets in, resolved against the record it
/// is checked on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Grant {
    /// The caller a field of the record points at: a reference to the
    /// principal's user or service entity, or that entity's own `id`.
    Caller { field: String, kind: PrincipalKind },
    /// A caller holding `role` in membership entity `membership`, held in
    /// the record field `via` points at (`id`: the record is the scope
    /// itself), or anywhere for a membership across the whole application
    /// (`via: None`).
    Role {
        membership: String,
        role: String,
        via: Option<String>,
    },
    /// `Access.authenticated`: anyone signed in.
    Signed,
    /// `Access.public`: anyone at all.
    Public,
    /// `Access.system`: schedules and subscribers (area 12).
    System,
}

/// One effect's list, as written and resolved.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct GrantList {
    /// Each entry as written: `owner_id`, `CompanyRole.editor`.
    pub written: Vec<String>,
    /// The entries that resolved, in written order.
    pub grants: Vec<Grant>,
}

/// Where the tenant (root `policy { tenant: }`) applies to an entity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tenancy {
    /// The tenant entity: `Company`.
    pub entity: String,
    /// The record's field that refers to it; `id` on the tenant itself.
    pub via: String,
}

/// One entity's policy, resolved.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EntityPolicy {
    pub entity: String,
    /// Each effect's list; an effect left out grants nobody.
    pub lists: BTreeMap<Effect, GrantList>,
    /// Where the tenant applies; `None` when the entity is shared by every
    /// tenant, or there is no tenant.
    pub tenancy: Option<Tenancy>,
    /// Whether it says `tenant: Tenancy.shared`.
    pub shared: bool,
    /// The fields only an action listed in root `policy { actions: }`
    /// changes: each field a grant points at the caller through, each
    /// reference a role or the tenant applies through, and a membership's
    /// `scope:` and `member:` (area 9: handing a record to someone else is
    /// a named action).
    pub fixed: Vec<String>,
    /// The fields whose own `policy:` narrows these grants, in declared
    /// order.
    pub fields: Vec<FieldPolicy>,
}

impl EntityPolicy {
    /// What `effect` grants, resolved; empty when nobody.
    pub fn grants(&self, effect: Effect) -> &[Grant] {
        self.lists
            .get(&effect)
            .map_or(&[], |list| list.grants.as_slice())
    }

    /// The field called `name`'s own policy, when it has one.
    pub fn field(&self, name: &str) -> Option<&FieldPolicy> {
        self.fields.iter().find(|field| field.field == name)
    }

    /// Who may do `effect` to the field called `name`, when its own
    /// `policy:` narrows the record's grants: `Some(&[])` for nobody, `None`
    /// when the field follows the record.
    pub fn field_grants(&self, name: &str, effect: Effect) -> Option<&[Grant]> {
        self.field(name)?.grants(effect)
    }
}

/// A field's own `policy:` (area 9, "Field restrictions"): the record's
/// grants, narrowed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldPolicy {
    pub field: String,
    /// `FieldAccess.server_only`: no client may read or write it.
    pub server_only: bool,
    /// Each effect it narrows (`create`, `read`, `update`), as written and
    /// resolved; an effect left out follows the record's list.
    pub lists: BTreeMap<Effect, GrantList>,
}

impl FieldPolicy {
    /// The effects a field's `policy:` can narrow: a field is deleted with
    /// its record.
    pub const EFFECTS: [Effect; 3] = [Effect::Create, Effect::Read, Effect::Update];

    /// Who may do `effect` to the field: `Some(&[])` for nobody, `None`
    /// when it follows the record's list.
    pub fn grants(&self, effect: Effect) -> Option<&[Grant]> {
        if self.server_only {
            return (effect != Effect::Delete).then_some(&[]);
        }
        self.lists.get(&effect).map(|list| list.grants.as_slice())
    }
}

/// A membership entity's `membership:`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Membership {
    /// The membership entity: `CompanyMembership`.
    pub entity: String,
    /// The reference naming what a role is held in; `None` for the whole
    /// application.
    pub scope: Option<String>,
    /// The entity `scope` refers to: `Company`.
    pub scope_entity: Option<String>,
    /// The field naming who holds it, a reference to the user entity.
    pub member: String,
    /// The field holding the role.
    pub role: String,
    /// That field's enum: `CompanyRole`.
    pub roles: String,
}

/// Who an `actions:` line in root `policy { }` lets call its action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ActionWho {
    /// `Todo.owner_id`: the caller that field of the record points at.
    Field {
        entity: String,
        field: String,
        kind: PrincipalKind,
    },
    /// A membership role, applied through each record's reference to the
    /// membership's scope.
    Role {
        membership: String,
        role: String,
    },
    Signed,
    Public,
    System,
}

/// One `actions:` line.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActionGrants {
    /// The action's name: `transfer_todo`.
    pub action: String,
    /// As written: `todos.transfer_todo`.
    pub written: String,
    pub who: Vec<ActionWho>,
    /// Each entry as written.
    pub written_who: Vec<String>,
}

impl ActionGrants {
    /// What the line grants on `entity`'s records: a field of another
    /// entity grants nothing here, and a role only where the record refers
    /// to the membership's scope.
    pub fn on(&self, entity: &EntityModel, memberships: &[Membership]) -> Vec<Grant> {
        let mut out = Vec::new();
        for who in &self.who {
            let grant = match who {
                ActionWho::Field {
                    entity: holder,
                    field,
                    kind,
                } if *holder == entity.name => Some(Grant::Caller {
                    field: field.clone(),
                    kind: *kind,
                }),
                ActionWho::Field { .. } => None,
                ActionWho::Role { membership, role } => memberships
                    .iter()
                    .find(|m| m.entity == *membership)
                    .and_then(|m| match &m.scope_entity {
                        None => Some(None),
                        Some(scope) => only_via(entity, scope).map(Some),
                    })
                    .map(|via| Grant::Role {
                        membership: membership.clone(),
                        role: role.clone(),
                        via,
                    }),
                ActionWho::Signed => Some(Grant::Signed),
                ActionWho::Public => Some(Grant::Public),
                ActionWho::System => Some(Grant::System),
            };
            if let Some(grant) = grant
                && !out.contains(&grant)
            {
                out.push(grant);
            }
        }
        out
    }

    /// Who the line lets call its action as it starts (area 9, "Actions"),
    /// given the entities whose records the action's own body creates,
    /// changes or deletes (`writes`). A user field or a role held in a
    /// record lets someone in only on those records, so when one applies
    /// to a record the body writes, the statements decide; otherwise such
    /// an entry lets nobody in, and the action is checked against the
    /// line's other entries as it starts, wherever it is called from.
    pub fn gate(&self, writes: &[&EntityModel], memberships: &[Membership]) -> LineGate {
        if self.who.contains(&ActionWho::Public) {
            return LineGate::Anyone;
        }
        if self.who.contains(&ActionWho::Signed) {
            return LineGate::Signed;
        }
        let applies = writes.iter().any(|entity| {
            self.on(entity, memberships).iter().any(|grant| {
                matches!(
                    grant,
                    Grant::Caller { .. } | Grant::Role { via: Some(_), .. }
                )
            })
        });
        if applies {
            return LineGate::Statements;
        }
        let roles: Vec<(String, String)> = self
            .who
            .iter()
            .filter_map(|who| match who {
                ActionWho::Role { membership, role }
                    if memberships
                        .iter()
                        .any(|m| m.entity == *membership && m.scope.is_none()) =>
                {
                    Some((membership.clone(), role.clone()))
                }
                _ => None,
            })
            .collect();
        if roles.is_empty() {
            LineGate::Nobody
        } else {
            LineGate::Roles(roles)
        }
    }
}

/// Who a listed action's line lets call it as it starts: what the plan
/// checks then, the proofs count on, and a route that could never succeed
/// is refused for (area 9, "Actions").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LineGate {
    /// `Access.public`: anyone at all, so nothing is checked.
    Anyone,
    /// An entry that depends on a record applies to a record the action's
    /// own body writes: those statements decide, with the line's grants.
    Statements,
    /// `Access.authenticated`: anyone signed in.
    Signed,
    /// A caller holding one of these roles held across the whole
    /// application: each by its membership entity and role.
    Roles(Vec<(String, String)>),
    /// Nobody: the line names only `Access.system`, which no request is,
    /// or callers that only a record the action never writes could name.
    Nobody,
}

/// The reference through which a role held in `scope` applies to
/// `entity`'s records when the grant names none: `id` on the scope itself,
/// or its one reference to the scope.
fn only_via(entity: &EntityModel, scope: &str) -> Option<String> {
    if entity.name == scope {
        return Some("id".to_string());
    }
    let refs = references_to(entity, scope);
    match refs.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// `entity`'s fields that refer to `target`, in declared order.
fn references_to(entity: &EntityModel, target: &str) -> Vec<String> {
    entity
        .fields
        .iter()
        .filter(|field| {
            field
                .reference
                .as_ref()
                .is_some_and(|reference| reference.target == target)
        })
        .map(|field| field.name.clone())
        .collect()
}

/// Root `policy { }`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RootPolicy {
    /// `tenant: Company`.
    pub tenant: Option<String>,
    pub actions: Vec<ActionGrants>,
    /// The memberships held in the tenant.
    pub tenant_memberships: Vec<String>,
}

impl RootPolicy {
    pub fn action(&self, name: &str) -> Option<&ActionGrants> {
        self.actions.iter().find(|line| line.action == name)
    }
}

/// Everything policy decides, for the plan and the report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Access {
    pub root: RootPolicy,
    pub memberships: Vec<Membership>,
    /// Each entity's policy, by entity name; an entity without `policy:`
    /// has one whose every list is empty.
    pub entities: BTreeMap<String, EntityPolicy>,
}

impl Access {
    pub fn entity(&self, name: &str) -> Option<&EntityPolicy> {
        self.entities.get(name)
    }

    pub fn membership(&self, entity: &str) -> Option<&Membership> {
        self.memberships.iter().find(|m| m.entity == entity)
    }

    /// When a reference from a record of `from` to one of `to` stays
    /// inside the tenant (area 9, "References stay inside the tenant"):
    /// both belong to the tenant and `to` isn't the tenant itself, so the
    /// record it points at belongs to the same tenant, which every create
    /// and update that sets it checks. Each one's tenancy, in that order.
    ///
    /// A reference to the tenant itself is the record's tenancy, or, beside
    /// the one an entity names with `tenant:`, another tenant on purpose.
    pub fn kept_in_tenant(&self, from: &str, to: &str) -> Option<(&Tenancy, &Tenancy)> {
        let from = self.entity(from)?.tenancy.as_ref()?;
        let to = self.entity(to)?.tenancy.as_ref()?;
        (to.via != "id").then_some((from, to))
    }
}

// --- reading -------------------------------------------------------------------

/// One entity's `membership:` and `policy:`, read with their problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipRead {
    pub model: Option<Membership>,
    pub problems: Vec<Diagnostic>,
}

/// One entity's `policy:`, read and resolved with its problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyRead {
    pub model: Arc<EntityPolicy>,
    pub path: String,
    /// Where each part is written, for the graph: each effect's list by
    /// its word, `tenant`, and each field's own policy by the field's name
    /// after `field `.
    pub places: Vec<(String, Span)>,
    pub problems: Vec<Diagnostic>,
}

/// Root `policy { }`, read with its problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootRead {
    pub model: RootPolicy,
    /// The file it is written in.
    pub path: Option<String>,
    /// Where each line under `actions:` is written, by its action.
    pub places: Vec<(String, Span)>,
    pub problems: Vec<Diagnostic>,
}

/// The entity declaration called `name`, with its file's path.
fn entity_decl(
    lookup: &Lookup<'_>,
    db: &dyn Db,
    project: Project,
    name: &str,
) -> Option<(String, ast::EntityDecl)> {
    let symbol = lookup
        .declared(name)
        .iter()
        .find(|s| s.kind == ItemKind::Entity)?;
    let file = crate::lookups::file_at(db, project, Name::new(db, symbol.path.clone()))?;
    let source = tree::source(db, file)?;
    let Some(Decl::EntityDecl(decl)) = tree::decl_at(&source, symbol.index) else {
        return None;
    };
    Some((symbol.path.clone(), decl))
}

/// An entity's members whose key is `word`.
fn sections(decl: &ast::EntityDecl, word: &str) -> Vec<ast::Member> {
    tree::members(decl.member_list())
        .into_iter()
        .filter(|(name, _)| tree::name_text(name).as_deref() == Some(word))
        .map(|(_, member)| member)
        .collect()
}

/// The key span of a member.
fn key_span(path: &str, member: &ast::Member) -> Span {
    member.member_key().map_or_else(
        || node_span(path, member.syntax()),
        |key| node_span(path, key.syntax()),
    )
}

/// `a.b` as its two names.
fn pair(expr: &Expr) -> Option<(String, String)> {
    let (first, rest) = tree::dotted(expr)?;
    match rest.as_slice() {
        [second] => Some((tree::name_ref_text(&first)?, tree::name_ref_text(second)?)),
        _ => None,
    }
}

/// The entity the principal's `kind` names.
fn principal_entity(db: &dyn Db, project: Project, kind: PrincipalKind) -> Option<String> {
    crate::identity::identity(db, project)
        .variant(kind)
        .map(|variant| variant.entity.clone())
}

/// The kind of caller a field of `entity` points at: a reference to the
/// principal's user or service entity, or that entity's own `id`.
fn caller_kind(
    db: &dyn Db,
    project: Project,
    entity: &EntityModel,
    field: &str,
) -> Option<PrincipalKind> {
    for kind in [PrincipalKind::User, PrincipalKind::Service] {
        let Some(principal) = principal_entity(db, project, kind) else {
            continue;
        };
        if field == "id" && entity.name == principal {
            return Some(kind);
        }
        let points = entity.fields.iter().any(|f| {
            f.name == field
                && f.reference
                    .as_ref()
                    .is_some_and(|reference| reference.target == principal)
        });
        if points {
            return Some(kind);
        }
    }
    None
}

/// The membership the entity called `name` declares.
#[salsa::tracked(returns(ref))]
pub fn membership_read<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> Option<MembershipRead> {
    let lookup = Lookup::new(db, project);
    let entity = Entities::new(db, project).get(name.text(db))?;
    let (path, decl) = entity_decl(&lookup, db, project, name.text(db))?;
    let found = sections(&decl, "membership");
    let first = found.first()?;
    let mut problems = Vec::new();
    for again in found.iter().skip(1) {
        problems.push(
            Diagnostic::new(
                &rule::POLICY_MEMBERSHIP_DUPLICATE,
                format!("`{}` declares `membership:` twice", entity.name),
            )
            .at(key_span(&path, again), "declared again")
            .also(key_span(&path, first), "first declared here")
            .reason("A membership entity stores one role, held in one place by one member, so it says so once.")
            .next("Remove one `membership:`, or keep the second role in an entity of its own"),
        );
    }
    let at = key_span(&path, first);
    let mut parts: BTreeMap<String, (Span, Option<Expr>)> = BTreeMap::new();
    for item in first.block().iter().flat_map(|list| list.member_items()) {
        let MemberItem::Member(member) = item else {
            if let MemberItem::Elision(_) = item {
                continue;
            }
            problems.push(unknown_part(
                &path,
                item.syntax(),
                &tree::written(item.syntax()),
            ));
            continue;
        };
        let Some(key) = member.member_key() else {
            continue;
        };
        let word = tree::written(key.syntax());
        let span = node_span(&path, key.syntax());
        if !["scope", "member", "role"].contains(&word.as_str()) {
            problems.push(unknown_part(&path, key.syntax(), &word));
            continue;
        }
        if let Some((first_at, _)) = parts.get(&word) {
            let rule = if word == "scope" {
                &rule::POLICY_SCOPE_DUPLICATE
            } else {
                &rule::POLICY_MEMBERSHIP_DUPLICATE
            };
            problems.push(
                Diagnostic::new(rule, format!("`{word}:` is given twice"))
                    .at(span, "given again")
                    .also(first_at.clone(), "first given here")
                    .reason("A membership says once where its role is held, who holds it and which field holds it, so there is never a question of which counts.")
                    .next(format!("Remove one `{word}:`")),
            );
            continue;
        }
        parts.insert(word, (span, member.value()));
    }
    let required = [
        (
            "scope",
            &rule::POLICY_MEMBERSHIP_SCOPE_REQUIRED,
            "what the role is held in: a field typed `<Entity>.id`, or `MembershipScope.application` for a role held across the whole application",
        ),
        (
            "member",
            &rule::POLICY_MEMBERSHIP_MEMBER_REQUIRED,
            "who holds it: a reference to the user entity",
        ),
        (
            "role",
            &rule::POLICY_MEMBERSHIP_ROLE_REQUIRED,
            "which field holds the role, a field typed with the role's enum",
        ),
    ];
    for (word, rule, says) in required {
        if !parts.contains_key(word) {
            problems.push(
                Diagnostic::new(rule, format!("`{}`'s membership has no `{word}:`", entity.name))
                    .at(at.clone(), format!("no `{word}:`"))
                    .reason(format!("A membership says where a role is held (`scope:`), who holds it (`member:`) and which field holds it (`role:`); `{word}:` is {says}."))
                    .next(format!("Add `{word}:` to the membership")),
            );
        }
    }
    let user = principal_entity(db, project, PrincipalKind::User);
    let field_named = |word: &str| -> Option<(String, Span)> {
        let (span, value) = parts.get(word)?;
        match value {
            Some(Expr::NameRef(name)) => Some((tree::name_ref_text(name)?, span.clone())),
            _ => None,
        }
    };
    // `scope:`: a reference, or the whole application.
    let mut scope = None;
    let mut scope_entity = None;
    let mut scope_ok = parts.contains_key("scope");
    if let Some((span, value)) = parts.get("scope") {
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let application = value.as_ref().and_then(pair)
            == Some((
                MembershipScope::NAME.to_string(),
                MembershipScope::Application.name().to_string(),
            ));
        if !application {
            match field_named("scope").and_then(|(f, _)| entity.fields.iter().find(|x| x.name == f))
            {
                Some(field) if field.reference.is_some() => {
                    scope = Some(field.name.clone());
                    scope_entity = field.reference.as_ref().map(|r| r.target.clone());
                }
                _ => {
                    scope_ok = false;
                    problems.push(binding(
                        span.clone(),
                        format!("`scope: {written}` isn't a reference of `{}`", entity.name),
                        "not a reference",
                        "A membership's `scope:` names the reference to the record its role is held in, or says `MembershipScope.application` for a role held across the whole application.",
                        "Name a field typed `<Entity>.id`, or write `MembershipScope.application`",
                    ));
                }
            }
        }
    }
    // `member:`: a reference to the user entity.
    let mut member = None;
    if let Some((span, value)) = parts.get("member") {
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let found =
            field_named("member").and_then(|(f, _)| entity.fields.iter().find(|x| x.name == f));
        match (found, &user) {
            (Some(field), Some(user))
                if field.reference.as_ref().is_some_and(|r| r.target == *user) =>
            {
                member = Some(field.name.clone());
            }
            _ => problems.push(binding(
                span.clone(),
                format!("`member: {written}` doesn't point at a user"),
                "not a reference to the user entity",
                &match &user {
                    Some(user) => format!("A role is held by a user, so a membership's `member:` names its reference to `{user}`, the entity the principal's `user:` names (area 8)."),
                    None => "A role is held by a user, so a membership's `member:` names a reference to the entity the principal's `user:` names, and `app.jadpo` declares no `principal { user: }` (area 8).".to_string(),
                },
                "Name the field that refers to the user who holds the role",
            )),
        }
    }
    // `role:`: a field typed with a plain enum.
    let mut role = None;
    if let Some((span, value)) = parts.get("role") {
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let found =
            field_named("role").and_then(|(f, _)| entity.fields.iter().find(|x| x.name == f));
        let roles = found.and_then(|field| match lookup.structure(&field.ty) {
            crate::types::Ty::Named(name) => match lookup.def(&name) {
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                    Some(name)
                }
                _ => None,
            },
            _ => None,
        });
        match (found, roles) {
            (Some(field), Some(roles)) => role = Some((field.name.clone(), roles)),
            _ => problems.push(binding(
                span.clone(),
                format!("`role: {written}` isn't a field holding a role"),
                "not an enum field",
                "A membership's `role:` names the field that stores the role, typed with an enum whose values a grant names (`<Role>.<value>`).",
                "Name a field typed with the role's enum",
            )),
        }
    }
    let model = match (scope_ok, member, role) {
        (true, Some(member), Some((role, roles))) => Some(Membership {
            entity: entity.name.clone(),
            scope,
            scope_entity,
            member,
            role,
            roles,
        }),
        _ => None,
    };
    Some(MembershipRead { model, problems })
}

fn unknown_part(path: &str, node: &jadpo_syntax::SyntaxNode, word: &str) -> Diagnostic {
    let span = node_span(path, node);
    let near = nearest(word, ["scope", "member", "role"]);
    let problem = Diagnostic::new(
        &rule::POLICY_EFFECT_UNKNOWN,
        format!("`{word}` isn't part of a membership"),
    )
    .at(span.clone(), "not a part of a membership")
    .reason(match near {
        Some(near) => {
            format!("A membership holds `scope:`, `member:` and `role:`. Did you mean `{near}`?")
        }
        None => "A membership holds `scope:`, `member:` and `role:`.".to_string(),
    });
    match near {
        Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
        None => problem.next(format!("Remove `{word}`")),
    }
}

fn binding(span: Span, headline: String, label: &str, reason: &str, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::POLICY_BINDING_INVALID, headline)
        .at(span, label)
        .reason(reason)
        .next(next)
}

/// Every membership, in entity name order.
#[salsa::tracked(returns(ref))]
pub fn memberships(db: &dyn Db, project: Project) -> Arc<Vec<Membership>> {
    Arc::new(
        crate::data::entity_names(db, project)
            .iter()
            .filter_map(|name| {
                membership_read(db, project, Name::new(db, name.clone()))
                    .as_ref()
                    .and_then(|read| read.model.clone())
            })
            .collect(),
    )
}

/// What one entry of a grant list is, as written.
enum Written {
    /// `owner_id`.
    Field(String),
    /// `Access.public`.
    Access(String),
    /// `CompanyRole.editor`, or `TeamRole.lead(from_team_id)`.
    Role {
        roles: String,
        role: String,
        via: Option<String>,
    },
    /// Anything else.
    Other,
}

fn written_who(expr: &Expr) -> Written {
    match expr {
        Expr::NameRef(name) => tree::name_ref_text(name).map_or(Written::Other, Written::Field),
        Expr::FieldExpr(_) => match pair(expr) {
            Some((host, name)) if host == AccessWord::NAME => Written::Access(name),
            Some((roles, role)) => Written::Role {
                roles,
                role,
                via: None,
            },
            None => Written::Other,
        },
        Expr::CallExpr(call) => {
            let Some((roles, role)) = call.expr().as_ref().and_then(pair) else {
                return Written::Other;
            };
            let args: Vec<Expr> = call
                .arg_list()
                .into_iter()
                .flat_map(|list| list.args())
                .filter_map(|arg| arg.expr())
                .collect();
            match args.as_slice() {
                [Expr::NameRef(name)] => match tree::name_ref_text(name) {
                    Some(via) => Written::Role {
                        roles,
                        role,
                        via: Some(via),
                    },
                    None => Written::Other,
                },
                _ => Written::Other,
            }
        }
        _ => Written::Other,
    }
}

/// The entries of a grant list: `[a, b]`.
fn entries(value: Option<&Expr>) -> Option<Vec<Expr>> {
    match value? {
        Expr::ListExpr(list) => Some(list.exprs().collect()),
        _ => None,
    }
}

/// What role enum `roles` and its `role` resolve to: the membership that
/// holds the enum, or the problem.
#[allow(clippy::result_large_err)]
fn role_of<'m>(
    lookup: &Lookup<'_>,
    memberships: &'m [Membership],
    roles: &str,
    role: &str,
    span: &Span,
    written: &str,
) -> Result<&'m Membership, Diagnostic> {
    let variants = match lookup.def(roles) {
        Some(Def::Enum { variants }) => variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
        _ => {
            return Err(binding(
                span.clone(),
                format!("`{written}` isn't someone a grant can name"),
                "not a user field, a role or `Access`",
                "A grant names a field of the record that points at the caller, a membership role (`<Role>.<value>`), or `Access.authenticated`, `Access.public` or `Access.system`.",
                "Name one of these",
            ));
        }
    };
    let Some(membership) = memberships.iter().find(|m| m.roles == roles) else {
        return Err(Diagnostic::new(
            &rule::POLICY_ROLE_UNBOUND,
            format!("No membership holds a `{roles}`"),
        )
        .at(span.clone(), "a role nobody can hold")
        .reason(format!(
            "A role exists only where it is stored: in a membership entity whose `membership:` names a field typed `{roles}` as its `role:`. Declaring the enum alone gives the role to nobody."
        ))
        .next(format!(
            "Add a membership entity holding `{roles}`, or name someone else in the grant"
        )));
    };
    if !variants.iter().any(|v| v == role) {
        let near = nearest(role, variants.iter().map(String::as_str));
        let problem = binding(
            span.clone(),
            format!("`{roles}` has no role `{role}`"),
            "not one of its values",
            &format!(
                "`{roles}`'s roles are {}.",
                crate::and_list(
                    &variants
                        .iter()
                        .map(|v| format!("`{v}`"))
                        .collect::<Vec<_>>()
                )
            ),
            &match near {
                Some(near) => format!("Use `{roles}.{near}`"),
                None => format!("Name one of `{roles}`'s roles"),
            },
        );
        return Err(problem);
    }
    Ok(membership)
}

/// The project's root `policy { }`, read and resolved.
#[salsa::tracked(returns(ref))]
pub fn root_read(db: &dyn Db, project: Project) -> Arc<RootRead> {
    let mut out = RootRead::default();
    let lookup = Lookup::new(db, project);
    let mut found: Vec<(String, Span, Option<ast::MemberList>)> = Vec::new();
    for &file in project.files(db) {
        let items = item_tree(db, file);
        let indexes: Vec<u32> = items
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Singleton && item.name == "policy")
            .map(|item| item.index)
            .collect();
        if indexes.is_empty() {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let path = file.path(db).clone();
        for index in indexes {
            let Some(Decl::SingletonDecl(decl)) = tree::decl_at(&source, index) else {
                continue;
            };
            let span = decl.keyword().map_or_else(
                || node_span(&path, decl.syntax()),
                |token| {
                    Span::new(
                        &path,
                        u32::from(token.text_range().start()),
                        u32::from(token.text_range().end()),
                    )
                },
            );
            found.push((path.clone(), span, decl.member_list()));
        }
    }
    let Some((path, first_span, list)) = found.first().cloned() else {
        return Arc::new(out);
    };
    for (_, span, _) in found.iter().skip(1) {
        out.problems.push(
            Diagnostic::new(&rule::POLICY_DECLARATION_DUPLICATE, "`policy { }` is declared twice")
                .at(span.clone(), "declared again")
                .also(first_span.clone(), "first declared here")
                .reason("Application-wide policy is declared once, in `policy.jadpo`, so one place says what the tenant is and which actions are exceptions.")
                .next("Move what this `policy` holds into the first, and remove it"),
        );
    }
    out.path = Some(path.clone());
    let memberships = memberships(db, project);
    let entities = Entities::new(db, project);
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    let mut tenant_member = None;
    let mut actions_member = None;
    for item in list.iter().flat_map(|list| list.member_items()) {
        let MemberItem::Member(member) = item else {
            if let MemberItem::Elision(_) = item {
                continue;
            }
            out.problems.push(root_unknown(
                &path,
                item.syntax(),
                &tree::written(item.syntax()),
            ));
            continue;
        };
        let Some(key) = member.member_key() else {
            continue;
        };
        let word = tree::written(key.syntax());
        let span = node_span(&path, key.syntax());
        if word != "tenant" && word != "actions" {
            out.problems.push(root_unknown(&path, key.syntax(), &word));
            continue;
        }
        if let Some(first) = seen.get(&word) {
            let rule = if word == "tenant" {
                &rule::POLICY_SCOPE_DUPLICATE
            } else {
                &rule::POLICY_EFFECT_DUPLICATE
            };
            out.problems.push(
                Diagnostic::new(rule, format!("`{word}:` is given twice"))
                    .at(span, "given again")
                    .also(first.clone(), "first given here")
                    .reason("Root `policy { }` says once what the tenant is and which actions are exceptions, so there is never a question of which counts.")
                    .next(format!("Remove one `{word}:`")),
            );
            continue;
        }
        seen.insert(word.clone(), span);
        if word == "tenant" {
            tenant_member = Some(member);
        } else {
            actions_member = Some(member);
        }
    }
    if let Some(member) = tenant_member {
        let value = member.value();
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let span = value.as_ref().map_or_else(
            || key_span(&path, &member),
            |v| node_span(&path, v.syntax()),
        );
        match &value {
            Some(Expr::NameRef(name))
                if tree::name_ref_text(name).is_some_and(|n| entities.get(&n).is_some()) =>
            {
                let tenant = tree::name_ref_text(name).unwrap_or_default();
                let held: Vec<String> = memberships
                    .iter()
                    .filter(|m| m.scope_entity.as_deref() == Some(tenant.as_str()))
                    .map(|m| m.entity.clone())
                    .collect();
                if held.is_empty() {
                    out.problems.push(
                        Diagnostic::new(&rule::POLICY_ROLE_UNBOUND, format!("No membership is held in `{tenant}`"))
                            .at(span, "a tenant nobody belongs to")
                            .reason(format!("Every grant on a tenant's records applies only within the `{tenant}`s where the caller has a membership, so the tenant needs a membership entity whose `scope:` refers to `{tenant}`; without one nobody could reach anything.")
                            )
                            .next(format!("Add a membership entity with `scope:` a reference to `{tenant}`")),
                    );
                }
                out.model.tenant_memberships = held;
                out.model.tenant = Some(tenant);
            }
            _ => out.problems.push(binding(
                span,
                format!("`tenant: {written}` isn't an entity"),
                "not an entity",
                "Root `policy { }`'s `tenant:` names the entity each tenant's data belongs to.",
                "Name an entity",
            )),
        }
    }
    if let Some(member) = actions_member {
        let mut listed: BTreeMap<String, Span> = BTreeMap::new();
        for item in member.block().iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(line) = item else {
                continue;
            };
            let Some(key) = line.member_key() else {
                continue;
            };
            let written = tree::written(key.syntax());
            let span = node_span(&path, key.syntax());
            let names: Vec<String> = key.names().filter_map(|n| tree::name_text(&n)).collect();
            let action = action_named(&lookup, &names);
            let Some(action) = action else {
                out.problems.push(binding(
                    span,
                    format!("`{written}` isn't an action"),
                    "no such action",
                    "Each line under `actions:` names an action, `domain.action` (or just the action in a single-file project), and lists who may call it.",
                    "Name an action, or remove the line",
                ));
                continue;
            };
            if let Some(first) = listed.get(&action) {
                out.problems.push(
                    Diagnostic::new(&rule::POLICY_EFFECT_DUPLICATE, format!("`{written}` is listed twice"))
                        .at(span, "listed again")
                        .also(first.clone(), "first listed here")
                        .reason("An action takes its permission from one line, so there is never a question of which counts.")
                        .next("Merge the two lines into one"),
                );
                continue;
            }
            listed.insert(action.clone(), span.clone());
            out.places
                .push((action.clone(), node_span(&path, line.syntax())));
            let Some(items) = entries(line.value().as_ref()) else {
                out.problems.push(binding(
                    span,
                    format!("`{written}` isn't given a list"),
                    "not a list",
                    "Each line under `actions:` lists who may call the action: `[Access.authenticated]`.",
                    "Write the list in `[ ]`",
                ));
                continue;
            };
            let mut who = Vec::new();
            let mut written_who: Vec<String> = Vec::new();
            for expr in items {
                let text = tree::written(expr.syntax());
                let at = node_span(&path, expr.syntax());
                if written_who.contains(&text) {
                    out.problems.push(subject_duplicate(at, &text));
                    continue;
                }
                written_who.push(text.clone());
                let resolved = match written_who_kind(&expr) {
                    Written::Access(word) => access_word(&word)
                        .ok_or_else(|| access_unknown(at.clone(), &word))
                        .map(|a| match a {
                            AccessWord::Authenticated => ActionWho::Signed,
                            AccessWord::Public => ActionWho::Public,
                            AccessWord::System => ActionWho::System,
                        }),
                    Written::Role {
                        roles,
                        role,
                        via: None,
                    } if entities.get(&roles).is_some() => {
                        // `Todo.owner_id`: a field of an entity.
                        let entity = entities.get(&roles);
                        match entity
                            .and_then(|e| caller_kind(db, project, e, &role).map(|k| (e, k)))
                        {
                            Some((e, kind)) => Ok(ActionWho::Field {
                                entity: e.name.clone(),
                                field: role.clone(),
                                kind,
                            }),
                            None => Err(binding(
                                at.clone(),
                                format!("`{text}` doesn't point at a caller"),
                                "not a user field",
                                &format!(
                                    "An `actions:` line names a field of a record that points at the caller, a reference to the user or service entity; `{roles}.{role}` is not one."
                                ),
                                "Name a field that refers to the user, or a role, or `Access`",
                            )),
                        }
                    }
                    Written::Role {
                        roles,
                        role,
                        via: None,
                    } => role_of(&lookup, memberships, &roles, &role, &at, &text).map(|m| {
                        ActionWho::Role {
                            membership: m.entity.clone(),
                            role: role.clone(),
                        }
                    }),
                    Written::Field(field) => Err(binding(
                        at.clone(),
                        format!("`{field}` doesn't say whose field it is"),
                        "a bare field",
                        "An `actions:` line applies to every record its action reaches, so a field names its entity: `<Entity>.<field>`.",
                        &format!("Write `<Entity>.{field}`"),
                    )),
                    _ => Err(binding(
                        at.clone(),
                        format!("`{text}` isn't someone an action can be granted to"),
                        "not a field, a role or `Access`",
                        "An `actions:` line names a field of a record the action reaches that points at the caller (`<Entity>.<field>`), a membership role (`<Role>.<value>`), or `Access.authenticated`, `Access.public` or `Access.system`.",
                        "Name one of these",
                    )),
                };
                match resolved {
                    Ok(w) => who.push(w),
                    Err(problem) => out.problems.push(problem),
                }
            }
            out.model.actions.push(ActionGrants {
                action,
                written,
                who,
                written_who,
            });
        }
    }
    Arc::new(out)
}

fn written_who_kind(expr: &Expr) -> Written {
    written_who(expr)
}

fn access_word(word: &str) -> Option<AccessWord> {
    AccessWord::from_name(word)
}

fn access_unknown(span: Span, word: &str) -> Diagnostic {
    let near = nearest(word, AccessWord::NAMES.iter().copied());
    binding(
        span,
        format!("`Access.{word}` isn't someone a grant can name"),
        "not one of `Access`'s values",
        "`Access.authenticated` is anyone signed in, `Access.public` anyone at all, and `Access.system` schedules and subscribers.",
        &match near {
            Some(near) => format!("Use `Access.{near}`"),
            None => "Use `Access.authenticated`, `Access.public` or `Access.system`".to_string(),
        },
    )
}

fn subject_duplicate(span: Span, written: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::POLICY_SUBJECT_DUPLICATE,
        format!("`{written}` is listed twice"),
    )
    .at(span, "listed again")
    .reason("A grant list means \"any of these\", so naming someone twice adds nothing and hides which line was meant.")
    .next(format!("Remove the second `{written}`"))
}

fn root_unknown(path: &str, node: &jadpo_syntax::SyntaxNode, word: &str) -> Diagnostic {
    let span = node_span(path, node);
    let near = nearest(word, ["tenant", "actions"]);
    let problem = Diagnostic::new(
        &rule::POLICY_EFFECT_UNKNOWN,
        format!("`{word}` isn't part of root `policy {{ }}`"),
    )
    .at(span.clone(), "not part of root policy")
    .reason(match near {
        Some(near) => format!("Root `policy {{ }}` holds `tenant:` and `actions:`; everything else is written beside each entity. Did you mean `{near}`?"),
        None => "Root `policy { }` holds `tenant:` and `actions:`; everything else is written beside each entity.".to_string(),
    });
    match near {
        Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
        None => problem.next(format!(
            "Remove `{word}`, or write it in the entity's `policy:`"
        )),
    }
}

/// The action `names` names: `domain.action`, or the bare action of a
/// single-file project.
fn action_named(lookup: &Lookup<'_>, names: &[String]) -> Option<String> {
    let (domain, action) = match names {
        [action] => (None, action),
        [domain, action] => (Some(domain), action),
        _ => return None,
    };
    let symbol = lookup
        .declared(action)
        .iter()
        .find(|s| s.kind == ItemKind::Action)?;
    let matches = match (&symbol.ownership, domain) {
        (Ownership::Domain(holder), Some(domain)) => holder == domain,
        (Ownership::Loose, None) => true,
        _ => false,
    };
    matches.then(|| action.clone())
}

/// The entity called `name`'s policy, read and resolved.
#[salsa::tracked(returns(ref))]
pub fn policy_read<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Option<PolicyRead> {
    let lookup = Lookup::new(db, project);
    let entity = Entities::new(db, project).get(name.text(db))?;
    let (path, decl) = entity_decl(&lookup, db, project, name.text(db))?;
    let memberships = memberships(db, project);
    let root = root_read(db, project);
    let mut problems = Vec::new();
    let found = sections(&decl, "policy");
    for again in found.iter().skip(1) {
        problems.push(
            Diagnostic::new(
                &rule::POLICY_DECLARATION_DUPLICATE,
                format!("`{}` declares `policy:` twice", entity.name),
            )
            .at(key_span(&path, again), "declared again")
            .also(key_span(&path, &found[0]), "first declared here")
            .reason("An entity's grants are one list per effect in one `policy:`, so \"who can delete this?\" is always one line.")
            .next("Move what the second `policy:` holds into the first, and remove it"),
        );
    }
    let mut policy = EntityPolicy {
        entity: entity.name.clone(),
        lists: BTreeMap::new(),
        tenancy: None,
        shared: false,
        fixed: Vec::new(),
        fields: Vec::new(),
    };
    let mut tenant_written: Option<(Expr, Span)> = None;
    let mut places: Vec<(String, Span)> = Vec::new();
    if let Some(section) = found.first() {
        let mut seen: BTreeMap<String, Span> = BTreeMap::new();
        for item in section.block().iter().flat_map(|list| list.member_items()) {
            let MemberItem::Member(member) = item else {
                if let MemberItem::Elision(_) = item {
                    continue;
                }
                problems.push(effect_unknown(
                    &path,
                    item.syntax(),
                    &tree::written(item.syntax()),
                ));
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let word = tree::written(key.syntax());
            let span = node_span(&path, key.syntax());
            let known = Effect::from_word(&word).is_some() || word == "tenant" || word == "scopes";
            if !known {
                problems.push(effect_unknown(&path, key.syntax(), &word));
                continue;
            }
            if let Some(first) = seen.get(&word) {
                let rule = if word == "tenant" {
                    &rule::POLICY_SCOPE_DUPLICATE
                } else {
                    &rule::POLICY_EFFECT_DUPLICATE
                };
                problems.push(
                    Diagnostic::new(rule, format!("`{word}:` is given twice"))
                        .at(span, "given again")
                        .also(first.clone(), "first given here")
                        .reason(format!("`{}`'s policy gives each list once, so \"who may {word}?\" is always one line.", entity.name))
                        .next(format!("Merge the two `{word}:` lists into one")),
                );
                continue;
            }
            seen.insert(word.clone(), span.clone());
            if word == "scopes" {
                problems.push(
                    Diagnostic::new(&rule::TOOLCHAIN_NOT_BUILT_YET, "A policy's `scopes:` isn't built yet")
                        .at(span, "decided, not built")
                        .reason("`scopes:` narrows what a token issued to another application may do (area 9, question 6). OAuth tokens and declared external-token scope mappings are not built yet; an external method without those mappings carries no policy scopes.")
                        .next("Leave `scopes:` out for now"),
                );
                continue;
            }
            places.push((word.clone(), node_span(&path, member.syntax())));
            if word == "tenant" {
                if let Some(value) = member.value() {
                    let at = node_span(&path, value.syntax());
                    tenant_written = Some((value, at));
                }
                continue;
            }
            let Some(effect) = Effect::from_word(&word) else {
                continue;
            };
            let Some(items) = entries(member.value().as_ref()) else {
                problems.push(binding(
                    member
                        .value()
                        .map_or(span.clone(), |v| node_span(&path, v.syntax())),
                    format!("`{word}:` isn't given a list"),
                    "not a list",
                    "Each grant lists who may do it, in `[ ]`: `read: [Access.authenticated]`.",
                    "Write the list in `[ ]`",
                ));
                continue;
            };
            let mut list = GrantList::default();
            for expr in items {
                let text = tree::written(expr.syntax());
                let at = node_span(&path, expr.syntax());
                if list.written.contains(&text) {
                    problems.push(subject_duplicate(at, &text));
                    continue;
                }
                list.written.push(text.clone());
                match resolve_who(db, project, &lookup, entity, memberships, &expr, &text, &at) {
                    Ok(grant) => {
                        if !list.grants.contains(&grant) {
                            list.grants.push(grant);
                        }
                    }
                    Err(problem) => problems.push(problem),
                }
            }
            policy.lists.insert(effect, list);
        }
    }
    // Tenancy (area 9, "Tenancy").
    let tenant = root.model.tenant.clone();
    match (&tenant, &tenant_written) {
        (_, Some((value, at)))
            if pair(value)
                == Some((TenancyWord::NAME.to_string(), TenancyWord::Shared.name().to_string())) =>
        {
            let _ = at;
            policy.shared = true;
        }
        (Some(tenant), Some((value, at))) => {
            let field = match value {
                Expr::NameRef(name) => tree::name_ref_text(name),
                _ => None,
            };
            let refers = field.as_deref().is_some_and(|f| {
                entity.fields.iter().any(|x| {
                    x.name == f && x.reference.as_ref().is_some_and(|r| r.target == *tenant)
                })
            });
            match field {
                Some(field) if refers => {
                    policy.tenancy = Some(Tenancy {
                        entity: tenant.clone(),
                        via: field,
                    })
                }
                _ => problems.push(binding(
                    at.clone(),
                    format!("`tenant: {}` isn't a reference to `{tenant}`", tree::written(value.syntax())),
                    "not a reference to the tenant",
                    &format!("An entity's `tenant:` names which of its references to `{tenant}` its records belong to, or says `Tenancy.shared` for records every tenant shares."),
                    &format!("Name a field typed `{tenant}.id`, or write `Tenancy.shared`"),
                )),
            }
        }
        (None, Some((value, at))) => problems.push(binding(
            at.clone(),
            format!("`tenant: {}` names a tenant nobody declared", tree::written(value.syntax())),
            "no tenant",
            "An entity's `tenant:` says which tenant its records belong to, and root `policy { }` declares no `tenant:`.",
            "Remove `tenant:`, or declare the tenant in `policy.jadpo`",
        )),
        (Some(tenant), None) => {
            let refs = references_to(entity, tenant);
            if entity.name == *tenant {
                policy.tenancy = Some(Tenancy {
                    entity: tenant.clone(),
                    via: "id".to_string(),
                });
            } else {
                match refs.as_slice() {
                    [one] => {
                        policy.tenancy = Some(Tenancy {
                            entity: tenant.clone(),
                            via: one.clone(),
                        })
                    }
                    [] => problems.push(
                        Diagnostic::new(
                            &rule::POLICY_SCOPE_MISSING,
                            format!("`{}` doesn't belong to a `{tenant}`", entity.name),
                        )
                        .at(lookup_name_span(&path, &decl), format!("no reference to `{tenant}`"))
                        .reason(format!(
                            "Root `policy {{ }}` names `{tenant}` as the tenant, so every entity carries its own reference to it and its grants apply only within the `{tenant}`s where the caller has a membership; an entity every tenant shares says `tenant: Tenancy.shared`."
                        ))
                        .next(format!(
                            "Add a field typed `{tenant}.id` to `{}`, or `tenant: Tenancy.shared` to its `policy:`",
                            entity.name
                        )),
                    ),
                    _ => problems.push(
                        Diagnostic::new(
                            &rule::POLICY_SCOPE_AMBIGUOUS,
                            format!("`{}` refers to `{tenant}` twice", entity.name),
                        )
                        .at(lookup_name_span(&path, &decl), "which one is its tenant?")
                        .reason(format!(
                            "`{}` has {} references to `{tenant}`, the tenant, so its `policy:` says which one its records belong to.",
                            entity.name,
                            refs.len()
                        ))
                        .next(format!("Add `tenant: {}` to its `policy:`", refs[0])),
                    ),
                }
            }
        }
        (None, None) => {}
    }
    // Each field's own `policy:` (area 9, "Field restrictions").
    for (field, member) in field_policies(&decl, entity) {
        places.push((format!("field {field}"), node_span(&path, member.syntax())));
        if let Some(read) = field_policy(
            db,
            project,
            &lookup,
            entity,
            memberships,
            &policy,
            &path,
            &field,
            &member,
            &mut problems,
        ) {
            policy.fields.push(read);
        }
    }
    // The fields only a listed action changes.
    let mut fixed: Vec<String> = Vec::new();
    let field_lists = policy.fields.iter().flat_map(|field| field.lists.values());
    for list in policy.lists.values().chain(field_lists) {
        for grant in &list.grants {
            let field = match grant {
                Grant::Caller { field, .. } => Some(field),
                Grant::Role { via: Some(via), .. } => Some(via),
                _ => None,
            };
            if let Some(field) = field
                && field != "id"
                && !fixed.contains(field)
            {
                fixed.push(field.clone());
            }
        }
    }
    if let Some(tenancy) = &policy.tenancy
        && tenancy.via != "id"
        && !fixed.contains(&tenancy.via)
    {
        fixed.push(tenancy.via.clone());
    }
    if let Some(membership) = memberships.iter().find(|m| m.entity == entity.name) {
        for field in membership.scope.iter().chain([&membership.member]) {
            if !fixed.contains(field) {
                fixed.push(field.clone());
            }
        }
    }
    fixed.sort_by_key(|name| entity.fields.iter().position(|f| f.name == *name));
    policy.fixed = fixed;
    Some(PolicyRead {
        model: Arc::new(policy),
        path,
        places,
        problems,
    })
}

fn lookup_name_span(path: &str, decl: &ast::EntityDecl) -> Span {
    decl.name().map_or_else(
        || node_span(path, decl.syntax()),
        |name| node_span(path, name.syntax()),
    )
}

fn effect_unknown(path: &str, node: &jadpo_syntax::SyntaxNode, word: &str) -> Diagnostic {
    let span = node_span(path, node);
    let known = ["create", "read", "update", "delete", "tenant"];
    let near = nearest(word, known);
    let problem = Diagnostic::new(
        &rule::POLICY_EFFECT_UNKNOWN,
        format!("`{word}` isn't something a policy grants"),
    )
    .at(span.clone(), "not a grant or a setting")
    .reason(match near {
        Some(near) => format!("An entity's `policy:` lists who may `create`, `read`, `update` and `delete` its records, and may say which `tenant:` they belong to. Every action takes its permission from these. Did you mean `{near}`?"),
        None => "An entity's `policy:` lists who may `create`, `read`, `update` and `delete` its records, and may say which `tenant:` they belong to. Every action takes its permission from these; one that needs others is listed under `actions:` in `policy.jadpo`.".to_string(),
    });
    match near {
        Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
        None => problem.next(format!("Remove `{word}`")),
    }
}

/// One entry of an entity's grant list, resolved against the entity.
#[allow(clippy::too_many_arguments, clippy::result_large_err)]
fn resolve_who(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    entity: &EntityModel,
    memberships: &[Membership],
    expr: &Expr,
    text: &str,
    at: &Span,
) -> Result<Grant, Diagnostic> {
    match written_who(expr) {
        Written::Field(field) => {
            if let Some(kind) = caller_kind(db, project, entity, &field) {
                return Ok(Grant::Caller { field, kind });
            }
            let user = principal_entity(db, project, PrincipalKind::User);
            let exists = entity.fields.iter().any(|f| f.name == field);
            let reason = match (&user, exists) {
                (Some(user), true) => format!(
                    "A field names who may do this when it points at the caller: a reference to `{user}`, the principal's user entity (area 8), typed `{user}.id`. `{}.{field}` isn't.",
                    entity.name
                ),
                (Some(user), false) => format!(
                    "`{}` has no field `{field}`. A field names who may do this when it is a reference to `{user}`, the principal's user entity.",
                    entity.name
                ),
                (None, _) => "A field names who may do this when it points at the caller, a reference to the principal's user entity, and `app.jadpo` declares no `principal { }` (area 8).".to_string(),
            };
            Err(binding(
                at.clone(),
                format!("`{field}` doesn't point at a caller"),
                "not a user field",
                &reason,
                "Name a reference to the user, a membership role, or `Access`",
            ))
        }
        Written::Access(word) => match access_word(&word) {
            Some(AccessWord::Authenticated) => Ok(Grant::Signed),
            Some(AccessWord::Public) => Ok(Grant::Public),
            Some(AccessWord::System) => Ok(Grant::System),
            None => Err(access_unknown(at.clone(), &word)),
        },
        Written::Role { roles, role, via } => {
            let membership = role_of(lookup, memberships, &roles, &role, at, text)?;
            let Some(scope) = &membership.scope_entity else {
                if let Some(via) = via {
                    return Err(binding(
                        at.clone(),
                        format!("`{roles}` is held across the whole application"),
                        format!("`({via})` names nothing to apply through").as_str(),
                        &format!(
                            "`{}`'s roles are held across the whole application (`MembershipScope.application`), so a grant of one applies to every record and names no reference.",
                            membership.entity
                        ),
                        &format!("Write `{roles}.{role}`"),
                    ));
                }
                return Ok(Grant::Role {
                    membership: membership.entity.clone(),
                    role,
                    via: None,
                });
            };
            if let Some(via) = via {
                let refers = (via == "id" && entity.name == *scope)
                    || entity.fields.iter().any(|f| {
                        f.name == via && f.reference.as_ref().is_some_and(|r| r.target == *scope)
                    });
                if !refers {
                    return Err(binding(
                        at.clone(),
                        format!("`{via}` isn't a reference to `{scope}`"),
                        "not a reference to the role's scope",
                        &format!(
                            "`{roles}` is held in a `{scope}`, so `{roles}.{role}(...)` names one of `{}`'s references to `{scope}`.",
                            entity.name
                        ),
                        &format!("Name a field of `{}` typed `{scope}.id`", entity.name),
                    ));
                }
                return Ok(Grant::Role {
                    membership: membership.entity.clone(),
                    role,
                    via: Some(via),
                });
            }
            if entity.name == *scope {
                return Ok(Grant::Role {
                    membership: membership.entity.clone(),
                    role,
                    via: Some("id".to_string()),
                });
            }
            let refs = references_to(entity, scope);
            match refs.as_slice() {
                [one] => Ok(Grant::Role {
                    membership: membership.entity.clone(),
                    role,
                    via: Some(one.clone()),
                }),
                [] => Err(Diagnostic::new(
                    &rule::POLICY_SCOPE_MISSING,
                    format!("`{}` has no reference to `{scope}`", entity.name),
                )
                .at(at.clone(), format!("a role held in a `{scope}`"))
                .reason(format!(
                    "`{roles}` is held in a `{scope}` (`{}`'s `scope:`), and a role applies to a record through the record's reference to its scope, so `{}` needs a field typed `{scope}.id`.",
                    membership.entity, entity.name
                ))
                .next(format!(
                    "Add a reference to `{scope}` to `{}`, or name someone else",
                    entity.name
                ))),
                _ => Err(Diagnostic::new(
                    &rule::POLICY_SCOPE_AMBIGUOUS,
                    format!("`{}` refers to `{scope}` more than once", entity.name),
                )
                .at(at.clone(), "through which reference?")
                .reason(format!(
                    "`{roles}` is held in a `{scope}`, and `{}` has {} references to `{scope}` ({}), so the grant names the one it means.",
                    entity.name,
                    refs.len(),
                    crate::and_list(&refs.iter().map(|r| format!("`{r}`")).collect::<Vec<_>>())
                ))
                .next(format!("Write `{roles}.{role}({})`", refs[0]))),
            }
        }
        Written::Other => Err(binding(
            at.clone(),
            format!("`{text}` isn't someone a grant can name"),
            "not a user field, a role or `Access`",
            "A grant names a field of the record that points at the caller, a membership role (`<Role>.<value>`), or `Access.authenticated`, `Access.public` or `Access.system`.",
            "Name one of these",
        )),
    }
}

/// Each field of `decl` with a `policy:` in its block, with that member.
fn field_policies(decl: &ast::EntityDecl, entity: &EntityModel) -> Vec<(String, ast::Member)> {
    let mut out = Vec::new();
    for (name, member) in tree::members(decl.member_list()) {
        let Some(field) = tree::name_text(&name) else {
            continue;
        };
        if !entity.fields.iter().any(|f| f.name == field) {
            continue;
        }
        let Some(Expr::RecordExpr(record)) = member.value() else {
            continue;
        };
        if let Some((_, setting)) = tree::members(record.member_list())
            .into_iter()
            .find(|(key, _)| tree::name_text(key).as_deref() == Some("policy"))
        {
            out.push((field, setting));
        }
    }
    out
}

/// Whether the record's list `entity` lets in everyone `grant` names: the
/// grant itself, anyone signed in for a field, a role or anyone signed in,
/// or anyone at all.
pub fn admits(entity: &[Grant], grant: &Grant) -> bool {
    entity.contains(grant)
        || entity.contains(&Grant::Public)
        || (entity.contains(&Grant::Signed)
            && matches!(
                grant,
                Grant::Caller { .. } | Grant::Role { .. } | Grant::Signed
            ))
}

/// A grant as a list writes it: `owner_id`, `CompanyRole.editor`,
/// `Access.authenticated`.
pub fn written_grant(memberships: &[Membership], grant: &Grant) -> String {
    match grant {
        Grant::Caller { field, .. } => field.clone(),
        Grant::Role {
            membership, role, ..
        } => match memberships.iter().find(|m| m.entity == *membership) {
            Some(held) => format!("{}.{role}", held.roles),
            None => role.clone(),
        },
        Grant::Signed => format!("{}.{}", AccessWord::NAME, AccessWord::Authenticated.name()),
        Grant::Public => format!("{}.{}", AccessWord::NAME, AccessWord::Public.name()),
        Grant::System => format!("{}.{}", AccessWord::NAME, AccessWord::System.name()),
    }
}

/// The field `field`'s own `policy:` (area 9, "Field restrictions"), read
/// and resolved against its entity, which it can only narrow.
#[allow(clippy::too_many_arguments)]
fn field_policy(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    entity: &EntityModel,
    memberships: &[Membership],
    policy: &EntityPolicy,
    path: &str,
    field: &str,
    member: &ast::Member,
    problems: &mut Vec<Diagnostic>,
) -> Option<FieldPolicy> {
    let mut out = FieldPolicy {
        field: field.to_string(),
        server_only: false,
        lists: BTreeMap::new(),
    };
    let shape = "A field's `policy:` narrows its record's grants in the same shape, `{ read: [...] update: [...] }`, or says `FieldAccess.server_only` for a field no client may read or write.";
    if let Some(value) = member.value() {
        let at = node_span(path, value.syntax());
        let written = tree::written(value.syntax());
        match pair(&value) {
            Some((host, word)) if host == FieldAccess::NAME => {
                if FieldAccess::from_name(&word) == Some(FieldAccess::ServerOnly) {
                    out.server_only = true;
                    return Some(out);
                }
                problems.push(binding(
                    at,
                    format!("`{written}` isn't one of `FieldAccess`'s values"),
                    "not a `FieldAccess` value",
                    shape,
                    "Use `FieldAccess.server_only`",
                ));
            }
            _ => problems.push(binding(
                at,
                format!(
                    "`{entity}.{field}`'s `policy:` isn't a list of grants",
                    entity = entity.name
                ),
                "not grants",
                shape,
                "Write `policy: { read: [...] }`, or `policy: FieldAccess.server_only`",
            )),
        }
        return None;
    }
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    for item in member.block().iter().flat_map(|list| list.member_items()) {
        let MemberItem::Member(line) = item else {
            if let MemberItem::Elision(_) = item {
                continue;
            }
            problems.push(field_effect_unknown(
                path,
                item.syntax(),
                &tree::written(item.syntax()),
            ));
            continue;
        };
        let Some(key) = line.member_key() else {
            continue;
        };
        let word = tree::written(key.syntax());
        let span = node_span(path, key.syntax());
        let Some(effect) =
            Effect::from_word(&word).filter(|effect| FieldPolicy::EFFECTS.contains(effect))
        else {
            problems.push(field_effect_unknown(path, key.syntax(), &word));
            continue;
        };
        if let Some(first) = seen.get(&word) {
            problems.push(
                Diagnostic::new(&rule::POLICY_EFFECT_DUPLICATE, format!("`{word}:` is given twice"))
                    .at(span, "given again")
                    .also(first.clone(), "first given here")
                    .reason(format!("`{}.{field}`'s policy gives each list once, so \"who may {word} it?\" is always one line.", entity.name))
                    .next(format!("Merge the two `{word}:` lists into one")),
            );
            continue;
        }
        seen.insert(word.clone(), span.clone());
        let Some(items) = entries(line.value().as_ref()) else {
            problems.push(binding(
                line.value()
                    .map_or(span.clone(), |v| node_span(path, v.syntax())),
                format!("`{word}:` isn't given a list"),
                "not a list",
                "Each grant lists who may do it, in `[ ]`: `read: [Access.authenticated]`.",
                "Write the list in `[ ]`",
            ));
            continue;
        };
        let record = policy.grants(effect);
        let mut list = GrantList::default();
        for expr in items {
            let text = tree::written(expr.syntax());
            let at = node_span(path, expr.syntax());
            if list.written.contains(&text) {
                problems.push(subject_duplicate(at, &text));
                continue;
            }
            list.written.push(text.clone());
            match resolve_who(db, project, lookup, entity, memberships, &expr, &text, &at) {
                Ok(grant) if !admits(record, &grant) => {
                    problems.push(widens(entity, field, effect, &text, at, policy));
                }
                Ok(grant) => {
                    if !list.grants.contains(&grant) {
                        list.grants.push(grant);
                    }
                }
                Err(problem) => problems.push(problem),
            }
        }
        out.lists.insert(effect, list);
    }
    Some(out)
}

/// `policy.field_widens_entity`: a field's list naming `who`, whom its
/// record's list for `effect` doesn't let in.
fn widens(
    entity: &EntityModel,
    field: &str,
    effect: Effect,
    who: &str,
    at: Span,
    policy: &EntityPolicy,
) -> Diagnostic {
    let word = effect.word();
    let names = match policy.lists.get(&effect) {
        Some(list) if !list.written.is_empty() => format!(
            "which names {}",
            crate::and_list(
                &list
                    .written
                    .iter()
                    .map(|w| format!("`{w}`"))
                    .collect::<Vec<_>>()
            )
        ),
        _ => "which grants nobody".to_string(),
    };
    Diagnostic::new(
        &rule::POLICY_FIELD_WIDENS_ENTITY,
        format!(
            "`{}.{field}` lets `{who}` {word} it, which `{}` doesn't",
            entity.name, entity.name
        ),
    )
    .at(at, format!("not in `{}`'s `{word}:`", entity.name))
    .reason(format!(
        "A field's `policy:` only takes away from its record's grants: it narrows who may {word} the field, and never lets in anyone `{}`'s `{word}:` doesn't, {names}.",
        entity.name
    ))
    .next(format!("Remove `{who}` from `{field}`'s `{word}:`"))
}

fn field_effect_unknown(path: &str, node: &jadpo_syntax::SyntaxNode, word: &str) -> Diagnostic {
    let span = node_span(path, node);
    let known = ["create", "read", "update"];
    let near = (word != "delete").then(|| nearest(word, known)).flatten();
    let reason = if word == "delete" {
        "A field is deleted with its record, so its `policy:` narrows who may `create`, `read` and `update` it, and the record's `delete:` decides the rest.".to_string()
    } else {
        match near {
            Some(near) => format!("A field's `policy:` narrows who may `create`, `read` and `update` it, or says `FieldAccess.server_only`. Did you mean `{near}`?"),
            None => "A field's `policy:` narrows who may `create`, `read` and `update` it, or says `FieldAccess.server_only`; the tenant and the rest are the record's.".to_string(),
        }
    };
    let problem = Diagnostic::new(
        &rule::POLICY_EFFECT_UNKNOWN,
        format!("`{word}` isn't something a field's policy narrows"),
    )
    .at(span.clone(), "not a field's grant")
    .reason(reason);
    match near {
        Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
        None => problem.next(format!("Remove `{word}`")),
    }
}

/// Every entity's policy, resolved: what the plan compiles and the report
/// shows.
#[salsa::tracked(returns(ref))]
pub fn access(db: &dyn Db, project: Project) -> Arc<Access> {
    let mut entities = BTreeMap::new();
    for name in crate::data::entity_names(db, project) {
        if let Some(read) = policy_read(db, project, Name::new(db, name.clone())) {
            entities.insert(name.clone(), (*read.model).clone());
        }
    }
    Arc::new(Access {
        root: root_read(db, project).model.clone(),
        memberships: (**memberships(db, project)).clone(),
        entities,
    })
}

/// The entity called `name`'s policy, resolved, without positions: what
/// every check that proves against one entity's grants reads, so a comment
/// or another entity's policy never reaches it (chunk 4.11).
#[salsa::tracked(returns(ref))]
pub fn entity_policy<'db>(
    db: &'db dyn Db,
    project: Project,
    name: Name<'db>,
) -> Option<Arc<EntityPolicy>> {
    policy_read(db, project, name)
        .as_ref()
        .map(|read| Arc::clone(&read.model))
}

/// Root `policy { }`, resolved, without positions.
#[salsa::tracked(returns(ref))]
pub fn root_policy(db: &dyn Db, project: Project) -> RootPolicy {
    root_read(db, project).model.clone()
}

/// Policy read by name, as [`Access`] holds it whole: each entity's
/// policy is a read of that one entity, so a check depends only on the
/// policies it asks for. Root `policy { }` and the memberships are read
/// whole, being few and read by nearly every proof.
#[derive(Clone, Copy)]
pub struct Policies<'db> {
    db: &'db dyn Db,
    project: Project,
}

impl std::fmt::Debug for Policies<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Policies").finish_non_exhaustive()
    }
}

impl<'db> Policies<'db> {
    pub fn new(db: &'db dyn Db, project: Project) -> Policies<'db> {
        Policies { db, project }
    }

    pub fn entity(&self, name: &str) -> Option<&'db EntityPolicy> {
        entity_policy(self.db, self.project, Name::new(self.db, name.to_string())).as_deref()
    }

    pub fn root(&self) -> &'db RootPolicy {
        root_policy(self.db, self.project)
    }

    pub fn memberships(&self) -> &'db [Membership] {
        memberships(self.db, self.project).as_slice()
    }

    pub fn membership(&self, entity: &str) -> Option<&'db Membership> {
        self.memberships().iter().find(|m| m.entity == entity)
    }

    /// See [`Access::kept_in_tenant`].
    pub fn kept_in_tenant(&self, from: &str, to: &str) -> Option<(&'db Tenancy, &'db Tenancy)> {
        let from = self.entity(from)?.tenancy.as_ref()?;
        let to = self.entity(to)?.tenancy.as_ref()?;
        (to.via != "id").then_some((from, to))
    }
}

/// Whether the action called `name` is listed under root `policy {
/// actions: }`.
pub fn listed(db: &dyn Db, project: Project, name: &str) -> bool {
    root_policy(db, project).action(name).is_some()
}

/// The fields of `entity` only a listed action changes.
pub fn fixed_fields(db: &dyn Db, project: Project, entity: &str) -> Vec<String> {
    entity_policy(db, project, Name::new(db, entity.to_string()))
        .as_ref()
        .map(|model| model.fixed.clone())
        .unwrap_or_default()
}

/// Area 9's problems in one file: its entities' memberships and policies,
/// and root `policy { }` when it is written there.
#[salsa::tracked(returns(ref))]
pub fn policy_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let path = file.path(db);
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for item in &item_tree(db, file).items {
        if item.kind != ItemKind::Entity || !seen.insert(item.name.as_str()) {
            continue;
        }
        let name = Name::new(db, item.name.clone());
        if let Some(read) = membership_read(db, project, name)
            && read
                .problems
                .iter()
                .all(|p| p.primary.as_ref().is_none_or(|l| l.span.path == *path))
        {
            out.extend(read.problems.iter().cloned());
        }
        if let Some(read) = policy_read(db, project, name)
            && read.path == *path
        {
            out.extend(read.problems.iter().cloned());
        }
    }
    let root = root_read(db, project);
    if root.path.as_deref() == Some(path.as_str()) {
        out.extend(root.problems.iter().cloned());
    }
    out
}

/// Whether every policy, membership and root `policy { }` read without a
/// problem: what the route checks and the report need.
#[salsa::tracked(returns(copy))]
pub fn policy_clean(db: &dyn Db, project: Project) -> bool {
    if !root_read(db, project).problems.is_empty() {
        return false;
    }
    crate::data::entity_names(db, project).iter().all(|name| {
        let name = Name::new(db, name.clone());
        membership_read(db, project, name)
            .as_ref()
            .is_none_or(|read| read.problems.is_empty())
            && policy_read(db, project, name)
                .as_ref()
                .is_none_or(|read| read.problems.is_empty())
    })
}
