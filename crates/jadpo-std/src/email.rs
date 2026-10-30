//! The `email` capability's plan (area 16, "Email"; chunk 7.3): plain data
//! `jadpo-plan` decides and the runtime sends from, so both executors send
//! an email the same way.
//!
//! An email is emitted as an event is, and its delivery hands it to the
//! connection `application { email: }` names once its action commits. At
//! send, the runtime renders it whole: the callables its `.html` and
//! `.txt` templates became, given its fields, each wrapped in the shared
//! layout's; its subject, its member in its domain's language files, in
//! its locale; and each `File` field read from its store as an
//! attachment. The connection's driver is sent the message
//! ([`MESSAGE_FIELDS`]) and answers whether it went.

use crate::LocalePlan;
use crate::translation::TranslationEntry;

/// The operation a connection that fills `email` is sent a message with.
pub const SEND: &str = "send";

/// The generated namespace a test reads the mail it sent through:
/// `email.sent` (area 16, "Email"; chunk 7.3c).
pub const NAMESPACE: &str = "email";

/// The built-in type each email `email.sent` lists is.
pub const SENT_EMAIL: &str = "SentEmail";

/// What a `SentEmail` holds, in order: the address it went to, the one it
/// is from, its subject, its HTML and its text (each `none` without that
/// template), every link in it, each attachment's file name, and, for the
/// sign-in email, the link's token and the code it carries (`none` for
/// any other), which a route test posts to the confirmation (area 8, "In
/// development and tests").
pub const SENT_EMAIL_FIELDS: [&str; 9] = [
    "to",
    "from",
    "subject",
    "html",
    "text",
    "links",
    "attachments",
    "sign_in_token",
    "sign_in_code",
];

/// The fields a caught sign-in email's message carries beside its parts,
/// only where the host catches the mail in place of sending it: the
/// link's token and the code, read back as `SentEmail`'s.
pub const SIGN_IN_TOKEN: &str = "sign_in_token";
pub const SIGN_IN_CODE: &str = "sign_in_code";

/// The fields of the message the runtime sends a driver, in order:
/// `from`, `to`, `subject`, `html` (`none` without an `.html` template),
/// `text` (`none` without a `.txt` template) and `attachments`, each a
/// record of [`ATTACHMENT_FIELDS`].
pub const MESSAGE_FIELDS: [&str; 6] = ["from", "to", "subject", "html", "text", "attachments"];

/// The type name the message's record carries.
pub const MESSAGE: &str = "EmailMessage";

/// One attachment's fields: its file name, its media type and its bytes.
pub const ATTACHMENT_FIELDS: [&str; 3] = ["name", "content_type", "content"];

/// The type name an attachment's record carries.
pub const ATTACHMENT: &str = "EmailAttachment";

/// The failure a driver answers with when the server refused the
/// address it was sent to for good (area 11's `EmailFailure`).
pub const RECIPIENT_REJECTED: &str = "recipient_rejected";

/// The failure a driver answers with when the message itself was refused
/// for good (area 11's `EmailFailure`).
pub const MESSAGE_REJECTED: &str = "message_rejected";

/// The port a connection with `driver: Driver.smtp` reaches its server on
/// when it gives none: submission, which Cloudflare Workers allows (area
/// 16, "Email").
pub const SMTP_PORT: u16 = 587;

/// Every email the application sends, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EmailsPlan {
    /// The connection `application { email: { connection: } }` names.
    pub connection: Option<String>,
    /// Who every email is from: `email: { from: }`.
    pub from: Option<Sender>,
    /// Each email type, in key order.
    pub emails: Vec<EmailPlan>,
    /// The shared layout's callables (`shared/emails/layout.html` and
    /// `layout.txt`), each given the rendered body, the subject and the
    /// locale.
    pub layout_html: Option<u32>,
    pub layout_text: Option<u32>,
    /// The locales the application supports, which a subject's wording
    /// falls back through.
    pub locales: Option<LocalePlan>,
    /// How the mail package `connection` names sends a message, when it
    /// is a package's (`provides/email.jadpo`; chunk 7.3c).
    pub package: Option<PackageEmail>,
}

/// A mail package's `provide email` (area 11, "How a package provides
/// one"; chunk 7.3c): the operation a message is sent through, its input
/// built from the message as data, where the provider's id for it is, and
/// how its failures are an email's.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackageEmail {
    /// `emails.send`.
    pub operation: String,
    /// Its number in the plan's provider operations (chunk 8.1d2).
    pub called: u32,
    /// Each field of its input, by its spelling on the wire.
    pub input: Vec<(String, MailInput)>,
    /// Where the provider's id for the message is in its answer, by field
    /// names.
    pub receipt: Option<Vec<String>>,
    /// Each of the operation's failures, by its code, and the email failure
    /// ([`RECIPIENT_REJECTED`] or [`MESSAGE_REJECTED`]) it is.
    pub failures: Vec<(String, String)>,
    /// The total size of attachments the provider accepts, in bytes: a
    /// message over it is refused before it goes out.
    pub attachments: u64,
}

/// What a mail package's mapping gives one field of its operation's input.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MailInput {
    /// A part of the message: `from`, `to`, `subject`, `html` or `text`.
    Message(String),
    /// A part of one attachment: `name`, `content` or `content_type`.
    Item(String),
    /// The value of the setting numbered here, from the connection's
    /// setting of the package.
    Setting(u32),
    Text(String),
    Int(i64),
    Bool(bool),
    /// An object's fields, by their wire names.
    Object(Vec<(String, MailInput)>),
    /// The attachments, each as an object of these fields.
    Each(Vec<(String, MailInput)>),
}

