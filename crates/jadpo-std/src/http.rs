//! What the runtime's HTTP component runs (area 7; docs/compiler-runtime.md
//! §2.6): the route table, as plain data `jadpo-plan` decides from the
//! checked routes and `jadpo-runtime` runs for either executor, so the
//! interpreter and compiled code route, decode and answer a request the
//! same way.
//!
//! - A [`RoutePlan`] is one route: its method and path, where its input
//!   comes from, how its output is written, its success status, its
//!   deadline, what it runs and how each argument is made from the
//!   placeholders and the input.
//! - A [`Codec`] says how a value of one type crosses the boundary as JSON
//!   (area 2): a scalar checked by its validator, an object's fields in
//!   declared order, an enum's variants by name, collections, and `none`.
//! - [`RouteTable`] holds them all, numbered, with each declared failure's
//!   client fields' codecs and the default rate limits (area 16).

use crate::read::{Column, ReadPlan};
use crate::types::{ShapeId, ValidatorId};
use crate::{AuthenticationMethod, Builtin, ClientAddressSource, HttpMethod, SuccessStatus};

/// A route's deadline when it says none (area 7, "Time limits").
pub const DEFAULT_DEADLINE_MS: u32 = 10_000;
/// The longest deadline a route may give.
pub const MAX_DEADLINE_MS: u32 = 60_000;
/// The largest body a request may send (area 7, question 5), answered 413
/// past it.
pub const BODY_LIMIT: usize = 1024 * 1024;

/// How long a stored response answers a repeat of its request (area 7,
/// question 17): a lookup ignores a key kept longer.
pub const IDEMPOTENCY_KEEP_MS: i64 = 24 * 60 * 60 * 1000;
/// The longest `Idempotency-Key` the runtime takes, in characters.
pub const IDEMPOTENCY_KEY_MAX: usize = 255;
/// How long `GET /health/ready` waits for the stores (area 7, "Health").
pub const READY_WITHIN_MS: u32 = 2_000;
/// The paths the runtime answers itself (area 7, "Health"): whether the
/// process runs, and whether its stores answer.
pub const HEALTH_LIVE: [&str; 2] = ["health", "live"];
pub const HEALTH_READY: [&str; 2] = ["health", "ready"];

/// The largest whole number JavaScript holds exactly: an `Int` whose
/// declared range stays within it is a JSON number, any other a string
/// (area 2, question 8).
pub const SAFE_INTEGER: i128 = 9_007_199_254_740_991;

/// A codec, by number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodecId(pub u32);

/// How a value of one type is read from JSON and written as JSON.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Codec {
    /// A value of a type built on `builtin`, checked against `validator`.
    /// An `Int` is a JSON number when `int_as_number` (its declared range
    /// fits [`SAFE_INTEGER`]), a string otherwise; a `Decimal` is always a
    /// string.
    Scalar {
        builtin: Builtin,
        validator: ValidatorId,
        int_as_number: bool,
    },
    /// An object: the shape's fields in declared order, each with its codec.
    Object {
        shape: ShapeId,
        fields: Vec<CodecId>,
    },
    /// An enum: a plain variant is its name as a string; a variant with
    /// fields is an object with `tag` and its fields (area 2). A
    /// `ZonedTime` is a [`Codec::Scalar`] written as an object of its
    /// `instant` and `zone` (area 10).
    Enum {
        ty: String,
        variants: Vec<VariantCodec>,
    },
    /// A list, checked against the lengths its type adds.
    List {
        item: CodecId,
        validator: ValidatorId,
    },
    /// A set: a JSON array refused when it repeats a value (area 2,
    /// question 5).
    Set {
        item: CodecId,
        validator: ValidatorId,
    },
    /// A map: a JSON object, keyed by text or UUIDs.
    Map {
        key: CodecId,
        value: CodecId,
        validator: ValidatorId,
    },
    /// `T?`: `null`, or a `T`.
    Nullable(CodecId),
    /// A page of results (area 6): `{ "items": [...], "next": cursor }`.
    Page { item: CodecId },
    /// A standard type held as a record of its parts whose JSON is its
    /// canonical text (area 19, "Time and calendar additions"): a
    /// `LocalTime` is `"09:30:00"`, read and written by the contract's
    /// one implementation for the type called `ty`.
    Standard { ty: String },
}

