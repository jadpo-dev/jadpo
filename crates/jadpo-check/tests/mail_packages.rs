//! A mail package as the `email` connection (area 16, "Email"; area 11,
//! "How a package provides one"; chunk 7.3c): the package maps the
//! capability's `send` onto one of its operations in
//! `provides/email.jadpo`, as data, with the total size of attachments it
//! accepts in `limits:`; `email: { connection: }` may name its connection,
//! and an email's `File` fields' `max_size:` must then fit that limit.

use jadpo_db::Layout;

use crate::support;

const HOME: &str = "shared/providers/postbox/v1";

const MAPPING: &str = "/// Sends each email as one message through Postbox's API.
provide email {
    send: {
        call: emails.send
        input: {
            from: message.from
            to: message.to
            subject: message.subject
            html: message.html
            text: message.text
            stream: settings.stream
            attachments: {
                each: message.attachments
                as: {
                    filename: item.name
                    content: item.content
                    content_type: item.content_type
                }
            }
        }
        receipt: response.id
        failures: {
            inactive_recipient: EmailFailure.recipient_rejected
            validation_error: EmailFailure.message_rejected
        }
    }
    limits: {
        attachments: 10mb
    }
}
";

fn package(mapping: &str) -> Vec<(String, String)> {
    vec![
        (
            format!("{HOME}/package.jadpo"),
            "/// Postbox.\npackage Postbox.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        (
            format!("{HOME}/provider.jadpo"),
            "/// Postbox's JSON API.\nprovider {\n    protocol: Protocol.json_http\n    credential: CredentialScheme.bearer\n    error_code: \"name\"\n    settings: {\n        stream: Text\n    }\n}\n".to_string(),
        ),
        (
            format!("{HOME}/resources/emails.jadpo"),
            "public resource emails {\n    send: {\n        method: HttpMethod.post\n        path: \"/emails\"\n        input: OutgoingEmail\n        output: SentReceipt\n        changes: true\n        failures: [InactiveRecipient, ValidationError]\n        source: PackageSource.written\n    }\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/outgoing_email.jadpo"),
            "/// One email to send.\npublic type OutgoingEmail = Object {\n    from: Text\n    to: Text\n    subject: Text\n    html: Text?\n    text: Text?\n    stream: Text\n    attachments: List<OutgoingAttachment>\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/outgoing_attachment.jadpo"),
            "/// One file sent with an email.\npublic type OutgoingAttachment = Object {\n    filename: Text\n    content: Bytes\n    content_type: Text?\n}\n".to_string(),
        ),
        (
            format!("{HOME}/types/sent_receipt.jadpo"),
            "/// What Postbox answers a send with.\npublic type SentReceipt = Object {\n    id: Text\n}\n".to_string(),
        ),
        (
            format!("{HOME}/failures/inactive_recipient.jadpo"),
            "/// The address bounced before, so Postbox won't send to it.\npublic type InactiveRecipient = Failure {\n    kind: FailureKind.rejected\n    message: \"The address is inactive.\"\n}\n".to_string(),
        ),
        (
            format!("{HOME}/failures/validation_error.jadpo"),
            "/// Postbox refused the message as written.\npublic type ValidationError = Failure {\n    kind: FailureKind.invalid_value\n    message: \"The message is invalid.\"\n}\n".to_string(),
        ),
        (format!("{HOME}/provides/email.jadpo"), mapping.to_string()),
    ]
}

/// The app sending email through `connection`, with `postbox` a
/// connection to the package: one only `email:` uses lists no domain.
fn app(connection: &str) -> String {
    let domains = if connection == "postbox" {
        ""
    } else {
        "billing"
    };
    format!(
        "application {{\n    display_name: \"Notes\"\n    connections: {{\n        postbox: {{\n            package: Postbox.v1\n            domains: [{domains}]\n            credential: config.postbox_token\n            host: config.postbox_host\n            stream: \"outbound\"\n        }}\n        mail: {{\n            driver: Driver.smtp\n            host: \"smtp.notes.example\"\n        }}\n        files: {{\n            driver: Driver.disk\n        }}\n    }}\n    email: {{\n        connection: {connection}\n        from: \"hello@notes.example\"\n    }}\n    events: {{\n        store: main\n    }}\n}}\n\nlocales {{\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}}\n"
    )
}

const CONFIG: &str =
    "config {\n    postbox_token: Text {\n        secret: true\n    }\n    postbox_host: Url\n}\n";

const INVOICE: &str = "/// Sends a customer their invoice.\ntype InvoiceEmail = EmailMessage {\n    name: Text\n    invoice: File {\n        max_size: 4mb\n    }\n    terms: File {\n        max_size: 2mb\n    }\n}\n";

fn problems_with(connection: &str, mapping: &str, invoice: &str) -> Vec<String> {
    let mut files = vec![
        ("app.jadpo".to_string(), app(connection)),
        ("config.jadpo".to_string(), CONFIG.to_string()),
        (
            "domains/billing/translations/en_gb.jadpo".to_string(),
            "translations {\n    invoice_email: \"Your invoice, {name}\"\n}\n".to_string(),
        ),
        (
            "domains/billing/emails/invoice_email.jadpo".to_string(),
            invoice.to_string(),
        ),
        (
            "domains/billing/emails/invoice_email.txt".to_string(),
            "Hello {{ name }}, your invoice is attached.\n".to_string(),
        ),
    ];
    files.extend(package(mapping));
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| {
            !["import.unused", "layout.unused_declaration"]
                .iter()
                .any(|rule| problem.contains(&format!(" {rule}:")))
        })
        .collect()
}

