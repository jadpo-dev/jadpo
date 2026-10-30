//! `jadpo dev`'s inbox (area 16, "Email"; chunk 7.3c2): every mail the
//! running app sends is caught whole in place of being sent, as a test's
//! mail is ([`jadpo_runtime::capability::Capability::catches_mail`]),
//! whichever connection `email:` names, so nothing leaves the machine.
//!
//! Each mail is kept in `build/dev/inbox/` as `mail_<n>.json`, its
//! attachments' bytes beside it in `mail_<n>.files/`, written whole and
//! then renamed, so a reader never sees half of one. The inbox keeps the
//! last [`KEPT`]; `jadpo email clear` empties it, and numbers carry on
//! from the last one given, so an id never names two mails. `jadpo email`
//! and the inbox page read it from there, while `jadpo dev` runs or after.
//!
//! A mail holds its sign-in links and codes in full, as development
//! needs: the inbox is the one place they are written. `jadpo dev`'s
//! line for a caught mail names its id, its email and its address, never
//! its subject or words.
//!
//! Before a mail is kept it is run through SpamAssassin when that is
//! installed ([`crate::spam`]; chunk 7.3c3), as the SMTP driver writes
//! it, and what it found is kept with the mail.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability, MailSent, Source};
use jadpo_std::email_health::{self, Finding};
use jadpo_value::Value;

use crate::spam::{Spam, SpamFilter};

/// The inbox, relative to the project.
pub(crate) const INBOX: &str = "build/dev/inbox";
/// How many mails it keeps.
pub(crate) const KEPT: usize = 200;
/// The number the last mail was given, which carries on past a clear.
const LAST: &str = "last";

/// One attachment of a caught mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Attachment {
    pub name: String,
    pub content_type: String,
    pub size: u64,
}

/// One caught mail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mail {
    /// `mail_12`.
    pub id: String,
    pub number: u64,
    /// When it was caught, as an instant is written: `2026-10-20T14:02:31Z`.
    pub at: String,
    /// The email it was rendered from, `accounts.WelcomeEmail`.
    pub email: String,
    /// The trace of the work that sent it, 32 hex digits.
    pub trace: Option<String>,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<Attachment>,
    /// What SpamAssassin made of it.
    pub spam: Spam,
}

impl Mail {
    /// Every link in it, once each: the HTML's, then the text's.
    pub fn links(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for link in self
            .html
            .as_deref()
            .map(email_health::html_links)
            .unwrap_or_default()
            .into_iter()
            .chain(
                self.text
                    .as_deref()
                    .map(email_health::text_links)
                    .unwrap_or_default(),
            )
        {
            if !out.contains(&link) {
                out.push(link);
            }
        }
        out
    }

    /// What Jadpo's checks found in it.
    pub fn health(&self) -> Vec<Finding> {
        email_health::check_message(&self.subject, self.html.as_deref(), self.text.as_deref())
    }

    /// How many problems its health has: Jadpo's checks', and one more
    /// when SpamAssassin calls it spam.
    pub fn problems(&self) -> usize {
        self.health().len() + usize::from(self.spam.is_spam())
    }

    /// The email as people read it: its type's name, without its domain.
    pub fn email_short(&self) -> &str {
        self.email.rsplit('.').next().unwrap_or(&self.email)
    }

    fn to_json(&self) -> serde_json::Value {
        let mut json = serde_json::json!({
            "id": self.id,
            "at": self.at,
            "email": self.email,
            "trace": self.trace,
            "from": self.from,
            "to": self.to,
            "subject": self.subject,
            "html": self.html,
            "text": self.text,
            "attachments": self.attachments.iter().map(|attachment| serde_json::json!({
                "name": attachment.name,
                "contentType": attachment.content_type,
                "size": attachment.size,
            })).collect::<Vec<_>>(),
        });
        if let (Some(spam), Some(members)) = (self.spam.to_json(), json.as_object_mut()) {
            members.insert("spamassassin".to_string(), spam);
        }
        json
    }

