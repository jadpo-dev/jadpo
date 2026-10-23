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
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct CodecId(pub u32);

/// How a value of one type is read from JSON and written as JSON.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
    /// An object: the shape's fields in declared order, each with its codec,
    /// and, for a type built on a standard object that adds rules (a rate
    /// type's currencies, chunk 5.5h4), the validator the whole value is
    /// checked against once read.
    Object {
        shape: ShapeId,
        fields: Vec<CodecId>,
        validator: Option<ValidatorId>,
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
    /// A vector's item (area 19, chunk 5.5s2): a JSON number, held as the
    /// decimal that is the nearest single-precision float
    /// (`standard::ai::float`) and written as that decimal.
    Float,
    /// A GeoJSON `Feature` (area 20, chunk 5.5u6): `{"type": "Feature",
    /// "geometry": ..., "properties": {...}}`, its geometry and its
    /// properties each read and written by its own codec; held as a
    /// `Feature` record of the two.
    Feature {
        geometry: CodecId,
        properties: CodecId,
    },
    /// A GeoJSON `FeatureCollection` (chunk 5.5u6): `{"type":
    /// "FeatureCollection", "features": [...]}`, each feature read and
    /// written by `feature`.
    FeatureCollection { feature: CodecId },
    /// A provider package's field that names its encoding on the wire
    /// (area 11, "Decoding"; chunk 7.2c2): read and written as `coding`
    /// says on a provider's wire, and as `contract` everywhere else, so a
    /// value the app writes with `json.write` or answers a route with is
    /// the contract's.
    Wired {
        contract: CodecId,
        coding: WireCoding,
    },
}

/// How a provider package's field is sent ([`Codec::Wired`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WireCoding {
    /// One of the built-in encodings its `wire:` names; `currency` is the
    /// code of the one currency a `Wire.minor_units` amount is in.
    Encoding {
        encoding: crate::provider::WireEncoding,
        currency: Option<String>,
    },
    /// Its own `decode:` and `encode:`, the package's functions by
    /// identity key, run by the executor at the call's site: the value on
    /// the wire is read and written with `wire`, the codec of the type
    /// `decode:` takes.
    Functions {
        decode: String,
        encode: String,
        wire: CodecId,
    },
    /// Its `wire_parts:` (chunk 7.2e): standard type `standard`'s parts,
    /// each read from and written to the member of the object holding the
    /// field that `parts` spells it, in the type's own order. The field
    /// has no member of its own on the provider's wire.
    Parts {
        standard: String,
        parts: Vec<(String, String)>,
    },
}

/// One variant of an enum codec.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Segment {
    Fixed(String),
    /// A typed placeholder, decoded from the segment's text.
    Param {
        name: String,
        codec: CodecId,
    },
}

/// Where a route's `input:` comes from (area 7, "One input").
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// One of the routes `passkey:` generates (area 8, "Passkeys"), which
    /// the runtime answers itself.
    Passkey(PasskeyStep),
    /// One of the routes `magic_link:` generates (area 8, "Magic links"),
    /// which the runtime answers itself.
    MagicLink(MagicLinkStep),
    /// `GET <prefix>/openapi.json` (area 7, "OpenAPI"): the API's own
    /// OpenAPI document, its text as `jadpo check` writes it, which the
    /// runtime answers with as it is.
    OpenApi(String),
}

/// The routes `sign_in: { passkey: }` generates (area 8, "Passkeys"),
/// each under its `path:`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PasskeyStep {
    /// `POST <path>/sign-in/options`: a challenge to sign in with.
    SignInOptions,
    /// `POST <path>/sign-in`: the device's answer, which starts a browser
    /// session.
    SignIn,
    /// `POST <path>/register/options`: a challenge to add a passkey with.
    RegisterOptions,
    /// `POST <path>/register`: the new passkey and its name, kept.
    Register,
    /// `GET <path>`: the person's passkeys.
    List,
    /// `PATCH <path>/{passkey_id}`: renames one.
    Rename,
    /// `DELETE <path>/{passkey_id}`: removes one.
    Remove,
}

