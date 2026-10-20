//! Area 16, "Email" (chunk 7.3c), in the checker: a partial is a `type
//! Name = EmailPartial { ... }` in `shared/emails/` or a domain's
//! `emails/`, its `.html` and `.txt` beside it, included from a template
//! as `{{ include name(field: value, ...) }}` with its fields by name; the
//! including template's locale goes with it. A partial that doesn't exist,
//! can't be reached, lacks the template it is included from, is given the
//! wrong fields or includes itself is `email.partial_invalid`.

use jadpo_db::Layout;

use crate::support;

const APP: (&str, &str) = (
    "app.jadpo",
    r#"application {
    display_name: "Notes"
    connections: {
        mail: {
            driver: Driver.smtp
            host: "smtp.notes.example"
        }
    }
    email: {
        connection: mail
        from: "hello@notes.example"
    }
    events: {
        store: main
    }
}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb]
}
"#,
);

const TRANSLATIONS: (&str, &str) = (
    "domains/accounts/translations/en_gb.jadpo",
    "translations {\n    open_notes: \"Open your notes\"\n    welcome_email: \"Welcome, {name}\"\n}\n",
);

const OPEN_NOTES: (&str, &str) = (
    "domains/accounts/translations/open_notes.jadpo",
    "/// The welcome email's button.\ntype OpenNotes = Message {}\n",
);

const EMAIL: (&str, &str) = (
    "domains/accounts/emails/welcome_email.jadpo",
    "/// Sent once someone signs up.\ntype WelcomeEmail = EmailMessage {\n    name: Text\n    notes_link: Url\n}\n",
);

const BUTTON: (&str, &str) = (
    "shared/emails/button.jadpo",
    "/// A link set as a button.\ntype Button = EmailPartial {\n    label: PresentationText\n    url: Url\n}\n",
);

const BUTTON_HTML: (&str, &str) = (
    "shared/emails/button.html",
    "<a href=\"{{ url }}\" style=\"padding:12px 24px\">{{ label }}</a>\n",
);

const BUTTON_TEXT: (&str, &str) = ("shared/emails/button.txt", "{{ label }}: {{ url }}\n");

const SIGNATURE: (&str, &str) = (
    "domains/accounts/emails/signature.jadpo",
    "/// How every account email signs off.\ntype Signature = EmailPartial {\n    name: Text\n}\n",
);

const SIGNATURE_HTML: (&str, &str) = (
    "domains/accounts/emails/signature.html",
    "<p>Thanks, {{ name }}.</p>\n",
);

const SIGNATURE_TEXT: (&str, &str) = (
    "domains/accounts/emails/signature.txt",
    "Thanks, {{ name }}.\n",
);

const WELCOME_HTML: &str = "<h1>Welcome, {{ name }}</h1>
{{ include button(label: translations.accounts.open_notes(), url: notes_link) }}
{{ include signature(name: name) }}
";

const WELCOME_TEXT: &str = "Welcome, {{ name }}

{{ include button(label: translations.accounts.open_notes(), url: notes_link) }}
{{ include signature(name: name) }}
";

/// The project with the welcome email's templates and `more`.
fn files<'a>(html: &'a str, text: &'a str, more: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut files = vec![
        APP,
        TRANSLATIONS,
        OPEN_NOTES,
        EMAIL,
        ("domains/accounts/emails/welcome_email.html", html),
        ("domains/accounts/emails/welcome_email.txt", text),
    ];
    files.extend_from_slice(more);
    files
}

fn problems(html: &str, text: &str, more: &[(&str, &str)]) -> Vec<String> {
    support::problems(Layout::Project, &files(html, text, more))
}

const PARTIALS: [(&str, &str); 6] = [
    BUTTON,
    BUTTON_HTML,
    BUTTON_TEXT,
    SIGNATURE,
    SIGNATURE_HTML,
    SIGNATURE_TEXT,
];

