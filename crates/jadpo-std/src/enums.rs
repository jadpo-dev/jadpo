//! The standard library's enums that Rust code interprets, declared once:
//! the checker builds the Jadpo enums of the same names from these lists,
//! so a variant added here exists for programs and for the runtime alike.

macro_rules! std_enum {
    (
        $(#[$meta:meta])*
        $name:ident: $written:literal { $($(#[$vmeta:meta])* $variant:ident: $word:literal,)* }
    ) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize,
        )]
        pub enum $name {
            $($(#[$vmeta])* $variant,)*
        }

        impl $name {
            /// The enum's name in Jadpo.
            pub const NAME: &'static str = $written;

            /// Every variant, in the order Jadpo lists them.
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            /// The variants' names in Jadpo, in order.
            pub const NAMES: &'static [&'static str] = &[$($word,)*];

            /// The variant's name in Jadpo.
            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $word,)*
                }
            }

            pub fn from_name(name: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|variant| variant.name() == name)
            }
        }
    };
}

std_enum! {
    /// How a division or a rounding that doesn't come out exact is rounded
    /// (area 3, question 7; area 19, "Numbers").
    Rounding: "Rounding" {
        /// To the nearest; a tie goes to the even neighbour.
        HalfEven: "half_even",
        /// To the nearest; a tie goes away from zero.
        HalfUp: "half_up",
        /// Away from zero.
        Up: "up",
        /// Toward zero: the dropped digits are dropped (area 19, "`Rounding.down`").
        Down: "down",
    }
}

std_enum! {
    /// The formats a `Text` type's `format:` setting accepts (area 2,
    /// question 9).
    TextFormat: "TextFormat" {
        /// An email address.
        Email: "email",
    }
}

std_enum! {
    /// The UUID versions a `Uuid` type's `version:` setting can pin (area 2,
    /// question 13).
    UuidVersion: "UuidVersion" {
        V1: "v1",
        V2: "v2",
        V3: "v3",
        V4: "v4",
        V5: "v5",
        V6: "v6",
        V7: "v7",
        V8: "v8",
    }
}

impl UuidVersion {
    /// The version digit a UUID's text carries.
    pub fn digit(self) -> u8 {
        match self {
            UuidVersion::V1 => 1,
            UuidVersion::V2 => 2,
            UuidVersion::V3 => 3,
            UuidVersion::V4 => 4,
            UuidVersion::V5 => 5,
            UuidVersion::V6 => 6,
            UuidVersion::V7 => 7,
            UuidVersion::V8 => 8,
        }
    }
}

std_enum! {
    /// A day of the week (area 10).
    Weekday: "Weekday" {
        Monday: "monday",
        Tuesday: "tuesday",
        Wednesday: "wednesday",
        Thursday: "thursday",
        Friday: "friday",
        Saturday: "saturday",
        Sunday: "sunday",
    }
}

std_enum! {
    /// A month of the year (area 19, "Time and calendar additions"),
    /// January first, as ISO 8601 numbers them from 1.
    Month: "Month" {
        January: "january",
        February: "february",
        March: "march",
        April: "april",
        May: "may",
        June: "june",
        July: "july",
        August: "august",
        September: "september",
        October: "october",
        November: "november",
        December: "december",
    }
}

std_enum! {
    /// The way a locale's text runs (area 10, "Locales"): what `locale.direction`
    /// gives. Its wording, for a template's `dir=` attribute, is `ltr` and
    /// `rtl`.
    TextDirection: "TextDirection" {
        LeftToRight: "left_to_right",
        RightToLeft: "right_to_left",
    }
}

std_enum! {
    /// Whether wording outside the default locale needs a person's
    /// acceptance (area 10, "Translations as messages"): the `review:` of
    /// `translations: { }` in `app.jadpo`.
    TranslationReview: "TranslationReview" {
        /// Nothing waits for review; the default.
        None: "none",
        /// New or changed wording stays unaccepted until `jadpo
        /// translations accept <locale>`.
        Required: "required",
    }
}

std_enum! {
    /// Where a batch run stands (area 2, "The batch types' fields"; area
    /// 12, "Batches"): what `batches.status` gives as its `state`.
    BatchState: "BatchState" {
        /// Its batches are still going, or waiting to resume.
        Running: "running",
        /// Its last batch and `finished:` committed.
        Finished: "finished",
        /// It stopped for good: a fault it can't go past, or a deploy that
        /// changed what it saved.
        Failed: "failed",
        /// `batches.cancel` stopped it after the batch in hand.
        Cancelled: "cancelled",
    }
}