/// One variant of an enum codec.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantCodec {
    pub name: String,
    /// The string a plain variant is sent as: its name, except that a
    /// `Zone` is its IANA name (`Europe/London`) and a `Locale` its BCP 47
    /// tag (`en-GB`), as area 10's table and a setting spell them.
    pub wire: String,
    /// Its place among the enum's variants, from 0.
    pub ordinal: u32,
    /// A payload variant's shape and its fields' codecs.
    pub fields: Option<(ShapeId, Vec<CodecId>)>,
}

/// One segment of a route's full path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Segment {
    Fixed(String),
    /// A typed placeholder, decoded from the segment's text.
    Param {
        name: String,
        codec: CodecId,
    },
}

/// Where a route's `input:` comes from (area 7, "One input").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InputPlan {
    /// The `Input` object's codec.
    pub codec: CodecId,
    /// Read from the query string (`GET`, `DELETE`) rather than the body.
    pub query: bool,
    /// The fields read from headers: each field's name and the header's,
    /// lower case.
    pub headers: Vec<(String, String)>,
}

/// What a route runs (area 7, "What a route runs").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    /// An action, by its callable number in the plan.
    Action(u32),
    /// A named query's read.
    Query(Box<ReadPlan>),
    /// `sessions.tokens` (or the service entity's `keys` when `keys`) run
    /// by a `GET` itself (area 8): the holder's tokens or keys that
    /// haven't ended, limited by its record's `read` grants (area 9).
    Listing { keys: bool },
    /// `POST /auth/exchange`, generated with a revocation delay and
    /// service keys (area 8, "Ways to sign in"): the service key the
    /// request signed in with, traded for a short-lived signed token.
    Exchange,
}

/// What one argument of the call is made from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArgRoot {
    /// A placeholder, by its place among the path's placeholders.
    Param(u32),
    Input,
    /// The signed-in caller, `current_principal` (area 8); a variant the
    /// route selects is the route's [`RoutePlan::caller`].
    Caller,
}

/// Whether a route needs a signed-in caller (area 8, "Routes: signed in
/// by default").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Authentication {
    /// Every route's unless it says otherwise.
    #[default]
    Required,
    /// `RouteAuthentication.none`: no credential is read.
    Public,
    /// `RouteAuthentication.fresh`: the caller's record is read now, even
    /// with a revocation delay.
    Fresh,
}

impl Authentication {
    /// Whether a caller must be signed in.
    pub fn needs_caller(self) -> bool {
        !matches!(self, Authentication::Public)
    }
}

/// The two kinds of caller (area 8, "The principal").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PrincipalKind {
    /// A person.
    User,
    /// Another system.
    Service,
}

impl PrincipalKind {
    /// As code selects it: `current_principal.user`.
    pub fn name(self) -> &'static str {
        match self {
            PrincipalKind::User => "user",
            PrincipalKind::Service => "service",
        }
    }

    pub fn from_name(name: &str) -> Option<PrincipalKind> {
        match name {
            "user" => Some(PrincipalKind::User),
            "service" => Some(PrincipalKind::Service),
            _ => None,
        }
    }

    /// The caller value's type name: `Principal.user`.
    pub fn type_name(self) -> &'static str {
        match self {
            PrincipalKind::User => "Principal.user",
            PrincipalKind::Service => "Principal.service",
        }
    }
}

/// What an action does with sessions, API tokens and service keys (area
/// 8): the operations the compiler generates in the `sessions` namespace
/// and on the principal's service entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SessionOp {
    /// `sessions.start(user_id)`.
    Start,
    /// `sessions.end()`.
    End,
    /// `sessions.issue_token(user_id)`: an `ApiToken`, shown once.
    IssueToken,
    /// `sessions.tokens(user_id)`: the user's live tokens.
    Tokens,
    /// `sessions.revoke_token(token_id)`.
    RevokeToken,
    /// `Service.issue_key(service_id, expires_at)`: a `ServiceKey`, shown
    /// once.
    IssueKey,
    /// `Service.keys(service_id)`: the service's live keys.
    Keys,
    /// `Service.revoke_key(key_id)`.
    RevokeKey,
}

