//! The standard library's enums that Rust code interprets, declared once:
//! the checker builds the Jadpo enums of the same names from these lists,
//! so a variant added here exists for programs and for the runtime alike.

macro_rules! std_enum {
    (
        $(#[$meta:meta])*
        $name:ident: $written:literal { $($(#[$vmeta:meta])* $variant:ident: $word:literal,)* }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// How a division that doesn't come out exact is rounded (area 3,
    /// question 7).
    Rounding: "Rounding" {
        /// To the nearest; a tie goes to the even neighbour.
        HalfEven: "half_even",
        /// To the nearest; a tie goes away from zero.
        HalfUp: "half_up",
        /// Away from zero.
        Up: "up",
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
    /// Which instant a local time takes when a zone's clocks change around
    /// it (area 10, question 7): an `overlap` happens twice, a `gap` never.
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
    HttpMethod: "HttpMethod" {
        Get: "get",
        Post: "post",
        Put: "put",
        Patch: "patch",
        Delete: "delete",
    }
}

std_enum! {
    /// Whether an API's writes need an `Idempotency-Key` (area 7, question
    /// 17): every route but a `GET` honours one a client sends; an API
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
/// ones Rust interprets, then `Zone` and `Locale` from their tables.
pub fn enums() -> Vec<(&'static str, Vec<&'static str>)> {
    let names = |list: &'static [(&'static str, &'static str)]| {
        list.iter().map(|(variant, _)| *variant).collect()
    };
    vec![
        (Rounding::NAME, Rounding::NAMES.to_vec()),
        (TextFormat::NAME, TextFormat::NAMES.to_vec()),
        (UuidVersion::NAME, UuidVersion::NAMES.to_vec()),
        (Weekday::NAME, Weekday::NAMES.to_vec()),
        (TextDirection::NAME, TextDirection::NAMES.to_vec()),
        (TranslationReview::NAME, TranslationReview::NAMES.to_vec()),
        (Dst::NAME, Dst::NAMES.to_vec()),
        (InvalidDay::NAME, InvalidDay::NAMES.to_vec()),
        (ComponentStyle::NAME, ComponentStyle::NAMES.to_vec()),
        (Generated::NAME, Generated::NAMES.to_vec()),
        (OnDelete::NAME, OnDelete::NAMES.to_vec()),
        (FieldChange::NAME, FieldChange::NAMES.to_vec()),
        (Driver::NAME, Driver::NAMES.to_vec()),
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
        (
            ClientAddressSource::NAME,
            ClientAddressSource::NAMES.to_vec(),
        ),
        (Access::NAME, Access::NAMES.to_vec()),
        (Tenancy::NAME, Tenancy::NAMES.to_vec()),
        (MembershipScope::NAME, MembershipScope::NAMES.to_vec()),
        (FieldAccess::NAME, FieldAccess::NAMES.to_vec()),
        (ZONE, names(crate::zones::ZONES)),
        (LOCALE, names(crate::zones::LOCALES)),
    ]
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
        TextDirection::NAME => position(TextDirection::NAMES),
        TranslationReview::NAME => position(TranslationReview::NAMES),
        Dst::NAME => position(Dst::NAMES),
        InvalidDay::NAME => position(InvalidDay::NAMES),
        ComponentStyle::NAME => position(ComponentStyle::NAMES),
        Generated::NAME => position(Generated::NAMES),
        OnDelete::NAME => position(OnDelete::NAMES),
        FieldChange::NAME => position(FieldChange::NAMES),
        Driver::NAME => position(Driver::NAMES),
        SuccessStatus::NAME => position(SuccessStatus::NAMES),
        HttpMethod::NAME => position(HttpMethod::NAMES),
        Idempotency::NAME => position(Idempotency::NAMES),
        RouteAuthentication::NAME => position(RouteAuthentication::NAMES),
        AuthenticationMethod::NAME => position(AuthenticationMethod::NAMES),
        AuthenticationStrength::NAME => position(AuthenticationStrength::NAMES),
        ClientAddressSource::NAME => position(ClientAddressSource::NAMES),
        Access::NAME => position(Access::NAMES),
        Tenancy::NAME => position(Tenancy::NAMES),
        MembershipScope::NAME => position(MembershipScope::NAMES),
        FieldAccess::NAME => position(FieldAccess::NAMES),
        ZONE => crate::zones::ZONES.iter().position(|(v, _)| *v == variant),
        LOCALE => crate::zones::LOCALES
            .iter()
            .position(|(v, _)| *v == variant),
        _ => None,
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
        assert_eq!(Rounding::from_name("down"), None);
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
