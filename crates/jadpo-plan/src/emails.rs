//! The email plan (area 16, "Email"; chunk 7.3): every email type with the
//! callables its templates became, its subject's wording and its
//! attachments, the shared layout's callables, who emails are from and
//! the SMTP connections; and a subscriber for each email type, so emitting
//! one leaves a delivery that sends it once its action commits.

use std::collections::HashMap;

use jadpo_check::lookups::Name;
use jadpo_db::{Db, Project};
use jadpo_graph::Key;
use jadpo_std::email::{EmailPlan, EmailsPlan, Sender, SmtpConnection, SmtpHost};
use jadpo_std::subscriber::{SubscriberPlan, Target, TenantFrom};
use jadpo_std::translation::TranslationParam;

/// The key of each template's callable, by the template's path.
pub(crate) fn template_keys(db: &dyn Db, project: Project) -> Vec<(String, Key)> {
    jadpo_check::emails::template_paths(db, project)
        .into_iter()
        .filter_map(|path| {
            let checked =
                jadpo_check::types::template_body(db, project, Name::new(db, path.clone()));
            Some((path, checked.key.clone()?))
        })
        .collect()
}

/// Every email, and the subscriber each has, which hands it to the
/// connection `email:` names: none when nothing sends email, which the
/// checker refuses for an email that is emitted.
pub(crate) fn plan(
    db: &dyn Db,
    project: Project,
    by_key: &HashMap<Key, u32>,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> (EmailsPlan, Vec<SubscriberPlan>) {
    let settings = jadpo_check::emails::settings(db, project);
    let analysis = jadpo_check::emails::analysis(db, project);
    let callable_of = |path: &Option<String>| {
        let path = path.as_ref()?;
        let checked = jadpo_check::types::template_body(db, project, Name::new(db, path.clone()));
        by_key.get(checked.key.as_ref()?).copied()
    };
    let translations = jadpo_check::translations::plan(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    // Each email's plan, with the file its type is declared in.
    let mut emails: Vec<(EmailPlan, String)> = Vec::new();
    for email in &analysis.emails {
        let Some(key) = jadpo_check::graph::declaration_key(&lookup, &email.canonical) else {
            continue;
        };
        // Its subject's wording, given its fields a wording may write by
        // name.
        let subject = email.scope().and_then(|scope| {
            let (_, entry) = translations.entry(&format!("{scope}.{}", email.subject))?;
            let subjects = jadpo_check::translations::subjects(db, project, Name::new(db, scope));
            let found = subjects.get(&email.subject)?;
            let mut entry = entry.clone();
            entry.params = found
                .placeholders
                .iter()
                .map(|(name, kind)| TranslationParam {
                    name: name.clone(),
                    kind: *kind,
                })
                .collect();
            Some(entry)
        });
        emails.push((
            EmailPlan {
                key: key.as_str().to_string(),
                name: jadpo_check::emails::email_name(email),
                fields: email.fields.iter().map(|(name, ..)| name.clone()).collect(),
                html: callable_of(&email.html),
                text: callable_of(&email.text),
                subject,
                attachments: email.attachments.clone(),
            },
            email.path.clone(),
        ));
    }
    emails.sort_by(|a, b| a.0.key.cmp(&b.0.key));
    let files: Vec<String> = emails.iter().map(|(_, path)| path.clone()).collect();
    let emails: Vec<EmailPlan> = emails.into_iter().map(|(email, _)| email).collect();
    let smtp = jadpo_check::emails::smtp_connections(db, project)
        .list
        .iter()
        .filter_map(|connection| {
            Some(SmtpConnection {
                name: connection.name.clone(),
                host: match connection.host.as_ref()? {
                    jadpo_check::emails::SmtpHost::Setting(name) => {
                        SmtpHost::Setting(setting(name)?)
                    }
                    jadpo_check::emails::SmtpHost::Written(host) => SmtpHost::Written(host.clone()),
                },
                port: connection.port,
                username: connection.username.as_deref().and_then(setting),
                password: connection.password.as_deref().and_then(setting),
            })
        })
        .collect();
    let plan = EmailsPlan {
        connection: settings.connection.clone(),
        from: settings.from.as_ref().and_then(|from| match from {
            jadpo_check::emails::Sender::Setting(name) => setting(name).map(Sender::Setting),
            jadpo_check::emails::Sender::Address(address) => Some(Sender::Address(address.clone())),
        }),
        emails,
        layout_html: callable_of(&analysis.layout_html),
        layout_text: callable_of(&analysis.layout_text),
        locales: jadpo_check::locales::locales(db, project).plan.clone(),
        smtp,
    };
    let Some(connection) = plan.connection.clone() else {
        return (plan, Vec::new());
    };
    // Each email's delivery: tried and given up as a subscriber's is by
    // default (area 12), sent with no key, and recorded by its delivery
    // alone.
    let defaults = jadpo_check::events::DEFAULT_RETRY;
    let subscribers = plan
        .emails
        .iter()
        .zip(files)
        .map(|(email, file)| SubscriberPlan {
            key: format!("email {}", email.name),
            name: format!("send {}", email.name),
            event: email.key.clone(),
            target: Target::Email {
                connection: connection.clone(),
            },
            attempts: defaults.attempts,
            within_ms: defaults.within_ms,
            // It runs no action, so reaches no tenant's records.
            tenant: TenantFrom::Nowhere,
            store: None,
            file,
            line: 1,
        })
        .collect();
    (plan, subscribers)
}