impl PasskeyStep {
    pub const ALL: [PasskeyStep; 7] = [
        PasskeyStep::SignInOptions,
        PasskeyStep::SignIn,
        PasskeyStep::RegisterOptions,
        PasskeyStep::Register,
        PasskeyStep::List,
        PasskeyStep::Rename,
        PasskeyStep::Remove,
    ];

    pub fn method(self) -> crate::HttpMethod {
        match self {
            PasskeyStep::List => crate::HttpMethod::Get,
            PasskeyStep::Rename => crate::HttpMethod::Patch,
            PasskeyStep::Remove => crate::HttpMethod::Delete,
            _ => crate::HttpMethod::Post,
        }
    }

    /// Its segments after `path:`; [`PASSKEY_ID`] is the placeholder.
    pub fn tail(self) -> &'static [&'static str] {
        match self {
            PasskeyStep::SignInOptions => &["sign-in", "options"],
            PasskeyStep::SignIn => &["sign-in"],
            PasskeyStep::RegisterOptions => &["register", "options"],
            PasskeyStep::Register => &["register"],
            PasskeyStep::List => &[],
            PasskeyStep::Rename | PasskeyStep::Remove => &[PASSKEY_ID],
        }
    }

    /// Signing in is open to anyone; adding a passkey reads the person's
    /// record now, so a disabled account adds none even with a revocation
    /// delay; listing, renaming and removing need the person signed in.
    pub fn authentication(self) -> Authentication {
        match self {
            PasskeyStep::SignInOptions | PasskeyStep::SignIn => Authentication::Public,
            PasskeyStep::RegisterOptions | PasskeyStep::Register => Authentication::Fresh,
            _ => Authentication::Required,
        }
    }

    pub fn status(self) -> crate::SuccessStatus {
        match self {
            PasskeyStep::Register => crate::SuccessStatus::Created,
            PasskeyStep::SignIn | PasskeyStep::Remove => crate::SuccessStatus::NoContent,
            _ => crate::SuccessStatus::Ok,
        }
    }

    /// What `jadpo routes` and the API record say it runs: `passkey
    /// sign-in options`.
    pub fn label(self) -> &'static str {
        match self {
            PasskeyStep::SignInOptions => "passkey sign-in options",
            PasskeyStep::SignIn => "passkey sign-in",
            PasskeyStep::RegisterOptions => "passkey registration options",
            PasskeyStep::Register => "passkey registration",
            PasskeyStep::List => "passkey listing",
            PasskeyStep::Rename => "passkey renaming",
            PasskeyStep::Remove => "passkey removal",
        }
    }

    /// What it answers with, by the name the API record gives it: the
    /// options WebAuthn's own `PublicKeyCredential*OptionsJSON` reads, or
    /// a passkey as its holder sees it.
    pub fn output(self) -> Option<&'static str> {
        match self {
            PasskeyStep::SignInOptions => Some(PASSKEY_REQUEST_OPTIONS),
            PasskeyStep::RegisterOptions => Some(PASSKEY_CREATION_OPTIONS),
            PasskeyStep::Register | PasskeyStep::Rename => Some(PASSKEY_VIEW),
            PasskeyStep::List => Some("List<PasskeyView>"),
            PasskeyStep::SignIn | PasskeyStep::Remove => None,
        }
    }

    /// Its full path's segments under `path`.
    pub fn segments(self, path: &[String]) -> Vec<String> {
        path.iter()
            .cloned()
            .chain(self.tail().iter().map(|text| text.to_string()))
            .collect()
    }
}

/// The placeholder in the paths that name one passkey.
pub const PASSKEY_ID: &str = "{passkey_id}";

/// Where `passkey:`'s routes go unless its `path:` says otherwise.
pub const PASSKEY_PATH: [&str; 2] = ["auth", "passkey"];

/// How long a passkey ceremony's challenge lasts unless `expires_after:`
/// says otherwise (area 8, chosen in chunk 5.5z), and the shortest and
/// longest it may.
pub const PASSKEY_EXPIRES_MS: i64 = 5 * 60 * 1000;
pub const PASSKEY_EXPIRES_MIN_MS: i64 = 60 * 1000;
pub const PASSKEY_EXPIRES_MAX_MS: i64 = 10 * 60 * 1000;

/// The most passkeys one person has (area 8, "Passkeys").
pub const PASSKEY_LIMIT: i64 = 20;

