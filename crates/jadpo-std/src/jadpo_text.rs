//! Jadpo's own text (area 10, "Jadpo's and packages' text"): the messages
//! Jadpo writes itself, which are its failures' wording for client apps, the
//! built-in emails' wording and the wording of its own enums. Each is a
//! member of the root `translations/<locale>.jadpo` file's `jadpo: { }`
//! group, keyed by its name, with the wording Jadpo ships as the default.
//!
//! Jadpo ships its wording in `en_gb` and `en_us`; an app that supports any
//! other locale words the messages of the features it has switched on, in
//! that group. Rewording a message of a feature that is off is an error.
//! Every message may write `{app_name}`, the application's `display_name:`.

use crate::translation::ParamKind;

/// The locales Jadpo ships wording for, as `Locale` variants.
pub const SHIPPED: [&str; 2] = ["en_gb", "en_us"];

/// The name of the owner of Jadpo's own messages in the root file.
pub const OWNER: &str = "jadpo";

/// The placeholder every Jadpo message may write: the application's
/// `display_name:`.
pub const APP_NAME: &str = "app_name";

/// What switches a group of messages on. Only the messages of the features
/// an app has on are listed, required in its locales and rewordable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Feature {
    /// Any route is served: the failures every request can meet.
    Requests,
    /// A route needs a caller: the failures of signing in.
    SignIn,
    /// The API signs users in (a browser session or an API token).
    UserSignIn,
    /// The API signs services in (a service key).
    ServiceSignIn,
    /// The API accepts browser sessions, whose writes carry a CSRF token.
    BrowserSession,
    /// The application declares its locales: the wording of `TextDirection`.
    Locales,
    /// Magic-link sign-in (area 8): its email and its failures. Not built
    /// yet, so no application has it on.
    MagicLink,
}

impl Feature {
    /// The name `jadpo translations builtin` groups the feature's messages
    /// under.
    pub fn heading(self) -> &'static str {
        match self {
            Feature::Requests => "Requests",
            Feature::SignIn => "Signing in",
            Feature::UserSignIn => "Signing users in",
            Feature::ServiceSignIn => "Signing services in",
            Feature::BrowserSession => "Browser sessions",
            Feature::Locales => "Locales",
            Feature::MagicLink => "Magic links",
        }
    }

    /// The word `--json` gives it.
    pub fn word(self) -> &'static str {
        match self {
            Feature::Requests => "requests",
            Feature::SignIn => "sign_in",
            Feature::UserSignIn => "user_sign_in",
            Feature::ServiceSignIn => "service_sign_in",
            Feature::BrowserSession => "browser_session",
            Feature::Locales => "locales",
            Feature::MagicLink => "magic_link",
        }
    }

    /// What switches it on, in a few words.
    pub fn when(self) -> &'static str {
        match self {
            Feature::Requests => "an API serves routes",
            Feature::SignIn => "a route needs a caller",
            Feature::UserSignIn => "an API signs users in",
            Feature::ServiceSignIn => "an API signs services in",
            Feature::BrowserSession => "an API accepts browser sessions",
            Feature::Locales => "the application declares its locales",
            Feature::MagicLink => "the application signs people in with magic links",
        }
    }

    pub const ALL: [Feature; 7] = [
        Feature::Requests,
        Feature::SignIn,
        Feature::UserSignIn,
        Feature::ServiceSignIn,
        Feature::BrowserSession,
        Feature::Locales,
        Feature::MagicLink,
    ];
}

/// What a message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Text Jadpo writes for people, such as an email's line.
    Message,
    /// A failure's text for client apps, keyed by its code.
    Failure,
    /// The wording of one of Jadpo's enums, one for each value.
    Enum,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Message => "message",
            Kind::Failure => "failure",
            Kind::Enum => "enum",
        }
    }
}

/// How a message is worded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One wording.
    Plain(&'static str),
    /// One wording for each value of an enum, by the value's name.
    Variants(&'static [(&'static str, &'static str)]),
}

/// Where Jadpo ships a message's wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ships {
    /// In [`SHIPPED`]'s locales.
    English,
    /// In every locale, because the wording is the same in all of them
    /// (the HTML values `ltr` and `rtl`).
    Everywhere,
}