#[test]
fn a_shared_partial_and_a_domains_own_are_included_by_their_fields() {
    let found = problems(WELCOME_HTML, WELCOME_TEXT, &PARTIALS);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_partial_reads_its_own_fields_and_the_locale_it_is_given() {
    let found = problems(
        WELCOME_HTML,
        WELCOME_TEXT,
        &[
            BUTTON,
            (
                "shared/emails/button.html",
                "<a dir=\"{{ locale.direction }}\" href=\"{{ url }}\">{{ label }}</a>{{ app.display_name }}\n",
            ),
            BUTTON_TEXT,
            SIGNATURE,
            SIGNATURE_HTML,
            SIGNATURE_TEXT,
        ],
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_partial_that_does_not_exist_is_refused_with_the_nearest() {
    let found = problems(
        "{{ include buton(label: translations.accounts.open_notes(), url: notes_link) }}{{ name }}\n",
        WELCOME_TEXT,
        &PARTIALS,
    );
    assert_eq!(
        found,
        vec![
            "domains/accounts/emails/welcome_email.html email.partial_invalid: `buton` isn't a partial"
        ]
    );
    let db = jadpo_db::Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &files(
            "{{ include buton(label: translations.accounts.open_notes(), url: notes_link) }}{{ name }}\n",
            WELCOME_TEXT,
            &PARTIALS,
        ),
    );
    let diagnostics = jadpo_check::check(&db, project);
    assert_eq!(
        diagnostics[0].next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Include `button`, or declare `type Buton = EmailPartial { ... }` in `shared/emails/` or this domain's `emails/`"
        )
    );
}

#[test]
fn a_partial_is_given_each_of_its_fields_once_by_name() {
    let missing = problems(
        "{{ include button(label: translations.accounts.open_notes()) }}{{ name }}{{ notes_link }}\n",
        WELCOME_TEXT,
        &PARTIALS,
    );
    assert_eq!(
        missing,
        vec![
            "domains/accounts/emails/welcome_email.html email.partial_invalid: `button` is given no `url:`"
        ]
    );
    let unknown = problems(
        "{{ include button(label: translations.accounts.open_notes(), url: notes_link, colour: name) }}\n",
        WELCOME_TEXT,
        &PARTIALS,
    );
    assert_eq!(
        unknown,
        vec![
            "domains/accounts/emails/welcome_email.html email.partial_invalid: `button` has no field `colour`"
        ]
    );
    let unnamed = problems(
        "{{ include button(translations.accounts.open_notes(), url: notes_link) }}{{ name }}\n",
        WELCOME_TEXT,
        &PARTIALS,
    );
    assert_eq!(
        unnamed,
        vec![
            "domains/accounts/emails/welcome_email.html email.partial_invalid: `button` is given a value without its field's name"
        ]
    );
    let locale = problems(
        "{{ include button(label: translations.accounts.open_notes(), url: notes_link, locale: Locale.en_gb) }}{{ name }}\n",
        WELCOME_TEXT,
        &PARTIALS,
    );
    assert_eq!(
        locale,
        vec![
            "domains/accounts/emails/welcome_email.html email.partial_invalid: `button` is given `locale:`, which goes with it"
        ]
    );
}

#[test]
fn a_partials_field_is_checked_against_its_type() {
    let found = problems(
        "{{ include button(label: translations.accounts.open_notes(), url: name) }}{{ notes_link }}\n",
        WELCOME_TEXT,
        &PARTIALS,
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].starts_with("domains/accounts/emails/welcome_email.html type."),
        "{found:#?}"
    );
}

#[test]
fn a_partial_included_from_text_needs_its_txt() {
    let found = problems(
        WELCOME_HTML,
        WELCOME_TEXT,
        &[
            BUTTON,
            BUTTON_HTML,
            SIGNATURE,
            SIGNATURE_HTML,
            SIGNATURE_TEXT,
        ],
    );
    assert_eq!(
        found,
        vec![
            "domains/accounts/emails/welcome_email.txt email.partial_invalid: `button` has no `.txt` template"
        ]
    );
}