/// What the options routes answer with, as the API record names them.
pub const PASSKEY_CREATION_OPTIONS: &str = "PasskeyCreationOptions";
pub const PASSKEY_REQUEST_OPTIONS: &str = "PasskeyRequestOptions";

/// A passkey as its holder sees it: `id`, `name`, `registered_at` and
/// `last_used_at`, never its key or credential ID.
pub const PASSKEY_VIEW: &str = "PasskeyView";

/// What `passkey:` generates and how the runtime answers it (area 8,
/// "Passkeys").
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PasskeyPlan {
    /// `path:`'s segments.
    pub path: Vec<String>,
    /// How long a challenge lasts, in milliseconds.
    pub expires_after_ms: i64,
    /// A passkey found by its credential ID with its user's record, `$1`
    /// the credential ID and `$2` the time now: the passkey's `id`, its
    /// `WebAuthnCredential` JSON and the encoding that JSON was written in,
    /// its user handle, the user's `id` and subject, and whether the
    /// user's `active:` holds. None when no passkey has it or its user
    /// isn't visible.
    pub lookup: String,
    /// `WebAuthnCredential`'s number in the plan's JSON table, whose codec
    /// writes and reads a passkey as the contract's JSON (chunk 5.9).
    pub credential: u32,
    /// The user's address, the field `magic_link:`'s `address:` names,
    /// `$1` the user's `id` and `$2` the time now: what the person's
    /// device lists a new passkey under (chunk 5.5z2). `None` without
    /// magic links, when the subject is listed instead.
    pub address: Option<String>,
}

/// The routes `sign_in: { magic_link: }` generates (area 8, "Magic
/// links"), each under its `path:`, each open to anyone.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum MagicLinkStep {
    /// `POST <path>`: an address asks for a link and a code.
    Request,
    /// `POST <path>/confirm`: a link's token, or an address and its code,
    /// which starts a browser session.
    Confirm,
    /// `POST <path>/redeem`: an address and its code, traded for an API
    /// token; only with `api_token:`.
    Redeem,
}

impl MagicLinkStep {
    pub const ALL: [MagicLinkStep; 3] = [
        MagicLinkStep::Request,
        MagicLinkStep::Confirm,
        MagicLinkStep::Redeem,
    ];

    pub fn method(self) -> crate::HttpMethod {
        crate::HttpMethod::Post
    }

    /// Its segments after `path:`.
    pub fn tail(self) -> &'static [&'static str] {
        match self {
            MagicLinkStep::Request => &[],
            MagicLinkStep::Confirm => &["confirm"],
            MagicLinkStep::Redeem => &["redeem"],
        }
    }

    /// Anyone may call each: the person isn't signed in yet.
    pub fn authentication(self) -> Authentication {
        Authentication::Public
    }

    /// A request is accepted whether or not an account has the address.
    pub fn status(self) -> crate::SuccessStatus {
        match self {
            MagicLinkStep::Request => crate::SuccessStatus::Accepted,
            _ => crate::SuccessStatus::Ok,
        }
    }

    /// What `jadpo routes` and the API record say it runs: `magic link
    /// request`.
    pub fn label(self) -> &'static str {
        match self {
            MagicLinkStep::Request => "magic link request",
            MagicLinkStep::Confirm => "magic link confirmation",
            MagicLinkStep::Redeem => "magic link redemption",
        }
    }

    /// What it reads from the body, by the name the API record gives it.
    pub fn input(self) -> &'static str {
        match self {
            MagicLinkStep::Request => MAGIC_LINK_ASK,
            MagicLinkStep::Confirm => MAGIC_LINK_CONFIRMATION,
            MagicLinkStep::Redeem => MAGIC_LINK_CODE,
        }
    }

    /// What it answers with, by the name the API record gives it.
    pub fn output(self) -> Option<&'static str> {
        match self {
            MagicLinkStep::Request => None,
            MagicLinkStep::Confirm => Some(MAGIC_LINK_CONFIRMED),
            MagicLinkStep::Redeem => Some(MAGIC_LINK_TOKEN),
        }
    }

    /// Its full path's segments under `path`.
    pub fn segments(self, path: &[String]) -> Vec<String> {
        path.iter()
            .cloned()
            .chain(self.tail().iter().map(|text| text.to_string()))
            .collect()
    }
}

