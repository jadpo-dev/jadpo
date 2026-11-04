//! A route that sends raw bytes and an answer that never changes (area 7,
//! "Sending raw bytes" and "Answers that never change"; chunk G3): what
//! `sends:` and `cache: immutable` are refused on, what the API record and
//! OpenAPI say, and that field policy and `personal:` marks hold for the
//! field whatever the route does with it.

use crate::support;

use std::collections::BTreeMap;

use jadpo_db::{Database, Layout};
use jadpo_graph::json::Value;

const ROUTE_HEAD: &str = "use shop/actions { worked_out }\nuse shop/queries { all_listed, archive_of, marked_scan, scan_through }\nuse shop/types { Archive, Listing, Maybe, NotTar, Reference, Sealed, Twin, Worded }\n\nroutes Archives {\n    prefix: /archives\n\n";

fn base() -> Vec<(String, String)> {
    [
        ("app.jadpo", "application {}\n"),
        (
            "domains/shop/types/reference.jadpo",
            "type Reference = Text {\n    max_length: 71\n}\n",
        ),
        (
            "domains/shop/types/archive.jadpo",
            "type Archive = Output {\n    tar: Bytes\n}\n",
        ),
        (
            "domains/shop/types/not_tar.jadpo",
            "use shop/types { Raw }\n\ntype NotTar = Output {\n    tar: Raw\n}\n",
        ),
        ("domains/shop/types/raw.jadpo", "type Raw = Bytes {}\n"),
        (
            "domains/shop/types/twin.jadpo",
            "type Twin = Output {\n    tar: Bytes\n    other: Bytes\n}\n",
        ),
        (
            "domains/shop/types/maybe.jadpo",
            "type Maybe = Output {\n    tar: Bytes?\n}\n",
        ),
        (
            "domains/shop/types/worded.jadpo",
            "use shop/types { Reference }\n\ntype Worded = Output {\n    digest: Reference\n}\n",
        ),
        (
            "domains/shop/types/listing.jadpo",
            "use shop/types { Reference }\n\ntype Listing = Output {\n    digest: Reference\n}\n",
        ),
        (
            "domains/shop/types/sealed.jadpo",
            "type Sealed = Output {\n    secret: Bytes\n}\n",
        ),
        (
            "domains/shop/entities/release.jadpo",
            "use shop/failures { ArchiveNotFound }\nuse shop/types { Reference }\n\nentity Release {\n    id: Uuid\n    digest: Reference {\n        unique: true\n    }\n    tar: Bytes\n    secret: Bytes {\n        policy: FieldAccess.server_only\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/shop/entities/scan.jadpo",
            "use shop/types { Reference }\n\nentity Scan {\n    id: Uuid\n    digest: Reference {\n        unique: true\n    }\n    tar: Bytes {\n        personal: PersonalData.health\n    }\n    person: true\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/shop/failures/archive_not_found.jadpo",
            "type ArchiveNotFound = Failure {\n    kind: FailureKind.not_found\n    message: \"No release has this digest.\"\n}\n",
        ),
        (
            "domains/shop/queries/archive_of.jadpo",
            "use shop/entities { Release }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { Archive, Reference }\n\nquery archive_of(digest: Reference) fails ArchiveNotFound -> Archive {\n    from: Release\n    where: Release.digest == digest\n    missing: ArchiveNotFound\n}\n",
        ),
        (
            "domains/shop/queries/all_listed.jadpo",
            "use shop/entities { Release }\nuse shop/types { Listing }\n\nquery all_listed() -> List<Listing> {\n    from: Release\n    order: [Release.digest asc]\n    limit: 10\n}\n",
        ),
        (
            "domains/shop/queries/marked_scan.jadpo",
            "use shop/entities { Scan }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { Archive, Reference }\n\nquery marked_scan(digest: Reference) fails ArchiveNotFound -> Archive {\n    from: Scan\n    where: Scan.digest == digest\n    missing: ArchiveNotFound\n}\n",
        ),
        (
            "domains/shop/queries/scan_through.jadpo",
            "use shop/entities { Scan }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { Reference }\n\nquery scan_through(digest: Reference) fails ArchiveNotFound -> Scan {\n    from: Scan\n    where: Scan.digest == digest\n    missing: ArchiveNotFound\n}\n",
        ),
        (
            "domains/shop/actions/worked_out.jadpo",
            "use shop/failures { ArchiveNotFound }\nuse shop/queries { archive_of }\nuse shop/types { Archive, Reference }\n\naction worked_out(digest: Reference) fails ArchiveNotFound -> Archive {\n    return attempt archive_of(digest)\n}\n",
        ),
        (
            "policy.jadpo",
            "use shop\n\npolicy {\n    actions: {\n        shop.worked_out: [Access.public]\n    }\n}\n",
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect()
}

/// The base project with `routes` as its route group's and `more` files.
fn with(routes: &str, more: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut files = base();
    files.push((
        "domains/shop/routes/archives.jadpo".to_string(),
        format!("{ROUTE_HEAD}{routes}}}\n"),
    ));
    for (path, text) in more {
        files.retain(|(held, _)| held != path);
        files.push((path.to_string(), text.to_string()));
    }
    files
}

const IGNORED: [&str; 3] = [
    "import.unused",
    "layout.unused_declaration",
    "record.out_of_date",
];

/// Each problem `files` gives, as `rule: message`.
fn problems(files: &[(String, String)]) -> Vec<String> {
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project_with_migrated_records(&db, Layout::Project, &files, &[]);
    jadpo_check::check(&db, project)
        .iter()
        .filter(|d| !IGNORED.contains(&d.rule.id))
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

fn route(items: &str) -> String {
    format!(
        "    GET /{{digest: Reference}} {{\n        authentication: RouteAuthentication.none\n{items}    }}\n"
    )
}

const SENT: &str = "        output: Archive\n        sends: \"application/x-tar\"\n        run: archive_of(digest)\n";

#[test]
fn a_query_or_a_read_only_action_gives_what_is_sent() {
    let both = format!(
        "{}\n    GET /worked/{{digest: Reference}} {{\n        authentication: RouteAuthentication.none\n        output: Archive\n        sends: \"application/x-tar\"\n        cache: immutable\n        run: worked_out(digest)\n    }}\n",
        route(&format!("{SENT}        cache: immutable\n"))
    );
    assert_eq!(problems(&with(&both, &[])), Vec::<String>::new());
    // A field typed on `Bytes` is `Bytes`.
    let refined = route(
        "        output: NotTar\n        sends: \"application/x-tar\"\n        run: body_of(digest)\n",
    );
    let group = format!(
        "use shop/queries {{ body_of }}\nuse shop/types {{ NotTar, Reference }}\n\nroutes Archives {{\n    prefix: /archives\n\n{refined}}}\n"
    );
    let files = with(
        &refined,
        &[
            ("domains/shop/routes/archives.jadpo", &group),
            (
                "domains/shop/entities/sheet.jadpo",
                "use shop/types { Raw, Reference }\n\nentity Sheet {\n    id: Uuid\n    digest: Reference {\n        unique: true\n    }\n    tar: Raw\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
            ),
            (
                "domains/shop/queries/body_of.jadpo",
                "use shop/entities { Sheet }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { NotTar, Reference }\n\nquery body_of(digest: Reference) fails ArchiveNotFound -> NotTar {\n    from: Sheet\n    where: Sheet.digest == digest\n    missing: ArchiveNotFound\n}\n",
            ),
        ],
    );
    assert_eq!(problems(&files), Vec::<String>::new());
}

/// `output`, what runs and the rule's message: each `Output` that isn't one
/// `Bytes` field.
#[test]
fn sends_needs_exactly_one_bytes_field() {
    for (ty, query, expected) in [
        (
            "Twin",
            "twin_of",
            "`GET /archives/{digest}` sends raw bytes, but `Twin` isn't exactly one `Bytes` field",
        ),
        (
            "Maybe",
            "maybe_of",
            "`GET /archives/{digest}` sends raw bytes, but `Maybe` isn't exactly one `Bytes` field",
        ),
        (
            "Worded",
            "worded_of",
            "`GET /archives/{digest}` sends raw bytes, but `Worded` isn't exactly one `Bytes` field",
        ),
    ] {
        let routes = route(&format!(
            "        output: {ty}\n        sends: \"application/x-tar\"\n        run: {query}(digest)\n"
        ));
        let declared = format!(
            "use shop/entities {{ Release }}\nuse shop/failures {{ ArchiveNotFound }}\nuse shop/types {{ Reference, {ty} }}\n\nquery {query}(digest: Reference) fails ArchiveNotFound -> {ty} {{\n    from: Release\n    where: Release.digest == digest\n    missing: ArchiveNotFound\n}}\n"
        );
        let group = format!(
            "use shop/queries {{ {query} }}\nuse shop/types {{ Reference, {ty} }}\n\nroutes Archives {{\n    prefix: /archives\n\n{routes}}}\n"
        );
        let path = format!("domains/shop/queries/{query}.jadpo");
        let files = with(
            &routes,
            &[
                ("domains/shop/routes/archives.jadpo", &group),
                (&path, &declared),
            ],
        );
        let found = problems(&files);
        assert!(
            found
                .iter()
                .any(|p| p == &format!("route.sends_invalid: {expected}")),
            "{ty}: {found:#?}"
        );
    }
    // A list isn't one field either.
    let routes = "    GET / {\n        authentication: RouteAuthentication.none\n        output: List<Listing>\n        sends: \"application/x-tar\"\n        run: all_listed()\n    }\n";
    let found = problems(&with(routes, &[]));
    assert!(
        found.iter().any(|p| p.starts_with("route.sends_invalid: ")
            && p.contains("`List<Listing>` isn't an `Output`")),
        "{found:#?}"
    );
}

#[test]
fn sends_needs_a_media_type() {
    for (written, expected) in [
        ("\"tar\"", "`tar` isn't a media type"),
        ("\"application/\"", "`application/` isn't a media type"),
        (
            "\"application/x tar\"",
            "`application/x tar` isn't a media type",
        ),
        (
            "\"Application/X-Tar\"",
            "`Application/X-Tar` isn't a media type",
        ),
        (
            "\"text/plain; charset\"",
            "`text/plain; charset` isn't a media type",
        ),
        (
            "\"application/json\"",
            "`application/json` isn't a media type",
        ),
        (
            "\"application/problem+json\"",
            "`application/problem+json` isn't a media type",
        ),
        (
            "\"text/event-stream\"",
            "`text/event-stream` isn't a media type",
        ),
        ("tar", "gives `sends:` something other than text"),
    ] {
        let routes = route(&format!(
            "        output: Archive\n        sends: {written}\n        run: archive_of(digest)\n"
        ));
        let found = problems(&with(&routes, &[]));
        assert!(
            found.len() == 1
                && found[0].starts_with("route.sends_invalid: ")
                && found[0].contains(expected),
            "{written}: {found:#?}"
        );
    }
    for written in [
        "image/svg+xml",
        "text/csv; charset=utf-8",
        "application/octet-stream",
    ] {
        let routes = route(&format!(
            "        output: Archive\n        sends: \"{written}\"\n        run: archive_of(digest)\n"
        ));
        assert_eq!(
            problems(&with(&routes, &[])),
            Vec::<String>::new(),
            "{written}"
        );
    }
}

#[test]
fn cache_immutable_is_for_a_get_anyone_may_call_answering_200() {
    fn refused(routes: &str, expected: &str) {
        let found = problems(&with(routes, &[]));
        assert!(
            found.len() == 1
                && found[0].starts_with("route.cache_invalid: ")
                && found[0].contains(expected),
            "{expected}: {found:#?}"
        );
    }
    refused(
        &route(&format!("{SENT}        cache: forever\n")),
        "`forever` isn't something a route's `cache:` says",
    );
    refused(
        &route(&format!("{SENT}        cache: \"immutable\"\n")),
        "isn't something a route's `cache:` says",
    );
    refused(
        "    POST /{digest: Reference} {\n        authentication: RouteAuthentication.none\n        output: Archive\n        cache: immutable\n        run: worked_out(digest)\n    }\n",
        "`POST /archives/{digest}` keeps its answer with `cache: immutable`",
    );
    refused(
        "    GET /{digest: Reference} {\n        output: Archive\n        cache: immutable\n        run: archive_of(digest)\n    }\n",
        "`GET /archives/{digest}` needs a signed-in caller, but says `cache: immutable`",
    );
    refused(
        &route(
            "        output: Archive\n        status: SuccessStatus.accepted\n        cache: immutable\n        run: archive_of(digest)\n",
        ),
        "answers `SuccessStatus.accepted`, but says `cache: immutable`",
    );
    // A signed-in route may send bytes; it just isn't kept.
    let signed_in = "    GET /{digest: Reference} {\n        output: Archive\n        sends: \"application/x-tar\"\n        run: archive_of(digest)\n    }\n";
    let found = problems(&with(signed_in, &[]));
    assert!(
        !found.iter().any(|p| p.starts_with("route.sends_invalid")),
        "{found:#?}"
    );
}

#[test]
fn a_route_that_sends_bytes_keeps_field_policy() {
    // `server_only` is refused however the field is sent.
    let routes = route(
        "        output: Sealed\n        sends: \"application/x-tar\"\n        run: secret_of(digest)\n",
    );
    let group = format!(
        "use shop/queries {{ secret_of }}\nuse shop/types {{ Reference, Sealed }}\n\nroutes Archives {{\n    prefix: /archives\n\n{routes}}}\n"
    );
    let found = problems(&with(
        &routes,
        &[
            ("domains/shop/routes/archives.jadpo", &group),
            (
                "domains/shop/queries/secret_of.jadpo",
                "use shop/entities { Release }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { Reference, Sealed }\n\nquery secret_of(digest: Reference) fails ArchiveNotFound -> Sealed {\n    from: Release\n    where: Release.digest == digest\n    missing: ArchiveNotFound\n}\n",
            ),
        ],
    ));
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("policy.output_field_unproved: ")),
        "{found:#?}"
    );
}

#[test]
fn a_public_cache_never_keeps_what_is_marked_personal() {
    // Sent raw and not kept, a marked field is as any other route's.
    let sent = route(
        "        output: Archive\n        sends: \"application/x-tar\"\n        run: marked_scan(digest)\n",
    );
    assert_eq!(problems(&with(&sent, &[])), Vec::<String>::new());
    // Kept, from a query.
    let kept = route(
        "        output: Archive\n        sends: \"application/x-tar\"\n        cache: immutable\n        run: marked_scan(digest)\n",
    );
    let found = problems(&with(&kept, &[]));
    assert!(
        found.len() == 1
            && found[0].starts_with("route.cache_invalid: ")
            && found[0].contains("`Scan.tar`"),
        "{found:#?}"
    );
    // Kept, through an action.
    let action = "use shop/entities { Scan }\nuse shop/failures { ArchiveNotFound }\nuse shop/queries { scan_through }\nuse shop/types { Archive, Reference }\n\naction worked_out(digest: Reference) fails ArchiveNotFound -> Archive {\n    var scan: Scan = attempt scan_through(digest)\n    return Archive {\n        tar: scan.tar\n    }\n}\n";
    let kept = route(
        "        output: Archive\n        sends: \"application/x-tar\"\n        cache: immutable\n        run: worked_out(digest)\n",
    );
    let found = problems(&with(
        &kept,
        &[("domains/shop/actions/worked_out.jadpo", action)],
    ));
    assert!(
        found.len() == 1
            && found[0].starts_with("route.cache_invalid: ")
            && found[0].contains("`Scan.tar`"),
        "{found:#?}"
    );
}

/// A standard type that is personal data counts as a marked field does, as
/// it does in the personal-data report (area 15, "Where personal data goes"):
/// no erasure reaches a public cache either way.
#[test]
fn a_public_cache_never_keeps_a_standard_type_that_is_personal() {
    let customer = (
        "domains/shop/entities/customer.jadpo",
        "use shop/types { Reference }\n\nentity Customer {\n    id: Uuid\n    digest: Reference {\n        unique: true\n    }\n    address: PhoneNumber\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    let reach = (
        "domains/shop/types/reach.jadpo",
        "use shop/entities { Customer }\n\ntype Reach = Output {\n    pick: [Customer.address]\n}\n",
    );
    let label = (
        "domains/shop/types/label.jadpo",
        "use shop/entities { Customer }\n\ntype Label = Output {\n    pick: [Customer.digest]\n}\n",
    );
    let reach_of = (
        "domains/shop/queries/reach_of.jadpo",
        "use shop/entities { Customer }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { Reach, Reference }\n\nquery reach_of(digest: Reference) fails ArchiveNotFound -> Reach {\n    from: Customer\n    where: Customer.digest == digest\n    missing: ArchiveNotFound\n}\n",
    );
    let label_of = (
        "domains/shop/queries/label_of.jadpo",
        "use shop/entities { Customer }\nuse shop/failures { ArchiveNotFound }\nuse shop/types { Label, Reference }\n\nquery label_of(digest: Reference) fails ArchiveNotFound -> Label {\n    from: Customer\n    where: Customer.digest == digest\n    missing: ArchiveNotFound\n}\n",
    );
    // The route group, importing what these routes use.
    let group = |routes: &str| {
        format!(
            "use shop/actions {{ written }}\nuse shop/queries {{ label_of, reach_of }}\nuse shop/types {{ Label, Reach, Reference, Typed }}\n\nroutes Archives {{\n    prefix: /archives\n\n{routes}}}\n"
        )
    };
    let typed = (
        "domains/shop/types/typed.jadpo",
        "type Typed = Output {\n    address: PhoneNumber\n}\n",
    );
    let written = (
        "domains/shop/actions/written.jadpo",
        "use shop/types { Reference, Typed }\n\naction written(digest: Reference, address: PhoneNumber) -> Typed {\n    assert digest != \"\"\n    return Typed {\n        address: address\n    }\n}\n",
    );
    let policy = (
        "policy.jadpo",
        "use shop\n\npolicy {\n    actions: {\n        shop.written: [Access.public]\n    }\n}\n",
    );
    let found_in = |routes: &str| {
        let group = group(routes);
        problems(&with(
            "",
            &[
                customer,
                reach,
                label,
                reach_of,
                label_of,
                typed,
                written,
                policy,
                ("domains/shop/routes/archives.jadpo", group.as_str()),
            ],
        ))
    };
    // Not kept, it is as any other route's.
    let sent = route("        output: Reach\n        run: reach_of(digest)\n");
    assert_eq!(found_in(&sent), Vec::<String>::new());
    // Kept, from a field no `personal:` marks.
    let kept =
        route("        output: Reach\n        cache: immutable\n        run: reach_of(digest)\n");
    let found = found_in(&kept);
    assert!(
        found.len() == 1
            && found[0].starts_with("route.cache_invalid: ")
            && found[0].contains("`Customer.address`, a `PhoneNumber`, which is personal data"),
        "{found:#?}"
    );
    // Kept, what the record holds but the answer doesn't give is no matter.
    let label =
        route("        output: Label\n        cache: immutable\n        run: label_of(digest)\n");
    assert_eq!(found_in(&label), Vec::<String>::new());
    // Kept, what the output's own type says it holds, wherever it came from.
    let kept = "    GET /{digest: Reference}/{address: PhoneNumber} {\n        authentication: RouteAuthentication.none\n        output: Typed\n        cache: immutable\n        run: written(digest, address)\n    }\n";
    let found = found_in(kept);
    assert!(
        found.len() == 1
            && found[0].starts_with("route.cache_invalid: ")
            && found[0].contains("a `PhoneNumber`, which is personal data"),
        "{found:#?}"
    );
}

/// The API record and OpenAPI of `routes`.
fn written(routes: &str) -> (String, Value) {
    let files = with(routes, &[]);
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project_with_migrated_records(&db, Layout::Project, &files, &[]);
    let record = jadpo_check::routes::record::record_texts(&db, project)
        .get("main")
        .expect("the record")
        .clone();
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "archives", "1")
        .expect("the document");
    (record, document)
}