#[test]
fn a_partial_in_another_domain_cant_be_included() {
    let found = problems(
        WELCOME_HTML,
        WELCOME_TEXT,
        &[
            BUTTON,
            BUTTON_HTML,
            BUTTON_TEXT,
            (
                "domains/billing/emails/signature.jadpo",
                "/// How every billing email signs off.\ntype Signature = EmailPartial {\n    name: Text\n}\n",
            ),
            (
                "domains/billing/emails/signature.html",
                "<p>{{ name }}</p>\n",
            ),
            ("domains/billing/emails/signature.txt", "{{ name }}\n"),
        ],
    );
    assert_eq!(
        found,
        vec![
            "domains/accounts/emails/welcome_email.html email.partial_invalid: `signature` is domain `billing`'s partial",
            "domains/accounts/emails/welcome_email.txt email.partial_invalid: `signature` is domain `billing`'s partial",
        ]
    );
}

#[test]
fn a_partial_with_no_template_has_nothing_to_include() {
    let found = problems(
        "<h1>{{ name }}</h1>{{ notes_link }}\n",
        "{{ name }}\n",
        &[BUTTON],
    );
    assert_eq!(
        found,
        vec!["shared/emails/button.jadpo email.template_missing: `Button` has no template"]
    );
}

#[test]
fn a_partial_that_includes_itself_is_refused() {
    let found = problems(
        WELCOME_HTML,
        WELCOME_TEXT,
        &[
            BUTTON,
            (
                "shared/emails/button.html",
                "<a href=\"{{ url }}\">{{ label }}</a>{{ include button(label: label, url: url) }}\n",
            ),
            BUTTON_TEXT,
            SIGNATURE,
            SIGNATURE_HTML,
            SIGNATURE_TEXT,
        ],
    );
    assert_eq!(
        found,
        vec!["shared/emails/button.html email.partial_invalid: `button` includes itself"]
    );
}

#[test]
fn a_partial_writes_its_fields_or_says_why_not() {
    let found = problems(
        WELCOME_HTML,
        WELCOME_TEXT,
        &[
            BUTTON,
            (
                "shared/emails/button.html",
                "<a href=\"{{ url }}\">Open</a>\n",
            ),
            ("shared/emails/button.txt", "{{ url }}\n"),
            SIGNATURE,
            SIGNATURE_HTML,
            SIGNATURE_TEXT,
        ],
    );
    assert_eq!(
        found,
        vec!["shared/emails/button.jadpo email.field_unused: `Button.label` is written nowhere"]
    );
}

#[test]
fn a_partial_has_the_locale_it_is_included_in() {
    let found = problems(
        WELCOME_HTML,
        WELCOME_TEXT,
        &[
            (
                "shared/emails/button.jadpo",
                "/// A link set as a button.\ntype Button = EmailPartial {\n    locale: Locale\n    label: PresentationText\n    url: Url\n}\n",
            ),
            BUTTON_HTML,
            BUTTON_TEXT,
            SIGNATURE,
            SIGNATURE_HTML,
            SIGNATURE_TEXT,
        ],
    );
    assert_eq!(
        found,
        vec![
            "shared/emails/button.jadpo type.duplicate_field: `Button` declares `locale`, which every partial has"
        ]
    );
}

#[test]
fn an_email_in_shared_emails_belongs_in_a_domain() {
    let found = support::problems(
        Layout::Project,
        &[
            APP,
            (
                "shared/emails/receipt_email.jadpo",
                "/// A receipt.\ntype ReceiptEmail = EmailMessage {\n    total: Text\n}\n",
            ),
            ("shared/emails/receipt_email.txt", "{{ total }}\n"),
        ],
    );
    assert!(
        found
            .iter()
            .any(|problem| problem.starts_with("shared/emails/receipt_email.jadpo layout.")),
        "{found:#?}"
    );
}