/// Where `magic_link:`'s routes go unless its `path:` says otherwise.
pub const MAGIC_LINK_PATH: [&str; 2] = ["auth", "magic-link"];

/// What the magic-link routes read and answer with, as the API record
/// names them.
pub const MAGIC_LINK_ASK: &str = "MagicLinkAsk";
pub const MAGIC_LINK_CONFIRMATION: &str = "MagicLinkConfirmation";
pub const MAGIC_LINK_CODE: &str = "MagicLinkCode";
pub const MAGIC_LINK_CONFIRMED: &str = "MagicLinkConfirmed";
pub const MAGIC_LINK_TOKEN: &str = "MagicLinkToken";

/// `magic_link:`'s defaults (area 8, "Magic links"), and the furthest
/// each may be loosened without an acceptance.
pub const MAGIC_LINK_EXPIRES_MS: i64 = 15 * 60 * 1000;
pub const MAGIC_LINK_EXPIRES_MAX_MS: i64 = 60 * 60 * 1000;
pub const MAGIC_LINK_CODE_ATTEMPTS: u32 = 5;
pub const MAGIC_LINK_CODE_ATTEMPTS_MAX: u32 = 10;
pub const MAGIC_LINK_CODES_PER_HOUR: u32 = 10;
pub const MAGIC_LINK_FAILED_CODES_PER_DAY: u32 = 20;
pub const MAGIC_LINK_FAILED_CODES_PER_DAY_MAX: u32 = 50;
pub const MAGIC_LINK_RESEND_AFTER_MS: i64 = 60 * 1000;

/// The longest `return_to` a request may give, in bytes.
pub const RETURN_TO_MAX: usize = 2048;

/// What `magic_link:` generates and how the runtime answers it (area 8,
/// "Magic links").
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MagicLinkPlan {
    /// `path:`'s segments.
    pub path: Vec<String>,
    /// `link:`, the frontend path the email's link opens: `/sign-in/confirm`.
    pub link: String,
    /// How long a link and its code last, in milliseconds.
    pub expires_after_ms: i64,
    /// Wrong guesses that end a code.
    pub code_attempts: u32,
    /// Sends to one address in an hour.
    pub codes_per_hour: u32,
    /// Wrong guesses for one address in a UTC day before codes stop.
    pub failed_codes_per_day: u32,
    /// The least time between two sends to one address, in milliseconds.
    pub resend_after_ms: i64,
    /// Whether a link works only in the browser that asked for it.
    pub same_browser: bool,
    /// The user found by the address, `$1` the address as stored and `$2`
    /// the time now: the user's `id`, and whether its `active:` holds.
    /// None when no visible user has it.
    pub lookup: String,
    /// The same, `$1` the user's `id`: the user `on_first_sign_in:`'s
    /// action gave.
    pub lookup_id: String,
    /// `on_first_sign_in:`'s action, by callable number: run for an
    /// address no user has.
    pub first_sign_in: Option<u32>,
}

/// The built-in `FirstSignIn` (area 8, "A person the app has not seen"):
/// what `on_first_sign_in:`'s action is given.
pub const FIRST_SIGN_IN: &str = "FirstSignIn";

/// The built-in events every sign-in publishes (area 8, "Events"; chunk
/// 7.3b4), which only the runtime emits: a session or API token started
/// (`SignedIn`), a link, code or first sign-in refused (`SignInRefused`),
/// and an address past `failed_codes_per_day:` (`SignInCodesDisabled`).
/// None carries an address, a link, a code or a token.
pub const SIGNED_IN: &str = "SignedIn";
pub const SIGN_IN_REFUSED: &str = "SignInRefused";
pub const SIGN_IN_CODES_DISABLED: &str = "SignInCodesDisabled";
/// A person registered a passkey (`PasskeyAdded`) or removed one of
/// theirs (`PasskeyRemoved`; chunk 5.5z2): the user's `id` and the
/// passkey's, never its credential ID or key.
pub const PASSKEY_ADDED: &str = "PasskeyAdded";
pub const PASSKEY_REMOVED: &str = "PasskeyRemoved";
pub const SIGN_IN_EVENTS: [&str; 5] = [
    SIGNED_IN,
    SIGN_IN_REFUSED,
    SIGN_IN_CODES_DISABLED,
    PASSKEY_ADDED,
    PASSKEY_REMOVED,
];

