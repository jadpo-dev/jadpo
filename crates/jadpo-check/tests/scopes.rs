//! Scopes (area 8, "`external_token`"; area 9, "Scopes"; chunk N2): the
//! scopes `external_token:` declares it reads, an entity's `scopes:`
//! naming one per effect, `OAuthScope` narrowed to those declared, a route
//! test's `scopes:`, the permissions report and the access record. Every
//! mistake is refused, and a mistake only ever leaves a scope out, which
//! narrows; what a token may then do at run time is `jadpo-cli`'s
//! `scopes` test.

use jadpo_db::Layout;

use crate::support;

/// A writer, a note, the external token's `scopes:` as `declared` writes
/// it, `Note`'s policy's `scopes:` as `needs` writes it, and `code`.
fn with(declared: &str, needs: &str, code: &str) -> Vec<String> {
    support::problems(
        Layout::Loose,
        &[("main.jadpo", &project(declared, needs, code))],
    )
}

fn project(declared: &str, needs: &str, code: &str) -> String {
    format!(
        r#"config {{
    issuer: Url
    audience: Text
}}

principal {{
    user: {{
        subject: Writer.handle
    }}
}}

authentication {{
    browser_session: {{
        cookie: "notes"
    }}
    external_token: {{
        issuer: config.issuer
        audience: config.audience
{declared}
    }}
}}

type Handle = Text {{
    max_length: 40
}}

type NoteText = Text {{
    max_length: 200
}}

entity Writer {{
    id: Uuid
    handle: Handle {{
        unique: true
    }}

    policy: {{
        read: [id]
    }}
}}

entity Note {{
    id: Uuid
    writer_id: Writer.id {{
        on_delete: OnDelete.cascade
    }}
    text: NoteText

    policy: {{
        create: [writer_id]
        read: [writer_id]
        update: [writer_id]
{needs}
    }}
}}

{code}"#
    )
}

const DECLARED: &str = r#"        scopes: {
            notes_read: "notes:read"
            notes_write: "https://notes.example/write"
        }"#;

const NEEDS: &str = r#"        scopes: {
            read: OAuthScope.notes_read
            create: OAuthScope.notes_write
        }"#;

/// Each problem's rule and message, without its path.
fn found_lines(found: &[String]) -> Vec<String> {
    found
        .iter()
        .map(|line| {
            line.split_once(' ')
                .map_or(line.as_str(), |(_, rest)| rest)
                .to_string()
        })
        .collect()
}

#[test]
fn declared_scopes_and_an_entitys_scopes_check() {
    let found = with(DECLARED, NEEDS, "");
    assert_eq!(found, Vec::<String>::new(), "{found:#?}");
}

#[test]
fn each_mistake_in_a_declaration_is_refused() {
    for (declared, message) in [
        (
            "        scopes: [\"notes:read\"]",
            "semantic.auth_setting_invalid: `scopes:` isn't a block of scopes",
        ),
        (
            "        scopes: {}",
            "semantic.auth_setting_invalid: `scopes:` declares no scope",
        ),
        (
            "        scopes: {\n            NotesRead: \"notes:read\"\n        }",
            "semantic.auth_setting_invalid: `NotesRead` isn't a scope's name",
        ),
        (
            "        scopes: {\n            notes_read: OAuthScope.notes_read\n        }",
            "semantic.auth_setting_invalid: `notes_read:` isn't given its scope-token",
        ),
        (
            "        scopes: {\n            notes_read: \"notes read\"\n        }",
            "semantic.auth_setting_invalid: `notes read` isn't a scope-token",
        ),
        (
            "        scopes: {\n            notes_read: \"\"\n        }",
            "semantic.auth_setting_invalid: `` isn't a scope-token",
        ),
        (
            "        scopes: {\n            notes_read: \"say\\\"hi\\\"\"\n        }",
            "semantic.auth_setting_invalid: `say\"hi\"` isn't a scope-token",
        ),
        (
            "        scopes: {\n            notes_read: \"notes:read\"\n            notes_read: \"notes:all\"\n        }",
            "semantic.auth_setting_invalid: `scopes:` names `notes_read` twice",
        ),
        (
            "        scopes: {\n            notes_read: \"notes:read\"\n            notes_view: \"notes:read\"\n        }",
            "semantic.auth_setting_invalid: `notes_view` and `notes_read` both read `notes:read`",
        ),
        (
            "        scopes: {\n            notes_read: \"notes:read\"\n        }\n        scopes: {\n            notes_write: \"notes:write\"\n        }",
            "semantic.auth_setting_invalid: `external_token:` gives `scopes:` twice",
        ),
    ] {
        let found = found_lines(&with(declared, "", ""));
        assert!(
            found.iter().any(|line| line == message),
            "{declared}: {found:#?}"
        );
        assert!(
            !found.iter().any(|line| line.starts_with("toolchain.")),
            "{declared}: {found:#?}"
        );
    }
}

