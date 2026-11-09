//! Data requests past the asking (area 15, "Exports", "Erasures" and
//! "Deadlines and events"): what the compiler and the runtime both decide
//! of the work a request leads to.
//!
//! Each piece of that work is a *step*, kept as an outbox row
//! ([`STEP_EVENT`]) with one delivery to the generated subscriber
//! [`STEPS_SUBSCRIBER`], due when the step is: an export at once, an
//! erasure when its cooling-off ends, the deadline's alert and event at
//! their times. So a step is retried, shown and kept like any delivery,
//! and nothing needs a schedule of its own.

use crate::LocalePlan;
use crate::translation::TranslationEntry;

/// The identity key a step's outbox row names.
pub const STEP_EVENT: &str = "jadpo.PrivacyStep";

/// The generated subscriber every step is delivered to.
pub const STEPS_SUBSCRIBER: &str = "privacy requests";

/// What a request's `state` says.
pub const RECEIVED: &str = "received";
pub const READY: &str = "ready";
pub const DOWNLOADED: &str = "downloaded";
pub const EXPIRED: &str = "expired";
pub const ERASING: &str = "erasing";
pub const STALLED: &str = "stalled";
pub const COMPLETED: &str = "completed";
pub const CANCELLED: &str = "cancelled";

/// One piece of work a data request leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The person's export made, kept and announced.
    Export,
    /// The person erased, once the cooling-off is over.
    Erase,
    /// The erasure finished, once every provider has confirmed.
    Complete,
    /// An export's file deleted as its link ends.
    Expire,
    /// The alert for a request nearing its deadline.
    Nearing,
    /// `PrivacyRequestOverdue` for a request past its deadline.
    Overdue,
}

impl Step {
    pub const ALL: [Step; 6] = [
        Step::Export,
        Step::Erase,
        Step::Complete,
        Step::Expire,
        Step::Nearing,
        Step::Overdue,
    ];

    /// As its payload names it.
    pub fn name(self) -> &'static str {
        match self {
            Step::Export => "export",
            Step::Erase => "erase",
            Step::Complete => "complete",
            Step::Expire => "expire",
            Step::Nearing => "nearing",
            Step::Overdue => "overdue",
        }
    }

    pub fn named(name: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|step| step.name() == name)
    }
}

/// What the steps need of the application: where its requests are kept,
/// whose they are, how to reach the person and what an export needs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrivacySteps {
    /// The store `jadpo_privacy_requests` is in: the person entity's.
    pub store: String,
    /// The person entity, as its erasure and export plans name it.
    pub person: String,
    /// The person's address by their `id`, `$1`: what an email goes to.
    pub address: Option<String>,
    /// `exports:`.
    pub exports: Option<Exports>,
    /// The setting the browser session's `origin:` reads: what a link in
    /// an email opens.
    pub origin: Option<u32>,
    /// The locales a date is written in.
    pub locales: LocalePlan,
    /// Each entity an erasure keeps with `Erasure.retained`, with its
    /// wording.
    pub kept: Vec<Kept>,
}

/// `privacy: { exports: { store: keep: link: } }`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Exports {
    /// The store of files the encrypted exports are in.
    pub store: String,
    /// How long a link and its file last, in milliseconds.
    pub keep_ms: i64,
    /// The frontend page a link opens.
    pub link: String,
}

/// An entity an erasure keeps because a law says so, as the completion
/// email lists it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    pub entity: String,
    /// Its entry in its domain's language files.
    pub wording: TranslationEntry,
}

/// The built-in type a completion email's `kept` holds a list of.
pub const KEPT_RECORD: &str = "PrivacyKeptRecord";

/// `PrivacyKeptRecord`'s fields, in order.
pub const KEPT_RECORD_FIELDS: [&str; 2] = ["name", "kept_until"];

/// The folder a store of files keeps exports in, which no file's key
/// ([`crate::files::key_for`]) can name.
pub const EXPORT_FOLDER: &str = "privacy-exports";

/// The key of the encrypted export of the request whose `id`, in its text
/// form, is `id`: a retry writes the same file again.
pub fn export_key(id: &str) -> String {
    format!("{EXPORT_FOLDER}/{id}")
}

/// Whether `key` is one [`export_key`] made, so a driver refuses a key that
/// could name a path outside its own.
pub fn is_export_key(key: &str) -> bool {
    let Some(id) = key
        .strip_prefix(EXPORT_FOLDER)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return false;
    };
    id.len() == 36
        && id.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        })
}

/// The file name an export is downloaded as.
pub const EXPORT_FILE_NAME: &str = "personal-data-export.json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_export_key_names_one_request_and_nothing_else() {
        let id = "0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b";
        assert!(is_export_key(&export_key(id)));
        assert!(!crate::files::is_key(&export_key(id)));
        for wrong in [
            "privacy-exports",
            "privacy-exports/",
            "privacy-exports/../0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
            "privacy-exports/0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b/more",
            "privacy-exports/0190A2B3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
            "other/0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
        ] {
            assert!(!is_export_key(wrong), "{wrong}");
        }
    }

    #[test]
    fn every_step_is_named_and_read_back() {
        for step in Step::ALL {
            assert_eq!(Step::named(step.name()), Some(step));
        }
        assert_eq!(Step::named("nothing"), None);
    }
}
