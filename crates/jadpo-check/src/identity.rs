//! Area 8's declarations: who can call the application and how they sign
//! in. `principal { }` says which entity field identifies a user and a
//! service and when each counts as active; `authentication { }` names the
//! ways a caller signs in, each one block from a fixed list; `application
//! { }` holds the one revocation setting.
//!
//! Read once per project ([`identity`]) into an [`Identity`], which the
//! routes (whether anything signs anyone in, what `current_principal`
//! is), the bodies (`Principal.user`, `sessions.start`), the schema (the
//! sessions and service keys tables) and `jadpo-plan` read. Built in
//! chunk 4.3: browser sessions; in chunk 4.3b: API tokens and service
//! keys; in chunk 5.1b: the revocation delay, with each method's
//! `signing_key:` and a browser session's `origin:` read from settings
//! (area 10), and the generated `POST /auth/exchange`; in chunk 5.5z:
//! `sign_in:` with `passkey:`, whose routes the runtime answers. External
//! tokens, magic links, sign-in providers and acting as an OAuth provider
//! come later: each is `toolchain.not_built_yet` where it is written.

use std::sync::Arc;

use jadpo_db::{Db, ItemKind, Project, item_tree};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::AuthenticationMethod;
use jadpo_std::http::PrincipalKind;
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use crate::case::starts_upper;
use crate::data::queries::{Compare, Condition, Value, comparable, ordered};
use crate::data::{EntityModel, FieldModel};
use crate::lookups::Lookup;
use crate::tree::{self, dotted, node_span};
use crate::types::{Const, Def, Prim, Ty, TypeView};

/// The fields every caller has, built in (area 8, question 2).
pub const PRINCIPAL_FIELDS: [&str; 3] = ["id", "subject", "strength"];

/// `type.auth_sessions_context` for an operation whose sign-in method
/// `authentication { }` doesn't declare: `sessions.issue_token` with no
/// `api_token:`. `ends` is `sessions.end`, which either browser sessions or
/// API tokens serve.
pub fn needs_method(
    written: &str,
    method: AuthenticationMethod,
    ends: bool,
    span: Span,
) -> Diagnostic {
    let (needs, declare) = match method {
        AuthenticationMethod::BrowserSession if ends => (
            "browser sessions or API tokens",
            "A session is a browser session, which `authentication { browser_session: { cookie: \"<name>\" } }` declares, or an API token, which `api_token: {}` declares.",
        ),
        AuthenticationMethod::BrowserSession => (
            "browser sessions",
            "A session is a browser session, which `authentication { browser_session: { cookie: \"<name>\" } }` declares.",
        ),
        AuthenticationMethod::ApiToken => (
            "API tokens",
            "An API token signs a user in from the application's own clients, which `authentication { api_token: {} }` declares.",
        ),
        _ => (
            "service keys",
            "A service key signs a service in, which `authentication { service_key: {} }` declares.",
        ),
    };
    Diagnostic::new(
        &rule::TYPE_AUTH_SESSIONS_CONTEXT,
        format!("`{written}` needs {needs}"),
    )
    .at(span, format!("no `{}` declared", method.name()))
    .reason(declare)
    .next(format!(
        "Declare `{}:` in `authentication.jadpo`",
        method.name()
    ))
}

/// The type of the whole caller, either kind: `Principal`.
pub const PRINCIPAL: &str = "Principal";

/// The settings `browser_session` takes.
const BROWSER_SESSION: [&str; 3] = ["cookie", "origin", "signing_key"];

/// The settings `passkey` takes (area 8, "Passkeys").
const PASSKEY: [&str; 2] = ["expires_after", "path"];

/// The ways `sign_in:` checks a person (area 8, "Checking the person").
const SIGN_IN_WAYS: [&str; 3] = ["magic_link", "passkey", "providers"];

/// What `authentication { }` may hold besides its methods, each built by a
/// later chunk.
const LATER_BLOCKS: [(&str, &str); 2] = [
    (
        "sign_in_providers",
        "Sign-in providers (\"Sign in with Google\") come after the first build (area 8, question 11)",
    ),
    (
        "oauth_provider",
        "Acting as an OAuth provider comes after the first build (area 8, question 13)",
    ),
];

/// One kind of caller, as `principal { }` declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub kind: PrincipalKind,
    /// The entity that holds it: `User`.
    pub entity: String,
    /// The unique field that identifies it: `authentication_subject`.
    pub subject: String,
    /// When its record counts as active; `None` when every record does.
    pub active: Option<Condition>,
    /// The store the entity lives in, which keeps its sessions.
    pub store: String,
}

/// A sign-in method `authentication { }` declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Method {
    pub method: AuthenticationMethod,
    /// Whether this chunk builds it; one that isn't is reported where it is
    /// written and signs no one in.
    pub built: bool,
    /// `browser_session`'s cookie.
    pub cookie: Option<String>,
    /// The setting its `signing_key:` reads (area 10, "Signing keys").
    pub signing_key: Option<String>,
    /// The setting a browser session's `origin:` reads.
    pub origin: Option<String>,
}

/// `sign_in: { passkey: }` (area 8, "Passkeys"), read cleanly and with
/// the browser session and origin it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Passkey {
    /// How long a ceremony's challenge lasts, in milliseconds.
    pub expires_after_ms: i64,
    /// Where the generated routes go: `path:`'s segments.
    pub path: Vec<String>,
}

/// Area 8's declarations, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    /// Each declared variant that read cleanly.
    pub variants: Vec<Variant>,
    /// Whether `principal { }` is written at all.
    pub principal_declared: bool,
    /// The kinds of caller it writes, read cleanly or not.
    pub written: Vec<PrincipalKind>,
    /// Each declared method, in written order.
    pub methods: Vec<Method>,
    /// `application { revocation_delay: }` in milliseconds, when it is a
    /// duration above zero.
    pub revocation_delay: Option<i64>,
    /// `sign_in: { passkey: }`, when it reads cleanly and a browser session
    /// with its `origin:` signs the person in.
    pub passkey: Option<Passkey>,
    pub problems: Vec<Diagnostic>,
}

impl Identity {
    pub fn variant(&self, kind: PrincipalKind) -> Option<&Variant> {
        self.variants.iter().find(|variant| variant.kind == kind)
    }

    /// Whether any built method signs a caller in: without one, a route
    /// that needs a signed-in caller can never be called.
    pub fn signs_in(&self) -> bool {
        self.methods.iter().any(|method| method.built)
    }

    pub fn method(&self, method: AuthenticationMethod) -> Option<&Method> {
        self.methods.iter().find(|found| found.method == method)
    }