#[test]
fn each_mistake_in_an_entitys_scopes_is_refused() {
    for (declared, needs, message) in [
        (
            DECLARED,
            "        scopes: [OAuthScope.notes_read]",
            "policy.binding_invalid: `scopes:` isn't a block",
        ),
        (
            DECLARED,
            "        scopes: {\n            view: OAuthScope.notes_read\n        }",
            "policy.effect_unknown: `view` isn't an effect a scope narrows",
        ),
        (
            DECLARED,
            "        scopes: {\n            read: OAuthScope.notes_read\n            read: OAuthScope.notes_write\n        }",
            "policy.effect_duplicate: `scopes:` gives `read:` twice",
        ),
        (
            DECLARED,
            "        scopes: {\n            read: [OAuthScope.notes_read, OAuthScope.notes_write]\n        }",
            "policy.binding_invalid: `read: [OAuthScope.notes_read,OAuthScope.notes_write]` isn't a scope",
        ),
        (
            DECLARED,
            "        scopes: {\n            read: Access.authenticated\n        }",
            "policy.binding_invalid: `read: Access.authenticated` isn't a scope",
        ),
        (
            DECLARED,
            "        scopes: {\n            read: OAuthScope.notes_reed\n        }",
            "policy.binding_invalid: `OAuthScope.notes_reed` isn't a declared scope",
        ),
        (
            "",
            "        scopes: {\n            read: OAuthScope.notes_read\n        }",
            "policy.binding_invalid: `OAuthScope.notes_read` isn't a declared scope",
        ),
        (
            DECLARED,
            "        scopes: {\n            read: OAuthScope.notes_read\n        }\n        scopes: {\n            create: OAuthScope.notes_write\n        }",
            "policy.effect_duplicate: `scopes:` is given twice",
        ),
    ] {
        let found = found_lines(&with(declared, needs, ""));
        assert!(
            found.iter().any(|line| line == message),
            "{needs}: {found:#?}"
        );
    }
}

/// The reason the problem of `rule` in `text` gives.
fn reason_of(text: &str, rule: &str) -> String {
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text)]);
    jadpo_check::check(&db, project)
        .iter()
        .find(|d| d.rule.id == rule)
        .and_then(|d| d.reason.clone())
        .expect("refused")
}

#[test]
fn a_misspelt_scope_is_named_near_what_is_declared() {
    let reason = reason_of(
        &project(
            DECLARED,
            "        scopes: {\n            read: OAuthScope.notes_reed\n        }",
            "",
        ),
        "policy.binding_invalid",
    );
    assert!(
        reason.contains("`OAuthScope.notes_read` and `OAuthScope.notes_write`")
            && reason.contains("Did you mean `OAuthScope.notes_read`?"),
        "{reason}"
    );
    let reason = reason_of(
        &project(
            "",
            "        scopes: {\n            read: OAuthScope.notes_read\n        }",
            "",
        ),
        "policy.binding_invalid",
    );
    assert!(reason.contains("no way in declares the scopes"), "{reason}");
}

const ROUTES: &str = r#"type NoteView = Output {
    pick: [Note.id, Note.text]
}

type NoteMissing = Failure {
    kind: FailureKind.not_found
}

query one_note(note_id: Note.id) fails NoteMissing -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteMissing
}

routes Notes {
    prefix: /notes
    GET /{note_id: Note.id} {
        output: NoteView
        run: one_note(note_id)
    }
}

type HandleTaken = Failure {
    kind: FailureKind.conflict
}

action join(handle: Writer.handle) fails HandleTaken -> Writer {
    return attempt Writer.create {
        set: {
            handle: handle
        }
        conflict: {
            Writer.handle: HandleTaken
        }
    }
}
"#;