impl SessionOp {
    pub const ALL: [SessionOp; 8] = [
        SessionOp::Start,
        SessionOp::End,
        SessionOp::IssueToken,
        SessionOp::Tokens,
        SessionOp::RevokeToken,
        SessionOp::IssueKey,
        SessionOp::Keys,
        SessionOp::RevokeKey,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SessionOp::Start => "start",
            SessionOp::End => "end",
            SessionOp::IssueToken => "issue_token",
            SessionOp::Tokens => "tokens",
            SessionOp::RevokeToken => "revoke_token",
            SessionOp::IssueKey => "issue_key",
            SessionOp::Keys => "keys",
            SessionOp::RevokeKey => "revoke_key",
        }
    }

    /// The operation `sessions.<name>` names.
    pub fn from_name(name: &str) -> Option<SessionOp> {
        SessionOp::ALL
            .into_iter()
            .find(|op| !op.on_service() && op.name() == name)
    }

    /// The operation `<Service>.<name>` names on the service entity.
    pub fn key_op(name: &str) -> Option<SessionOp> {
        SessionOp::ALL
            .into_iter()
            .find(|op| op.on_service() && op.name() == name)
    }

    /// Whether it is the service entity's (`Service.issue_key`) rather
    /// than the `sessions` namespace's.
    pub fn on_service(self) -> bool {
        matches!(
            self,
            SessionOp::IssueKey | SessionOp::Keys | SessionOp::RevokeKey
        )
    }

    /// Whether it only reads.
    pub fn reads_only(self) -> bool {
        matches!(self, SessionOp::Tokens | SessionOp::Keys)
    }

    /// The sign-in method it needs declared.
    pub fn method(self) -> crate::AuthenticationMethod {
        match self {
            SessionOp::Start | SessionOp::End => crate::AuthenticationMethod::BrowserSession,
            SessionOp::IssueToken | SessionOp::Tokens | SessionOp::RevokeToken => {
                crate::AuthenticationMethod::ApiToken
            }
            SessionOp::IssueKey | SessionOp::Keys | SessionOp::RevokeKey => {
                crate::AuthenticationMethod::ServiceKey
            }
        }
    }
}

/// What `sessions.tokens` lists, never the token itself: `IssuedToken`.
pub const ISSUED_TOKEN: &str = "IssuedToken";

/// What `Service.keys` lists, never the key itself: `IssuedKey`.
pub const ISSUED_KEY: &str = "IssuedKey";

/// The fields of an `IssuedToken` and an `IssuedKey`, in order.
pub const ISSUED_FIELDS: [(&str, crate::Builtin); 3] = [
    ("id", crate::Builtin::Uuid),
    ("started_at", crate::Builtin::Instant),
    ("ends_at", crate::Builtin::Instant),
];

/// The first problem `json.read`'s `mismatch` found (area 19, "Data
/// formats"): the field's path, list positions as numbers
/// (`lines.2.quantity`), and the rule (`max`), never the value. Only the
/// runtime builds one, for a failure mapped from `mismatch` that declares
/// a field [`DECODE_PROBLEM_FIELD`] of this type.
pub const DECODE_PROBLEM: &str = "DecodeProblem";

/// The name of the failure field that receives a [`DECODE_PROBLEM`].
pub const DECODE_PROBLEM_FIELD: &str = "problem";

/// The fields of a `DecodeProblem`, in order, named as area 7's input
/// problems are: `{ field, rule }`.
pub const DECODE_PROBLEM_FIELDS: [(&str, crate::Builtin); 2] = [
    ("field", crate::Builtin::Text),
    ("rule", crate::Builtin::Text),
];

/// The fields of a built-in object type the boundary carries, which the
/// runtime builds and a program only reads: `IssuedToken` and `IssuedKey`
/// (area 8), and `DecodeProblem` (area 19).
pub fn builtin_object_fields(name: &str) -> Option<&'static [(&'static str, crate::Builtin)]> {
    match name {
        ISSUED_TOKEN | ISSUED_KEY => Some(&ISSUED_FIELDS),
        DECODE_PROBLEM => Some(&DECODE_PROBLEM_FIELDS),
        _ => None,
    }
}