/// The identity key the outbox keeps the built-in sign-in event `name`
/// under, and its subscribers listen for: `jadpo.SignedIn`.
pub fn sign_in_event_key(name: &str) -> Option<&'static str> {
    match name {
        SIGNED_IN => Some("jadpo.SignedIn"),
        SIGN_IN_REFUSED => Some("jadpo.SignInRefused"),
        SIGN_IN_CODES_DISABLED => Some("jadpo.SignInCodesDisabled"),
        PASSKEY_ADDED => Some("jadpo.PasskeyAdded"),
        PASSKEY_REMOVED => Some("jadpo.PasskeyRemoved"),
        _ => None,
    }
}

/// Whether the built-in event `name` is a passkey's own, which only
/// `passkey:` publishes, rather than one every way in publishes.
pub fn passkey_event(name: &str) -> bool {
    matches!(name, PASSKEY_ADDED | PASSKEY_REMOVED)
}

/// What publishes the built-in event `name`, for a message: `signing in`,
/// `adding a passkey` or `removing a passkey`.
pub fn published_by(name: &str) -> &'static str {
    match name {
        PASSKEY_ADDED => "adding a passkey",
        PASSKEY_REMOVED => "removing a passkey",
        _ => "signing in",
    }
}

/// The fields, in order, of the built-in objects a test, fixture or seed
/// may build to call an action directly (chunk 7.3b4): `FirstSignIn`,
/// which `on_first_sign_in:`'s action is given, and the sign-in events,
/// which a subscriber's action is given. Only the runtime makes them
/// anywhere else.
pub fn test_built_fields(name: &str) -> Option<&'static [&'static str]> {
    match name {
        FIRST_SIGN_IN => Some(&["method", "address", "address_verified", "subject", "name"]),
        SIGNED_IN => Some(&["user_id", "method", "first_time"]),
        SIGN_IN_REFUSED => Some(&["method", "reason", "user_id"]),
        SIGN_IN_CODES_DISABLED => Some(&["user_id", "until"]),
        PASSKEY_ADDED | PASSKEY_REMOVED => Some(&["user_id", "passkey_id"]),
        _ => None,
    }
}

/// The built-in sign-in event whose identity key is `key`.
pub fn sign_in_event_named(key: &str) -> Option<&'static str> {
    SIGN_IN_EVENTS
        .into_iter()
        .find(|name| sign_in_event_key(name) == Some(key))
}

/// The namespace of magic links' generated operations (area 8, "Writing
/// the routes yourself").
pub const MAGIC_LINKS: &str = "magic_links";

/// The namespace only route tests call (area 8, "In development and
/// tests"; chunk 5.5z2): a software authenticator driving the generated
/// passkey routes, `test_passkeys.register` and `.sign_in`.
pub const TEST_PASSKEYS: &str = "test_passkeys";

/// The device holding a passkey `test_passkeys.register` added: only a
/// test holds one, and nothing reads its parts.
pub const TEST_PASSKEY: &str = "TestPasskey";

/// The built-in `MagicLinkRedeemed`: what `magic_links.redeem` and
/// `.redeem_code` give, the user's `id` (`user_id`) and the path the
/// person asked to go back to (`return_to`, `/` unless they gave one).
pub const MAGIC_LINK_REDEEMED: &str = "MagicLinkRedeemed";

/// What one argument of the call is made from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
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
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
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
    /// `magic_links.request(address, locale, return_to)` (area 8, "Writing
    /// the routes yourself"; chunk 7.3b3): asks for a link and a code, as
    /// `POST /auth/magic-link` does.
    MagicLinkRequest,
    /// `magic_links.redeem(token)`: uses a link's token, giving a
    /// `MagicLinkRedeemed`.
    MagicLinkRedeem,
    /// `magic_links.redeem_code(address, code)`: the same for a code.
    MagicLinkRedeemCode,
    /// `test_passkeys.register(user, name)` (area 8, "In development and
    /// tests"; chunk 5.5z2): only a test calls it, adding a passkey on a
    /// software authenticator through the generated routes, signed in as
    /// `as:` signs in, and giving the `TestPasskey` that holds it.
    TestPasskeyRegister,
    /// `test_passkeys.sign_in(passkey)`: signs in with it through the two
    /// sign-in routes, giving the second's `Response`.
    TestPasskeySignIn,
}