/// A route test of the notebook, its one request ending `items`.
fn requesting(declared: &str, items: &str) -> Vec<String> {
    let test = format!(
        r#"test "a token's scopes" {{
    var ann: Writer = attempt join("ann")
    var read: Response = request {{
        method: HttpMethod.get
        path: "/notes/{{note_id}}"
        params: {{ note_id: Note.id("00000000-0000-4000-8000-000000000001") }}
{items}
    }}
    assert read.status == 404
}}
"#
    );
    let main = project(declared, NEEDS, ROUTES).replace(
        "    policy: {\n        read: [id]\n    }",
        "    policy: {\n        create: [Access.public]\n        read: [id]\n    }",
    );
    support::problems(
        Layout::Loose,
        &[
            ("main.jadpo", main.as_str()),
            ("main.test.jadpo", test.as_str()),
        ],
    )
}

#[test]
fn a_route_test_names_the_scopes_its_token_carries() {
    for items in [
        "        as: Principal.user(ann)\n        scopes: [OAuthScope.notes_read]",
        "        as: Principal.user(ann)\n        scopes: [OAuthScope.notes_read, OAuthScope.notes_write]",
        "        as: Principal.user(ann)\n        scopes: []",
    ] {
        let found = requesting(DECLARED, items);
        assert_eq!(found, Vec::<String>::new(), "{items}: {found:#?}");
    }
}

#[test]
fn each_mistake_in_a_route_tests_scopes_is_refused() {
    for (declared, items, message) in [
        (
            DECLARED,
            "        scopes: [OAuthScope.notes_read]",
            "semantic.request_invalid: `scopes:` says which scopes `as:`'s user's token carries, and this request signs no user in",
        ),
        (
            "",
            "        as: Principal.user(ann)\n        scopes: [OAuthScope.notes_read]",
            "semantic.request_invalid: `scopes:` gives a token's scopes, and no way in narrows its tokens",
        ),
        (
            DECLARED,
            "        as: Principal.user(ann)\n        scopes: OAuthScope.notes_read",
            "semantic.request_invalid: `scopes:` takes a list of scopes, `[OAuthScope.<name>]`",
        ),
        (
            DECLARED,
            "        as: Principal.user(ann)\n        scopes: [\"notes:read\"]",
            "semantic.request_invalid: `scopes:` lists scopes, each `OAuthScope.<name>`",
        ),
        (
            DECLARED,
            "        as: Principal.user(ann)\n        scopes: [OAuthScope.notes_all]",
            "semantic.request_invalid: `OAuthScope.notes_all` isn't a scope `external_token:` declares",
        ),
        (
            DECLARED,
            "        as: Principal.user(ann)\n        scopes: [OAuthScope.notes_read, OAuthScope.notes_read]",
            "semantic.request_invalid: `OAuthScope.notes_read` is listed twice",
        ),
        (
            DECLARED,
            "        as: Principal.user(ann)\n        scopes: [OAuthScope.notes_read]\n        scopes: [OAuthScope.notes_write]",
            "semantic.request_item_duplicate: `scopes:` is given twice",
        ),
    ] {
        let found = found_lines(&requesting(declared, items));
        assert!(
            found.iter().any(|line| line == message),
            "{items}: {found:#?}"
        );
        assert!(
            !found.iter().any(|line| line.starts_with("type.")),
            "a scope is named, never read as a value: {items}: {found:#?}"
        );
    }
}

/// The permissions report of `text`: its Markdown, and its JSON without
/// spaces.
fn report_of(text: &str) -> (String, String) {
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", text)]);
    let report = jadpo_check::policy::report::report(&db, project);
    let json = report
        .json
        .to_document()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    (report.markdown, json)
}

#[test]
fn the_permissions_report_shows_each_effects_scope() {
    let (markdown, json) = report_of(&project(DECLARED, NEEDS, ""));
    assert!(markdown.contains("is narrowed to its scopes"), "{markdown}");
    assert!(
        markdown.contains(
            "| scope | `OAuthScope.notes_write` | `OAuthScope.notes_read` | refused | refused |"
        ),
        "{markdown}"
    );
    assert!(
        markdown.contains("| scope | refused | refused | refused | refused |"),
        "the writer, whose policy names no scope: {markdown}"
    );
    assert!(
        json.contains(
            r#""scopes":{"create":"OAuthScope.notes_write","read":"OAuthScope.notes_read"}"#
        ),
        "{json}"
    );
    // An app that narrows no token shows no scope.
    let (markdown, json) = report_of(&project("", "", ""));
    assert!(!markdown.contains("| scope |"), "{markdown}");
    assert!(!markdown.contains("narrowed"), "{markdown}");
    assert!(!json.contains("scopes"), "{json}");
}