/// What an API token starts with, before its 64 hexadecimal digits.
pub const API_TOKEN_PREFIX: &str = "jadpo_token_";

/// What a service key starts with, before its 64 hexadecimal digits.
pub const SERVICE_KEY_PREFIX: &str = "jadpo_key_";

/// What the token `POST /auth/exchange` gives a service starts with,
/// before its signed part (area 8, chosen in chunk 5.1b).
pub const SERVICE_TOKEN_PREFIX: &str = "jadpo_service_token_";

/// The path of the generated exchange (area 8, "Ways to sign in").
pub const EXCHANGE_PATH: [&str; 2] = ["auth", "exchange"];

/// What `POST /auth/exchange` answers with: the token and when it
/// expires.
pub const EXCHANGED_TOKEN: &str = "ExchangedToken";

/// The response header a renewed API token comes back in (area 8, chosen
/// in chunk 5.1b): a client that keeps it signs in without a read until
/// it expires.
pub const TOKEN_HEADER: &str = "x-jadpo-token";

/// How long a browser session lasts from when it starts (area 8, chosen
/// in chunk 4.3): 30 days, after which the caller signs in again.
pub const SESSION_LIFETIME_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The header a browser sends its session's CSRF token back in (area 8).
pub const CSRF_HEADER: &str = "x-jadpo-csrf";

/// Who can sign in and how (area 8): the cookie a browser session travels
/// in, which bearer credentials are accepted, and how each kind of caller
/// is looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityPlan {
    /// The store `jadpo_sessions` lives in: the user entity's, so a
    /// session is read with its user's record.
    pub store: String,
    /// Whether anything signs a caller in; without, the plan only makes
    /// callers from records in tests.
    pub signs_in: bool,
    /// The browser session's cookie, when `browser_session` is declared;
    /// its CSRF token travels in the same name with `_csrf` after it.
    pub cookie: Option<String>,
    /// Whether `api_token` is declared.
    pub api_tokens: bool,
    pub user: Option<PrincipalPlan>,
    pub service: Option<PrincipalPlan>,
    /// The credential statements the owning record's grants limit (area
    /// 9, question 8), each `None` when nothing declares what it needs.
    pub gated: GatedStatements,
    /// `application { revocation_delay: }`, in milliseconds (area 8, "The
    /// application and revocation"): with one, sessions, API tokens and
    /// exchanged service tokens carry a signed part that signs them in
    /// without a read until it expires, at most this long after the
    /// record was last read. `None` checks the record on every request.
    pub revocation_delay_ms: Option<i64>,
    /// The setting each method's `signing_key:` reads (area 10, "Signing
    /// keys"), by method.
    pub signing_keys: Vec<(AuthenticationMethod, String)>,
    /// The setting a browser session's `origin:` reads: a cookie write's
    /// `Origin` must be exactly it. `None` takes the server's own, the one
    /// the `Host` header names.
    pub origin: Option<String>,
}

/// Listing, issuing and revoking credentials, limited to holders whose
/// record the caller may read (to list) or update (to issue or revoke).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GatedStatements {
    /// `sessions.tokens(user_id)`: `$1` the user, `$2` the time now.
    pub tokens: Option<crate::policy::Gated>,
    /// `sessions.revoke_token(token_id)`: `$1` the token.
    pub revoke_token: Option<crate::policy::Gated>,
    /// `Service.issue_key(...)`: `$1` the key's `id`, `$2` the service,
    /// `$3` its verifier, `$4` the time now and `$5` when it expires.
    pub issue_key: Option<crate::policy::Gated>,
    /// `Service.keys(service_id)`: `$1` the service, `$2` the time now.
    pub keys: Option<crate::policy::Gated>,
    /// `Service.revoke_key(key_id)`: `$1` the key.
    pub revoke_key: Option<crate::policy::Gated>,
}