fn problems(mapping: &str) -> Vec<String> {
    problems_with("postbox", mapping, INVOICE)
}

const NONE: &[&str] = &[];

#[test]
fn a_mail_package_that_provides_email_is_the_email_connection() {
    assert_eq!(problems(MAPPING), NONE);
    // The swap: the same emails through the SMTP connection.
    assert_eq!(problems_with("mail", MAPPING, INVOICE), NONE);
}

/// Area 16: a connection only `email:` names needs no `domains:`, since
/// no domain's action calls it (chunk 7.8d); one `email:` doesn't name
/// still lists its callers.
#[test]
fn a_connection_only_email_names_needs_no_domains() {
    let bare = |connection: &str| {
        let mut files = vec![
            (
                "app.jadpo".to_string(),
                app(connection)
                    .replace("            domains: []\n", "")
                    .replace("            domains: [billing]\n", ""),
            ),
            ("config.jadpo".to_string(), CONFIG.to_string()),
        ];
        files.extend(package(MAPPING));
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        support::problems(Layout::Project, &files)
    };
    assert_eq!(bare("postbox"), NONE);
    let unnamed = bare("mail");
    assert!(
        unnamed
            .iter()
            .any(|p| p.contains("connection.key_missing: `postbox` doesn't give `domains:`")),
        "{unnamed:#?}"
    );
}

#[test]
fn a_package_that_provides_no_email_cant_be_the_email_connection() {
    let mut files = vec![
        ("app.jadpo".to_string(), app("postbox")),
        ("config.jadpo".to_string(), CONFIG.to_string()),
    ];
    files.extend(
        package(MAPPING)
            .into_iter()
            .filter(|(path, _)| !path.ends_with("provides/email.jadpo")),
    );
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    assert_eq!(
        support::problems(Layout::Project, &files),
        vec!["app.jadpo email.settings_invalid: `connection: postbox` sends no email"]
    );
}

#[test]
fn the_mapping_gives_every_part_of_the_message() {
    let cases: [(&str, &str, &str); 6] = [
        (
            "            text: message.text\n",
            "",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `send:` doesn't give the message's `text`",
        ),
        (
            "            attachments: {\n                each: message.attachments\n                as: {\n                    filename: item.name\n                    content: item.content\n                    content_type: item.content_type\n                }\n            }\n",
            "            attachments: []\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `[]` isn't a value a mapping can give",
        ),
        (
            "            subject: message.subject\n",
            "            subject: message.title\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `message.title` isn't part of the message",
        ),
        (
            "            html: message.html\n",
            "            html: message.from\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `send:` doesn't give the message's `html`",
        ),
        (
            "                    content: item.content\n",
            "",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `emails.send`'s input field `content` isn't mapped",
        ),
        (
            "            to: message.to\n",
            "            to: message.html\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `message.html` may be `none`, and `to` can't be",
        ),
    ];
    for (from, to, expected) in cases {
        let mapping = MAPPING.replace(from, to);
        assert_ne!(mapping, MAPPING, "{from}");
        let found = problems(&mapping);
        assert!(
            found.iter().any(|problem| problem == expected),
            "{expected}\n{found:#?}"
        );
    }
}

#[test]
fn the_mapping_names_the_packages_operation_failures_and_limit() {
    let cases: [(&str, &str, &str); 5] = [
        (
            "        call: emails.send\n",
            "        call: emails.sned\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `emails.sned` isn't one of the package's operations",
        ),
        (
            "            inactive_recipient: EmailFailure.recipient_rejected\n",
            "            inactive: EmailFailure.recipient_rejected\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `emails.send` has no failure `inactive`",
        ),
        (
            "            validation_error: EmailFailure.message_rejected\n",
            "            validation_error: EmailFailure.rejected\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `EmailFailure.rejected` isn't an email's failure",
        ),
        (
            "        receipt: response.id\n",
            "        receipt: response.message_id\n",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `response.message_id` has no field `message_id`",
        ),
        (
            "    limits: {\n        attachments: 10mb\n    }\n",
            "",
            "shared/providers/postbox/v1/provides/email.jadpo package.provider_invalid: `provide email` doesn't say how large attachments may be",
        ),
    ];
    for (from, to, expected) in cases {
        let mapping = MAPPING.replace(from, to);
        assert_ne!(mapping, MAPPING, "{from}");
        let found = problems(&mapping);
        assert!(
            found.iter().any(|problem| problem == expected),
            "{expected}\n{found:#?}"
        );
    }
}

#[test]
fn an_emails_attachments_fit_the_packages_limit() {
    let over = INVOICE.replace("max_size: 2mb", "max_size: 8mb");
    assert_eq!(
        problems_with("postbox", MAPPING, &over),
        vec![
            "domains/billing/emails/invoice_email.jadpo email.attachment_limit: `InvoiceEmail`'s attachments may be 12 MB, over the 10 MB `postbox` accepts"
        ]
    );
    let unsized_terms = INVOICE.replace(
        "    terms: File {\n        max_size: 2mb\n    }\n",
        "    terms: File\n",
    );
    assert_eq!(
        problems_with("postbox", MAPPING, &unsized_terms),
        vec![
            "domains/billing/emails/invoice_email.jadpo email.attachment_limit: `InvoiceEmail.terms` doesn't say how large it may be"
        ]
    );
    // Through SMTP, which declares no limit, neither is a problem.
    assert_eq!(problems_with("mail", MAPPING, &over), NONE);
    assert_eq!(problems_with("mail", MAPPING, &unsized_terms), NONE);
}