impl SessionOp {
    pub const ALL: [SessionOp; 13] = [
        SessionOp::Start,
        SessionOp::End,
        SessionOp::IssueToken,
        SessionOp::Tokens,
        SessionOp::RevokeToken,
        SessionOp::IssueKey,
        SessionOp::Keys,
        SessionOp::RevokeKey,
        SessionOp::MagicLinkRequest,
        SessionOp::MagicLinkRedeem,
        SessionOp::MagicLinkRedeemCode,
        SessionOp::TestPasskeyRegister,
        SessionOp::TestPasskeySignIn,
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
            SessionOp::MagicLinkRequest => "request",
            SessionOp::MagicLinkRedeem => "redeem",
            SessionOp::MagicLinkRedeemCode => "redeem_code",
            SessionOp::TestPasskeyRegister => "register",
            SessionOp::TestPasskeySignIn => "sign_in",
        }
    }

    /// The namespace it is called through: `sessions`, `magic_links`, or
    /// none for the service entity's.
    pub fn namespace(self) -> Option<&'static str> {
        if self.on_service() {
            None
        } else if self.magic_link() {
            Some(MAGIC_LINKS)
        } else if self.test_passkey() {
            Some(TEST_PASSKEYS)
        } else {
            Some("sessions")
        }
    }

    /// The operation `sessions.<name>` names.
    pub fn from_name(name: &str) -> Option<SessionOp> {
        SessionOp::ALL
            .into_iter()
            .find(|op| op.namespace() == Some("sessions") && op.name() == name)
    }

    /// The operation `magic_links.<name>` names.
    pub fn magic_link_op(name: &str) -> Option<SessionOp> {
        SessionOp::ALL
            .into_iter()
            .find(|op| op.magic_link() && op.name() == name)
    }

    /// Whether it is one of magic links' generated operations, which the
    /// application's own routes call with `generate_routes: false`.
    pub fn magic_link(self) -> bool {
        matches!(
            self,
            SessionOp::MagicLinkRequest
                | SessionOp::MagicLinkRedeem
                | SessionOp::MagicLinkRedeemCode
        )
    }

    /// The operation `test_passkeys.<name>` names.
    pub fn test_passkey_op(name: &str) -> Option<SessionOp> {
        SessionOp::ALL
            .into_iter()
            .find(|op| op.test_passkey() && op.name() == name)
    }

    /// Whether it is one of `test_passkeys`' operations, which only route
    /// tests call.
    pub fn test_passkey(self) -> bool {
        matches!(
            self,
            SessionOp::TestPasskeyRegister | SessionOp::TestPasskeySignIn
        )
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
            // Magic links' operations need `magic_link:`, which signs a
            // browser session in, rather than a method of their own.
            SessionOp::Start
            | SessionOp::End
            | SessionOp::MagicLinkRequest
            | SessionOp::MagicLinkRedeem
            | SessionOp::MagicLinkRedeemCode
            | SessionOp::TestPasskeyRegister
            | SessionOp::TestPasskeySignIn => crate::AuthenticationMethod::BrowserSession,
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

/// The last segment of the path each API serves its OpenAPI document at,
/// after its prefix (area 7, "OpenAPI"): `/v1/openapi.json`.
pub const OPENAPI_PATH: &str = "openapi.json";

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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// An external provider, checked independently of the revocation delay.
    #[serde(default)]
    pub external_token: Option<ExternalTokenPlan>,
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
    /// `sign_in: { passkey: }` (area 8, "Passkeys"), when declared.
    pub passkeys: Option<PasskeyPlan>,
    /// `sign_in: { magic_link: }` (area 8, "Magic links"), when declared.
    pub magic_links: Option<MagicLinkPlan>,
}