impl IdentityPlan {
    /// The cookie the CSRF token travels in: `todo_session_csrf`.
    pub fn csrf_cookie(&self) -> Option<String> {
        self.cookie.as_ref().map(|cookie| format!("{cookie}_csrf"))
    }

    pub fn principal(&self, kind: PrincipalKind) -> Option<&PrincipalPlan> {
        match kind {
            PrincipalKind::User => self.user.as_ref(),
            PrincipalKind::Service => self.service.as_ref(),
        }
    }

    /// The setting `method`'s `signing_key:` reads.
    pub fn signing_key(&self, method: AuthenticationMethod) -> Option<&str> {
        self.signing_keys
            .iter()
            .find(|(found, _)| *found == method)
            .map(|(_, setting)| setting.as_str())
    }

    /// The store `jadpo_service_keys` lives in, when service keys are
    /// declared: the service entity's.
    pub fn keys_store(&self) -> Option<&str> {
        self.service
            .as_ref()
            .filter(|service| service.lookup.is_some())
            .map(|service| service.store.as_str())
    }
}

/// How one kind of caller is looked up (area 8, "The principal").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrincipalPlan {
    /// The entity it is: `User`.
    pub entity: String,
    /// The store it lives in, which keeps its sessions or keys.
    pub store: String,
    /// The field `subject:` names, which a caller made from a record in a
    /// test reads (`Principal.user(user)`).
    pub subject: String,
    /// How the subject's column reads back.
    pub column: Column,
    /// How a credential finds it, `$1` the credential's verifier and `$2`
    /// the time now: for a user, a session's lookup, one row of the
    /// session's id, its CSRF token's verifier (`NULL` for an API token),
    /// the record's `id` and subject, whether its `active:` holds and when
    /// the session ends; for a service, a key's, the same without the CSRF
    /// token's verifier.
    /// None when no live session or key or visible record has it. `None`
    /// when nothing signs this kind of caller in.
    pub lookup: Option<String>,
}

/// One argument: a placeholder or the input, or a field read through them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Arg {
    pub root: ArgRoot,
    pub path: Vec<String>,
}

/// One route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePlan {
    /// Its identity key: `route POST /todos`.
    pub key: String,
    pub method: HttpMethod,
    /// Its full path, its group's prefix first.
    pub segments: Vec<Segment>,
    /// The API it belongs to, by number.
    pub api: u32,
    pub input: Option<InputPlan>,
    /// The output's codec; `None` for `no_content`.
    pub output: Option<CodecId>,
    pub status: SuccessStatus,
    pub deadline_ms: u32,
    pub run: Run,
    /// The store an action keeps this route's `Idempotency-Key`s in: the
    /// one it writes, so a key commits with the writes it answers for.
    /// `None` for a `GET`, and for an action that writes no store, which
    /// a repeat can run again without changing anything.
    pub keys: Option<String>,
    pub args: Vec<Arg>,
    /// Whether it needs a signed-in caller (area 8).
    pub authentication: Authentication,
    /// The kind of caller its call selects (`current_principal.user`): a
    /// caller of the other kind is refused before it runs.
    pub caller: Option<PrincipalKind>,
    /// Where it is written, for logs and `jadpo dev`.
    pub file: String,
    pub line: u32,
}

impl RoutePlan {
    /// `/todos/{todo_id}`.
    pub fn path(&self) -> String {
        path_text(&self.segments)
    }

    /// `POST /todos`.
    pub fn label(&self) -> String {
        format!("{} {}", self.method.word(), self.path())
    }
}

/// A path as written in a route, its placeholders by name: `/todos/{todo_id}`.
pub fn path_text(segments: &[Segment]) -> String {
    if segments.is_empty() {
        return "/".to_string();
    }
    segments
        .iter()
        .map(|segment| match segment {
            Segment::Fixed(text) => format!("/{text}"),
            Segment::Param { name, .. } => format!("/{{{name}}}"),
        })
        .collect()
}

/// An API (area 7, "APIs and versions").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ApiPlan {
    pub name: String,
    /// Its prefix's segments; empty for `/`.
    pub prefix: Vec<String>,
    /// Whether its writes need an `Idempotency-Key`
    /// (`idempotency: Idempotency.required`).
    pub idempotency: crate::Idempotency,
    /// The sign-in methods it accepts (area 8): its `authentication:`, or
    /// every declared one.
    pub methods: Vec<AuthenticationMethod>,
}