std_enum! {
    /// Who a CSV or TSV file is written for (area 19, "Data formats"; area
    /// 12, "Batches"; chunk 6.4b2): what `csv.write`'s and `tsv.write`'s
    /// required `format:` says.
    CsvFormat: "CsvFormat" {
        /// A spreadsheet: a UTF-8 byte-order mark first, CRLF line
        /// endings, and a text cell a spreadsheet would read as a formula
        /// written with `'` before it.
        Spreadsheet: "spreadsheet",
        /// A program: CRLF line endings only.
        Standard: "standard",
    }
}

std_enum! {
    /// Which instant a local time takes when a zone's clocks change around
    /// it (area 10, "Time library and formatting"): an `overlap` happens twice, a `gap` never.
    /// Without `dst:`, or with `reject`, such a time is a failure the
    /// caller maps.
    Dst: "Dst" {
        /// Fail: the call's `attempt` maps `dst:`.
        Reject: "reject",
        /// The earlier instant in an overlap; in a gap, the clock time
        /// moved back by the gap's length (01:30 in a 01:00 to 02:00 gap
        /// is 00:30).
        Earlier: "earlier",
        /// The later instant in an overlap; in a gap, the clock time moved
        /// forward by the gap's length (01:30 is 02:30).
        Later: "later",
        /// Temporal's choice: earlier in an overlap, later in a gap.
        Compatible: "compatible",
    }
}

std_enum! {
    /// What moving a date by months or years does when the month it lands
    /// in has no such day, as 31 January plus a month (area 10).
    InvalidDay: "InvalidDay" {
        /// Fail: the call's `attempt` maps `invalid_day:`.
        Reject: "reject",
        /// The month's last day.
        LastValidDay: "last_valid_day",
    }
}

std_enum! {
    /// How one part of a declared `TimeFormat` is shown (area 10, "Showing
    /// dates and times to people"), as `Intl.DateTimeFormat` and CLDR's
    /// skeletons name them: each part takes some of these
    /// ([`crate::Component::styles`]), and the locale decides the rest.
    ComponentStyle: "ComponentStyle" {
        /// Digits as the locale writes them: `5`, `2026`.
        Numeric: "numeric",
        /// Always two digits: `05`, `26`.
        TwoDigit: "two_digit",
        /// An abbreviated name: `Oct`, `Sun`, `BST`.
        Short: "short",
        /// The full name: `October`, `Sunday`, `British Summer Time`.
        Long: "long",
        /// The narrowest form: `O`, `S`.
        Narrow: "narrow",
    }
}

std_enum! {
    /// When a generated `Instant` field is set (area 5, "Fields and their
    /// settings").
    Generated: "Generated" {
        /// Once, when the record is created.
        OnCreate: "on_create",
        /// When the record is created and on every change.
        OnChange: "on_change",
    }
}

std_enum! {
    /// What deleting a referenced record does to the records that refer to
    /// it (area 5, "References and `on_delete`").
    OnDelete: "OnDelete" {
        /// The delete fails with a conflict while any record refers to it.
        Restricted: "restricted",
        /// Records that refer to it are deleted in the same transaction.
        Cascade: "cascade",
        /// The reference is set to `none`.
        Cleared: "cleared",
    }
}

std_enum! {
    /// What erasing a person does to a record that refers to them, or to
    /// one of their records (area 15, "Erasure"; chunk P3): a reference's
    /// `erase:`, which an `OnDelete.restricted` reference into a person's
    /// records must give.
    Erasure: "Erasure" {
        /// The record is deleted, with everything that cascades from it.
        Deleted: "deleted",
        /// The record is kept; the reference and every marked field are
        /// cleared.
        Anonymised: "anonymised",
        /// The record is kept as it is until its own retention ends; only
        /// the reference is cleared.
        Retained: "retained",
    }
}

std_enum! {
    /// What a change an audit trail entry records did to its record (area
    /// 16, "Audit trail"; chunk B11).
    AuditChange: "AuditChange" {
        /// The record was created.
        Created: "created",
        /// One or more of its fields changed: an update, a patch, a
        /// counter or a lifecycle's transition, its `delete` among them.
        Updated: "updated",
        /// The record was deleted.
        Deleted: "deleted",
    }
}

std_enum! {
    /// Who made a change an audit trail entry records (area 16, "Audit
    /// trail"; chunk B11).
    AuditBy: "AuditBy" {
        /// A signed-in user: the entry's `user_id`.
        User: "user",
        /// A signed-in service: the entry's `service_id`.
        Service: "service",
        /// The application, for a schedule's run or a subscriber's
        /// delivery: the entry's `trace` leads back to the request that
        /// set it off.
        Application: "application",
        /// No one: a test's, a seed's or a tool's call without a caller.
        NoCaller: "no_caller",
    }
}