/// The part of `value` each of `path` names, in turn.
fn at<'v>(value: &'v Value, path: &[&str]) -> Option<&'v Value> {
    path.iter().try_fold(value, |value, key| value.get(key))
}

const OPERATION: [&str; 3] = ["paths", "/archives/{digest}", "get"];

#[test]
fn the_record_and_openapi_say_what_is_sent() {
    let (record, document) = written(&route(&format!("{SENT}        cache: immutable\n")));
    assert!(
        record.contains(
            "GET /archives/{digest: Text { max_length: 71 }}\n    authentication none\n    output Archive\n    sends application/x-tar\n    status 200\n    cache immutable\n    fails archive_not_found\n"
        ),
        "{record}"
    );
    assert!(
        record.contains("output Archive\n    tar: Bytes\n"),
        "{record}"
    );

    let shown = document.to_document();
    let operation = at(&document, &OPERATION).expect("the route");
    let answer = at(operation, &["responses", "200"]).expect("its 200");
    assert_eq!(
        at(answer, &["content", "application/x-tar", "x-jadpo-sends"]),
        Some(&Value::from(BTreeMap::from([
            ("field".to_string(), Value::from("tar")),
            ("output".to_string(), Value::from("Archive")),
        ]))),
        "{shown}"
    );
    assert!(
        at(answer, &["content", "application/json"]).is_none(),
        "{shown}"
    );
    assert_eq!(
        at(answer, &["headers", "Content-Length", "schema", "type"]),
        Some(&Value::from("integer")),
        "{shown}"
    );
    assert_eq!(
        at(answer, &["headers", "Cache-Control", "schema", "const"]),
        Some(&Value::from("public, max-age=31536000, immutable")),
        "{shown}"
    );
    assert_eq!(
        operation.get("x-jadpo-cache"),
        Some(&Value::from("immutable")),
        "{shown}"
    );
    // Failures stay JSON, in the envelope.
    assert!(
        at(
            operation,
            &["responses", "404", "content", "application/json", "schema"]
        )
        .is_some(),
        "{shown}"
    );
    // The body's fields aren't JSON, so no schema describes them.
    assert!(
        at(&document, &["components", "schemas", "Archive"]).is_none(),
        "{shown}"
    );
}

#[test]
fn without_cache_nothing_says_it_may_be_kept() {
    let (record, document) = written(&route(SENT));
    assert!(!record.contains("cache"), "{record}");
    let operation = at(&document, &OPERATION).expect("the route");
    assert!(operation.get("x-jadpo-cache").is_none());
    assert!(at(operation, &["responses", "200", "headers", "Cache-Control"]).is_none());
}
