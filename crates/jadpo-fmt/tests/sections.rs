//! The order of sections inside a declaration (area 14, decided
//! 2026-10-13): the formatter puts a failure's, a route entry's and an
//! entity's sections in one order, moves each section's comments with it,
//! and keeps fields and everything it does not order in written order.

use jadpo_fmt::{format_file, format_snippet};

fn fmt(text: &str) -> String {
    let out = format_file(text).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert_eq!(
        format_file(&out).expect("formatted output parses"),
        out,
        "formatting is idempotent"
    );
    out
}

#[test]
fn a_failure_is_kind_code_message_client_internal() {
    assert_eq!(
        fmt(concat!(
            "type QuantityUnavailable = Failure {\n",
            "    internal: {\n",
            "        product_id: Product.id\n",
            "    }\n",
            "    client: {\n",
            "        available: Quantity\n",
            "    }\n",
            "    message: \"The requested quantity is unavailable.\"\n",
            "    code: \"quantity_missing\"\n",
            "    kind: FailureKind.conflict\n",
            "}\n",
        )),
        concat!(
            "type QuantityUnavailable = Failure {\n",
            "    kind: FailureKind.conflict\n",
            "    code: \"quantity_missing\"\n",
            "    message: \"The requested quantity is unavailable.\"\n",
            "    client: {\n",
            "        available: Quantity\n",
            "    }\n",
            "    internal: {\n",
            "        product_id: Product.id\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn other_types_keep_their_written_order() {
    let text = "type Size = Enum {\n    small\n    medium\n}\n\ntype Line = Object {\n    message: Text\n    kind: Text\n}\n";
    assert_eq!(fmt(text), text);
}

#[test]
fn a_route_is_authentication_rate_limit_input_output_status_deadline_run() {
    assert_eq!(
        fmt(concat!(
            "routes Imports {\n",
            "    prefix: /imports\n",
            "    POST / {\n",
            "        run: import_todos(input)\n",
            "        deadline: 30s\n",
            "        status: SuccessStatus.accepted\n",
            "        output: ImportResult\n",
            "        input: ImportTodos\n",
            "        rate_limit: imports\n",
            "        confirm: Confirmation.passkey\n",
            "        authentication: RouteAuthentication.fresh\n",
            "    }\n",
            "}\n",
        )),
        concat!(
            "routes Imports {\n",
            "    prefix: /imports\n",
            "    POST / {\n",
            "        authentication: RouteAuthentication.fresh\n",
            "        confirm: Confirmation.passkey\n",
            "        rate_limit: imports\n",
            "        input: ImportTodos\n",
            "        output: ImportResult\n",
            "        status: SuccessStatus.accepted\n",
            "        deadline: 30s\n",
            "        run: import_todos(input)\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn an_entity_is_fields_then_its_sections() {
    assert_eq!(
        fmt(concat!(
            "entity CompanyMembership {\n",
            "    policy: {\n",
            "        read: [CompanyRole.owner]\n",
            "    }\n",
            "    id: Uuid\n",
            "    identity: id\n",
            "    membership: {\n",
            "        scope: company_id\n",
            "    }\n",
            "    company_id: Company.id\n",
            "    persistence: {\n",
            "        store: primary\n",
            "    }\n",
            "    role: CompanyRole\n",
            "}\n",
        )),
        concat!(
            "entity CompanyMembership {\n",
            "    id: Uuid\n",
            "    company_id: Company.id\n",
            "    role: CompanyRole\n",
            "    identity: id\n",
            "    persistence: {\n",
            "        store: primary\n",
            "    }\n",
            "    membership: {\n",
            "        scope: company_id\n",
            "    }\n",
            "    policy: {\n",
            "        read: [CompanyRole.owner]\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn every_listed_entity_section_has_its_place() {
    assert_eq!(
        fmt(concat!(
            "entity User {\n",
            "    person: true\n",
            "    guard: optimistic\n",
            "    freshness: authoritative\n",
            "    policy: {}\n",
            "    membership: {}\n",
            "    lifecycle: {}\n",
            "    persistence: {}\n",
            "    identity: id\n",
            "    id: Uuid\n",
            "}\n",
        )),
        concat!(
            "entity User {\n",
            "    id: Uuid\n",
            "    identity: id\n",
            "    persistence: {}\n",
            "    lifecycle: {}\n",
            "    membership: {}\n",
            "    policy: {}\n",
            "    freshness: authoritative\n",
            "    guard: optimistic\n",
            "    person: true\n",
            "}\n",
        )
    );
}

#[test]
fn other_entity_sections_follow_the_listed_ones_in_written_order() {
    assert_eq!(
        fmt(concat!(
            "entity Invoice {\n",
            "    search: {}\n",
            "    id: Uuid\n",
            "    retention: {}\n",
            "    identity: id\n",
            "    audit: {}\n",
            "    issued_at: Instant\n",
            "    unique: {}\n",
            "}\n",
        )),
        concat!(
            "entity Invoice {\n",
            "    id: Uuid\n",
            "    issued_at: Instant\n",
            "    identity: id\n",
            "    search: {}\n",
            "    retention: {}\n",
            "    audit: {}\n",
            "    unique: {}\n",
            "}\n",
        )
    );
}

#[test]
fn fields_are_never_sorted() {
    let text = "entity Todo {\n    title: TodoTitle\n    id: Uuid\n    due_at: Instant?\n\n    identity: id\n}\n";
    assert_eq!(fmt(text), text);
}

#[test]
fn comments_move_with_their_section() {
    assert_eq!(
        fmt(concat!(
            "type TodoNotFound = Failure {\n",
            "    // Never shown to the caller.\n",
            "    internal: {\n",
            "        todo_id: Todo.id  // the one asked for\n",
            "    }\n",
            "\n",
            "    /// Fixed, safe text.\n",
            "    message: \"Todo not found.\"  // shown as is\n",
            "    kind: FailureKind.not_found  // 404\n",
            "    // nothing after kind\n",
            "}\n",
        )),
        concat!(
            "type TodoNotFound = Failure {\n",
            "    kind: FailureKind.not_found  // 404\n",
            "\n",
            "    /// Fixed, safe text.\n",
            "    message: \"Todo not found.\"  // shown as is\n",
            "    // Never shown to the caller.\n",
            "    internal: {\n",
            "        todo_id: Todo.id  // the one asked for\n",
            "    }\n",
            "    // nothing after kind\n",
            "}\n",
        )
    );
}

#[test]
fn a_section_keeps_the_blank_line_written_before_it() {
    assert_eq!(
        fmt(concat!(
            "entity Todo {\n",
            "    id: Uuid\n",
            "\n",
            "    persistence: {\n",
            "        store: primary\n",
            "    }\n",
            "\n",
            "    identity: id\n",
            "    title: TodoTitle\n",
            "}\n",
        )),
        concat!(
            "entity Todo {\n",
            "    id: Uuid\n",
            "    title: TodoTitle\n",
            "\n",
            "    identity: id\n",
            "\n",
            "    persistence: {\n",
            "        store: primary\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn elisions_stay_with_the_fields() {
    assert_eq!(
        format_snippet("entity Todo {\n    identity: id\n    ...\n    due_at: Instant?\n}\n")
            .expect("formats"),
        "entity Todo {\n    ...\n    due_at: Instant?\n    identity: id\n}\n"
    );
}

/// Area 10, "Fields in name order" (2026-10-16, built in chunk 5.3f):
/// `config { }`'s fields sorted by name, each with the comments above it
/// and after it on its line, so parallel work adding settings rarely
/// touches the same lines.
#[test]
fn config_fields_are_sorted_by_name_with_their_comments() {
    assert_eq!(
        fmt(concat!(
            "config {\n",
            "    /// Where mail comes from.\n",
            "    mail_sender: Email\n",
            "    // Tuned per deployment.\n",
            "    page_size: ListLimit {\n",
            "        default: 25\n",
            "    }\n",
            "    business_zone: Zone {\n",
            "        default: Zone.europe_london\n",
            "    }  // the shop's own\n",
            "    database_url: PostgresUrl\n",
            "}\n",
        )),
        concat!(
            "config {\n",
            "    business_zone: Zone {\n",
            "        default: Zone.europe_london\n",
            "    }  // the shop's own\n",
            "    database_url: PostgresUrl\n",
            "    /// Where mail comes from.\n",
            "    mail_sender: Email\n",
            "    // Tuned per deployment.\n",
            "    page_size: ListLimit {\n",
            "        default: 25\n",
            "    }\n",
            "}\n",
        )
    );
}

#[test]
fn config_fields_in_name_order_stay_as_they_are() {
    let text = "config {\n    browser_origin: Url\n    database_url: PostgresUrl\n    soft_delete_retention: Duration {\n        default: 30d\n    }\n}\n";
    assert_eq!(fmt(text), text);
}

#[test]
fn other_singletons_keep_their_written_order() {
    let text = "application {\n    name: \"shop\"\n    api: {\n        prefix: /api\n    }\n}\n";
    assert_eq!(fmt(text), text);
}