std_enum! {
    /// A special kind of index an entity's `indexes:` entry asks for with
    /// `kind:` (area 5, "Indexes"), where the store builds it.
    IndexKind: "IndexKind" {
        /// A GiST index on one geometry field, which `geo.` conditions
        /// search (area 20, "Indexes"; chunk 5.5u4).
        Geographic: "geographic",
        /// pgvector's HNSW index on one vector field, which `nearest`
        /// searches, tuned by `m:` and `ef_construction:` (area 21, "The
        /// index"; chunk 5.5v1c).
        Hnsw: "hnsw",
    }
}

std_enum! {
    /// A migration's decision about a field that is gone from its entity
    /// (area 5, "Migrations").
    FieldChange: "FieldChange" {
        /// Code stops using it; the column stays, for servers still running
        /// the old build.
        Retired: "retired",
        /// The column is removed, in a later migration than the retirement.
        Dropped: "dropped",
    }
}

std_enum! {
    /// The built-in drivers a connection in `app.jadpo` can name (area 11,
    /// "Capabilities").
    Driver: "Driver" {
        /// PostgreSQL, the `database` capability.
        Postgres: "postgres",
        /// Redis, the `cache` capability.
        Redis: "redis",
        /// The server's own disk, the `files` capability on the native
        /// target (area 16, "The disk driver"; chunk 6.4b).
        Disk: "disk",
        /// An SMTP server, the `email` capability (area 16, "Email"; chunk
        /// 7.3).
        Smtp: "smtp",
        /// A server that speaks OpenAI's Chat Completions API, the `ai`
        /// capability's local driver: Ollama, llama.cpp's server, vLLM, LM
        /// Studio (area 21, "How a provider fills it"; chunk 5.5v3).
        OpenaiCompatible: "openai_compatible",
        /// A queue on Cloudflare Queues, the `receiving` capability on
        /// Workers, its batches pushed to the app's consumer (area 11,
        /// "Queues"; chunk 7.7c2).
        CloudflareQueues: "cloudflare_queues",
        /// Cloudflare's rate-limiting binding, a store `rate_limits:` can
        /// count route rate limits in on Workers (area 16, "Rate limits";
        /// chunk 8.2m2).
        CloudflareRateLimits: "cloudflare_rate_limits",
        /// An OpenID Connect provider people sign in with, such as Google
        /// or Microsoft, named by `sign_in: { providers: }` (area 8,
        /// "Providers"; chunk A7b).
        Oidc: "oidc",
    }
}

std_enum! {
    /// What a deployable is compiled for (area 14, "Deployables"; decision
    /// register 2026-10-13): `deployables: { api: { target: Target.wasm } }`.
    DeployTarget: "Target" {
        /// A native process on a server.
        Native: "native",
        /// WebAssembly on Cloudflare Workers (roadmap chunk 8.2).
        Wasm: "wasm",
    }
}

impl DeployTarget {
    /// The host it runs on, which decides the drivers it can build (area
    /// 11, question 24).
    pub fn host(self) -> crate::connections::Target {
        match self {
            DeployTarget::Native => crate::connections::Target::Native,
            DeployTarget::Wasm => crate::connections::Target::Workers,
        }
    }
}

std_enum! {
    /// The background work a deployable runs besides the routes it serves
    /// (area 14, "Deployables"): `runs: [Trigger.schedules,
    /// Trigger.subscribers]`.
    Trigger: "Trigger" {
        /// The schedules, each entity's purge and each poll, on the tick.
        Schedules: "schedules",
        /// Each event's and email's deliveries, after its action commits
        /// and from the outbox on the tick, and the queues `receiving:`
        /// reads.
        Subscribers: "subscribers",
    }
}

std_enum! {
    /// What ticks a native deployable (area 12, question 16; chunk 8.2g):
    /// `schedules: { trigger: ScheduleTrigger.external }`. A Worker's tick
    /// is always Cloudflare's minute cron, whichever is written.
    #[derive(Default)]
    ScheduleTrigger: "ScheduleTrigger" {
        /// The server's own timer, at each whole period of UTC: the
        /// default.
        #[default]
        Native: "native",
        /// Another scheduler running the deployable's `tick` command, one
        /// tick and exit: the server never ticks.
        External: "external",
    }
}