    fn from_json(json: &serde_json::Value) -> Option<Mail> {
        let text = |name: &str| json.get(name)?.as_str().map(str::to_string);
        let id = text("id")?;
        let number = id.strip_prefix("mail_")?.parse::<u64>().ok()?;
        Some(Mail {
            id,
            number,
            at: text("at").unwrap_or_default(),
            email: text("email").unwrap_or_default(),
            trace: text("trace"),
            from: text("from").unwrap_or_default(),
            to: text("to").unwrap_or_default(),
            subject: text("subject").unwrap_or_default(),
            html: text("html"),
            text: text("text"),
            attachments: json
                .get("attachments")?
                .as_array()?
                .iter()
                .filter_map(|attachment| {
                    Some(Attachment {
                        name: attachment.get("name")?.as_str()?.to_string(),
                        content_type: attachment.get("contentType")?.as_str()?.to_string(),
                        size: attachment.get("size")?.as_u64()?,
                    })
                })
                .collect(),
            spam: Spam::from_json(json.get("spamassassin")),
        })
    }
}

fn folder(root: &Path) -> PathBuf {
    root.join(INBOX)
}

/// Every mail in the inbox, oldest first.
pub(crate) fn read_all(root: &Path) -> Vec<Mail> {
    let Ok(entries) = std::fs::read_dir(folder(root)) else {
        return Vec::new();
    };
    let mut mails: Vec<Mail> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("mail_") && name.ends_with(".json")
        })
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            Mail::from_json(&serde_json::from_str(&text).ok()?)
        })
        .collect();
    mails.sort_by_key(|mail| mail.number);
    mails
}

/// The mail `id` names: `mail_12`, or `12`.
pub(crate) fn read(root: &Path, id: &str) -> Option<Mail> {
    let id = if id.starts_with("mail_") {
        id.to_string()
    } else {
        format!("mail_{id}")
    };
    if !id
        .strip_prefix("mail_")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let text = std::fs::read_to_string(folder(root).join(format!("{id}.json"))).ok()?;
    Mail::from_json(&serde_json::from_str(&text).ok()?)
}

/// The bytes of attachment `index` (from 0) of `mail`.
pub(crate) fn attachment(root: &Path, mail: &Mail, index: usize) -> Option<Vec<u8>> {
    if index >= mail.attachments.len() {
        return None;
    }
    std::fs::read(
        folder(root)
            .join(format!("{}.files", mail.id))
            .join(index.to_string()),
    )
    .ok()
}

/// Empties the inbox: how many mails it held.
pub(crate) fn clear(root: &Path) -> std::io::Result<usize> {
    let mails = read_all(root);
    for mail in &mails {
        remove(root, &mail.id)?;
    }
    Ok(mails.len())
}

