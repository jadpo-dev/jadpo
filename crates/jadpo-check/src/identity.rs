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
//! `sign_in:` with `passkey:`, whose routes the runtime answers; in chunk
//! 7.3b: `magic_link:` and `on_first_sign_in:`; in chunk 9.1d1:
//! `external_token:` for provisioned active subjects. Its first-sign-in
//! hook, sign-in providers and acting as an OAuth provider come later:
//! each is `toolchain.not_built_yet` where it is written.

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

/// The settings `magic_link` takes (area 8, "Magic links").
const MAGIC_LINK: [&str; 12] = [
    "address",
    "expires_after",
    "code_attempts",
    "codes_per_hour",
    "failed_codes_per_day",
    "resend_after",
    "same_browser",
    "link",
    "path",
    "generate_routes",
    "email",
    "send",
];

/// The ways `sign_in:` checks a person (area 8, "Checking the person").
const SIGN_IN_WAYS: [&str; 3] = ["magic_link", "passkey", "providers"];

/// What `authentication { }` may hold besides its methods, each built by a
/// later chunk.
const LATER_BLOCKS: [(&str, &str); 1] = [(
    "oauth_provider",
    "Acting as an OAuth provider comes after the first build (area 8, question 13)",
)];

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

/// An external identity provider: configuration names, never values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalToken {
    pub issuer: String,
    pub audience: String,
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

/// `sign_in: { magic_link: }` (area 8, "Magic links"), read cleanly and
/// with the browser session and origin it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MagicLink {
    /// The user entity's field `address:` names: an `Email`, unique.
    pub address: String,
    /// How long a link and its code last, in milliseconds.
    pub expires_after_ms: i64,
    pub code_attempts: u32,
    pub codes_per_hour: u32,
    pub failed_codes_per_day: u32,
    /// The least time between two sends to one address, in milliseconds.
    pub resend_after_ms: i64,
    pub same_browser: bool,
    /// The frontend path the email's link opens.
    pub link: String,
    /// Where the generated routes go: `path:`'s segments.
    pub path: Vec<String>,
    /// `generate_routes:`: `false` generates no routes, and the
    /// application writes its own over `magic_links.request`, `.redeem`
    /// and `.redeem_code` (chunk 7.3b3).
    pub generate_routes: bool,
    /// `email:`, the application's own sign-in email (area 16, "Built-in
    /// emails", level 4), as written.
    pub email: Option<FirstSignIn>,
    /// `send:`, the action that sends the sign-in email itself (level 5),
    /// as written.
    pub send: Option<FirstSignIn>,
    /// Where `magic_link:` is written.
    pub span: Span,
}