std_enum! {
    /// Which of a provider package's hosts a connection reaches (area 11,
    /// "Connecting in `app.jadpo`"): the `environment:` setting a
    /// connection to a package with `hosts:` reads (chunk 7.1c).
    ProviderEnvironment: "ProviderEnvironment" {
        /// The provider's sandbox: `hosts: { sandbox: }`.
        Sandbox: "sandbox",
        /// The provider's live API: `hosts: { live: }`.
        Live: "live",
    }
}

/// The name of the built-in enum of IANA time zones (area 10).
pub const ZONE: &str = "Zone";

/// The name of the built-in enum of standard locales (area 10).
pub const LOCALE: &str = "Locale";

std_enum! {
    /// A route's success status (area 7, "Output and status").
    SuccessStatus: "SuccessStatus" {
        /// 200, with the output.
        Ok: "ok",
        /// 201, with the output.
        Created: "created",
        /// 202, with the output; the work continues in the background.
        Accepted: "accepted",
        /// 204, with no output.
        NoContent: "no_content",
    }
}

impl SuccessStatus {
    /// The HTTP status it answers with.
    pub fn code(self) -> u16 {
        match self {
            SuccessStatus::Ok => 200,
            SuccessStatus::Created => 201,
            SuccessStatus::Accepted => 202,
            SuccessStatus::NoContent => 204,
        }
    }
}

std_enum! {
    /// The methods a route is written with, as a route test names them
    /// (areas 7 and 13); the runtime answers `HEAD` and `OPTIONS` itself.
    /// Since chunk 5.5n it is the standard type `HttpMethod` too, its
    /// codes RFC 9110's tokens (`jadpo_std::standard::web::HTTP_METHODS`).
    HttpMethod: "HttpMethod" {
        Get: "get",
        Post: "post",
        Put: "put",
        Patch: "patch",
        Delete: "delete",
    }
}

std_enum! {
    /// Whether an API's writes need an `Idempotency-Key` (area 7, "Idempotent writes"): every route but a `GET` honours one a client sends; an API
    /// that says `Idempotency.required` refuses a write without one.
    Idempotency: "Idempotency" {
        /// A client may send the key; the default.
        Optional: "optional",
        /// A write without the key is refused.
        Required: "required",
    }
}

std_enum! {
    /// What a route says about signing in (area 8, "Routes: signed in by
    /// default"): every route needs a signed-in caller unless it says one
    /// of these.
    RouteAuthentication: "RouteAuthentication" {
        /// Anyone may call it; no credential is read.
        None: "none",
        /// The caller's record is checked now, even with a revocation
        /// delay.
        Fresh: "fresh",
    }
}

std_enum! {
    /// The ways a caller can sign in (area 8, "Ways to sign in"), each one
    /// named block in `authentication.jadpo`; an API lists the ones it
    /// accepts.
    AuthenticationMethod: "AuthenticationMethod" {
        /// A session cookie, for the application's own pages.
        BrowserSession: "browser_session",
        /// A bearer token, for the application's own mobile and
        /// command-line clients.
        ApiToken: "api_token",
        /// A bearer token another service issued (OpenID Connect).
        ExternalToken: "external_token",
        /// A service's key.
        ServiceKey: "service_key",
        /// A token the application issued as an OAuth provider.
        OauthToken: "oauth_token",
    }
}

std_enum! {
    /// Where the address of the client a request came from is read (area
    /// 10, "Client addresses"): the connection's own, or a header a proxy
    /// of the deployment's own adds.
    ClientAddressSource: "ClientAddressSource" {
        /// The connection's own address; no header is read.
        Connection: "connection",
        /// `X-Forwarded-For`.
        ForwardedFor: "forwarded_for",
        /// RFC 7239's `Forwarded: for=`.
        Forwarded: "forwarded",
        /// Cloudflare's `CF-Connecting-IP`.
        Cloudflare: "cloudflare",
    }
}

std_enum! {
    /// How strongly a caller proved who they are (area 8, "The principal").
    AuthenticationStrength: "AuthenticationStrength" {
        /// One factor: a session, a token or a key.
        Primary: "primary",
        /// More than one factor.
        MultiFactor: "multi_factor",
    }
}

std_enum! {
    /// The ways a person signs in that `sign_in:` and `authentication { }`
    /// declare (area 8, "A person the app has not seen"): what
    /// `FirstSignIn.method` says. A provider's value comes with providers.
    SignInMethod: "SignInMethod" {
        /// A magic link or its code.
        MagicLink: "magic_link",
        /// A passkey.
        Passkey: "passkey",
        /// A token from another provider (`external_token:`).
        ExternalToken: "external_token",
    }
}