fn remove(root: &Path, id: &str) -> std::io::Result<()> {
    let folder = folder(root);
    match std::fs::remove_file(folder.join(format!("{id}.json"))) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    match std::fs::remove_dir_all(folder.join(format!("{id}.files"))) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Keeps `message`, the rendered mail a delivery handed over, sent as
/// `sent`, caught at `at_ms`, with what SpamAssassin made of it; the
/// oldest past [`KEPT`] go.
pub(crate) fn keep(
    root: &Path,
    message: &Value,
    sent: Option<&MailSent>,
    at_ms: i64,
    spam: Spam,
) -> std::io::Result<Mail> {
    let folder = folder(root);
    std::fs::create_dir_all(&folder)?;
    let last = std::fs::read_to_string(folder.join(LAST))
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let held = read_all(root);
    let number = held
        .last()
        .map_or(last, |mail| mail.number.max(last))
        .saturating_add(1);
    let id = format!("mail_{number}");
    let field = |name: &str| match message {
        Value::Record(record) => record.get(name).cloned().unwrap_or(Value::None),
        _ => Value::None,
    };
    let text_of = |value: Value| match value {
        Value::Text(text) => Some(text.to_string()),
        _ => None,
    };
    let mut attachments = Vec::new();
    if let Value::List(items) = field("attachments") {
        let files = folder.join(format!("{id}.files"));
        for (index, item) in items.iter().enumerate() {
            let Value::Record(record) = item else {
                continue;
            };
            let bytes: &[u8] = match record.get("content") {
                Some(Value::Bytes(bytes)) => bytes,
                _ => &[],
            };
            std::fs::create_dir_all(&files)?;
            std::fs::write(files.join(index.to_string()), bytes)?;
            let text = |name: &str| match record.get(name) {
                Some(Value::Text(text)) => text.to_string(),
                _ => String::new(),
            };
            attachments.push(Attachment {
                name: text("name"),
                content_type: text("content_type"),
                size: bytes.len() as u64,
            });
        }
    }
    let mail = Mail {
        id: id.clone(),
        number,
        at: jadpo_std::format_instant(at_ms),
        email: sent.map(|sent| sent.email.clone()).unwrap_or_default(),
        trace: sent.and_then(|sent| sent.trace.clone()),
        from: text_of(field("from")).unwrap_or_default(),
        to: text_of(field("to")).unwrap_or_default(),
        subject: text_of(field("subject")).unwrap_or_default(),
        html: text_of(field("html")),
        text: text_of(field("text")),
        attachments,
        spam,
    };
    let partial = folder.join(format!("{id}.json.partial"));
    std::fs::write(&partial, format!("{}\n", mail.to_json()))?;
    std::fs::rename(&partial, folder.join(format!("{id}.json")))?;
    std::fs::write(folder.join(LAST), format!("{number}\n"))?;
    // The oldest past the limit go.
    let over = (held.len() + 1).saturating_sub(KEPT);
    for old in held.iter().take(over) {
        remove(root, &old.id)?;
    }
    Ok(mail)
}

/// The driver `jadpo dev` gives the connection `email:` names: it keeps
/// each message in the inbox and answers that it went. Any other call
/// through a mail package's connection goes to `other`, the package's
/// fake.
pub(crate) struct Catcher {
    pub root: PathBuf,
    /// Held while a mail is written, so two workers number theirs apart.
    pub lock: Arc<Mutex<()>>,
    /// Told of each mail kept, for `jadpo dev`'s line.
    pub caught: Arc<dyn Fn(&Mail) + Send + Sync>,
    pub other: Option<Rc<dyn Capability>>,
    /// Where each mail is scored before it is kept.
    pub spam: Rc<dyn SpamFilter>,
}

impl Capability for Catcher {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        if call.operation != jadpo_std::email::SEND {
            return match &self.other {
                Some(other) => other.call(call),
                None => Box::pin(async { Answer::Misconfigured }),
            };
        }
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or_default();
        Box::pin(async move {
            // Scored first, on another thread, then numbered and kept.
            let spam = match crate::spam::as_sent(&call.input, at) {
                Some(message) => self.spam.score(message).await,
                None => {
                    Spam::Failed("it couldn't be written as the SMTP driver writes it".to_string())
                }
            };
            let kept = {
                let _held = match self.lock.lock() {
                    Ok(held) => held,
                    Err(poisoned) => poisoned.into_inner(),
                };
                keep(&self.root, &call.input, call.mail.as_deref(), at, spam)
            };
            match kept {
                Ok(mail) => {
                    (self.caught)(&mail);
                    Answer::Value(Value::Unit)
                }
                // Nothing was kept, so nothing went: tried again.
                Err(_) => Answer::Unavailable,
            }
        })
    }

    fn source(&self) -> Source {
        Source::Fake
    }

    fn source_of(&self, call: &Call) -> Source {
        match &self.other {
            Some(other) if call.operation != jadpo_std::email::SEND => other.source_of(call),
            _ => Source::Fake,
        }
    }

    fn catches_mail(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("jadpo-dev-inbox-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    fn message(subject: &str, attachment: Option<&[u8]>) -> Value {
        let attachments: Vec<Value> = attachment
            .map(|bytes| {
                Value::record(
                    jadpo_std::email::ATTACHMENT,
                    jadpo_std::email::ATTACHMENT_FIELDS
                        .iter()
                        .zip([
                            Value::text("receipt.pdf"),
                            Value::text("application/pdf"),
                            Value::Bytes(bytes.to_vec().into()),
                        ])
                        .map(|(name, value)| (Rc::from(*name), value))
                        .collect(),
                )
            })
            .into_iter()
            .collect();
        Value::record(
            jadpo_std::email::MESSAGE,
            jadpo_std::email::MESSAGE_FIELDS
                .iter()
                .zip([
                    Value::text("hello@notes.example"),
                    Value::text("ada@notes.example"),
                    Value::text(subject),
                    Value::text("<p><a href=\"https://notes.example/a\">a</a></p>"),
                    Value::None,
                    Value::List(attachments.into()),
                ])
                .map(|(name, value)| (Rc::from(*name), value))
                .collect(),
        )
    }

    #[test]
    fn mails_are_kept_whole_numbered_and_cleared() {
        let root = root("kept");
        let sent = MailSent {
            email: "accounts.WelcomeEmail".to_string(),
            trace: Some("0af7651916cd43dd8448eb211c80319c".to_string()),
            sign_in: None,
        };
        let first = keep(
            &root,
            &message("Welcome, Ada", Some(b"%PDF")),
            Some(&sent),
            0,
            Spam::NotInstalled,
        )
        .expect("kept");
        assert_eq!(first.id, "mail_1");
        assert_eq!(first.at, "1970-01-01T00:00:00Z");
        assert_eq!(first.email_short(), "WelcomeEmail");
        assert_eq!(first.links(), ["https://notes.example/a"]);
        assert_eq!(
            first.health().iter().map(|f| f.check).collect::<Vec<_>>(),
            [email_health::Check::NoText]
        );
        let read = read(&root, "1").expect("read back");
        assert_eq!(read, first);
        assert_eq!(read.spam, Spam::NotInstalled);
        assert_eq!(attachment(&root, &read, 0).as_deref(), Some(&b"%PDF"[..]));
        assert_eq!(attachment(&root, &read, 1), None);
        keep(&root, &message("Again", None), None, 1, Spam::NotRun).expect("kept");
        assert_eq!(clear(&root).expect("cleared"), 2);
        assert!(read_all(&root).is_empty());
        // Numbers carry on past a clear.
        let next = keep(&root, &message("Later", None), None, 2, Spam::NotRun).expect("kept");
        assert_eq!(next.id, "mail_3");
        assert_eq!(super::read(&root, "../x"), None);
    }

    #[test]
    fn the_inbox_keeps_the_last_mails() {
        let root = root("last");
        for at in 0..(KEPT as i64 + 3) {
            keep(&root, &message("Hi there", None), None, at, Spam::NotRun).expect("kept");
        }
        let all = read_all(&root);
        assert_eq!(all.len(), KEPT);
        assert_eq!(all[0].id, "mail_4");
    }

    /// Answers every mail with one report, keeping what it was given.
    struct FakeFilter {
        given: std::cell::RefCell<Vec<Vec<u8>>>,
        answer: Spam,
    }

    impl SpamFilter for FakeFilter {
        fn score(&self, message: Vec<u8>) -> LocalFuture<'_, Spam> {
            self.given.borrow_mut().push(message);
            let answer = self.answer.clone();
            Box::pin(async move { answer })
        }
    }

    #[test]
    fn each_mail_is_scored_as_the_smtp_driver_writes_it_before_it_is_kept() {
        let root = root("scored");
        let report = crate::spam::Report {
            score: "6.2".to_string(),
            required: "5.0".to_string(),
            rules: vec![crate::spam::Rule {
                name: "MISSING_DATE".to_string(),
                points: "1.4".to_string(),
                description: "Missing Date: header".to_string(),
            }],
        };
        let filter = Rc::new(FakeFilter {
            given: std::cell::RefCell::new(Vec::new()),
            answer: Spam::Ran(report.clone()),
        });
        let catcher = Catcher {
            root: root.clone(),
            lock: Arc::new(Mutex::new(())),
            caught: Arc::new(|_: &Mail| {}),
            other: None,
            spam: filter.clone(),
        };
        let call = Call {
            connection: "mail".to_string(),
            operation: jadpo_std::email::SEND.to_string(),
            input: message("Welcome, Ada", None),
            repeat: jadpo_runtime::capability::Repeat::Unkeyed,
            request: None,
            step: None,
            key: None,
            attempt: 1,
            mail: None,
        };
        let answer = jadpo_runtime::block_on(catcher.call(&call)).expect("answered");
        assert!(matches!(answer, Answer::Value(Value::Unit)));
        let given = filter.given.borrow();
        let written = String::from_utf8_lossy(&given[0]);
        assert!(written.starts_with("Message-ID: <"), "{written}");
        assert!(
            written.contains("\r\nTo: ada@notes.example\r\n"),
            "{written}"
        );
        assert!(
            written.contains("Content-Transfer-Encoding: base64"),
            "{written}"
        );
        let kept = super::read(&root, "mail_1").expect("kept");
        assert_eq!(kept.spam, Spam::Ran(report));
        // SpamAssassin calling it spam is one more problem.
        assert_eq!(kept.health().len(), 1);
        assert_eq!(kept.problems(), 2);
    }
}
