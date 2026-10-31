//! The test accounts of `jadpo dev`'s account-chooser page (area 8,
//! "Providers", "In `jadpo dev`"; chunk A7c): what a developer adds there,
//! by provider, kept in [`DEV_ACCOUNTS`] so the next session lists them
//! too. The file is under `build/`, which `jadpo dev` keeps out of git, and
//! holds only what a provider would say about a made-up person.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_runtime::http::account_chooser::Accounts;
use jadpo_runtime::http::local_issuer::Account;
use serde_json::{Map, Value, json};

/// Where the test accounts are kept.
pub(crate) const DEV_ACCOUNTS: &str = "build/dev/accounts.json";

/// The project's test accounts, read and written whole under one lock:
/// `{ "<provider>": [{ "subject", "email", "emailVerified", "name" }] }`.
#[derive(Clone)]
pub(crate) struct KeptAccounts {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl KeptAccounts {
    pub(crate) fn under(root: &Path) -> KeptAccounts {
        KeptAccounts {
            path: root.join(DEV_ACCOUNTS),
            lock: Arc::default(),
        }
    }

    /// Every provider's accounts; none when the file is missing or can't
    /// be read.
    fn read(&self) -> Map<String, Value> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
}

fn account(value: &Value) -> Option<Account> {
    let text = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_string);
    Some(Account {
        subject: text("subject").filter(|subject| !subject.is_empty())?,
        email: text("email"),
        email_verified: value.get("emailVerified").and_then(Value::as_bool) == Some(true),
        name: text("name"),
        ..Default::default()
    })
}

impl Accounts for KeptAccounts {
    fn list(&self, provider: &str) -> Vec<Account> {
        let _held = self.lock.lock();
        self.read()
            .get(provider)
            .and_then(Value::as_array)
            .map(|kept| kept.iter().filter_map(account).collect())
            .unwrap_or_default()
    }

    fn keep(&self, provider: &str, account: &Account) {
        let _held = self.lock.lock();
        let mut all = self.read();
        let mut new = json!({
            "subject": account.subject,
            "emailVerified": account.email_verified,
        });
        if let Some(object) = new.as_object_mut() {
            if let Some(email) = &account.email {
                object.insert("email".into(), email.clone().into());
            }
            if let Some(name) = &account.name {
                object.insert("name".into(), name.clone().into());
            }
        }
        let entry = all
            .entry(provider.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if !entry.is_array() {
            *entry = Value::Array(Vec::new());
        }
        if let Some(kept) = entry.as_array_mut() {
            let same = |old: &Value| {
                old.get("subject").and_then(Value::as_str) == Some(account.subject.as_str())
            };
            match kept.iter_mut().find(|old| same(old)) {
                Some(old) => *old = new,
                None => kept.push(new),
            }
        }
        if let (Some(folder), Ok(text)) = (
            self.path.parent(),
            serde_json::to_string_pretty(&Value::Object(all)),
        ) {
            let _ = std::fs::create_dir_all(folder);
            let _ = std::fs::write(&self.path, format!("{text}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_are_kept_by_provider_and_replaced_by_subject() {
        let root = std::env::temp_dir().join(format!("jadpo-dev-accounts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let accounts = KeptAccounts::under(&root);
        assert!(accounts.list("google").is_empty());
        let ann = Account {
            subject: "1".into(),
            email: Some("ann@desk.example".into()),
            email_verified: true,
            name: Some("Ann".into()),
            ..Default::default()
        };
        accounts.keep("google", &ann);
        accounts.keep(
            "microsoft",
            &Account {
                subject: "2".into(),
                email: None,
                email_verified: false,
                name: None,
                ..Default::default()
            },
        );
        let renamed = Account {
            name: Some("Ann Example".into()),
            ..ann.clone()
        };
        accounts.keep("google", &renamed);
        assert_eq!(KeptAccounts::under(&root).list("google"), [renamed]);
        assert_eq!(accounts.list("microsoft").len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