/// A name `authentication { }` gives as written: `on_first_sign_in:`'s
/// action, `domain.action` or a single-file project's bare action, or
/// `magic_link:`'s `email:` type and `send:` action; and where it is
/// written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstSignIn {
    pub names: Vec<String>,
    pub span: Span,
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
    pub external_token: Option<ExternalToken>,
    /// `application { revocation_delay: }` in milliseconds, when it is a
    /// duration above zero.
    pub revocation_delay: Option<i64>,
    /// `sign_in: { passkey: }`, when it reads cleanly and a browser session
    /// with its `origin:` signs the person in.
    pub passkey: Option<Passkey>,
    /// `sign_in: { magic_link: }`, when it reads cleanly and a browser
    /// session with its `origin:` signs the person in.
    pub magic_link: Option<MagicLink>,
    /// `on_first_sign_in:`, as written.
    pub first_sign_in: Option<FirstSignIn>,
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

    /// The magic links the runtime answers for (area 8, "Magic links"),
    /// kept beside the sessions they start.
    pub fn magic_links(&self) -> Option<&MagicLink> {
        self.magic_link
            .as_ref()
            .filter(|_| self.sessions_store().is_some())
    }

    /// The magic links whose routes the runtime generates: none with
    /// `generate_routes: false`, where the application writes its own.
    pub fn magic_link_routes(&self) -> Option<&MagicLink> {
        self.magic_links().filter(|magic| magic.generate_routes)
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

/// The action `on_first_sign_in:` names, by its canonical name, when it
/// is an action taking one `FirstSignIn` and returning the user entity's
/// `id` (area 8, "A person the app has not seen"); with its problem when
/// it isn't.
#[salsa::tracked(returns(ref))]
pub fn first_sign_in_action(db: &dyn Db, project: Project) -> (Option<String>, Vec<Diagnostic>) {
    use crate::lookups::{NameView, SigView};
    use jadpo_db::{ItemKind, Ownership};
    let identity = identity(db, project);
    let Some(written) = &identity.first_sign_in else {
        return (None, Vec::new());
    };
    let span = written.span.clone();
    let shown = written.names.join(".");
    let user = identity.variant(PrincipalKind::User);
    let entity = user.map_or("<Entity>", |variant| variant.entity.as_str());
    let shape = format!(
        "`on_first_sign_in:` names an action that takes one `FirstSignIn` and returns `{entity}.id`, of a user it created or one that already exists; it refuses with any failure it declares."
    );
    let lookup = Lookup::new(db, project);
    let (domain, action) = match written.names.as_slice() {
        [action] => (None, action.clone()),
        [domain, action] => (Some(domain.clone()), action.clone()),
        _ => return (None, Vec::new()),
    };
    let symbol = lookup.declared(&action).iter().find(|symbol| {
        symbol.kind == ItemKind::Action
            && match (&symbol.ownership, domain.as_ref()) {
                (Ownership::Domain(holder), Some(domain)) => holder == domain,
                (Ownership::Loose, None) => true,
                _ => false,
            }
    });
    let Some(symbol) = symbol else {
        let problem = Diagnostic::new(
            &rule::SEMANTIC_AUTH_SETTING_INVALID,
            format!("`{shown}` isn't an action"),
        )
        .at(span, "no such action")
        .reason(shape)
        .next("Name an action, written `<domain>.<action>`");
        return (None, vec![problem]);
    };
    let Some(signature) = lookup.signature(&symbol.canonical).cloned() else {
        return (None, Vec::new());
    };
    let takes = matches!(
        signature.params.as_slice(),
        [(_, Ty::Opaque(name, args))] if name == jadpo_std::http::FIRST_SIGN_IN && args.is_empty()
    );
    let gives = user.is_some_and(|variant| {
        signature.result == Ty::Field(variant.entity.clone(), vec!["id".to_string()])
    });
    if takes && gives {
        return (Some(symbol.canonical.clone()), Vec::new());
    }
    let label = if takes {
        format!("doesn't return `{entity}.id`")
    } else {
        "doesn't take one `FirstSignIn`".to_string()
    };
    let problem = Diagnostic::new(
        &rule::SEMANTIC_AUTH_SETTING_INVALID,
        format!("`{shown}` can't decide a first sign-in"),
    )
    .at(span, label)
    .reason(shape)
    .next(format!(
        "Declare it `action {action}(first: FirstSignIn) -> {entity}.id`"
    ));
    (None, vec![problem])
}

/// The symbol `written` names among `kind`'s declarations: `domain.name`
/// in a domain, or a single-file project's bare name.
fn named_symbol(
    lookup: &Lookup<'_>,
    written: &FirstSignIn,
    wanted: impl Fn(&ItemKind) -> bool,
) -> Option<jadpo_db::Symbol> {
    use crate::lookups::NameView;
    use jadpo_db::Ownership;
    let (domain, name) = match written.names.as_slice() {
        [name] => (None, name.clone()),
        [domain, name] => (Some(domain.clone()), name.clone()),
        _ => return None,
    };
    lookup
        .declared(&name)
        .iter()
        .find(|symbol| {
            wanted(&symbol.kind)
                && match (&symbol.ownership, domain.as_ref()) {
                    (Ownership::Domain(holder), Some(domain)) => holder == domain,
                    (Ownership::Loose, None) => true,
                    _ => false,
                }
        })
        .cloned()
}

/// The application's own sign-in email `magic_link: { email: }` names
/// (area 16, "Built-in emails", level 4), by its canonical name, when it
/// is an email whose fields besides `to` and `locale` are each picked from
/// `MagicLinkEmail`; with its problem when it isn't.
#[salsa::tracked(returns(ref))]
pub fn own_magic_link_email(db: &dyn Db, project: Project) -> (Option<String>, Vec<Diagnostic>) {
    let identity = identity(db, project);
    let Some(written) = identity.magic_link.as_ref().and_then(|m| m.email.as_ref()) else {
        return (None, Vec::new());
    };
    let span = written.span.clone();
    let shown = written.names.join(".");
    let shape = "`email:` names the application's own sign-in email, `type <Name> = EmailMessage { pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code, ...] }`, whose fields Jadpo fills: each is one of `MagicLinkEmail`'s, `address`, `sign_in_link`, `code` and `expires_in`, besides the `to:` and `locale:` every email has.";
    let lookup = Lookup::new(db, project);
    let symbol = named_symbol(&lookup, written, |kind| matches!(kind, ItemKind::Type(_)));
    let Some(symbol) = symbol else {
        let problem = Diagnostic::new(
            &rule::SEMANTIC_AUTH_SETTING_INVALID,
            format!("`{shown}` isn't an email"),
        )
        .at(span, "no such email")
        .reason(shape)
        .next("Name an email type, written `<domain>.<Email>`, declared in the domain's `emails/` folder");
        return (None, vec![problem]);
    };
    let fields = match lookup.def(&symbol.canonical) {
        Some(Def::Fields {
            base: crate::types::FieldsBase::EmailMessage,
            fields,
            ..
        }) => fields.clone(),
        _ => {
            let problem = Diagnostic::new(
                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                format!("`{shown}` isn't an email"),
            )
            .at(span, "not an `EmailMessage`")
            .reason(shape)
            .next(format!(
                "Declare `type {} = EmailMessage {{ pick: [MagicLinkEmail.sign_in_link, MagicLinkEmail.code] }}`",
                symbol.name
            ));
            return (None, vec![problem]);
        }
    };
    let own: Vec<String> = fields
        .iter()
        .filter(|field| {
            !crate::types::EMAIL_FIELDS
                .iter()
                .any(|(name, _)| *name == field.name)
        })
        .filter(|field| {
            !(field.copied
                && matches!(&field.declared, Ty::Field(host, _)
                    if host == jadpo_std::email::MAGIC_LINK_EMAIL_TYPE))
        })
        .map(|field| format!("`{}`", field.name))
        .collect();
    if own.is_empty() {
        return (Some(symbol.canonical.clone()), Vec::new());
    }
    let problem = Diagnostic::new(
        &rule::SEMANTIC_AUTH_SETTING_INVALID,
        format!("`{shown}` has fields Jadpo can't fill"),
    )
    .at(
        span,
        format!("{} not from `MagicLinkEmail`", crate::and_list(&own)),
    )
    .reason(shape)
    .next(format!(
        "Take {} out of `{}`, picking only from `MagicLinkEmail`",
        crate::and_list(&own),
        symbol.name
    ));
    (None, vec![problem])
}

/// The action `magic_link: { send: }` names (area 16, "Built-in emails",
/// level 5), by its canonical name, when it takes one `MagicLinkRequest`
/// and returns `Unit`; with its problem when it isn't.
#[salsa::tracked(returns(ref))]
pub fn sign_in_send_action(db: &dyn Db, project: Project) -> (Option<String>, Vec<Diagnostic>) {
    use crate::lookups::SigView;
    let identity = identity(db, project);
    let Some(written) = identity.magic_link.as_ref().and_then(|m| m.send.as_ref()) else {
        return (None, Vec::new());
    };
    let span = written.span.clone();
    let shown = written.names.join(".");
    let action = written.names.last().cloned().unwrap_or_default();
    let shape = "`send:` names the action that sends the sign-in email itself, through any connection: it takes one `MagicLinkRequest` (the address, the link, the code, when they stop working and the locale) and returns `Unit`. It runs after the request commits, as a subscriber does, and a failure is retried as a subscriber's is.";
    let lookup = Lookup::new(db, project);
    let Some(symbol) = named_symbol(&lookup, written, |kind| *kind == ItemKind::Action) else {
        let problem = Diagnostic::new(
            &rule::SEMANTIC_AUTH_SETTING_INVALID,
            format!("`{shown}` isn't an action"),
        )
        .at(span, "no such action")
        .reason(shape)
        .next("Name an action, written `<domain>.<action>`");
        return (None, vec![problem]);
    };
    let Some(signature) = lookup.signature(&symbol.canonical).cloned() else {
        return (None, Vec::new());
    };
    let takes = matches!(
        signature.params.as_slice(),
        [(_, Ty::Opaque(name, args))] if name == jadpo_std::email::MAGIC_LINK_REQUEST && args.is_empty()
    );
    let gives = signature.result == Ty::Prim(Prim::Unit);
    if takes && gives {
        return (Some(symbol.canonical.clone()), Vec::new());
    }
    let label = if takes {
        "doesn't return `Unit`"
    } else {
        "doesn't take one `MagicLinkRequest`"
    };
    let problem = Diagnostic::new(
        &rule::SEMANTIC_AUTH_SETTING_INVALID,
        format!("`{shown}` can't send the sign-in email"),
    )
    .at(span, label)
    .reason(shape)
    .next(format!(
        "Declare it `action {action}(request: MagicLinkRequest) -> Unit`"
    ));
    (None, vec![problem])
}

/// What `sign_in:` needs from the rest of the application (area 8,
/// "Magic links"; area 16, "Built-in emails"): the `email:` connection
/// the sign-in email goes through and the `display_name:` it names the
/// application by; and the action `on_first_sign_in:` names.
#[salsa::tracked(returns(ref))]
pub fn sign_in_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = first_sign_in_action(db, project).1.clone();
    out.extend(own_magic_link_email(db, project).1.iter().cloned());
    out.extend(sign_in_send_action(db, project).1.iter().cloned());
    let identity = identity(db, project);
    let Some(magic) = &identity.magic_link else {
        return out;
    };
    let settings = crate::emails::settings(db, project);
    // An application that sends the sign-in email itself needs no email
    // connection for it (area 16, level 5).
    if settings.connection.is_none() && magic.send.is_none() {
        out.push(
            Diagnostic::new(
                &rule::EMAIL_CONNECTION_REQUIRED,
                "`magic_link:` sends email, but nothing sends email",
            )
            .at(magic.span.clone(), "no email connection")
            .reason("A magic link and its code are sent by email, through the connection `application { email: }` names, from the address its `from:` gives; `app.jadpo` names none.")
            .next("Declare a connection with `driver: Driver.smtp` in `connections:`, and add `email: { connection: <its name>  from: config.mail_sender }` to `application { }` in `app.jadpo`"),
        );
    }
    if settings.display_name.is_none() {
        out.push(
            Diagnostic::new(
                &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                "`magic_link:` needs the application's `display_name:`",
            )
            .at(magic.span.clone(), "no `display_name:`")
            .reason("Jadpo's sign-in email names the application, \"Sign in to Acme\", by `application { display_name: }`, the name its readers know it by.")
            .next("Add `display_name: \"<name>\"` to `application { }` in `app.jadpo`"),
        );
    }
    out
}

