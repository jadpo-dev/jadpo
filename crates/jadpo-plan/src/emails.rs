//! The email plan (area 16, "Email"; chunk 7.3): every email type with the
//! callables its templates became, its subject's wording and its
//! attachments, the shared layout's callables, who emails are from and
//! the SMTP connections; and a subscriber for each email type, so emitting
//! one leaves a delivery that sends it once its action commits. Jadpo's own
//! magic-link email is one of them, written by the runtime (chunk 7.3b).

use jadpo_check::lookups::Name;
use jadpo_db::{Db, Project};
use jadpo_graph::Key;
use jadpo_std::email::{
    BuiltinEmail, EmailPlan, EmailsPlan, MAGIC_LINK_EMAIL, MAGIC_LINK_EMAIL_FIELDS,
    MagicLinkEmailPlan, MailInput, PRIVACY_ADDRESS_CHECK_EMAIL, PRIVACY_ADDRESS_CHECK_EMAIL_FIELDS,
    PackageEmail, PrivacyAddressCheckEmailPlan, PrivacyNotice, PrivacyNoticePlan, Sender,
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
/// checker refuses for an email that is emitted. A mail package's
/// operation is named by its number in `numbering`. A deployable that
/// doesn't run the subscribers (`running` false) sends none, so it holds
/// no template, sender or mail package, and lists each email's subscriber
/// without what it runs (chunk 8.1o3).
pub(crate) fn plan(
    db: &dyn Db,
    project: Project,
    callable: &dyn Fn(&Key) -> Option<u32>,
    numbering: &crate::providers::Numbering,
    setting: &dyn Fn(&str) -> Option<u32>,
    running: bool,
) -> (EmailsPlan, Vec<SubscriberPlan>) {
    let settings = jadpo_check::emails::settings(db, project);
    let analysis = jadpo_check::emails::analysis(db, project);
    let callable_of = |path: &Option<String>| {
        let path = path.as_ref().filter(|_| running)?;
        let checked = jadpo_check::types::template_body(db, project, Name::new(db, path.clone()));
        callable(checked.key.as_ref()?)
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
            magic.send = sign_in_send(db, project)
                .filter(|_| running)
                .and_then(|key| callable(&key));
        }
        emails.push(builtin);
    }
    // Jadpo's own address-check email (area 15, "Self-service by email").
    if let Some(mut builtin) = privacy_address_check_email(db, project, &translations, setting) {
        if let Some(BuiltinEmail::PrivacyAddressCheck(check)) = &mut builtin.0.builtin {
            check.own = privacy_own(db, project, "address_check");
            check.send = privacy_send(db, project, "address_check")
                .filter(|_| running)
                .and_then(|key| callable(&key));
        }
        emails.push(builtin);
    }
    // The other three privacy emails, each sent on its event.
    for notice in PrivacyNotice::ALL {
        if let Some(mut builtin) = privacy_notice_email(db, project, &translations, notice) {
            if let Some(BuiltinEmail::PrivacyNotice(plan)) = &mut builtin.0.builtin {
                plan.own = privacy_own(db, project, notice.setting());
                plan.send = privacy_send(db, project, notice.setting())
                    .filter(|_| running)
                    .and_then(|key| callable(&key));
            }
            emails.push(builtin);
        }
    }
    emails.sort_by(|a, b| a.0.key.cmp(&b.0.key));
    let files: Vec<String> = emails.iter().map(|(_, path)| path.clone()).collect();
    let emails: Vec<EmailPlan> = emails.into_iter().map(|(email, _)| email).collect();
    // A mail package's mapping (chunk 7.3c), each setting it reads by the
    // number of the app's setting its connection gives.
    let package = settings
        .connection
        .as_deref()
        .filter(|_| running)
        .and_then(|connection| {
            let mapping = jadpo_check::mail::mapping_of(db, project, connection)?;
            let given = &jadpo_check::connections::package_connections(db, project)
                .connections
                .iter()
                .find(|known| known.name == connection)?
                .settings;
            Some(PackageEmail {
                operation: mapping.operation.clone(),
                called: numbering
                    .number(connection, &mapping.operation)
                    .unwrap_or(u32::MAX),
                input: mail_inputs(&mapping.input, given, setting)?,
                receipt: mapping.receipt.clone(),
                failures: mapping.failures.clone(),
                attachments: mapping.attachments,
            })
        });
    let plan = EmailsPlan {
        package,
        connection: settings.connection.clone(),
        from: settings
            .from
            .as_ref()
            .filter(|_| running)
            .and_then(|from| match from {
                jadpo_check::emails::Sender::Setting(name) => setting(name).map(Sender::Setting),
                jadpo_check::emails::Sender::Address(address) => {
                    Some(Sender::Address(address.clone()))
                }
            }),
        emails,
        layout_html: callable_of(&analysis.layout_html),
        layout_text: callable_of(&analysis.layout_text),
        locales: jadpo_check::locales::locales(db, project).plan.clone(),
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
                // Where it isn't sent, the action isn't held either.
                Some(BuiltinEmail::MagicLink(magic)) if running => magic.send.zip(send.clone()),
                Some(BuiltinEmail::MagicLink(_)) => send.clone().map(|key| (u32::MAX, key)),
                Some(BuiltinEmail::PrivacyAddressCheck(check)) if running => {
                    check.send.zip(privacy_send(db, project, "address_check"))
                }
                Some(BuiltinEmail::PrivacyAddressCheck(_)) => {
                    privacy_send(db, project, "address_check").map(|key| (u32::MAX, key))
                }
                Some(BuiltinEmail::PrivacyNotice(notice)) if running => notice
                    .send
                    .zip(privacy_send(db, project, notice.notice.setting())),
                Some(BuiltinEmail::PrivacyNotice(notice)) => {
                    privacy_send(db, project, notice.notice.setting()).map(|key| (u32::MAX, key))
                }
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
            let subscriber = SubscriberPlan {
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
            };
            Some(if running {
                subscriber
            } else {
                subscriber.elsewhere()
            })
        })
        .collect();
    (plan, subscribers)
}