    /// The type of field `field` of a caller of kind `kind`: its `id`, its
    /// `subject` or its `strength` (area 8, question 2); `None` for any
    /// other name, and `Error` when the variant didn't read cleanly.
    pub fn field_type(&self, kind: PrincipalKind, field: &str) -> Option<Ty> {
        if !PRINCIPAL_FIELDS.contains(&field) {
            return None;
        }
        let Some(variant) = self.variant(kind) else {
            return Some(Ty::Error);
        };
        Some(match field {
            "id" => Ty::Field(variant.entity.clone(), vec!["id".to_string()]),
            "subject" => Ty::Field(variant.entity.clone(), vec![variant.subject.clone()]),
            _ => Ty::Named(jadpo_std::AuthenticationStrength::NAME.to_string()),
        })
    }

    /// The kinds of caller code can select, for messages: "`user`".
    pub fn kinds_line(&self) -> String {
        let kinds: Vec<String> = self
            .written
            .iter()
            .map(|kind| format!("`{}`", kind.name()))
            .collect();
        if kinds.is_empty() {
            "no kind of caller".to_string()
        } else {
            crate::and_list(&kinds)
        }
    }

    /// The browser session's cookie, when `browser_session` is declared.
    pub fn cookie(&self) -> Option<&str> {
        self.method(AuthenticationMethod::BrowserSession)
            .and_then(|method| method.cookie.as_deref())
    }

    /// Whether `method` is declared and built.
    pub fn built(&self, method: AuthenticationMethod) -> bool {
        self.method(method).is_some_and(|found| found.built)
    }

    /// The store sessions and API tokens are kept in, when anything signs
    /// a user in: the user entity's.
    pub fn sessions_store(&self) -> Option<&str> {
        (self.built(AuthenticationMethod::BrowserSession)
            || self.built(AuthenticationMethod::ApiToken))
        .then(|| self.variant(PrincipalKind::User))
        .flatten()
        .map(|variant| variant.store.as_str())
    }

    /// The store service keys are kept in, when service keys are declared:
    /// the service entity's.
    pub fn keys_store(&self) -> Option<&str> {
        self.built(AuthenticationMethod::ServiceKey)
            .then(|| self.variant(PrincipalKind::Service))
            .flatten()
            .map(|variant| variant.store.as_str())
    }

    /// Whether the runtime answers `POST /auth/exchange` (area 8, "Ways to
    /// sign in"): with a revocation delay and service keys, a service
    /// trades its key for a short-lived signed token there.
    pub fn exchanges(&self) -> bool {
        self.revocation_delay.is_some() && self.keys_store().is_some()
    }

    /// The passkeys the runtime answers for (area 8, "Passkeys"), kept
    /// beside the sessions they start.
    pub fn passkeys(&self) -> Option<&Passkey> {
        self.passkey
            .as_ref()
            .filter(|_| self.sessions_store().is_some())
    }
}

/// The project's identity declarations, read with their problems.
#[salsa::tracked(returns(ref))]
pub fn identity(db: &dyn Db, project: Project) -> Arc<Identity> {
    let mut reader = Reader {
        db,
        project,
        out: Identity::default(),
    };
    reader.read();
    Arc::new(reader.out)
}

/// The problems in the project's identity declarations.
pub fn identity_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &identity(db, project).problems
}

/// One singleton as written: its file, its keyword's span and its block.
struct Found {
    path: String,
    keyword: Span,
    list: Option<ast::MemberList>,
}

struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    out: Identity,
}

