//! The email plan (area 16, "Email"; chunk 7.3): every email type with the
//! callables its templates became, its subject's wording and its
//! attachments, the shared layout's callables, who emails are from and
//! the SMTP connections; and a subscriber for each email type, so emitting
//! one leaves a delivery that sends it once its action commits. Jadpo's own
//! magic-link email is one of them, written by the runtime (chunk 7.3b).

use std::collections::HashMap;

use jadpo_check::lookups::Name;
use jadpo_db::{Db, Project};
use jadpo_graph::Key;
use jadpo_std::email::{
    BuiltinEmail, EmailPlan, EmailsPlan, MAGIC_LINK_EMAIL, MAGIC_LINK_EMAIL_FIELDS,
    MagicLinkEmailPlan, Sender, SmtpConnection, SmtpHost,
};
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
                builtin: None,
            },
            email.path.clone(),
        ));
    }
    // Jadpo's own magic-link email (area 16, "Built-in emails"; chunk
    // 7.3b), written by the runtime in Jadpo's wording.
    if let Some(mut builtin) = magic_link_email(db, project, &translations, setting) {
        // The application's own email (level 4) and sending (level 5).
        if let Some(BuiltinEmail::MagicLink(magic)) = &mut builtin.0.builtin {
            magic.own = jadpo_check::identity::own_magic_link_email(db, project)
                .0
                .as_ref()
                .and_then(|name| jadpo_check::graph::declaration_key(&lookup, name))
                .map(|key| key.as_str().to_string());
            magic.send = sign_in_send(db, project).and_then(|key| by_key.get(&key).copied());
        }
        emails.push(builtin);
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
    // Each email's delivery: tried and given up as a subscriber's is by
    // default (area 12), sent with no key through the `email:`
    // connection, and recorded by its delivery alone; and Jadpo's own
    // sign-in email, when the application sends it itself (area 16,
    // "Built-in emails", level 5), handed to its action as a subscriber's
    // action is, with or without the connection.
    let defaults = jadpo_check::events::DEFAULT_RETRY;
    let send = sign_in_send(db, project);
    let written = jadpo_check::types::written_stores(db, project);
    let called = jadpo_check::types::called_connections(db, project);
    let subscribers = plan
        .emails
        .iter()
        .zip(files)
        .filter_map(|(email, file)| {
            let sends_itself = match &email.builtin {
                Some(BuiltinEmail::MagicLink(magic)) => magic.send.zip(send.clone()),
                None => None,
            };
            let (target, store, connections) = match (sends_itself, &plan.connection) {
                (Some((callable, key)), _) => (
                    Target::Action {
                        callable,
                        args: vec![Vec::new()],
                    },
                    // The store its action writes, where its handled
                    // record is kept, as a subscriber's is.
                    written
                        .get(key.name())
                        .and_then(|stores| stores.keys().next().cloned()),
                    called
                        .get(key.name())
                        .map(|connections| connections.iter().cloned().collect())
                        .unwrap_or_default(),
                ),
                // No action: what it calls out through is its connection
                // (`SubscriberPlan::calls_through`; chunk 7.2b).
                (None, Some(connection)) => (
                    Target::Email {
                        connection: connection.clone(),
                    },
                    None,
                    Vec::new(),
                ),
                (None, None) => return None,
            };
            Some(SubscriberPlan {
                key: format!("email {}", email.name),
                name: format!("send {}", email.name),
                event: email.key.clone(),
                target,
                attempts: defaults.attempts,
                within_ms: defaults.within_ms,
                // It reaches no tenant's records: Jadpo's email runs no
                // action, and the application's sending is given only the
                // request.
                tenant: TenantFrom::Nowhere,
                store,
                connections,
                through: None,
                file,
                line: 1,
            })
        })
        .collect();
    (plan, subscribers)
}

/// The action `magic_link: { send: }` names, by its key, when it takes a
/// `MagicLinkRequest` and returns `Unit`.
fn sign_in_send(db: &dyn Db, project: Project) -> Option<Key> {
    let name = jadpo_check::identity::sign_in_send_action(db, project)
        .0
        .as_ref()?;
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    jadpo_check::graph::declaration_key(&lookup, name)
}

/// Jadpo's own magic-link email, with the file `sign_in:` is written in,
/// when `magic_link:` is declared and its email can be written: the
/// browser session's origin read from a setting, the application's
/// `display_name:` and Jadpo's wording.
fn magic_link_email(
    db: &dyn Db,
    project: Project,
    translations: &jadpo_std::translation::TranslationPlan,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Option<(EmailPlan, String)> {
    let identity = jadpo_check::identity::identity(db, project);
    let magic = identity.magic_links()?;
    let store = identity.sessions_store()?.to_string();
    let origin = identity
        .method(jadpo_std::AuthenticationMethod::BrowserSession)?
        .origin
        .as_deref()
        .and_then(setting)?;
    let app_name = jadpo_check::emails::settings(db, project)
        .display_name
        .clone()?;
    let messages: Vec<_> = translations
        .entries
        .iter()
        .filter(|entry| {
            entry
                .key
                .strip_prefix("jadpo.")
                .and_then(jadpo_std::jadpo_text::message_named)
                .is_some_and(|message| {
                    message.feature == jadpo_std::jadpo_text::Feature::MagicLink
                        && message.kind == jadpo_std::jadpo_text::Kind::Message
                })
        })
        .cloned()
        .collect();
    let subject = messages
        .iter()
        .find(|entry| entry.key == "jadpo.magic_link_email")
        .cloned()?;
    let locales = jadpo_check::locales::locales(db, project)
        .plan
        .clone()
        .unwrap_or_else(|| {
            let en_gb = jadpo_std::locale_ordinal("en_gb").unwrap_or_default();
            jadpo_std::LocalePlan {
                default: en_gb,
                supported: vec![en_gb],
                in_progress: Vec::new(),
            }
        });
    let file = magic.span.path.clone();
    Some((
        EmailPlan {
            key: MAGIC_LINK_EMAIL.to_string(),
            name: MAGIC_LINK_EMAIL.to_string(),
            fields: MAGIC_LINK_EMAIL_FIELDS
                .iter()
                .map(|f| f.to_string())
                .collect(),
            html: None,
            text: None,
            subject: Some(subject),
            attachments: Vec::new(),
            builtin: Some(BuiltinEmail::MagicLink(MagicLinkEmailPlan {
                store,
                origin,
                link: magic.link.clone(),
                expires_after_ms: magic.expires_after_ms,
                app_name,
                messages,
                locales,
                own: None,
                send: None,
            })),
        },
        file,
    ))
}