/// One message of Jadpo's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    /// The member that words it: the name in lower_snake_case, or a
    /// failure's code.
    pub key: &'static str,
    pub kind: Kind,
    /// For an enum: its name.
    pub type_name: &'static str,
    pub feature: Feature,
    /// The placeholders it may write besides `{app_name}`.
    pub fields: &'static [(&'static str, ParamKind)],
    /// The `///` description a translator reads.
    pub description: &'static str,
    /// The wording Jadpo ships.
    pub shape: Shape,
    pub ships: Ships,
}

impl Message {
    /// Whether Jadpo ships wording for the locale (a `Locale` variant).
    pub fn ships_in(&self, locale: &str) -> bool {
        match self.ships {
            Ships::English => SHIPPED.contains(&locale),
            Ships::Everywhere => true,
        }
    }

    /// The placeholders a wording may write: its fields, then `app_name`.
    pub fn placeholders(&self) -> impl Iterator<Item = (&'static str, ParamKind)> {
        self.fields
            .iter()
            .copied()
            .chain(std::iter::once((APP_NAME, ParamKind::Text)))
    }
}

const fn failure(
    key: &'static str,
    feature: Feature,
    description: &'static str,
    text: &'static str,
) -> Message {
    Message {
        key,
        kind: Kind::Failure,
        type_name: "",
        feature,
        fields: &[],
        description,
        shape: Shape::Plain(text),
        ships: Ships::English,
    }
}

const fn message(
    key: &'static str,
    feature: Feature,
    fields: &'static [(&'static str, ParamKind)],
    description: &'static str,
    text: &'static str,
) -> Message {
    Message {
        key,
        kind: Kind::Message,
        type_name: "",
        feature,
        fields,
        description,
        shape: Shape::Plain(text),
        ships: Ships::English,
    }
}