impl Reader<'_> {
    fn singletons(&self, keyword: &str) -> Vec<Found> {
        let mut found = Vec::new();
        for &file in self.project.files(self.db) {
            let items = item_tree(self.db, file);
            let indexes: Vec<u32> = items
                .items
                .iter()
                .filter(|item| item.kind == ItemKind::Singleton && item.name == keyword)
                .map(|item| item.index)
                .collect();
            if indexes.is_empty() {
                continue;
            }
            let Some(source) = tree::source(self.db, file) else {
                continue;
            };
            let path = file.path(self.db).clone();
            for index in indexes {
                let Some(ast::Decl::SingletonDecl(decl)) = tree::decl_at(&source, index) else {
                    continue;
                };
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
                found.push(Found {
                    path: path.clone(),
                    keyword,
                    list: decl.member_list(),
                });
            }
        }
        found
    }

    /// The first of a singleton, reporting every other.
    fn once(&mut self, keyword: &str) -> Option<Found> {
        let mut all = self.singletons(keyword).into_iter();
        let first = all.next()?;
        let (rule, what) = match keyword {
            "application" => (&rule::SEMANTIC_MULTIPLE_APPLICATIONS, "application"),
            "principal" => (&rule::SEMANTIC_MULTIPLE_PRINCIPALS, "principal"),
            _ => (&rule::SEMANTIC_MULTIPLE_AUTHENTICATION, "authentication"),
        };
        for other in all {
            let reason = match keyword {
                "application" => {
                    "A project is one application, declared once in `app.jadpo`; its settings all go in that one block."
                }
                "principal" => {
                    "The caller is declared once, in `app.jadpo`: one `user:` and at most one `service:`, so every route reads the same caller."
                }
                _ => {
                    "The ways a caller signs in are listed once, in `authentication.jadpo`, so one place says how anyone gets in."
                }
            };
            self.out.problems.push(
                Diagnostic::new(rule, format!("`{what} {{ }}` is declared twice"))
                    .at(other.keyword.clone(), "declared again")
                    .also(first.keyword.clone(), "first declared here")
                    .reason(reason)
                    .next(format!(
                        "Move what this `{what}` holds into the first, and remove it"
                    )),
            );
        }
        Some(first)
    }

    fn read(&mut self) {
        if let Some(application) = self.once("application") {
            self.application(&application);
        }
        if let Some(principal) = self.once("principal") {
            self.out.principal_declared = true;
            self.principal(&principal);
        }
        if let Some(authentication) = self.once("authentication") {
            self.authentication(&authentication);
        }
    }

    // --- application -------------------------------------------------------------

    fn application(&mut self, found: &Found) {
        for (key, member) in tree::members(found.list.clone()) {
            if tree::name_text(&key).as_deref() != Some("revocation_delay") {
                continue;
            }
            let value = member.value();
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            let span = value.as_ref().map_or_else(
                || node_span(&found.path, key.syntax()),
                |v| node_span(&found.path, v.syntax()),
            );
            let millis = jadpo_std::Dur::parse(&written).map(|dur| dur.millis);
            match millis {
                Some(millis) if millis > 0 => {
                    self.out.revocation_delay = Some(i64::try_from(millis).unwrap_or(i64::MAX));
                }
                _ => self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_REVOCATION_DELAY_INVALID,
                        format!("`{written}` can't be a revocation delay"),
                    )
                    .at(span, if millis.is_some() { "not above zero" } else { "not a duration" })
                    .reason("`revocation_delay:` is how long a disabled caller or a revoked session may keep working, a duration above zero such as `5m`; leaving it out checks the caller's record on every request.")
                    .next("Write a duration such as `5m`, or leave `revocation_delay:` out"),
                ),
            }
        }
    }

    // --- principal ---------------------------------------------------------------

    fn principal(&mut self, found: &Found) {
        let path = found.path.clone();
        let mut seen: Vec<(PrincipalKind, Span)> = Vec::new();
        for item in found.list.iter().flat_map(|list| list.member_items()) {
            let (word, span, member) = match &item {
                MemberItem::Member(member) => match member.member_key() {
                    Some(key) => (
                        tree::written(key.syntax()),
                        node_span(&path, key.syntax()),
                        Some(member.clone()),
                    ),
                    None => continue,
                },
                MemberItem::Elision(_) => continue,
                other => (
                    tree::written(other.syntax())
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    node_span(&path, other.syntax()),
                    None,
                ),
            };
            let Some(kind) = PrincipalKind::from_name(&word) else {
                let near = nearest(&word, ["user", "service"]);
                let problem = Diagnostic::new(
                    &rule::SEMANTIC_PRINCIPAL_VARIANT_UNKNOWN,
                    format!("`{word}` isn't a kind of caller"),
                )
                .at(span.clone(), "not `user` or `service`")
                .reason(format!(
                    "A principal declares `user:`, a person, and `service:`, another system; a role or a tenant is a field of the user's entity, which policy reads (area 9).{}",
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                ));
                self.out.problems.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{word}`"))
                        .edit(span, near),
                    None => problem.next(format!("Remove `{word}`")),
                });
                continue;
            };
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == kind) {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_PRINCIPAL_VARIANT_DUPLICATE,
                        format!("`{word}:` is declared twice"),
                    )
                    .at(span, "declared again")
                    .also(first.clone(), "first declared here")
                    .reason(format!(
                        "A principal says once which entity a {} is, so a caller is looked up one way.",
                        if kind == PrincipalKind::User { "user" } else { "service" }
                    ))
                    .next(format!("Remove one `{word}:`")),
                );
                continue;
            }
            seen.push((kind, span.clone()));
            self.out.written.push(kind);
            let Some(member) = member else { continue };
            if let Some(variant) = self.variant(&path, kind, &span, &member) {
                self.out.variants.push(variant);
            }
        }
        if seen.is_empty() {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_PRINCIPAL_VARIANT_REQUIRED,
                    "The principal declares no kind of caller",
                )
                .at(found.keyword.clone(), "no `user:` or `service:`")
                .reason("A principal says which entity a signed-in caller is: `user:` for a person, `service:` for another system, or both.")
                .next("Add `user: { subject: <Entity>.<field> }`"),
            );
        }
    }

    fn variant(
        &mut self,
        path: &str,
        kind: PrincipalKind,
        span: &Span,
        member: &ast::Member,
    ) -> Option<Variant> {
        let word = kind.name();
        let invalid = |message: String, at: Span, label: &str, reason: &str, next: &str| {
            Diagnostic::new(&rule::SEMANTIC_PRINCIPAL_VARIANT_INVALID, message)
                .at(at, label.to_string())
                .reason(reason.to_string())
                .next(next.to_string())
        };
        let shape = "A principal's variant names the unique field that identifies a caller, `subject: <Entity>.<field>`, and may say when a record counts as active, `active: <condition>`.";
        let Some(list) = member.block() else {
            self.out.problems.push(invalid(
                format!("`{word}:` isn't a block"),
                span.clone(),
                "a block goes here",
                shape,
                &format!("Write `{word}: {{ subject: <Entity>.<field> }}`"),
            ));
            return None;
        };
        let mut subject = None;
        let mut active = None;
        let mut ok = true;
        for (key, setting) in tree::members(Some(list)) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            match name.as_str() {
                "subject" => subject = Some((key_span, setting)),
                "active" => active = Some((key_span, setting)),
                other => {
                    ok = false;
                    let near = nearest(other, ["subject", "active"]);
                    let problem = invalid(
                        format!("`{other}` isn't something `{word}:` holds"),
                        key_span.clone(),
                        "unknown setting",
                        &format!(
                            "{shape} The caller's other details are read from its entity.{}",
                            near.map_or_else(String::new, |near| format!(
                                " Did you mean `{near}`?"
                            ))
                        ),
                        &match near {
                            Some(near) => format!("Use `{near}` instead of `{other}`"),
                            None => format!("Remove `{other}`"),
                        },
                    );
                    self.out.problems.push(match near {
                        Some(near) => problem.edit(key_span, near),
                        None => problem,
                    });
                }
            }
        }
        let Some((subject_span, subject)) = subject else {
            self.out.problems.push(invalid(
                format!("`{word}:` has no `subject:`"),
                span.clone(),
                "no `subject:`",
                shape,
                "Add `subject: <Entity>.<field>`, naming a unique field",
            ));
            return None;
        };
        let (entity, field, store) = self.subject(path, kind, &subject_span, &subject)?;
        let active = match active {
            Some((_, setting)) => {
                let lookup = Lookup::new(self.db, self.project);
                let value = setting.value()?;
                let mut condition = ActiveReader {
                    lookup: &lookup,
                    path,
                    entity: &entity,
                    out: &mut self.out.problems,
                };
                Some(condition.condition(&value)?)
            }
            None => None,
        };
        ok.then(|| Variant {
            kind,
            entity: entity.name.clone(),
            subject: field,
            active,
            store,
        })
    }

    /// `subject: Entity.field`: a unique field of an entity kept in a
    /// PostgreSQL store.
    fn subject(
        &mut self,
        path: &str,
        kind: PrincipalKind,
        key: &Span,
        member: &ast::Member,
    ) -> Option<(EntityModel, String, String)> {
        let value = member.value();
        let span = value
            .as_ref()
            .map_or_else(|| key.clone(), |v| node_span(path, v.syntax()));
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let names: Option<Vec<String>> = value.as_ref().and_then(dotted).map(|(first, rest)| {
            std::iter::once(first)
                .chain(rest)
                .filter_map(|name| tree::name_ref_text(&name))
                .collect()
        });
        let invalid = |message: String, label: &str, reason: String| {
            Diagnostic::new(&rule::TYPE_PRINCIPAL_SUBJECT_INVALID, message)
                .at(span.clone(), label.to_string())
                .reason(reason)
        };
        let (entity_name, field_name) = match names.as_deref() {
            Some([entity, field]) => (entity.clone(), field.clone()),
            _ => {
                self.out.problems.push(
                    invalid(
                        format!("`{written}` isn't an entity's field"),
                        "not `<Entity>.<field>`",
                        format!(
                            "A {} is looked up by a unique field of the entity it is stored as, written `<Entity>.<field>`, such as `User.email`.",
                            kind.name()
                        ),
                    )
                    .next("Name a unique field of the entity, such as `User.email`"),
                );
                return None;
            }
        };
        let found = crate::data::entity_model(
            self.db,
            self.project,
            crate::lookups::Name::new(self.db, entity_name.clone()),
        )
        .clone();
        let Some(entity) = found.map(|model| (*model).clone()) else {
            // A type that names nothing is resolution's one error.
            let lookup = Lookup::new(self.db, self.project);
            if lookup.def(&entity_name).is_none() {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_UNKNOWN_NAME,
                        format!("`{entity_name}` isn't declared"),
                    )
                    .at(span.clone(), "not declared")
                    .reason(format!(
                        "`subject:` names a field of the entity a {} is stored as.",
                        kind.name()
                    ))
                    .next(format!(
                        "Declare `entity {entity_name}`, or name the entity that is"
                    )),
                );
                return None;
            }
            self.out.problems.push(
                invalid(
                    format!("`{entity_name}` isn't an entity"),
                    "not stored",
                    format!(
                        "Authentication reads the caller's record from the database on every request, so a {} is an entity, a record kept in a store.",
                        kind.name()
                    ),
                )
                .next("Name a field of an entity"),
            );
            return None;
        };
        let databases = crate::data::connections(self.db, self.project).databases();
        let Some(store) = entity
            .store
            .clone()
            .filter(|store| databases.contains(store))
        else {
            if entity.store.is_some() {
                self.out.problems.push(
                    invalid(
                        format!("`{entity_name}` isn't kept in a PostgreSQL store"),
                        "not in PostgreSQL",
                        format!(
                            "Authentication reads a {}'s record and its sessions in one statement, which a PostgreSQL store can run; sessions in another kind of store come later.",
                            kind.name()
                        ),
                    )
                    .next(format!("Keep `{entity_name}` in a PostgreSQL store")),
                );
            }
            return None;
        };
        let Some(field) = entity.fields.iter().find(|f| f.name == field_name) else {
            let names: Vec<String> = entity
                .fields
                .iter()
                .map(|f| format!("`{}`", f.name))
                .collect();
            let near = nearest(&field_name, entity.fields.iter().map(|f| f.name.as_str()));
            self.out.problems.push(
                Diagnostic::new(
                    &rule::TYPE_UNKNOWN_FIELD,
                    format!("`{entity_name}` has no field `{field_name}`"),
                )
                .at(span.clone(), "not declared")
                .reason(format!(
                    "`{entity_name}`'s fields are {}.{}",
                    crate::and_list(&names),
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                ))
                .next(match near {
                    Some(near) => format!("Use `{entity_name}.{near}`"),
                    None => format!("Name a field of `{entity_name}`"),
                }),
            );
            return None;
        };
        if !field.unique || field.nullable {
            let why = if field.nullable {
                format!("`{entity_name}.{field_name}` may be `none`")
            } else {
                format!("`{entity_name}.{field_name}` isn't `unique: true`")
            };
            self.out.problems.push(
                Diagnostic::new(
                    &rule::TYPE_PRINCIPAL_SUBJECT_NOT_UNIQUE,
                    format!("`{entity_name}.{field_name}` doesn't identify one {}", kind.name()),
                )
                .at(span.clone(), "not unique")
                .reason(format!(
                    "A caller is looked up by its subject, so the subject names exactly one record: {why}."
                ))
                .next(format!(
                    "Make `{field_name}` a required field with `unique: true`, or name one that is"
                )),
            );
            return None;
        }
        let field = field.name.clone();
        Some((entity, field, store))
    }

    // --- authentication ----------------------------------------------------------

    fn authentication(&mut self, found: &Found) {
        let path = found.path.clone();
        let names: Vec<&str> = AuthenticationMethod::NAMES
            .iter()
            .copied()
            .chain(LATER_BLOCKS.iter().map(|(name, _)| *name))
            .chain(["sign_in"])
            .collect();
        // `passkey:`'s settings and where it is written, once read.
        let mut passkey: Option<(Passkey, Span)> = None;
        let mut seen: Vec<(String, Span)> = Vec::new();
        // The built methods with no `signing_key:`, which a revocation
        // delay needs.
        let mut unsigned: Vec<(AuthenticationMethod, Span)> = Vec::new();
        for item in found.list.iter().flat_map(|list| list.member_items()) {
            let (word, span, member) = match &item {
                MemberItem::Member(member) => match member.member_key() {
                    Some(key) => (
                        tree::written(key.syntax()),
                        node_span(&path, key.syntax()),
                        Some(member.clone()),
                    ),
                    None => continue,
                },
                MemberItem::Elision(_) => continue,
                other => (
                    tree::written(other.syntax())
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string(),
                    node_span(&path, other.syntax()),
                    None,
                ),
            };
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == word) {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_METHOD_DUPLICATE,
                        format!("`{word}:` is declared twice"),
                    )
                    .at(span, "declared again")
                    .also(first.clone(), "first declared here")
                    .reason("Each way to sign in is one block, written once, so its settings are in one place.")
                    .next(format!("Move the settings into the first `{word}:`, and remove this one")),
                );
                continue;
            }
            seen.push((word.clone(), span.clone()));
            if word == "sign_in" {
                if let Some(member) = &member {
                    passkey = self.sign_in(&path, &span, member);
                }
                continue;
            }
            if let Some((_, why)) = LATER_BLOCKS.iter().find(|(name, _)| *name == word) {
                self.later(span, &format!("`{word}:`"), why);
                continue;
            }
            let Some(method) = AuthenticationMethod::from_name(&word) else {
                let renamed = (word == "oidc").then_some("external_token");
                let near = renamed.or_else(|| nearest(&word, names.iter().copied()));
                let listed: Vec<String> = AuthenticationMethod::NAMES
                    .iter()
                    .map(|name| format!("`{name}`"))
                    .collect();
                let mut reason = format!(
                    "A caller signs in with one of the language's methods: {}. Each decides where its credential travels and how it is checked.",
                    crate::and_list(&listed)
                );
                if renamed.is_some() {
                    reason = format!(
                        "`oidc:` is now `external_token:`, so it isn't confused with signing in through a provider. {reason}"
                    );
                } else if let Some(near) = near {
                    reason.push_str(&format!(" Did you mean `{near}`?"));
                }
                let problem = Diagnostic::new(
                    &rule::SEMANTIC_AUTH_METHOD_UNKNOWN,
                    format!("`{word}` isn't a way to sign in"),
                )
                .at(span.clone(), "not a sign-in method")
                .reason(reason);
                self.out.problems.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{word}`"))
                        .edit(span, near),
                    None => problem.next(format!("Remove `{word}:`")),
                });
                continue;
            };
            let read = match method {
                AuthenticationMethod::BrowserSession => member
                    .as_ref()
                    .and_then(|member| self.browser_session(&path, &span, member)),
                AuthenticationMethod::ApiToken | AuthenticationMethod::ServiceKey => member
                    .as_ref()
                    .and_then(|member| self.bearer(&path, &span, method, member))
                    .map(|signing_key| Method {
                        method,
                        built: true,
                        cookie: None,
                        signing_key,
                        origin: None,
                    }),
                other => {
                    let why = match other {
                        AuthenticationMethod::ExternalToken => {
                            "Tokens from another provider need its issuer and audience from configuration (area 10), and come after the first build"
                        }
                        _ => {
                            "Acting as an OAuth provider comes after the first build (area 8, question 13)"
                        }
                    };
                    self.later(span.clone(), &format!("`{word}:`"), why);
                    Some(Method {
                        method,
                        built: false,
                        cookie: None,
                        signing_key: None,
                        origin: None,
                    })
                }
            };
            let Some(read) = read else { continue };
            if read.built && read.signing_key.is_none() {
                unsigned.push((method, span.clone()));
            }
            let kind = if method == AuthenticationMethod::ServiceKey {
                PrincipalKind::Service
            } else {
                PrincipalKind::User
            };
            if read.built && !self.out.written.contains(&kind) {
                let declared = self.out.principal_declared;
                let who = kind.name();
                let what = match method {
                    AuthenticationMethod::BrowserSession => "A browser session",
                    AuthenticationMethod::ApiToken => "An API token",
                    _ => "A service key",
                };
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_PRINCIPAL_REQUIRED,
                        format!("`{word}:` signs in {who}s, but no `{who}:` is declared"),
                    )
                    .at(span.clone(), "nobody to sign in")
                    .reason(format!(
                        "{what} signs in a {who}, looked up through the principal's `{who}:` in `app.jadpo`, which names the entity and the unique field that identify one."
                    ))
                    .next(if declared {
                        format!("Add `{who}: {{ subject: <Entity>.<field> }}` to `principal {{ }}`")
                    } else {
                        format!(
                            "Declare `principal {{ {who}: {{ subject: <Entity>.<field> }} }}` in `app.jadpo`"
                        )
                    }),
                );
            }
            self.out.methods.push(read);
        }
        // A passkey is bound to the browser session's origin.
        if let Some((read, span)) = passkey {
            let session = self.out.method(AuthenticationMethod::BrowserSession);
            match session.map(|session| session.origin.is_some()) {
                None => self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        "`passkey:` needs a browser session",
                    )
                    .at(span, "no `browser_session:`")
                    .reason("A passkey signs a person in to a browser session, at the origin the session's pages are served from, which every answer from a device is checked against.")
                    .next("Declare `browser_session: { cookie: \"<name>\"  origin: config.<name> }`"),
                ),
                Some(false) => self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        "`passkey:` needs the browser session's `origin:`",
                    )
                    .at(span, "no origin to bind to")
                    .reason("A passkey works only on the site it was made for: every answer from a device is checked against the origin `browser_session:`'s `origin:` reads from configuration (area 10), never one the request names.")
                    .next("Add `origin: config.<name>` to `browser_session:`, reading a `Url` setting"),
                ),
                Some(true) => self.out.passkey = Some(read),
            }
        }
        // With a revocation delay every method signs what it gives, with a
        // key from secret configuration.
        if self.out.revocation_delay.is_some() {
            for (method, span) in unsigned {
                let word = method.name();
                let what = match method {
                    AuthenticationMethod::BrowserSession => "a browser session",
                    AuthenticationMethod::ApiToken => "an API token",
                    _ => "a token exchanged for a service key",
                };
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        format!("`{word}:` has no `signing_key:`"),
                    )
                    .at(span, "no `signing_key:`")
                    .reason(format!(
                        "With `revocation_delay:`, {what} is signed, so a request signs in without reading the database, with a key from secret configuration (area 10)."
                    ))
                    .next("Add `signing_key: config.<name>`, reading a `SigningKey` setting"),
                );
            }
        }
    }

    /// A member that reads a setting of type `needs`: `config.<name>`,
    /// whose name it gives. A literal, anything else, or a setting of
    /// another type is `semantic.auth_setting_invalid`; a name no setting
    /// has is reported where it is read (area 10), and still given, so the
    /// member doesn't also count as missing.
    fn setting(
        &mut self,
        path: &str,
        key: &Span,
        member: &str,
        value: Option<&Expr>,
        needs: jadpo_std::Builtin,
    ) -> Option<String> {
        let at = value.map_or_else(|| key.clone(), |v| node_span(path, v.syntax()));
        let signing = needs == jadpo_std::Builtin::SigningKey;
        let declare = format!(
            "Declare `<name>: {}` in `config.jadpo`, and write `{member}: config.<name>`",
            needs.name()
        );
        let read = value
            .and_then(dotted)
            .filter(|(first, rest)| {
                tree::name_ref_text(first).as_deref() == Some("config") && rest.len() == 1
            })
            .and_then(|(_, rest)| rest.first().and_then(tree::name_ref_text));
        let Some(name) = read else {
            let literal = matches!(value, Some(Expr::Literal(_)));
            let (message, label, reason) = match (signing, literal) {
                (true, true) => (
                    "A signing key is written in the source".to_string(),
                    "a secret in source",
                    "A signing key comes only from secret configuration (area 10), so it never lands in the repository, a log or a review.",
                ),
                (false, true) => (
                    "A browser session's origin is written in the source".to_string(),
                    "written in source",
                    "A browser session's origin is the address its pages are served from, which differs from one deployment to the next, so it is a `Url` setting (area 10).",
                ),
                (true, false) => (
                    "`signing_key:` doesn't read a setting".to_string(),
                    "not `config.<name>`",
                    "A signing key comes only from secret configuration (area 10): a `SigningKey` setting, read as `config.<name>`.",
                ),
                (false, false) => (
                    "`origin:` doesn't read a setting".to_string(),
                    "not `config.<name>`",
                    "A browser session's origin is a `Url` setting (area 10), read as `config.<name>`.",
                ),
            };
            self.out.problems.push(
                Diagnostic::new(&rule::SEMANTIC_AUTH_SETTING_INVALID, message)
                    .at(at, label)
                    .reason(reason)
                    .next(declare),
            );
            return None;
        };
        let config = crate::config::config(self.db, self.project);
        if let Some(found) = config.setting(&name) {
            let lookup = Lookup::new(self.db, self.project);
            if lookup.base_prim(&found.ty) != Some(needs) {
                let reason = if signing {
                    "A signing key is a `SigningKey` setting: secret, at least 32 characters, and rotates by moving the old value to the variable with `_PREVIOUS` after its name (area 10)."
                } else {
                    "A browser session's origin is a `Url` setting: the scheme, host and port its pages are served from, such as `https://app.example`, which a cookie write's `Origin` must be."
                };
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_INVALID,
                        format!(
                            "`{member}:` reads `{name}`, which isn't a `{}`",
                            needs.name()
                        ),
                    )
                    .at(at, format!("`{name}` is `{}`", found.ty))
                    .reason(reason)
                    .next(declare),
                );
                return None;
            }
        }
        Some(name)
    }

    /// `api_token: {}` or `service_key: {}`: a bearer credential, whose
    /// only setting, `signing_key:`, signs its tokens with a revocation
    /// delay. `None` when it doesn't read cleanly; otherwise the setting
    /// its `signing_key:` reads, if it has one.
    fn bearer(
        &mut self,
        path: &str,
        span: &Span,
        method: AuthenticationMethod,
        member: &ast::Member,
    ) -> Option<Option<String>> {
        let word = method.name();
        let what = if method == AuthenticationMethod::ApiToken {
            "An API token"
        } else {
            "A service key"
        };
        let Some(list) = member.block() else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    format!("`{word}:` isn't a block"),
                )
                .at(span.clone(), "a block goes here")
                .reason(format!(
                    "{what} is declared as a block of its settings, empty when it takes none."
                ))
                .next(format!("Write `{word}: {{}}`")),
            );
            return None;
        };
        let mut ok = true;
        let mut signing_key = None;
        for (key, setting) in tree::members(Some(list)) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            let value = setting.value();
            match name.as_str() {
                "signing_key" => {
                    signing_key = self.setting(
                        path,
                        &key_span,
                        "signing_key",
                        value.as_ref(),
                        jadpo_std::Builtin::SigningKey,
                    );
                    ok &= signing_key.is_some();
                }
                other => {
                    ok = false;
                    let near = nearest(other, ["signing_key"]);
                    let problem = Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_UNKNOWN,
                        format!(
                            "`{other}` isn't {} `{word}` setting",
                            if method == AuthenticationMethod::ApiToken { "an" } else { "a" }
                        ),
                    )
                    .at(key_span.clone(), "unknown setting")
                    .reason(format!(
                        "{what} takes only `signing_key:`; the language sets its format, its lifetime and where it travels.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                    self.out.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        ok.then_some(signing_key)
    }

    /// `browser_session: { cookie: ... }`: its cookie's name, and the
    /// settings its `signing_key:` and `origin:` read.
    fn browser_session(&mut self, path: &str, span: &Span, member: &ast::Member) -> Option<Method> {
        let Some(list) = member.block() else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    "`browser_session:` has no settings",
                )
                .at(span.clone(), "a block goes here")
                .reason("A browser session travels in a cookie, which `cookie:` names.")
                .next("Write `browser_session: { cookie: \"<name>\" }`"),
            );
            return None;
        };
        let mut cookie = None;
        let mut signing_key = None;
        let mut origin = None;
        let mut ok = true;
        for (key, setting) in tree::members(Some(list)) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            let value = setting.value();
            let value_span = value
                .as_ref()
                .map_or_else(|| key_span.clone(), |v| node_span(path, v.syntax()));
            match name.as_str() {
                "cookie" => match value.as_ref().map(Const::from_expr) {
                    Some(Const::Str(text)) if cookie_name(&text) => cookie = Some(text),
                    _ => {
                        ok = false;
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                "`cookie:` isn't a cookie's name",
                            )
                            .at(value_span, "not a cookie name")
                            .reason("A browser session's cookie is named in text: 1 to 64 letters, digits, `_` and `-`, starting with a letter, such as `\"session\"`.")
                            .next("Write the cookie's name in quotes, such as `cookie: \"session\"`"),
                        );
                    }
                },
                "signing_key" => {
                    signing_key = self.setting(
                        path,
                        &key_span,
                        "signing_key",
                        value.as_ref(),
                        jadpo_std::Builtin::SigningKey,
                    );
                    ok &= signing_key.is_some();
                }
                "origin" => {
                    origin = self.setting(
                        path,
                        &key_span,
                        "origin",
                        value.as_ref(),
                        jadpo_std::Builtin::Url,
                    );
                    ok &= origin.is_some();
                }
                other => {
                    ok = false;
                    let near = nearest(other, BROWSER_SESSION.iter().copied());
                    let problem = Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_UNKNOWN,
                        format!("`{other}` isn't a `browser_session` setting"),
                    )
                    .at(key_span.clone(), "unknown setting")
                    .reason(format!(
                        "A browser session takes `cookie:`, its cookie's name, and `origin:`; the language sets the cookie's flags, its lifetime and the CSRF check.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                    self.out.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        if cookie.is_none() && ok {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    "`browser_session:` doesn't name its cookie",
                )
                .at(span.clone(), "no `cookie:`")
                .reason("A browser session travels in a cookie the application names, so two applications on one domain never share one.")
                .next("Add `cookie: \"<name>\"`, such as `cookie: \"session\"`"),
            );
            return None;
        }
        Some(Method {
            method: AuthenticationMethod::BrowserSession,
            built: true,
            cookie: Some(cookie.filter(|_| ok)?),
            signing_key,
            origin,
        })
    }

    /// `sign_in: { ... }`: the ways the language checks a person. Only
    /// `passkey:` is built; it is given with where it is written when it
    /// reads cleanly.
    fn sign_in(&mut self, path: &str, span: &Span, member: &ast::Member) -> Option<(Passkey, Span)> {
        let Some(list) = member.block() else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    "`sign_in:` isn't a block",
                )
                .at(span.clone(), "a block goes here")
                .reason("`sign_in:` lists the ways the language checks a person, each a block of its own settings.")
                .next("Write `sign_in: { passkey: {} }`"),
            );
            return None;
        };
        let mut seen: Vec<(String, Span)> = Vec::new();
        let mut passkey = None;
        for (key, setting) in tree::members(Some(list)) {
            let Some(word) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == word) {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_METHOD_DUPLICATE,
                        format!("`{word}:` is declared twice"),
                    )
                    .at(key_span, "declared again")
                    .also(first.clone(), "first declared here")
                    .reason("Each way to check a person is one block, written once, so its settings are in one place.")
                    .next(format!("Move the settings into the first `{word}:`, and remove this one")),
                );
                continue;
            }
            seen.push((word.clone(), key_span.clone()));
            match word.as_str() {
                "passkey" => {
                    passkey = self
                        .passkey(path, &key_span, &setting)
                        .map(|read| (read, key_span));
                }
                "magic_link" => self.later(
                    key_span,
                    "`magic_link:`",
                    "Magic links come with chunk 7.3b, once the application can send email",
                ),
                "providers" => self.later(
                    key_span,
                    "`providers:`",
                    "Signing in through a provider such as Google comes after magic links (area 8, question 11)",
                ),
                other => {
                    let near = nearest(other, SIGN_IN_WAYS.iter().copied());
                    let problem = Diagnostic::new(
                        &rule::SEMANTIC_AUTH_METHOD_UNKNOWN,
                        format!("`{other}` isn't a way to check a person"),
                    )
                    .at(key_span.clone(), "not a way to sign in")
                    .reason(format!(
                        "`sign_in:` lists the ways the language checks a person: `magic_link`, `passkey` and `providers`; each ends by starting a session.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                    self.out.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        passkey
    }

    /// `passkey: { ... }`: how long a challenge lasts and where the routes
    /// go. `None` when it doesn't read cleanly.
    fn passkey(&mut self, path: &str, span: &Span, member: &ast::Member) -> Option<Passkey> {
        let Some(list) = member.block() else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    "`passkey:` isn't a block",
                )
                .at(span.clone(), "a block goes here")
                .reason("Passkeys are declared as a block of their settings, empty when the defaults suit.")
                .next("Write `passkey: {}`"),
            );
            return None;
        };
        let mut read = Passkey {
            expires_after_ms: jadpo_std::http::PASSKEY_EXPIRES_MS,
            path: jadpo_std::http::PASSKEY_PATH
                .iter()
                .map(|text| text.to_string())
                .collect(),
        };
        let mut ok = true;
        for (key, setting) in tree::members(Some(list)) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            let value = setting.value();
            let value_span = value
                .as_ref()
                .map_or_else(|| key_span.clone(), |v| node_span(path, v.syntax()));
            match name.as_str() {
                "expires_after" => {
                    let written = value
                        .as_ref()
                        .map(|v| tree::written(v.syntax()))
                        .unwrap_or_default();
                    let millis = jadpo_std::Dur::parse(&written)
                        .and_then(|dur| i64::try_from(dur.millis).ok());
                    match millis {
                        Some(millis)
                            if (jadpo_std::http::PASSKEY_EXPIRES_MIN_MS
                                ..=jadpo_std::http::PASSKEY_EXPIRES_MAX_MS)
                                .contains(&millis) =>
                        {
                            read.expires_after_ms = millis;
                        }
                        _ => {
                            ok = false;
                            self.out.problems.push(
                                Diagnostic::new(
                                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                    format!("`{written}` can't be how long a passkey's challenge lasts"),
                                )
                                .at(value_span, if millis.is_some() { "not 1m to 10m" } else { "not a duration" })
                                .reason("`expires_after:` is how long a device has to answer a passkey's challenge, from `1m` to `10m`: long enough for the browser's own prompt, short enough that a challenge seen by someone else is soon of no use.")
                                .next("Write a duration from `1m` to `10m`, such as `5m`"),
                            );
                        }
                    }
                }
                "path" => match value.as_ref().map(Const::from_expr) {
                    Some(Const::Str(text)) if fixed_path(&text).is_some() => {
                        read.path = fixed_path(&text).unwrap_or_default();
                    }
                    _ => {
                        ok = false;
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                "`path:` isn't a path of fixed segments",
                            )
                            .at(value_span, "not a fixed path")
                            .reason("`path:` moves the generated passkey routes: `/` and one or more segments of lower-case letters, digits, `-` and `_`, in quotes, such as `\"/account/passkeys\"`.")
                            .next("Write a path such as `path: \"/account/passkeys\"`"),
                        );
                    }
                },
                other => {
                    ok = false;
                    let near = nearest(other, PASSKEY.iter().copied());
                    let problem = Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_UNKNOWN,
                        format!("`{other}` isn't a `passkey` setting"),
                    )
                    .at(key_span.clone(), "unknown setting")
                    .reason(format!(
                        "Passkeys take `expires_after:`, how long a challenge lasts, and `path:`, where the routes go; the language sets the ceremonies, the checks and what is kept.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                    self.out.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        ok.then_some(read)
    }

    fn later(&mut self, span: Span, what: &str, why: &str) {
        self.out.problems.push(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("{what} isn't built yet"),
            )
            .at(span, "decided, not built")
            .reason(format!("{why}."))
            .next(format!("Leave {what} out for now")),
        );
    }
}