/// External JWT authentication (area 8): configuration names and one
/// subject-to-current-user query, `$1` subject and `$2` invocation time.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExternalTokenPlan {
    pub issuer: String,
    pub audience: String,
    pub lookup: String,
    /// Subject field's codec in the program's JSON type table.
    pub subject_type: u32,
    pub subject_sql: crate::SqlType,
}

/// Listing, issuing and revoking credentials, limited to holders whose
/// record the caller may read (to list) or update (to issue or revoke).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Arg {
    pub root: ArgRoot,
    pub path: Vec<String>,
}

/// How a route waits for the work its action set off (area 7, "Waiting
/// for follow-on work"; chunk 6.3): `settle: { answer: within: }`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SettlePlan {
    /// The named query's read it answers with, run as the request's
    /// caller under the query's policy.
    pub answer: Box<ReadPlan>,
    /// Its arguments, each a path read through the action's result
    /// (`result.id` is `["id"]`).
    pub args: Vec<Vec<String>>,
    /// The codec each argument is kept with beside an `Idempotency-Key`,
    /// its parameter's type's, so a repeat runs the query again.
    pub codecs: Vec<CodecId>,
    /// How long the route waits, from when the request came in, before it
    /// answers 202 with what the query gives so far.
    pub within_ms: u32,
}

/// One route.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// With `settle:`, what it waits for and answers with; its `output` is
    /// then the answer query's result's codec.
    pub settle: Option<SettlePlan>,
    /// Whether it sends a model's answer as it comes, as server-sent
    /// events: its action returns an `AiStream` (area 7, "Streaming a
    /// model's answer"; chunk 5.5v6). Its `output` is then `None`.
    pub stream: bool,
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FailureCodec {
    /// The failure's identity key: `type QuantityUnavailable`.
    pub key: String,
    pub client: Vec<(String, CodecId)>,
}

/// How many requests a caller may make in a window (area 16, "Rate limits").
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
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

/// Every limit the HTTP component counts, in the order a store's bindings
/// are numbered (by window, then requests): the address's and the signed-in
/// caller's, until routes give their own (`rate_limit:`).
pub const COUNTED_LIMITS: [RateLimit; 2] = [ADDRESS_LIMIT, CALLER_LIMIT];

/// The windows Cloudflare's rate-limiting binding counts over: 10 seconds
/// and a minute (area 16, "Rate limits"; its `period`, 10 or 60).
pub const BINDING_WINDOWS_MS: [u32; 2] = [10_000, 60_000];

/// Where routes' rate limits are counted: the connection
/// `application { rate_limits: { store: } }` names (area 16, "Rate
/// limits"; chunk 8.2m2). A host that can't reach it counts in its own
/// memory, so no request fails for it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RateLimitStore {
    /// The connection's name.
    pub connection: String,
    /// On Cloudflare's rate-limiting binding, `Driver.cloudflare_rate_limits`,
    /// the binding each limit is counted in, since a binding counts one
    /// limit over one window; empty on Redis.
    pub bindings: Vec<RateLimitBinding>,
}

impl RateLimitStore {
    /// The binding `limit` is counted in, if the store has one for it.
    pub fn binding(&self, limit: RateLimit) -> Option<&RateLimitBinding> {
        self.bindings.iter().find(|binding| binding.limit == limit)
    }
}

/// One of Cloudflare's rate-limiting bindings: the Worker's binding of
/// that name, counting `limit` in the namespace numbered here.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RateLimitBinding {
    pub limit: RateLimit,
    /// The binding's name in the Worker's `env`: the connection's name in
    /// capitals, the requests and the window, `LIMITS_120_PER_60S`.
    pub name: String,
    /// Its `namespace_id` in Cloudflare.
    pub namespace: u32,
}

/// The bindings connection `connection` counts `limits` in, numbered from
/// `namespace`: one a limit, in the order of [`COUNTED_LIMITS`], each
/// named for the connection and the limit. `None` when the numbers would
/// pass the largest a `u32` holds.
pub fn rate_limit_bindings(
    connection: &str,
    namespace: u32,
    limits: &[RateLimit],
) -> Option<Vec<RateLimitBinding>> {
    let mut limits = limits.to_vec();
    limits.sort_by_key(|limit| (limit.window_ms, limit.requests));
    limits.dedup();
    limits
        .into_iter()
        .zip(0u32..)
        .map(|(limit, at)| {
            Some(RateLimitBinding {
                limit,
                name: format!(
                    "{}_{}_PER_{}S",
                    connection.to_ascii_uppercase(),
                    limit.requests,
                    limit.window_ms / 1000
                ),
                namespace: namespace.checked_add(at)?,
            })
        })
        .collect()
}