/// Every message, in the order `jadpo translations builtin` lists a
/// feature's.
pub static MESSAGES: [Message; 31] = [
    failure(
        "invalid_request",
        Feature::Requests,
        "The request could not be read: it is not JSON, or a header is malformed.",
        "The request couldn't be read. Try again.",
    ),
    failure(
        "invalid_input",
        Feature::Requests,
        "Something the request sent is missing, unknown or breaks a rule.",
        "Some of the information sent isn't valid. Check it and try again.",
    ),
    failure(
        "route_not_found",
        Feature::Requests,
        "No route answers the address.",
        "We couldn't find what you were looking for.",
    ),
    failure(
        "method_not_allowed",
        Feature::Requests,
        "The address exists but does not answer this method.",
        "That isn't something this address can do.",
    ),
    failure(
        "request_too_large",
        Feature::Requests,
        "The request body is over the limit.",
        "What was sent is too big. Send less and try again.",
    ),
    failure(
        "idempotency_key_reused",
        Feature::Requests,
        "An idempotency key came again with a different request.",
        "That request was already sent with different details. Start again.",
    ),
    failure(
        "rate_limited",
        Feature::Requests,
        "The caller sent too many requests.",
        "Too many requests. Wait a moment, then try again.",
    ),
    failure(
        "unavailable",
        Feature::Requests,
        "A store or service the route needs cannot be reached.",
        "A service this depends on isn't available right now. Try again soon.",
    ),
    failure(
        "internal_fault",
        Feature::Requests,
        "The server failed to answer.",
        "Something went wrong on our side. Try again soon.",
    ),
    failure(
        "deadline_exceeded",
        Feature::Requests,
        "The route's deadline passed and nothing it did was kept.",
        "That took too long, and nothing was saved. Try again.",
    ),
    failure(
        "authentication_required",
        Feature::SignIn,
        "The request carries no credential.",
        "Sign in to continue.",
    ),
    failure(
        "invalid_credentials",
        Feature::SignIn,
        "The credential is malformed, expired or unknown.",
        "Your sign-in is invalid or has expired. Sign in again.",
    ),
    failure(
        "ambiguous_credentials",
        Feature::SignIn,
        "The request carries more than one credential.",
        "More than one way of signing in was sent. Use one.",
    ),
    failure(
        "not_permitted",
        Feature::SignIn,
        "The caller is known but may not do this.",
        "You can't do that.",
    ),
    failure(
        "authentication_unavailable",
        Feature::SignIn,
        "The store sessions are kept in cannot be reached.",
        "Signing in isn't available right now. Try again soon.",
    ),
    failure(
        "user_disabled",
        Feature::UserSignIn,
        "The signed-in user's record is not active.",
        "This account is disabled.",
    ),
    failure(
        "service_disabled",
        Feature::ServiceSignIn,
        "The signed-in service's record is not active.",
        "This service is disabled.",
    ),
    failure(
        "csrf_rejected",
        Feature::BrowserSession,
        "A write from a browser session has no matching origin or CSRF token.",
        "That request didn't come from this site. Reload the page and try again.",
    ),
    Message {
        key: "text_direction",
        kind: Kind::Enum,
        type_name: "TextDirection",
        feature: Feature::Locales,
        fields: &[],
        description: "Which way a locale's text runs, written as the HTML values for a template's `dir=` attribute.",
        shape: Shape::Variants(&[("left_to_right", "ltr"), ("right_to_left", "rtl")]),
        ships: Ships::Everywhere,
    },
    message(
        "magic_link_email",
        Feature::MagicLink,
        &[],
        "The subject of the sign-in email.",
        "Sign in to {app_name}",
    ),
    message(
        "magic_link_heading",
        Feature::MagicLink,
        &[],
        "The heading of the sign-in email.",
        "Sign in",
    ),
    message(
        "magic_link_intro",
        Feature::MagicLink,
        &[("address", ParamKind::Text)],
        "Says who asked to sign in.",
        "Someone asked to sign in as {address}.",
    ),
    message(
        "magic_link_button",
        Feature::MagicLink,
        &[],
        "The sign-in button's label.",
        "Sign in",
    ),
    message(
        "magic_link_code_line",
        Feature::MagicLink,
        &[("code", ParamKind::Text)],
        "Offers the six-digit code as another way in.",
        "Or enter this code: {code}",
    ),
    message(
        "magic_link_expiry_line",
        Feature::MagicLink,
        &[("expires_in", ParamKind::Text)],
        "Says how long the link and the code work, as Jadpo formats the time.",
        "The link and the code work for {expires_in}.",
    ),
    message(
        "magic_link_ignore_line",
        Feature::MagicLink,
        &[],
        "Tells someone who did not ask to ignore the email.",
        "If you didn't ask to sign in, you can ignore this email.",
    ),
    failure(
        "sign_in_link_expired",
        Feature::MagicLink,
        "The sign-in link has expired or been used.",
        "This link has expired or has been used. Ask for a new one.",
    ),
    failure(
        "sign_in_link_other_browser",
        Feature::MagicLink,
        "The link was opened in a browser other than the one that asked for it.",
        "Open this link in the browser where you asked for it, or enter the code instead.",
    ),
    failure(
        "sign_in_code_invalid",
        Feature::MagicLink,
        "The code entered is wrong.",
        "That code isn't right. Check it, or ask for a new one.",
    ),
    failure(
        "sign_in_codes_disabled",
        Feature::MagicLink,
        "Too many wrong codes were entered today.",
        "Too many wrong codes today. Use the link in your email instead.",
    ),
    failure(
        "sign_in_refused",
        Feature::MagicLink,
        "The sign-in was refused.",
        "We couldn't sign you in.",
    ),
];

/// The message called `key`.
pub fn message_named(key: &str) -> Option<&'static Message> {
    MESSAGES.iter().find(|message| message.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation::parse_template;

    #[test]
    fn every_shipped_wording_reads_and_writes_only_its_fields() {
        for message in &MESSAGES {
            let texts: Vec<&str> = match message.shape {
                Shape::Plain(text) => vec![text],
                Shape::Variants(variants) => variants.iter().map(|(_, text)| *text).collect(),
            };
            for text in texts {
                let (_, uses) = parse_template(text)
                    .unwrap_or_else(|error| panic!("{}: {error:?}", message.key));
                for field in uses {
                    assert!(
                        message.placeholders().any(|(name, _)| name == field.name),
                        "{} writes {{{}}}",
                        message.key,
                        field.name
                    );
                }
            }
        }
    }

    #[test]
    fn keys_are_unique_snake_case() {
        let mut seen = std::collections::BTreeSet::new();
        for message in &MESSAGES {
            assert!(seen.insert(message.key), "{} twice", message.key);
            assert!(
                message
                    .key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                message.key
            );
            assert!(!message.description.is_empty());
        }
    }

    #[test]
    fn the_shipped_locales_are_locales() {
        for tag in SHIPPED {
            assert!(crate::locale_ordinal(tag).is_some(), "{tag}");
        }
    }
}