impl EmailsPlan {
    /// The email whose type has identity key `key`.
    pub fn email(&self, key: &str) -> Option<&EmailPlan> {
        self.emails.iter().find(|email| email.key == key)
    }
}

/// Where `email: { from: }` comes from.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Sender {
    /// `config.<name>`, by the setting's number.
    Setting(u32),
    /// An address written in `app.jadpo`.
    Address(String),
}

/// One email type: how its delivery renders it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EmailPlan {
    /// Its type's identity key, which its outbox rows name.
    pub key: String,
    /// As a log names it: `accounts.WelcomeEmail`.
    pub name: String,
    /// Its fields, `to` and `locale` first: what each template callable
    /// is given, in this order, read from the emitted value by name.
    pub fields: Vec<String>,
    /// The callables its `.html` and `.txt` templates became.
    pub html: Option<u32>,
    pub text: Option<u32>,
    /// Its subject: its member in its domain's language files, its
    /// parameters its fields the wording may write, given by name.
    pub subject: Option<TranslationEntry>,
    /// Its `File` fields, each an attachment, in order.
    pub attachments: Vec<String>,
    /// For one of Jadpo's own emails (area 16, "Built-in emails"), how
    /// Jadpo writes it; `None` for the application's.
    pub builtin: Option<BuiltinEmail>,
}

/// The identity key Jadpo's own magic-link email's outbox rows name, and
/// the name its delivery's subscriber is known by (area 16, "Built-in
/// emails"; chunk 7.3b).
pub const MAGIC_LINK_EMAIL: &str = "jadpo.MagicLinkEmail";

/// The fields of the magic-link email's outbox payload, in order: the
/// address it goes to, its locale, and the sign-in request whose link and
/// code it carries, which are made as it is sent and kept only as
/// verifiers, so the outbox never holds them.
pub const MAGIC_LINK_EMAIL_FIELDS: [&str; 3] = ["to", "locale", "request"];

/// The built-in type the application's own sign-in email takes its
/// fields from with `pick:` (area 16, "Built-in emails", level 4): the
/// address, the link, the code and how long both work, which Jadpo fills.
/// App code never makes or emits one.
pub const MAGIC_LINK_EMAIL_TYPE: &str = "MagicLinkEmail";

/// `MagicLinkEmail`'s fields, in order, each with the built-in type it
/// is.
pub const MAGIC_LINK_EMAIL_TYPE_FIELDS: [(&str, crate::Builtin); 4] = [
    ("address", crate::Builtin::Email),
    ("sign_in_link", crate::Builtin::Url),
    ("code", crate::Builtin::SignInCode),
    ("expires_in", crate::Builtin::PresentationText),
];

/// The built-in type the action `magic_link: { send: }` names is given
/// (area 16, "Built-in emails", level 5): the address, the link, the code,
/// when both stop working and the locale to write in, made as it runs.
pub const MAGIC_LINK_REQUEST: &str = "MagicLinkRequest";

/// `MagicLinkRequest`'s fields, in order.
pub const MAGIC_LINK_REQUEST_FIELDS: [&str; 5] =
    ["address", "sign_in_link", "code", "expires_at", "locale"];

/// One of Jadpo's own emails, which it writes itself.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BuiltinEmail {
    /// The magic-link email (area 8, "Magic links"), at level 1: Jadpo's
    /// HTML and text in its own wording, which the application may
    /// reword (level 2), wrapped in the application's layout (level 3).
    MagicLink(MagicLinkEmailPlan),
}

/// How Jadpo writes and sends its magic-link email.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MagicLinkEmailPlan {
    /// The store `jadpo_sign_in_requests` is in: the user entity's.
    pub store: String,
    /// The setting the browser session's `origin:` reads, which the link
    /// starts with.
    pub origin: u32,
    /// The frontend path the link opens.
    pub link: String,
    /// How long the link and the code work, in milliseconds.
    pub expires_after_ms: i64,
    /// `application { display_name: }`, which Jadpo's wording names the
    /// application by.
    pub app_name: String,
    /// Jadpo's messages the email is written with, by key
    /// (`jadpo.magic_link_heading`), each given its fields by name and
    /// then `app_name`.
    pub messages: Vec<TranslationEntry>,
    /// The locales its wording falls back through: the application's, or
    /// `en_gb` alone when it declares none.
    pub locales: LocalePlan,
    /// `email:`, the application's own sign-in email (area 16, "Built-in
    /// emails", level 4), by the key of its [`EmailPlan`]: Jadpo fills it
    /// from the link and the code it makes, and sends it in place of its
    /// own.
    pub own: Option<String>,
    /// `send:`, the action that sends the email itself (level 5), by
    /// callable number: given a `MagicLinkRequest` in place of Jadpo
    /// sending anything, run as a subscriber's action is.
    pub send: Option<u32>,
}

impl MagicLinkEmailPlan {
    /// The message called `key`, without its `jadpo.`.
    pub fn message(&self, key: &str) -> Option<&TranslationEntry> {
        self.messages
            .iter()
            .find(|entry| entry.key.strip_prefix("jadpo.") == Some(key))
    }
}

/// Where an SMTP server's name comes from.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SmtpHost {
    /// `config.<name>`, by the setting's number.
    Setting(u32),
    /// A name written in `app.jadpo`.
    Written(String),
}

/// `text` as it goes into an email's HTML template: `&`, `<`, `>`, `"`
/// and `'` written as their character references, so a value is only
/// ever text there, inside an element or an attribute.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escaping_leaves_only_text() {
        assert_eq!(
            escape_html(r#"<a href="x" title='y'>Tom & Jerry</a>"#),
            "&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;Tom &amp; Jerry&lt;/a&gt;"
        );
        assert_eq!(escape_html("café"), "café");
    }
}