/// The segments of a path written as `/` and one or more segments of
/// lower-case letters, digits, `-` and `_`: where generated routes go.
fn fixed_path(text: &str) -> Option<Vec<String>> {
    let rest = text.strip_prefix('/')?;
    let segments: Vec<String> = rest.split('/').map(str::to_string).collect();
    segments
        .iter()
        .all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        })
        .then_some(segments)
}

/// Whether `text` can name the session's cookie: a letter, then letters,
/// digits, `_` and `-`, 64 at most, so `<name>_csrf` is one too.
fn cookie_name(text: &str) -> bool {
    text.len() <= 64
        && text.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// --- `active:` -----------------------------------------------------------------

/// `active:` on a principal's variant: comparisons of the entity's own
/// fields, named through the entity as a query names them
/// (`User.status == UserStatus.active`), with literals, enum values,
/// `none` and `clock.now`, joined with `and` and `or`. It becomes part of
/// the statement that reads a session, so it is never anything a statement
/// can't say.
struct ActiveReader<'a, 'o> {
    lookup: &'a dyn TypeView,
    path: &'a str,
    entity: &'a EntityModel,
    out: &'o mut Vec<Diagnostic>,
}

impl ActiveReader<'_, '_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn invalid(&mut self, expr: &Expr, label: &str, reason: String, next: String) {
        let written = tree::written(expr.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_PRINCIPAL_ACTIVE_INVALID,
                format!("`{written}` isn't a condition `active:` can use"),
            )
            .at(self.span(expr), label.to_string())
            .reason(reason)
            .next(next),
        );
    }

    fn condition(&mut self, expr: &Expr) -> Option<Condition> {
        let expr = unparen(expr);
        let entity = self.entity.name.clone();
        match &expr {
            Expr::BinExpr(binary) => {
                let op = binary.op()?.text().to_string();
                let lhs = binary.lhs()?;
                let rhs = binary.rhs()?;
                if op == "and" || op == "or" {
                    let left = self.condition(&lhs);
                    let right = self.condition(&rhs);
                    let mut parts = Vec::new();
                    for part in [left?, right?] {
                        match (op.as_str(), part) {
                            ("and", Condition::And(inner)) | ("or", Condition::Or(inner)) => {
                                parts.extend(inner)
                            }
                            (_, other) => parts.push(other),
                        }
                    }
                    return Some(if op == "and" {
                        Condition::And(parts)
                    } else {
                        Condition::Or(parts)
                    });
                }
                let Some(compare) = compare_of(&op) else {
                    self.invalid(
                        &expr,
                        "not a comparison",
                        format!("`active:` compares `{entity}`'s fields with `==`, `!=`, `<`, `<=`, `>` or `>=`, joined with `and` and `or`."),
                        "Compare a field with `==`, `!=`, `<`, `<=`, `>` or `>=`".to_string(),
                    );
                    return None;
                };
                let field = self.field(&lhs, &expr)?;
                let field_ty = field_ty(self.entity, &field);
                if matches!(Const::from_expr(&rhs), Const::None)
                    && matches!(compare, Compare::Equal | Compare::NotEqual)
                {
                    if !field.nullable {
                        self.invalid(
                            &expr,
                            "never `none`",
                            format!(
                                "`{entity}.{}` is `{field_ty}`, which is never `none`.",
                                field.name
                            ),
                            "Remove the comparison, since it is always the same".to_string(),
                        );
                        return None;
                    }
                    return Some(Condition::IsNone {
                        field: field.name.clone(),
                        negated: compare == Compare::NotEqual,
                    });
                }
                if compare.is_ordering() && !ordered(self.lookup, &field_ty) {
                    self.invalid(
                        &expr,
                        "not ordered",
                        format!(
                            "`{op}` orders numbers, times, dates and durations, and `{entity}.{}` is `{}`.",
                            field.name,
                            field.ty.strip_nullable()
                        ),
                        format!("Compare `{}` with `==` or `!=`", field.name),
                    );
                    return None;
                }
                let value = self.value(&rhs, &field_ty)?;
                Some(Condition::Compare {
                    field: field.name.clone(),
                    op: compare,
                    value,
                })
            }
            _ => {
                self.invalid(
                    &expr,
                    "not a comparison",
                    format!(
                        "`active:` is a condition on `{entity}`'s own fields, such as `{entity}.<field> == <value>`; a field alone says nothing to compare it with."
                    ),
                    format!("Compare a field of `{entity}` with a value"),
                );
                None
            }
        }
    }

    /// The field a comparison tests: `Entity.field` on its left.
    fn field(&mut self, lhs: &Expr, whole: &Expr) -> Option<FieldModel> {
        let entity = self.entity.name.clone();
        let lhs = unparen(lhs);
        let named = dotted(&lhs).and_then(|(first, rest)| {
            let head = tree::name_ref_text(&first)?;
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            Some((head, rest))
        });
        match named {
            Some((head, rest)) if head == entity && rest.len() == 1 => {
                let name = &rest[0];
                if let Some(found) = self.entity.fields.iter().find(|f| &f.name == name) {
                    return Some(found.clone());
                }
                let names: Vec<String> = self
                    .entity
                    .fields
                    .iter()
                    .map(|f| format!("`{}`", f.name))
                    .collect();
                let near = nearest(name, self.entity.fields.iter().map(|f| f.name.as_str()));
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{entity}` has no field `{name}`"),
                    )
                    .at(self.span(&lhs), "not declared")
                    .reason(format!(
                        "`{entity}`'s fields are {}.{}",
                        crate::and_list(&names),
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ))
                    .next(match near {
                        Some(near) => format!("Use `{entity}.{near}`"),
                        None => format!("Name a field of `{entity}`"),
                    }),
                );
                None
            }
            Some((head, rest)) if rest.is_empty() && !starts_upper(&head) => {
                self.invalid(
                    whole,
                    "a field named bare",
                    format!("`active:` names `{entity}`'s fields through the entity, as a query does: `{entity}.{head}`."),
                    format!("Write `{entity}.{head}`"),
                );
                None
            }
            _ => {
                self.invalid(
                    whole,
                    "not one of the record's fields",
                    format!(
                        "`active:` tests `{entity}`'s own fields, on the left of each comparison, never another record or who is asking, so it joins the statement that reads a session."
                    ),
                    format!("Compare a field of `{entity}`, written `{entity}.<field>`"),
                );
                None
            }
        }
    }

    /// What a field is compared with: a literal, an enum value or
    /// `clock.now`.
    fn value(&mut self, expr: &Expr, expected: &Ty) -> Option<Value> {
        let expr = unparen(expr);
        let constant = Const::from_expr(&expr);
        if matches!(expr, Expr::Literal(_) | Expr::PrefixExpr(_))
            && !matches!(constant, Const::Other | Const::Path(_))
        {
            if let Err(problem) =
                crate::types::literal::check(self.lookup, expected.strip_nullable(), &constant)
            {
                self.out.push(crate::types::literal::diagnostic(
                    self.lookup,
                    problem,
                    &tree::written(expr.syntax()),
                    expected.strip_nullable(),
                    self.span(&expr),
                    false,
                ));
                return None;
            }
            return constant.literal().map(Value::Literal);
        }
        if let Some((first, rest)) = dotted(&expr) {
            let head = tree::name_ref_text(&first).unwrap_or_default();
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            if head == "clock" && rest == ["now"] {
                if !comparable(self.lookup, &Ty::Prim(Prim::Instant), expected) {
                    self.mismatch(&expr, &Ty::Prim(Prim::Instant), expected);
                    return None;
                }
                return Some(Value::Now);
            }
            if starts_upper(&head)
                && let ([variant], Some(Def::Enum { variants })) =
                    (rest.as_slice(), self.lookup.def(&head))
            {
                if !variants.iter().any(|v| v.name == *variant) {
                    let listed: Vec<String> =
                        variants.iter().map(|v| format!("`{}`", v.name)).collect();
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_UNKNOWN_ENUM_VARIANT,
                            format!("`{head}` has no variant `{variant}`"),
                        )
                        .at(self.span(&expr), "not a variant")
                        .reason(format!(
                            "`{head}`'s variants are {}.",
                            crate::and_list(&listed)
                        ))
                        .next(format!("Use one of `{head}`'s variants")),
                    );
                    return None;
                }
                let ty = Ty::Named(head.clone());
                if !comparable(self.lookup, &ty, expected) {
                    self.mismatch(&expr, &ty, expected);
                    return None;
                }
                return Some(Value::Variant {
                    ty: head,
                    variant: variant.clone(),
                });
            }
            // A type that names nothing is resolution's one error.
            if starts_upper(&head) && self.lookup.def(&head).is_none() {
                return None;
            }
        }
        let entity = self.entity.name.clone();
        self.invalid(
            &expr,
            "not a fixed value",
            format!(
                "`active:` compares `{entity}`'s fields with fixed values: a literal, an enum value, `none` or `clock.now`, so it joins the statement that reads a session."
            ),
            "Compare with a literal, an enum value, `none` or `clock.now`".to_string(),
        );
        None
    }

    fn mismatch(&mut self, expr: &Expr, actual: &Ty, expected: &Ty) {
        let written = tree::written(expr.syntax());
        let expected = expected.strip_nullable();
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}` can't be used where `{expected}` is required"),
            )
            .at(self.span(expr), format!("a `{actual}`"))
            .reason(format!(
                "`active:` compares a field with a value of the field's type; `{written}` is `{actual}`."
            ))
            .next(format!("Use a `{expected}` value")),
        );
    }
}