/// A mapping's input as the plan holds it: `settings.<name>` read from the
/// app's setting the connection gives, or the value it writes.
fn mail_inputs(
    mapped: &[(String, jadpo_check::mail::Mapped)],
    given: &[(String, jadpo_check::connections::Given)],
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Option<Vec<(String, MailInput)>> {
    use jadpo_check::connections::Given;
    use jadpo_check::mail::Mapped;
    mapped
        .iter()
        .map(|(name, value)| {
            let input = match value {
                Mapped::Message(part) => MailInput::Message(part.clone()),
                Mapped::Item(part) => MailInput::Item(part.clone()),
                Mapped::Text(text) => MailInput::Text(text.clone()),
                Mapped::Int(n) => MailInput::Int(*n),
                Mapped::Bool(flag) => MailInput::Bool(*flag),
                Mapped::Object(fields) => MailInput::Object(mail_inputs(fields, given, setting)?),
                Mapped::Each(fields) => MailInput::Each(mail_inputs(fields, given, setting)?),
                Mapped::Setting(name) => {
                    match given
                        .iter()
                        .find(|(known, _)| known == name)
                        .map(|(_, g)| g)?
                    {
                        Given::Setting(config) => MailInput::Setting(setting(config)?),
                        Given::Text(text) => MailInput::Text(text.clone()),
                        Given::Int(n) => MailInput::Int(*n),
                        Given::Bool(flag) => MailInput::Bool(*flag),
                        Given::Other => return None,
                    }
                }
            };
            Some((name.clone(), input))
        })
        .collect()
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

/// The application's own email for the privacy email `setting`, by its
/// identity key (level 4).
fn privacy_own(db: &dyn Db, project: Project, setting: &str) -> Option<String> {
    let name = jadpo_check::privacy::mail(db, project).own_of(setting)?;
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    jadpo_check::graph::declaration_key(&lookup, name).map(|key| key.as_str().to_string())
}

/// The action that sends the privacy email `setting` itself (level 5), by
/// its key.
fn privacy_send(db: &dyn Db, project: Project, setting: &str) -> Option<Key> {
    let name = jadpo_check::privacy::mail(db, project).send_of(setting)?;
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    jadpo_check::graph::declaration_key(&lookup, name)
}

/// One of the privacy emails sent on an event, with the file `privacy:` is
/// written in, when data requests are on and the application can send it:
/// its plan is keyed by its event, whose delivery hands it over.
fn privacy_notice_email(
    db: &dyn Db,
    project: Project,
    translations: &jadpo_std::translation::TranslationPlan,
    notice: PrivacyNotice,
) -> Option<(EmailPlan, String)> {
    let privacy = jadpo_check::privacy::privacy(db, project);
    if !privacy.takes_requests() {
        return None;
    }
    let settings = jadpo_check::emails::settings(db, project);
    let sends_itself = jadpo_check::privacy::mail(db, project)
        .send_of(notice.setting())
        .is_some();
    if settings.connection.is_none() && !sends_itself {
        return None;
    }
    let app_name = settings.display_name.clone().unwrap_or_default();
    let messages: Vec<_> = translations
        .entries
        .iter()
        .filter(|entry| {
            entry
                .key
                .strip_prefix("jadpo.")
                .and_then(jadpo_std::jadpo_text::message_named)
                .is_some_and(|message| {
                    message.feature == jadpo_std::jadpo_text::Feature::Privacy
                        && message.kind == jadpo_std::jadpo_text::Kind::Message
                })
        })
        .cloned()
        .collect();
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
    let file = privacy
        .span
        .as_ref()
        .map_or_else(|| "app.jadpo".to_string(), |span| span.path.clone());
    Some((
        EmailPlan {
            key: notice.key().to_string(),
            name: notice.name().to_string(),
            fields: Vec::new(),
            html: None,
            text: None,
            subject: None,
            attachments: Vec::new(),
            builtin: Some(BuiltinEmail::PrivacyNotice(PrivacyNoticePlan {
                notice,
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

/// Jadpo's own address-check email, with the file `privacy:` is written
/// in, when requests by email are on (area 15, "Self-service by email"):
/// the browser session's origin read from a setting, the application's
/// `display_name:` and Jadpo's wording.
fn privacy_address_check_email(
    db: &dyn Db,
    project: Project,
    translations: &jadpo_std::translation::TranslationPlan,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Option<(EmailPlan, String)> {
    let privacy = jadpo_check::privacy::privacy(db, project);
    if privacy.self_service.is_none() && privacy.generate_routes {
        return None;
    }
    if !privacy.takes_requests() {
        return None;
    }
    let identity = jadpo_check::identity::identity(db, project);
    let store = identity
        .variant(jadpo_std::http::PrincipalKind::User)?
        .store
        .clone();
    let origin = identity
        .method(jadpo_std::AuthenticationMethod::BrowserSession)?
        .origin
        .as_deref()
        .and_then(setting)?;
    let app_name = jadpo_check::emails::settings(db, project)
        .display_name
        .clone()
        .unwrap_or_default();
    let messages: Vec<_> = translations
        .entries
        .iter()
        .filter(|entry| {
            entry
                .key
                .strip_prefix("jadpo.")
                .and_then(jadpo_std::jadpo_text::message_named)
                .is_some_and(|message| {
                    message.feature == jadpo_std::jadpo_text::Feature::Privacy
                        && message.kind == jadpo_std::jadpo_text::Kind::Message
                })
        })
        .cloned()
        .collect();
    let subject = messages
        .iter()
        .find(|entry| entry.key == "jadpo.privacy_address_check_email")
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
    let link = privacy.self_service.as_ref().map_or_else(
        || jadpo_std::http::PRIVACY_LINK.to_string(),
        |found| found.link.clone(),
    );
    let file = privacy
        .span
        .as_ref()
        .map_or_else(|| "app.jadpo".to_string(), |span| span.path.clone());
    Some((
        EmailPlan {
            key: PRIVACY_ADDRESS_CHECK_EMAIL.to_string(),
            name: PRIVACY_ADDRESS_CHECK_EMAIL.to_string(),
            fields: PRIVACY_ADDRESS_CHECK_EMAIL_FIELDS
                .iter()
                .map(|f| f.to_string())
                .collect(),
            html: None,
            text: None,
            subject: Some(subject),
            attachments: Vec::new(),
            builtin: Some(BuiltinEmail::PrivacyAddressCheck(
                PrivacyAddressCheckEmailPlan {
                    store,
                    origin,
                    link,
                    expires_after_ms: jadpo_std::http::PRIVACY_CHECK_EXPIRES_AFTER_MS,
                    app_name,
                    messages,
                    locales,
                    own: None,
                    send: None,
                },
            )),
        },
        file,
    ))
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