std_enum! {
    /// Why a sign-in was refused (area 8, "Events"): what `SignInRefused`
    /// says and the log keeps, while the person sees only the refusal's
    /// own failure.
    SignInRefusal: "SignInRefusal" {
        /// The link expired, was used, was replaced or isn't one.
        LinkExpired: "link_expired",
        /// A `same_browser:` link opened in another browser.
        LinkOtherBrowser: "link_other_browser",
        /// The code was wrong, used, ended, expired or replaced.
        CodeInvalid: "code_invalid",
        /// The address is past the day's wrong codes.
        CodesDisabled: "codes_disabled",
        /// `on_first_sign_in:`'s action refused, or there is none.
        RefusedByApp: "refused_by_app",
        /// The user's `active:` doesn't hold.
        UserDisabled: "user_disabled",
        /// A passkey's challenge expired or was used.
        PasskeyChallengeExpired: "passkey_challenge_expired",
        /// A passkey's answer didn't check.
        PasskeyRejected: "passkey_rejected",
    }
}

std_enum! {
    /// The callers a grant can name besides a user field and a membership
    /// role (area 9, "Anyone signed in, or anyone").
    Access: "Access" {
        /// Anyone signed in, a user or a service.
        Authenticated: "authenticated",
        /// Anyone at all, signed in or not.
        Public: "public",
        /// Schedules and subscribers (area 12), within their event's
        /// tenant.
        System: "system",
    }
}

std_enum! {
    /// What an entity's `tenant:` says when it belongs to no tenant (area
    /// 9, "Tenancy").
    Tenancy: "Tenancy" {
        /// Shared by every tenant, such as a list of countries.
        Shared: "shared",
    }
}

std_enum! {
    /// What a field's `policy:` says when no client may read or write it
    /// (area 9, "Field restrictions").
    FieldAccess: "FieldAccess" {
        /// No client: no request reads or writes it.
        ServerOnly: "server_only",
    }
}

std_enum! {
    /// What a membership's `scope:` says when the role is held across the
    /// whole application (area 9, "A membership role").
    MembershipScope: "MembershipScope" {
        /// The whole application, such as staff.
        Application: "application",
    }
}

std_enum! {
    /// How many places `text.format` shows an amount of money with (area
    /// 19, "Money and payments").
    MoneyPrecision: "MoneyPrecision" {
        /// Every minor unit: the currency's places, `£1,234.50`.
        Minor: "minor",
        /// Whole major units, a half rounded away from zero: `£1,235`.
        Whole: "whole",
    }
}

std_enum! {
    /// How `text.format` names an amount's currency (area 19, "Money and
    /// payments").
    MoneyDisplay: "MoneyDisplay" {
        /// The locale's symbol: `£`, `€`, `US$`.
        Symbol: "symbol",
        /// The ISO 4217 code: `GBP`.
        Code: "code",
        /// The locale's name for it, in the amount's plural form: `British
        /// pounds`.
        Name: "name",
    }
}

std_enum! {
    /// How `text.format` writes a negative amount (area 19, "Money and
    /// payments").
    MoneySign: "MoneySign" {
        /// With the locale's minus sign: `-£5.00`.
        Standard: "standard",
        /// As the locale's accounts write it: `(£5.00)` in English.
        Accounting: "accounting",
    }
}

std_enum! {
    /// Another code system a closed list is written in (area 19, "Closed value sets are enums"): `text.parse`'s and `text.from`'s `form:`, beside the list's own
    /// code.
    CodeForm: "CodeForm" {
        /// Three letters: ISO 3166-1 alpha-3 for a country (`GBR`), ISO
        /// 639-2/T for a language (`eng`).
        Alpha3: "alpha3",
        /// Digits: ISO 3166-1 numeric for a country (`826`), UN M49 for a
        /// continent (`150`), ISO 15924's number for a script (`215`).
        Numeric: "numeric",
        /// An ISBN-10 (ISO 2108 before 2007), nine digits and a mod-11 check
        /// character, `0306406152`: read as its `Isbn`, never written
        /// (chunk 5.5m2).
        Isbn10: "isbn10",
    }
}

std_enum! {
    /// How a vector type's vectors are compared (area 21, "Storing a
    /// vector"; decision 14): a property of the model that made them, so
    /// `distance:` sets it on the type and every comparison and index
    /// follows it. Smaller is always nearer.
    VectorDistance: "VectorDistance" {
        /// One less the cosine of the angle between them, pgvector's
        /// `<=>`: most text embeddings. The default.
        Cosine: "cosine",
        /// The straight-line distance, pgvector's `<->`.
        Euclidean: "euclidean",
        /// The negative inner product, pgvector's `<#>`: for normalised
        /// vectors.
        InnerProduct: "inner_product",
    }
}