/// One singleton as written: its file, its keyword's span and its block./// One singleton as written: its file, its keyword's span and its block.
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
            .chain(["sign_in", "on_first_sign_in"])
            .collect();
        // `passkey:`'s and `magic_link:`'s settings and where each is
        // written, once read.
        let mut ways = Ways::default();
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
                    ways = self.sign_in(&path, &span, member);
                }
                continue;
            }
            if word == "on_first_sign_in" {
                if let Some(member) = &member {
                    self.first_sign_in(&path, &span, member);
                }
                continue;
            }
            // `sign_in_providers:` was renamed `sign_in:` (area 8,
            // revised 2026-10-16).
            if word == "sign_in_providers" {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_METHOD_UNKNOWN,
                        "`sign_in_providers:` is now `sign_in:`",
                    )
                    .at(span.clone(), "renamed")
                    .reason("`sign_in:` lists every way the language checks a person: `magic_link:`, `passkey:` and `providers: [...]`, the providers being connections in `app.jadpo`.")
                    .next("Rename `sign_in_providers:` to `sign_in:`")
                    .edit(span, "sign_in"),
                );
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
                AuthenticationMethod::ExternalToken => member
                    .as_ref()
                    .and_then(|member| self.external_token(&path, &span, member)),
                _ => {
                    let why = "Acting as an OAuth provider comes after the first build (area 8, question 13)";
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
            if read.built
                && method != AuthenticationMethod::ExternalToken
                && read.signing_key.is_none()
            {
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
                    AuthenticationMethod::ExternalToken => "An external token",
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
        let magic_link_written = ways.magic_link_written;
        // A magic link starts a browser session at its origin.
        if let Some((read, span)) = ways.magic_link {
            let session = self.out.method(AuthenticationMethod::BrowserSession);
            match session.map(|session| session.origin.is_some()) {
                None => self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        "`magic_link:` needs a browser session",
                    )
                    .at(span, "no `browser_session:`")
                    .reason("A magic link signs a person in to a browser session, at the origin the application's pages are served from, which the email's link opens.")
                    .next("Declare `browser_session: { cookie: \"<name>\"  origin: config.<name> }`"),
                ),
                Some(false) => self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        "`magic_link:` needs the browser session's `origin:`",
                    )
                    .at(span, "no origin to link to")
                    .reason("The email's link opens the frontend's `link:` path at the origin `browser_session:`'s `origin:` reads from configuration (area 10), never one the request names.")
                    .next("Add `origin: config.<name>` to `browser_session:`, reading a `Url` setting"),
                ),
                Some(true) => self.out.magic_link = Some(read),
            }
        }
        // A passkey is bound to the browser session's origin.
        if let Some((read, span)) = ways.passkey {
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
                // A person with no passkey left gets back in with a
                // magic link (chunk 5.5z2).
                Some(true) if !magic_link_written => self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        "`passkey:` needs `magic_link:` beside it",
                    )
                    .at(span, "no way back in")
                    .reason("A person who loses every device holding a passkey signs in with a magic link or its code and adds a new one; with no `magic_link:` in the same `sign_in:`, nothing gets them back in.")
                    .next("Add `magic_link: { link: \"<path>\" }` to `sign_in:`, beside `passkey:`"),
                ),
                Some(true) => self.out.passkey = Some(read),
            }
        }
        if self.out.external_token.is_some()
            && let Some(hook) = &self.out.first_sign_in
        {
            self.later(hook.span.clone(), "`on_first_sign_in:` with `external_token:`",
                "Creating or linking an external subject through the first-sign-in hook is not built yet; external tokens currently require a provisioned user");
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

    /// External tokens use deployment-owned issuer and audience settings.
    fn external_token(&mut self, path: &str, span: &Span, member: &ast::Member) -> Option<Method> {
        let mut issuer = None;
        let mut audience = None;
        let mut seen = Vec::new();
        let mut ok = true;
        if let Some(list) = member.block() {
            for (key, setting) in tree::members(Some(list)) {
                let Some(name) = tree::name_text(&key) else {
                    continue;
                };
                let at = node_span(path, key.syntax());
                if seen.contains(&name) {
                    ok = false;
                    self.out.problems.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_AUTH_SETTING_INVALID,
                            format!("`external_token:` gives `{name}:` twice"),
                        )
                        .at(at, "repeated setting")
                        .reason("An external token has one issuer and one audience.")
                        .next(format!("Remove the repeated `{name}:`")),
                    );
                    continue;
                }
                seen.push(name.clone());
                let needs = match name.as_str() {
                    "issuer" => jadpo_std::Builtin::Url,
                    "audience" => jadpo_std::Builtin::Text,
                    _ => {
                        ok = false;
                        self.out.problems.push(Diagnostic::new(
                            &rule::SEMANTIC_AUTH_SETTING_UNKNOWN,
                            format!("`{name}` isn't an `external_token` setting"),
                        ).at(at, "unknown setting")
                         .reason("External tokens take `issuer:` and `audience:` from configuration. The runtime owns discovery, keys and validation.")
                         .next(format!("Remove `{name}:`")));
                        continue;
                    }
                };
                let value = setting.value();
                let read = self.setting(path, &at, &name, value.as_ref(), needs);
                ok &= read.is_some();
                match name.as_str() {
                    "issuer" => issuer = read,
                    _ => audience = read,
                }
            }
        }
        for name in ["issuer", "audience"] {
            if !seen.iter().any(|seen| seen == name) {
                ok = false;
                self.out.problems.push(Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    format!("`external_token:` has no `{name}:`"),
                ).at(span.clone(), format!("no `{name}:`"))
                 .reason("An external token is checked against its configured issuer and audience before its subject identifies an active user.")
                 .next(format!("Add `{name}: config.<name>`, reading a `{}` setting", if name == "issuer" { "Url" } else { "Text" })));
            }
        }
        if !ok {
            return None;
        }
        let unsupported_subject = self
            .out
            .variants
            .iter()
            .find(|variant| variant.kind == PrincipalKind::User)
            .and_then(|variant| {
                crate::data::entity_model(
                    self.db,
                    self.project,
                    crate::lookups::Name::new(self.db, variant.entity.clone()),
                )
                .as_ref()
                .and_then(|entity| {
                    entity
                        .fields
                        .iter()
                        .find(|field| field.name == variant.subject)
                })
                .map(|field| {
                    field.compound.is_some()
                        || field.json.is_some()
                        || crate::data::sql_type(field).is_none()
                })
            })
            .unwrap_or(false);
        if unsupported_subject {
            self.later(span.clone(), "A compound external-token subject",
                "External-token subjects currently use a unique field with a scalar stored form; multi-column and structured subject lookups are not built yet");
            return None;
        }
        self.out.external_token = Some(ExternalToken {
            issuer: issuer?,
            audience: audience?,
        });
        Some(Method {
            method: AuthenticationMethod::ExternalToken,
            built: true,
            cookie: None,
            signing_key: None,
            origin: None,
        })
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
            let external = member == "issuer" || member == "audience";
            if external {
                self.out.problems.push(Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    format!("`{member}:` doesn't read a setting"),
                ).at(at, "not `config.<name>`")
                 .reason("An external token's issuer and audience come from deployment configuration, never source or the token itself.")
                 .next(declare));
                return None;
            }
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
                let reason = if member == "issuer" || member == "audience" {
                    "An external token reads its issuer from a `Url` setting and its audience from a `Text` setting."
                } else if signing {
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

    /// `sign_in: { ... }`: the ways the language checks a person.
    /// `passkey:` and `magic_link:` are built; each is given with where it
    /// is written when it reads cleanly.
    fn sign_in(&mut self, path: &str, span: &Span, member: &ast::Member) -> Ways {
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
            return Ways::default();
        };
        let mut seen: Vec<(String, Span)> = Vec::new();
        let mut ways = Ways::default();
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
                    ways.passkey = self
                        .passkey(path, &key_span, &setting)
                        .map(|read| (read, key_span));
                }
                "magic_link" => {
                    ways.magic_link_written = true;
                    ways.magic_link = self
                        .magic_link(path, &key_span, &setting)
                        .map(|read| (read, key_span));
                }
                "providers" => self.later(
                    key_span,
                    "`providers:`",
                    "Signing in through a provider such as Google comes after the first build (area 8, question 11)",
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
        ways
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

    /// `on_first_sign_in: <domain>.<action>`: kept as written; whether it
    /// names an action of the right shape is checked once every
    /// signature is known ([`sign_in_problems`]).
    fn first_sign_in(&mut self, path: &str, span: &Span, member: &ast::Member) {
        let value = member.value();
        let names: Option<Vec<String>> = value.as_ref().and_then(dotted).map(|(first, rest)| {
            std::iter::once(first)
                .chain(rest)
                .filter_map(|name| tree::name_ref_text(&name))
                .collect()
        });
        let at = value
            .as_ref()
            .map_or_else(|| span.clone(), |v| node_span(path, v.syntax()));
        match names.filter(|names| (1..=2).contains(&names.len())) {
            Some(names) => self.out.first_sign_in = Some(FirstSignIn { names, span: at }),
            None => self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    "`on_first_sign_in:` doesn't name an action",
                )
                .at(at, "not `<domain>.<action>`")
                .reason("`on_first_sign_in:` names the action that decides what a first sign-in does: it takes one `FirstSignIn` and returns the user entity's `id`, of a user it created or one that already exists.")
                .next("Write `on_first_sign_in: <domain>.<action>`"),
            ),
        }
    }

    /// `magic_link: { ... }`: its settings, each checked. `None` when it
    /// doesn't read cleanly.
    fn magic_link(&mut self, path: &str, span: &Span, member: &ast::Member) -> Option<MagicLink> {
        use jadpo_std::http as h;
        let Some(list) = member.block() else {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    "`magic_link:` isn't a block",
                )
                .at(span.clone(), "a block goes here")
                .reason("Magic links are declared as a block of their settings, holding at least `link:`, the frontend path the email's link opens.")
                .next("Write `magic_link: { link: \"/sign-in/confirm\" }`"),
            );
            return None;
        };
        let mut read = MagicLink {
            address: String::new(),
            expires_after_ms: h::MAGIC_LINK_EXPIRES_MS,
            code_attempts: h::MAGIC_LINK_CODE_ATTEMPTS,
            codes_per_hour: h::MAGIC_LINK_CODES_PER_HOUR,
            failed_codes_per_day: h::MAGIC_LINK_FAILED_CODES_PER_DAY,
            resend_after_ms: h::MAGIC_LINK_RESEND_AFTER_MS,
            same_browser: false,
            link: String::new(),
            path: h::MAGIC_LINK_PATH.iter().map(|t| t.to_string()).collect(),
            generate_routes: true,
            email: None,
            send: None,
            span: span.clone(),
        };
        let mut ok = true;
        let mut address: Option<(Vec<String>, Span)> = None;
        // Where `path:` and `generate_routes: false` are written, which
        // say opposite things together.
        let mut moved: Option<Span> = None;
        let mut switched_off: Option<Span> = None;
        let mut level: Vec<(String, Span)> = Vec::new();
        for (key, setting) in tree::members(Some(list)) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(path, key.syntax());
            let value = setting.value();
            let value_span = value
                .as_ref()
                .map_or_else(|| key_span.clone(), |v| node_span(path, v.syntax()));
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            let constant = value.as_ref().map(Const::from_expr);
            let whole = match &constant {
                Some(Const::Number {
                    value,
                    decimal: false,
                }) => u32::try_from(value.mantissa).ok(),
                _ => None,
            };
            let millis =
                jadpo_std::Dur::parse(&written).and_then(|dur| i64::try_from(dur.millis).ok());
            match name.as_str() {
                "address" => {
                    let names: Option<Vec<String>> =
                        value.as_ref().and_then(dotted).map(|(first, rest)| {
                            std::iter::once(first)
                                .chain(rest)
                                .filter_map(|name| tree::name_ref_text(&name))
                                .collect()
                        });
                    match names.filter(|names| names.len() == 2) {
                        Some(names) => address = Some((names, value_span)),
                        None => {
                            ok = false;
                            self.out.problems.push(
                                Diagnostic::new(
                                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                    format!("`{written}` isn't a field of the user entity"),
                                )
                                .at(value_span, "not `<Entity>.<field>`")
                                .reason("`address:` names the user entity's field holding the address a link is sent to: built on `Email` and `unique: true`, written `<Entity>.<field>`, such as `Member.email`.")
                                .next("Name the user entity's address field, such as `address: Member.email`"),
                            );
                        }
                    }
                }
                "expires_after" => match millis {
                    Some(millis) if (60_000..=h::MAGIC_LINK_EXPIRES_MAX_MS).contains(&millis) => {
                        read.expires_after_ms = millis;
                    }
                    Some(millis) if millis > h::MAGIC_LINK_EXPIRES_MAX_MS => {
                        ok = false;
                        self.acceptance(value_span, "`expires_after:`", &written, "`1h`");
                    }
                    _ => {
                        ok = false;
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                format!("`{written}` can't be how long a magic link lasts"),
                            )
                            .at(value_span, if millis.is_some() { "under 1m" } else { "not a duration" })
                            .reason("`expires_after:` is how long a link and its code work, a duration of at least `1m`; `15m` unless it says otherwise.")
                            .next("Write a duration such as `15m`"),
                        );
                    }
                },
                "resend_after" => match millis {
                    Some(millis) if millis > 0 => read.resend_after_ms = millis,
                    _ => {
                        ok = false;
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                format!("`{written}` can't be how long to wait between sends"),
                            )
                            .at(value_span, if millis.is_some() { "not above zero" } else { "not a duration" })
                            .reason("`resend_after:` is the least time between two emails to one address, a duration above zero; `1m` unless it says otherwise.")
                            .next("Write a duration such as `1m`"),
                        );
                    }
                },
                "code_attempts" | "codes_per_hour" | "failed_codes_per_day" => {
                    let most = match name.as_str() {
                        "code_attempts" => Some(h::MAGIC_LINK_CODE_ATTEMPTS_MAX),
                        "failed_codes_per_day" => Some(h::MAGIC_LINK_FAILED_CODES_PER_DAY_MAX),
                        _ => None,
                    };
                    match whole {
                        Some(count) if count >= 1 && most.is_none_or(|most| count <= most) => {
                            match name.as_str() {
                                "code_attempts" => read.code_attempts = count,
                                "codes_per_hour" => read.codes_per_hour = count,
                                _ => read.failed_codes_per_day = count,
                            }
                        }
                        Some(count) if count >= 1 => {
                            ok = false;
                            self.acceptance(
                                value_span,
                                &format!("`{name}:`"),
                                &written,
                                &format!("`{}`", most.unwrap_or_default()),
                            );
                        }
                        _ => {
                            ok = false;
                            let what = match name.as_str() {
                                "code_attempts" => "how many wrong guesses end a code",
                                "codes_per_hour" => {
                                    "how many emails one address is sent in an hour"
                                }
                                _ => "how many wrong codes one address may send in a day",
                            };
                            self.out.problems.push(
                                Diagnostic::new(
                                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                    format!("`{name}:` must be a whole number from 1"),
                                )
                                .at(value_span, "not a count")
                                .reason(format!("`{name}:` is {what}, a whole number from 1."))
                                .next(format!("Write a count, such as `{name}: 5`")),
                            );
                        }
                    }
                }
                "generate_routes" => match constant {
                    Some(Const::Bool(on)) => {
                        read.generate_routes = on;
                        if !on {
                            switched_off = Some(key_span.clone());
                        }
                    }
                    _ => {
                        ok = false;
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                "`generate_routes:` must be `true` or `false`",
                            )
                            .at(value_span, "not `true` or `false`")
                            .reason("`generate_routes: false` generates none of the magic-link routes, and the application writes its own over `magic_links.request`, `.redeem` and `.redeem_code`; they are generated unless it says otherwise.")
                            .next("Write `generate_routes: false`, or leave it out"),
                        );
                    }
                },
                "same_browser" => match constant {
                    Some(Const::Bool(on)) => read.same_browser = on,
                    _ => {
                        ok = false;
                        self.out.problems.push(
                            Diagnostic::new(
                                &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                "`same_browser:` must be `true` or `false`",
                            )
                            .at(value_span, "not `true` or `false`")
                            .reason("`same_browser: true` makes a link work only in the browser that asked for it; the code still works anywhere.")
                            .next("Write `same_browser: true` or `same_browser: false`"),
                        );
                    }
                },
                "link" | "path" => {
                    let text = match &constant {
                        Some(Const::Str(text)) => Some(text.clone()),
                        _ => None,
                    };
                    if name == "link" {
                        match text.filter(|text| site_path(text)) {
                            Some(text) => read.link = text,
                            None => {
                                ok = false;
                                self.out.problems.push(
                                    Diagnostic::new(
                                        &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                        "`link:` isn't a path on the application's site",
                                    )
                                    .at(value_span, "not a path")
                                    .reason("`link:` is the frontend's page the email's link opens, at the browser session's origin: a path starting with one `/`, with no query or fragment, such as `\"/sign-in/confirm\"`. The page posts the link's token to the confirm route.")
                                    .next("Write the page's path, such as `link: \"/sign-in/confirm\"`"),
                                );
                            }
                        }
                    } else {
                        match text.as_deref().and_then(fixed_path) {
                            Some(segments) => {
                                read.path = segments;
                                moved = Some(key_span.clone());
                            }
                            None => {
                                ok = false;
                                self.out.problems.push(
                                    Diagnostic::new(
                                        &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                        "`path:` isn't a path of fixed segments",
                                    )
                                    .at(value_span, "not a fixed path")
                                    .reason("`path:` moves the generated magic-link routes: `/` and one or more segments of lower-case letters, digits, `-` and `_`, in quotes, such as `\"/account/sign-in\"`.")
                                    .next("Write a path such as `path: \"/account/sign-in\"`"),
                                );
                            }
                        }
                    }
                }
                "email" | "send" => {
                    let names: Option<Vec<String>> =
                        value.as_ref().and_then(dotted).map(|(first, rest)| {
                            std::iter::once(first)
                                .chain(rest)
                                .filter_map(|name| tree::name_ref_text(&name))
                                .collect()
                        });
                    match names.filter(|names| (1..=2).contains(&names.len())) {
                        Some(names) => {
                            let written = FirstSignIn {
                                names,
                                span: value_span,
                            };
                            if name == "email" {
                                read.email = Some(written);
                            } else {
                                read.send = Some(written);
                            }
                        }
                        None => {
                            ok = false;
                            let (what, form) = if name == "email" {
                                ("an email type", "<domain>.<Email>")
                            } else {
                                ("an action", "<domain>.<action>")
                            };
                            self.out.problems.push(
                                Diagnostic::new(
                                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                                    format!("`{name}:` doesn't name {what}"),
                                )
                                .at(value_span, format!("not `{form}`"))
                                .reason(if name == "email" {
                                    "`email:` names the application's own sign-in email, a `type <Name> = EmailMessage { pick: [...] }` taking its fields from `MagicLinkEmail`, which Jadpo fills and sends in place of its own.".to_string()
                                } else {
                                    "`send:` names the action that sends the sign-in email itself: it takes one `MagicLinkRequest` and returns `Unit`.".to_string()
                                })
                                .next(format!("Write `{name}: {form}`")),
                            );
                        }
                    }
                    level.push((name.clone(), key_span));
                }
                other => {
                    ok = false;
                    let near = nearest(other, MAGIC_LINK.iter().copied());
                    let problem = Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_UNKNOWN,
                        format!("`{other}` isn't a `magic_link` setting"),
                    )
                    .at(key_span.clone(), "unknown setting")
                    .reason(format!(
                        "Magic links take `address:`, `link:`, `expires_after:`, `code_attempts:`, `codes_per_hour:`, `failed_codes_per_day:`, `resend_after:`, `same_browser:`, `path:`, `generate_routes:`, `email:` and `send:`; the language sets the tokens, the codes and how they are kept.{}",
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
        // `path:` moves routes `generate_routes: false` switches off.
        if let (Some(moved), Some(off)) = (moved, switched_off) {
            ok = false;
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    "`magic_link:` has both `path:` and `generate_routes: false`",
                )
                .at(moved, "moves the routes")
                .also(off, "switches them off")
                .reason("`path:` says where the generated routes go, and `generate_routes: false` generates none, so the application's own routes take their paths from its `routes` blocks.")
                .next("Remove `path:`, or `generate_routes: false`"),
            );
        }
        // Levels 4 and 5 of the built-in email (area 16) are exclusive.
        if let [(_, first), (_, second), ..] = level.as_slice() {
            ok = false;
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    "`magic_link:` has both `email:` and `send:`",
                )
                .at(second.clone(), "both")
                .also(first.clone(), "first here")
                .reason("`email:` names an email of the application's that Jadpo fills and sends; `send:` names an action that sends it some other way. One email is sent one way.")
                .next("Keep `email:` or `send:`, not both"),
            );
        }
        if read.link.is_empty() && ok {
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                    "`magic_link:` doesn't say which page the link opens",
                )
                .at(span.clone(), "no `link:`")
                .reason("Jadpo generates endpoints, never pages: the email's link opens the frontend's own page at `link:`, which posts the link's token to the confirm route.")
                .next("Add `link: \"/sign-in/confirm\"`, the path of the page that confirms"),
            );
            return None;
        }
        read.address = self.address_field(address, span)?;
        ok.then_some(read)
    }

    /// The user entity's field `address:` names, or its `email` field when
    /// it names none: built on `Email` and `unique: true`.
    fn address_field(
        &mut self,
        written: Option<(Vec<String>, Span)>,
        span: &Span,
    ) -> Option<String> {
        let variant = self.out.variant(PrincipalKind::User)?.clone();
        let entity = crate::data::entity_model(
            self.db,
            self.project,
            crate::lookups::Name::new(self.db, variant.entity.clone()),
        )
        .clone()
        .map(|model| (*model).clone())?;
        let (field_name, at) = match written {
            Some((names, at)) => {
                if names[0] != variant.entity {
                    self.out.problems.push(
                        Diagnostic::new(
                            &rule::SEMANTIC_AUTH_SETTING_INVALID,
                            format!("`{}` isn't the user entity", names[0]),
                        )
                        .at(at, "not the user entity")
                        .reason(format!(
                            "A link is sent to the address of the person signing in, a field of `{}`, the entity the principal's `user:` names.",
                            variant.entity
                        ))
                        .next(format!("Name a field of `{}`", variant.entity)),
                    );
                    return None;
                }
                (names[1].clone(), at)
            }
            None => ("email".to_string(), span.clone()),
        };
        let Some(field) = entity.fields.iter().find(|f| f.name == field_name) else {
            if at == *span {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::SEMANTIC_AUTH_SETTING_REQUIRED,
                        "`magic_link:` doesn't say where the address is kept",
                    )
                    .at(span.clone(), "no `address:`")
                    .reason(format!(
                        "A link is sent to the user's address, in `{}`'s `email` field unless `address:` names another; `{}` has no `email` field.",
                        variant.entity, variant.entity
                    ))
                    .next(format!(
                        "Add `address: {}.<field>`, naming its unique `Email` field",
                        variant.entity
                    )),
                );
            } else {
                self.out.problems.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{}` has no field `{field_name}`", variant.entity),
                    )
                    .at(at, "not declared")
                    .reason("`address:` names the user entity's field holding the address a link is sent to.")
                    .next(format!("Name a field of `{}`", variant.entity)),
                );
            }
            return None;
        };
        let lookup = Lookup::new(self.db, self.project);
        let email = lookup.base_prim(&field_ty(&entity, field)) == Some(jadpo_std::Builtin::Email);
        if !email || !field.unique {
            let why = if email {
                format!("`{}.{field_name}` isn't `unique: true`", variant.entity)
            } else {
                format!("`{}.{field_name}` isn't built on `Email`", variant.entity)
            };
            self.out.problems.push(
                Diagnostic::new(
                    &rule::SEMANTIC_AUTH_SETTING_INVALID,
                    format!("`{}.{field_name}` can't be where a link is sent", variant.entity),
                )
                .at(at, if email { "not unique" } else { "not an `Email`" })
                .reason(format!(
                    "A link is sent to an address, and an address names one person: `address:` names a field built on `Email` with `unique: true`; {why}."
                ))
                .next(format!(
                    "Make `{field_name}` an `Email` with `unique: true`, or name the field that is"
                )),
            );
            return None;
        }
        Some(field_name)
    }

    /// A setting loosened past what needs an acceptance (area 8, "Magic
    /// links"), which comes with acceptances (area 18).
    fn acceptance(&mut self, span: Span, setting: &str, written: &str, most: &str) {
        self.out.problems.push(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("{setting} `{written}` isn't built yet"),
            )
            .at(span, format!("past {most}"))
            .reason(format!(
                "Loosening {setting} past {most} weakens sign-in, so it needs a person's acceptance (area 8), which comes with acceptances (area 18)."
            ))
            .next(format!("Keep {setting} at {most} or under for now")),
        );
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

/// The ways `sign_in:` checks a person that read cleanly, each with
/// where it is written.
#[derive(Default)]
struct Ways {
    passkey: Option<(Passkey, Span)>,
    magic_link: Option<(MagicLink, Span)>,
    /// Whether `magic_link:` is written, read cleanly or not.
    magic_link_written: bool,
}

/// Whether `text` is a path on the application's own site, which a
/// browser can't read as another host: one `/`, then printable ASCII with
/// no `\`, query or fragment, 2,000 bytes at most.
fn site_path(text: &str) -> bool {
    text.len() <= 2000
        && text.starts_with('/')
        && !text.starts_with("//")
        && text
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'\\' | b'?' | b'#'))
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
