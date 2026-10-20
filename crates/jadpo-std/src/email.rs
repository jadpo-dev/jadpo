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

/// The port a connection with `driver: Driver.smtp` reaches its server on
/// when it gives none: submission, which Cloudflare Workers allows (area
/// 16, "Email").
pub const SMTP_PORT: u16 = 587;

/// Every email the application sends, and how.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// Each connection with `driver: Driver.smtp`, in name order.
    pub smtp: Vec<SmtpConnection>,
}

impl EmailsPlan {
    /// The email whose type has identity key `key`.
    pub fn email(&self, key: &str) -> Option<&EmailPlan> {
        self.emails.iter().find(|email| email.key == key)
    }

    /// The SMTP connection called `name`.
    pub fn smtp(&self, name: &str) -> Option<&SmtpConnection> {
        self.smtp.iter().find(|connection| connection.name == name)
    }
}

/// Where `email: { from: }` comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sender {
    /// `config.<name>`, by the setting's number.
    Setting(u32),
    /// An address written in `app.jadpo`.
    Address(String),
}

/// One email type: how its delivery renders it.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

/// A connection with `driver: Driver.smtp` (area 11's settings, chosen in
/// chunk 7.3): what a host builds its SMTP driver from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmtpConnection {
    pub name: String,
    /// The server's name: `host:`.
    pub host: SmtpHost,
    /// `port:`, [`SMTP_PORT`] when it gives none.
    pub port: u16,
    /// `credential: { username: password: }`, by their settings' numbers.
    pub username: Option<u32>,
    pub password: Option<u32>,
}

/// Where an SMTP server's name comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
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