std_enum! {
    /// The language an entity's `search:` reads its fields' words in
    /// (area 16, "Search"): PostgreSQL 16's text search configuration of
    /// the same name, which splits text into words, drops the language's
    /// stop words and reduces each word to its stem, so "running" finds
    /// "runs". `simple` drops nothing and stems nothing.
    SearchLanguage: "SearchLanguage" {
        Arabic: "arabic",
        Armenian: "armenian",
        Basque: "basque",
        Catalan: "catalan",
        Danish: "danish",
        Dutch: "dutch",
        English: "english",
        Finnish: "finnish",
        French: "french",
        German: "german",
        Greek: "greek",
        Hindi: "hindi",
        Hungarian: "hungarian",
        Indonesian: "indonesian",
        Irish: "irish",
        Italian: "italian",
        Lithuanian: "lithuanian",
        Nepali: "nepali",
        Norwegian: "norwegian",
        Portuguese: "portuguese",
        Romanian: "romanian",
        Russian: "russian",
        Serbian: "serbian",
        Simple: "simple",
        Spanish: "spanish",
        Swedish: "swedish",
        Tamil: "tamil",
        Turkish: "turkish",
        Yiddish: "yiddish",
    }
}

impl VectorDistance {
    /// pgvector's operator for it.
    pub fn operator(self) -> &'static str {
        match self {
            VectorDistance::Cosine => "<=>",
            VectorDistance::Euclidean => "<->",
            VectorDistance::InnerProduct => "<#>",
        }
    }

    /// pgvector's HNSW operator class for it on a `vector` column.
    pub fn operator_class(self) -> &'static str {
        match self {
            VectorDistance::Cosine => "vector_cosine_ops",
            VectorDistance::Euclidean => "vector_l2_ops",
            VectorDistance::InnerProduct => "vector_ip_ops",
        }
    }

    /// pgvector's HNSW operator class for it on a half-precision
    /// `halfvec`, which indexes a vector of 2,001 to 4,000 dimensions
    /// (area 21, "The index"; chunk 5.5v1c).
    pub fn half_operator_class(self) -> &'static str {
        match self {
            VectorDistance::Cosine => "halfvec_cosine_ops",
            VectorDistance::Euclidean => "halfvec_l2_ops",
            VectorDistance::InnerProduct => "halfvec_ip_ops",
        }
    }
}

impl HttpMethod {
    /// As a route entry and HTTP write it: `GET`.
    pub fn word(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
        }
    }

    pub fn from_word(word: &str) -> Option<HttpMethod> {
        HttpMethod::ALL
            .iter()
            .copied()
            .find(|method| method.word() == word)
    }

    /// Whether its input comes from the query string (`GET` and
    /// `DELETE`), rather than the JSON body (area 7, "One input").
    pub fn reads_query(self) -> bool {
        matches!(self, HttpMethod::Get | HttpMethod::Delete)
    }
}