/// A declared failure's client fields as they reach a client (area 4):
/// each field's name and codec, in declared order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FailureCodec {
    /// The failure's identity key: `type QuantityUnavailable`.
    pub key: String,
    pub client: Vec<(String, CodecId)>,
}

/// How many requests a caller may make in a window (area 16, "Rate limits").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RateLimit {
    pub requests: u32,
    pub window_ms: u32,
}

/// The limit on a signed-in caller when nothing says otherwise: 600 a
/// minute (area 16, question 6).
pub const CALLER_LIMIT: RateLimit = RateLimit {
    requests: 600,
    window_ms: 60_000,
};

/// The limit on an address when nothing says otherwise: 120 a minute.
pub const ADDRESS_LIMIT: RateLimit = RateLimit {
    requests: 120,
    window_ms: 60_000,
};

/// How much memory in-memory rate-limit counters may take before the
/// oldest are dropped (area 16: `memory: 16MB` by default).
pub const RATE_LIMIT_MEMORY: usize = 16 * 1024 * 1024;

/// How many proxies of the deployment's own sit in front of the app
/// (area 10, "Client addresses").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hops {
    /// Written out: a whole number from 1.
    Count(u32),
    /// Read from the `Int` setting of this name, which declares `min:` of
    /// at least 1.
    Setting(String),
}

/// Where the client's address is read: `application { client_address: }`
/// (area 10, "Client addresses"; chosen in chunk 5.1c). The runtime
/// chooses the address from it, so every host and executor counts a
/// client the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientAddressPlan {
    pub source: ClientAddressSource,
    /// Present exactly for the sources that count from the right of a
    /// header: `forwarded_for` and `forwarded`.
    pub hops: Option<Hops>,
}

/// Every route of a project, ready to run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteTable {
    pub apis: Vec<ApiPlan>,
    pub routes: Vec<RoutePlan>,
    pub codecs: Vec<Codec>,
    /// The client fields of every declared failure a route can reach.
    pub failures: Vec<FailureCodec>,
    /// The PostgreSQL stores the application's entities live in, in name
    /// order: what `GET /health/ready` checks.
    pub stores: Vec<String>,
    /// Who can sign in and how; `None` when nothing is declared.
    pub identity: Option<IdentityPlan>,
    /// Where the client's address is read; `None` takes the host's default
    /// (the connection's natively, `CF-Connecting-IP` on Workers).
    pub client_address: Option<ClientAddressPlan>,
}

impl RouteTable {
    pub fn codec(&self, id: CodecId) -> Option<&Codec> {
        self.codecs.get(id.0 as usize)
    }

    /// The client fields of failure `key`.
    pub fn failure(&self, key: &str) -> Option<&FailureCodec> {
        self.failures.iter().find(|failure| failure.key == key)
    }
}

/// The codecs `json.read` and `json.write` run with (area 19, "Data
/// formats"): the wire's codec for each type a call reads or writes,
/// numbered as [`RouteTable`]'s are, so a document in memory is read and
/// written exactly as a request and a response are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JsonTable {
    pub codecs: Vec<Codec>,
    /// Each call's type's codec, by the number its site gives it.
    pub types: Vec<CodecId>,
}

impl JsonTable {
    /// The codec of the type numbered `at`.
    pub fn codec_of(&self, at: u32) -> Option<CodecId> {
        self.types.get(at as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_print_their_placeholders_by_name() {
        let segments = vec![
            Segment::Fixed("todos".into()),
            Segment::Param {
                name: "todo_id".into(),
                codec: CodecId(0),
            },
        ];
        assert_eq!(path_text(&segments), "/todos/{todo_id}");
        assert_eq!(path_text(&[]), "/");
        assert_eq!(SuccessStatus::NoContent.code(), 204);
        assert!(HttpMethod::Delete.reads_query());
        assert_eq!(HttpMethod::from_word("PATCH"), Some(HttpMethod::Patch));
    }
}