/// How many proxies of the deployment's own sit in front of the app
/// (area 10, "Client addresses").
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClientAddressPlan {
    pub source: ClientAddressSource,
    /// Present exactly for the sources that count from the right of a
    /// header: `forwarded_for` and `forwarded`.
    pub hops: Option<Hops>,
}

/// Every route of a project, ready to run.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Where rate limits are counted; `None` when `app.jadpo` names no
    /// store, and the host counts in its own memory.
    pub rate_limits: Option<RateLimitStore>,
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
/// written exactly as a request and a response are. The same table holds
/// every type the runtime keeps as its contract's JSON: a field stored as
/// JSON (area 5, chunks 5.5w2b and 5.9b), a passkey's credential (chunk
/// 5.9), and each event type, whose payload the outbox keeps (area 12,
/// chunk 5.9b).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JsonTable {
    pub codecs: Vec<Codec>,
    /// Each call's type's codec, by the number its site gives it.
    pub types: Vec<CodecId>,
    /// Each event type's number in `types`, by the identity key its
    /// outbox row records (area 12, "A payload is the contract's JSON"),
    /// in key order.
    pub events: Vec<EventJson>,
}

/// An event type's place in the [`JsonTable`]: its payload is written and
/// read as its type's contract JSON (area 12; chunk 5.9b).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EventJson {
    /// The event type's identity key, which survives a rename: what the
    /// outbox's `event` column holds.
    pub key: String,
    /// Its number in [`JsonTable::types`].
    pub number: u32,
}

impl JsonTable {
    /// The codec of the type numbered `at`.
    pub fn codec_of(&self, at: u32) -> Option<CodecId> {
        self.types.get(at as usize).copied()
    }

    /// The number of the event type whose identity key is `key`, which
    /// its payload is written and read by.
    pub fn event(&self, key: &str) -> Option<u32> {
        self.events
            .iter()
            .find(|event| event.key == key)
            .map(|event| event.number)
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

    /// Each limit counted on Cloudflare's binding has a binding of its
    /// own, named for the connection and the limit and numbered from the
    /// connection's namespace in the order of window, then requests
    /// (chunk 8.2m2).
    #[test]
    fn each_limit_has_a_binding_numbered_from_the_namespace() {
        let hourly = RateLimit {
            requests: 5,
            window_ms: 3_600_000,
        };
        let bindings = rate_limit_bindings(
            "limits",
            1001,
            &[CALLER_LIMIT, hourly, ADDRESS_LIMIT, CALLER_LIMIT],
        )
        .expect("numbered");
        let named: Vec<(&str, u32)> = bindings
            .iter()
            .map(|binding| (binding.name.as_str(), binding.namespace))
            .collect();
        assert_eq!(
            named,
            [
                ("LIMITS_120_PER_60S", 1001),
                ("LIMITS_600_PER_60S", 1002),
                ("LIMITS_5_PER_3600S", 1003)
            ]
        );
        assert_eq!(
            rate_limit_bindings("limits", 1001, &COUNTED_LIMITS)
                .expect("numbered")
                .iter()
                .map(|binding| binding.limit)
                .collect::<Vec<_>>(),
            COUNTED_LIMITS
        );
        assert_eq!(
            rate_limit_bindings("limits", u32::MAX, &COUNTED_LIMITS),
            None
        );
        let store = RateLimitStore {
            connection: "limits".into(),
            bindings,
        };
        assert_eq!(
            store.binding(ADDRESS_LIMIT).map(|b| b.name.as_str()),
            Some("LIMITS_120_PER_60S")
        );
        assert!(BINDING_WINDOWS_MS.contains(&ADDRESS_LIMIT.window_ms));
        assert!(!BINDING_WINDOWS_MS.contains(&hourly.window_ms));
    }
}