/// Every enum declared here, by name, with its variants in order: the
/// ones Rust interprets, then `Zone`, `Locale`, `FileType` and `Currency`
/// from their tables, every other standard enum from its data entry (the
/// closed lists of chunk 5.5l), then each quantity's unit enum from its
/// units.
pub fn enums() -> Vec<(&'static str, Vec<&'static str>)> {
    let names = |list: &'static [(&'static str, &'static str)]| {
        list.iter().map(|(variant, _)| *variant).collect()
    };
    let mut all: Vec<(&'static str, Vec<&'static str>)> = vec![
        (Rounding::NAME, Rounding::NAMES.to_vec()),
        (TextFormat::NAME, TextFormat::NAMES.to_vec()),
        (UuidVersion::NAME, UuidVersion::NAMES.to_vec()),
        (Weekday::NAME, Weekday::NAMES.to_vec()),
        (Month::NAME, Month::NAMES.to_vec()),
        (TextDirection::NAME, TextDirection::NAMES.to_vec()),
        (TranslationReview::NAME, TranslationReview::NAMES.to_vec()),
        (BatchState::NAME, BatchState::NAMES.to_vec()),
        (CsvFormat::NAME, CsvFormat::NAMES.to_vec()),
        (Dst::NAME, Dst::NAMES.to_vec()),
        (InvalidDay::NAME, InvalidDay::NAMES.to_vec()),
        (ComponentStyle::NAME, ComponentStyle::NAMES.to_vec()),
        (Generated::NAME, Generated::NAMES.to_vec()),
        (OnDelete::NAME, OnDelete::NAMES.to_vec()),
        (Erasure::NAME, Erasure::NAMES.to_vec()),
        (AuditChange::NAME, AuditChange::NAMES.to_vec()),
        (AuditBy::NAME, AuditBy::NAMES.to_vec()),
        (IndexKind::NAME, IndexKind::NAMES.to_vec()),
        (FieldChange::NAME, FieldChange::NAMES.to_vec()),
        (Driver::NAME, Driver::NAMES.to_vec()),
        (DeployTarget::NAME, DeployTarget::NAMES.to_vec()),
        (Trigger::NAME, Trigger::NAMES.to_vec()),
        (ScheduleTrigger::NAME, ScheduleTrigger::NAMES.to_vec()),
        (
            ProviderEnvironment::NAME,
            ProviderEnvironment::NAMES.to_vec(),
        ),
        (SuccessStatus::NAME, SuccessStatus::NAMES.to_vec()),
        (HttpMethod::NAME, HttpMethod::NAMES.to_vec()),
        (Idempotency::NAME, Idempotency::NAMES.to_vec()),
        (
            RouteAuthentication::NAME,
            RouteAuthentication::NAMES.to_vec(),
        ),
        (
            AuthenticationMethod::NAME,
            AuthenticationMethod::NAMES.to_vec(),
        ),
        (
            AuthenticationStrength::NAME,
            AuthenticationStrength::NAMES.to_vec(),
        ),
        (SignInMethod::NAME, SignInMethod::NAMES.to_vec()),
        (SignInRefusal::NAME, SignInRefusal::NAMES.to_vec()),
        (
            ClientAddressSource::NAME,
            ClientAddressSource::NAMES.to_vec(),
        ),
        (Access::NAME, Access::NAMES.to_vec()),
        (Tenancy::NAME, Tenancy::NAMES.to_vec()),
        (MembershipScope::NAME, MembershipScope::NAMES.to_vec()),
        (FieldAccess::NAME, FieldAccess::NAMES.to_vec()),
        (MoneyPrecision::NAME, MoneyPrecision::NAMES.to_vec()),
        (MoneyDisplay::NAME, MoneyDisplay::NAMES.to_vec()),
        (MoneySign::NAME, MoneySign::NAMES.to_vec()),
        (ZONE, names(crate::zones::ZONES)),
        (LOCALE, names(crate::zones::LOCALES)),
        (crate::FILE_TYPE, names(crate::FILE_TYPES)),
        (crate::CURRENCY, names(crate::CURRENCIES)),
        (CodeForm::NAME, CodeForm::NAMES.to_vec()),
        (VectorDistance::NAME, VectorDistance::NAMES.to_vec()),
        (SearchLanguage::NAME, SearchLanguage::NAMES.to_vec()),
    ];
    // The standard enums, and the choices, whose variants carry data
    // (chunk 5.5i3).
    for entry in crate::STANDARD_TYPES {
        if let Some(variants) = entry.variants()
            && !all.iter().any(|(name, _)| *name == entry.name)
        {
            all.push((entry.name, variants));
        }
    }
    // Each quantity's units (area 19, "Physical quantities"): `LengthUnit`.
    all.extend(crate::QuantityKind::ALL.into_iter().map(|kind| {
        (
            kind.unit_enum(),
            kind.units().iter().map(|unit| unit.name).collect(),
        )
    }));
    all
}