fn unparen(expr: &Expr) -> Expr {
    match expr {
        Expr::ParenExpr(paren) => paren.expr().map_or_else(|| expr.clone(), |e| unparen(&e)),
        other => other.clone(),
    }
}

fn compare_of(token: &str) -> Option<Compare> {
    Some(match token {
        "==" => Compare::Equal,
        "!=" => Compare::NotEqual,
        "<" => Compare::Less,
        "<=" => Compare::LessEqual,
        ">" => Compare::Greater,
        ">=" => Compare::GreaterEqual,
        _ => return None,
    })
}

fn field_ty(entity: &EntityModel, field: &FieldModel) -> Ty {
    if field.ty.has_error() {
        return Ty::Error;
    }
    let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
    if field.nullable { ty.nullable() } else { ty }
}

#[cfg(test)]
mod tests {
    use super::cookie_name;

    #[test]
    fn a_cookie_is_named_with_letters_digits_underscores_and_hyphens() {
        assert!(cookie_name("todo_session"));
        assert!(cookie_name("a-b"));
        assert!(!cookie_name(""));
        assert!(!cookie_name("1session"));
        assert!(!cookie_name("has space"));
        assert!(!cookie_name("semi;colon"));
        assert!(!cookie_name(&"a".repeat(65)));
    }
}
