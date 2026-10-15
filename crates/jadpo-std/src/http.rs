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
use crate::{AuthenticationMethod, Builtin, HttpMethod, SuccessStatus};

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
    /// fields is an object with `tag` and its fields (area 2).
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
}

/// One variant of an enum codec.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantCodec {
    pub name: String,
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

/// What an action does to the request's session (area 8, "Starting and
/// ending a session").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SessionOp {
    /// `sessions.start(user_id)`.
    Start,
    /// `sessions.end()`.
    End,
}

impl SessionOp {
    pub fn name(self) -> &'static str {
        match self {
            SessionOp::Start => "start",
            SessionOp::End => "end",
        }
    }

    pub fn from_name(name: &str) -> Option<SessionOp> {
        match name {
            "start" => Some(SessionOp::Start),
            "end" => Some(SessionOp::End),
            _ => None,
        }
    }
}

/// How long a browser session lasts from when it starts (area 8, chosen
/// in chunk 4.3): 30 days, after which the caller signs in again.
pub const SESSION_LIFETIME_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The header a browser sends its session's CSRF token back in (area 8).
pub const CSRF_HEADER: &str = "x-jadpo-csrf";

/// Who can sign in and how (area 8): the store sessions are kept in, the
/// cookie a browser session travels in, and how each kind of caller is
/// looked up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityPlan {
    /// The store `jadpo_sessions` lives in: the user entity's, so a
    /// session is read with its user's record.
    pub store: String,
    /// The browser session's cookie, when `browser_session` is declared;
    /// its CSRF token travels in the same name with `_csrf` after it.
    pub cookie: Option<String>,
    pub user: Option<PrincipalPlan>,
    pub service: Option<PrincipalPlan>,
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
}

/// How one kind of caller is looked up (area 8, "The principal").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrincipalPlan {
    /// The entity it is: `User`.
    pub entity: String,
    /// The field `subject:` names, which a caller made from a record in a
    /// test reads (`Principal.user(user)`).
    pub subject: String,
    /// How the subject's column reads back.
    pub column: Column,
    /// A session's lookup, `$1` its verifier and `$2` the time now: one
    /// row of the session's id, its CSRF token's verifier, the record's
    /// `id` and subject, and whether its `active:` holds, or none when no
    /// live session or visible record has it.
    pub session: String,
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