/// Where variant `variant` stands among enum `name`'s variants, from 0, in
/// the order they are declared: what a set orders them by (area 2, "Set
/// order").
pub fn variant_ordinal(name: &str, variant: &str) -> Option<u32> {
    let position = |names: &[&str]| names.iter().position(|v| *v == variant);
    let found = match name {
        Rounding::NAME => position(Rounding::NAMES),
        TextFormat::NAME => position(TextFormat::NAMES),
        UuidVersion::NAME => position(UuidVersion::NAMES),
        Weekday::NAME => position(Weekday::NAMES),
        Month::NAME => position(Month::NAMES),
        TextDirection::NAME => position(TextDirection::NAMES),
        TranslationReview::NAME => position(TranslationReview::NAMES),
        BatchState::NAME => position(BatchState::NAMES),
        CsvFormat::NAME => position(CsvFormat::NAMES),
        Dst::NAME => position(Dst::NAMES),
        InvalidDay::NAME => position(InvalidDay::NAMES),
        ComponentStyle::NAME => position(ComponentStyle::NAMES),
        Generated::NAME => position(Generated::NAMES),
        OnDelete::NAME => position(OnDelete::NAMES),
        Erasure::NAME => position(Erasure::NAMES),
        AuditChange::NAME => position(AuditChange::NAMES),
        AuditBy::NAME => position(AuditBy::NAMES),
        IndexKind::NAME => position(IndexKind::NAMES),
        FieldChange::NAME => position(FieldChange::NAMES),
        Driver::NAME => position(Driver::NAMES),
        DeployTarget::NAME => position(DeployTarget::NAMES),
        Trigger::NAME => position(Trigger::NAMES),
        ScheduleTrigger::NAME => position(ScheduleTrigger::NAMES),
        ProviderEnvironment::NAME => position(ProviderEnvironment::NAMES),
        SuccessStatus::NAME => position(SuccessStatus::NAMES),
        HttpMethod::NAME => position(HttpMethod::NAMES),
        Idempotency::NAME => position(Idempotency::NAMES),
        RouteAuthentication::NAME => position(RouteAuthentication::NAMES),
        AuthenticationMethod::NAME => position(AuthenticationMethod::NAMES),
        AuthenticationStrength::NAME => position(AuthenticationStrength::NAMES),
        SignInMethod::NAME => position(SignInMethod::NAMES),
        SignInRefusal::NAME => position(SignInRefusal::NAMES),
        ClientAddressSource::NAME => position(ClientAddressSource::NAMES),
        Access::NAME => position(Access::NAMES),
        Tenancy::NAME => position(Tenancy::NAMES),
        MembershipScope::NAME => position(MembershipScope::NAMES),
        FieldAccess::NAME => position(FieldAccess::NAMES),
        MoneyPrecision::NAME => position(MoneyPrecision::NAMES),
        MoneyDisplay::NAME => position(MoneyDisplay::NAMES),
        MoneySign::NAME => position(MoneySign::NAMES),
        ZONE => crate::zones::ZONES.iter().position(|(v, _)| *v == variant),
        LOCALE => crate::zones::LOCALES
            .iter()
            .position(|(v, _)| *v == variant),
        crate::FILE_TYPE => crate::FILE_TYPES.iter().position(|(v, _)| *v == variant),
        crate::CURRENCY => crate::CURRENCIES.iter().position(|(v, _)| *v == variant),
        CodeForm::NAME => position(CodeForm::NAMES),
        VectorDistance::NAME => position(VectorDistance::NAMES),
        SearchLanguage::NAME => position(SearchLanguage::NAMES),
        other => crate::QuantityKind::of_unit_enum(other)
            .and_then(|kind| kind.units().iter().position(|unit| unit.name == variant))
            .or_else(|| {
                crate::standard_type(other)
                    .and_then(crate::StandardType::variants)
                    .and_then(|variants| variants.iter().position(|v| *v == variant))
            }),
    };
    found.and_then(|at| u32::try_from(at).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_stand_in_declared_order() {
        assert_eq!(variant_ordinal(Weekday::NAME, "monday"), Some(0));
        assert_eq!(variant_ordinal(Weekday::NAME, "sunday"), Some(6));
        assert_eq!(variant_ordinal(Rounding::NAME, "up"), Some(2));
        assert!(variant_ordinal(ZONE, "utc").is_some());
        assert_eq!(variant_ordinal(Weekday::NAME, "someday"), None);
        assert_eq!(variant_ordinal("Shade", "plain"), None);
    }

    #[test]
    fn variants_read_back_by_name() {
        assert_eq!(Rounding::from_name("half_up"), Some(Rounding::HalfUp));
        assert_eq!(TextFormat::from_name("email"), Some(TextFormat::Email));
        assert_eq!(
            UuidVersion::from_name("v7").map(UuidVersion::digit),
            Some(7)
        );
        assert_eq!(UuidVersion::NAMES.len(), 8);
        assert_eq!(Rounding::from_name("down"), Some(Rounding::Down));
        assert_eq!(Rounding::from_name("floor"), None);
        let all = enums();
        let zone = all.iter().find(|(name, _)| *name == ZONE).expect("Zone");
        assert!(zone.1.contains(&"europe_london") && zone.1.contains(&"utc"));
        let mut sorted = zone.1.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, zone.1, "zones sorted, each once");
        let locale = all
            .iter()
            .find(|(name, _)| *name == LOCALE)
            .expect("Locale");
        for tag in ["en_gb", "en_us", "fr_fr", "fr", "zh_hant_tw"] {
            assert!(locale.1.contains(&tag), "{tag}");
        }
        let mut sorted = locale.1.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, locale.1, "locales sorted, each once");
    }
}
