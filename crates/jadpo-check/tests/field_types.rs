//! A field's type is a type (area 2, "Field types"): a value or a call
//! written where a field's type goes is refused, naming the field, in
//! every block of fields, and never quietly dropped; and an enum's
//! variants are names, never given labels (area 2, "Enums"; chunk 7.9f).

use crate::support;

use jadpo_db::Layout;

fn check(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", source)])
}

fn found(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", source)])
}

const STATUS: &str = "type Status = Enum {\n    open\n    closed\n}\n\ntype Label = Text {\n    max_length: 20\n}\n\n";

fn with_status(source: &str) -> Vec<String> {
    check(&format!("{STATUS}{source}"))
}

#[test]
fn a_value_or_a_call_is_never_a_fields_type() {
    let cases: &[&str] = &[
        // Richard's two, in an input.
        "type HandedIn = Input {\n    label: Label\n    testy: Status(\"Ajuij\")\n}\n",
        "type HandedIn = Input {\n    label: Label\n    testy: Status.open\n}\n",
        // Literals, and a variant made nullable or given settings.
        "type Thing = Object {\n    count: 5\n}\n",
        "type Thing = Object {\n    name: \"Mug\"\n}\n",
        "type Thing = Output {\n    status: Status.open?\n}\n",
        "type Thing = Output {\n    label: Label(\"x\") {\n        max_length: 3\n    }\n}\n",
        // A variant inside a collection's type argument.
        "type Thing = Output {\n    statuses: List<Status.open>\n}\n",
        // Every other block of fields.
        "type Opened = Event {\n    status: Status(\"open\")\n}\n",
        "type Missing = Failure {\n    kind: FailureKind.not_found\n    client: {\n        status: Status.open\n    }\n}\n",
        "type Outcome = Enum {\n    paid: {\n        label: Label(\"x\")\n    }\n}\n",
        "entity Ticket {\n    id: Uuid\n    status: Status.open\n}\n",
        "entity Ticket {\n    id: Uuid\n    status: Status(\"open\")\n}\n",
        "config {\n    starting_status: Status.open\n}\n",
        "config {\n    starting_status: Status(\"open\")\n}\n",
        // A variant is no type in a signature either.
        "function first(status: Status.open) -> Label {\n    return \"x\"\n}\n",
        "function first(statuses: List<Status.closed>) -> Label {\n    return \"x\"\n}\n",
    ];
    for source in cases {
        assert_eq!(with_status(source), ["type.not_a_type"], "for:\n{source}");
    }
}

#[test]
fn a_block_of_fields_or_a_swift_list_is_no_type_either() {
    // There is no unnamed object (area 2, "Objects"), and `[Label]` is
    // Swift's list type.
    assert_eq!(
        with_status(
            "type Thing = Object {\n    label: Label\n    nested: {\n        a: Label\n    }\n}\n\nfunction make(label: Label) -> Thing {\n    return Thing {\n        label: label\n    }\n}\n"
        ),
        ["type.not_a_type", "type.missing_field"]
    );
    assert_eq!(
        with_status("type Thing = Object {\n    labels: [Label]\n}\n"),
        ["syntax.collection_type"]
    );
    assert_eq!(
        fixed(&format!(
            "{STATUS}type Thing = Object {{\n    labels: [Label]\n}}\n"
        )),
        format!("{STATUS}type Thing = Object {{\n    labels: List<Label>\n}}\n")
    );
}

#[test]
fn the_error_names_the_field_and_what_was_written() {
    let problems = found(&format!(
        "{STATUS}type HandedIn = Input {{\n    testy: Status(\"Ajuij\")\n}}\n"
    ));
    let joined = problems.join("\n");
    assert!(
        joined.contains("`testy`") && joined.contains("`Status(\"Ajuij\")`"),
        "{joined}"
    );
    let problems = found(&format!(
        "{STATUS}type HandedIn = Input {{\n    testy: Status.open\n}}\n"
    ));
    let joined = problems.join("\n");
    assert!(
        joined.contains("`testy`") && joined.contains("`Status.open`"),
        "{joined}"
    );
}

#[test]
fn a_field_written_as_a_value_is_never_dropped() {
    // The field stays, its type absorbing: building the input without it
    // is not a second error, and neither is giving it.
    assert_eq!(
        with_status(
            "type HandedIn = Input {\n    label: Label\n    testy: Status(\"x\")\n}\n\nfunction make(label: Label) -> HandedIn {\n    return HandedIn {\n        label: label\n        testy: Status.open\n    }\n}\n"
        ),
        ["type.not_a_type"]
    );
}

#[test]
fn types_in_every_written_form_still_check() {
    assert_eq!(
        with_status(
            "type Thing = Object {\n    status: Status\n    maybe: Status?\n    many: List<Status>\n    label: Label {\n        max_length: 10\n    }\n    copy: Thing.label\n}\n\ntype Outcome = Enum {\n    paid: {\n        label: Label\n    }\n}\n\ntype Paid = Object {\n    label: Outcome.paid.label\n}\n"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn an_enum_variant_is_never_given_a_label() {
    assert_eq!(
        check("type Test = Enum {\n    a: \"A\"\n    b: \"B\"\n    c: \"C\"\n}\n"),
        ["type.enum_label", "type.enum_label", "type.enum_label"]
    );
    assert_eq!(
        with_status("type Test = Enum {\n    a: Label\n}\n"),
        ["type.enum_label"]
    );
    let db = jadpo_db::Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", "type Test = Enum {\n    a: \"A\"\n}\n")],
    );
    let problems = jadpo_check::check(&db, project);
    let reason = problems[0].reason.clone().unwrap_or_default();
    assert!(reason.contains("translations"), "{reason}");
    // Plain names and variants with fields still check.
    assert_eq!(
        with_status(
            "type Outcome = Enum {\n    pending\n    paid: {\n        label: Label\n    }\n}\n"
        ),
        Vec::<String>::new()
    );
}

/// The source with every problem's edits made.
fn fixed(source: &str) -> String {
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    let mut edits: Vec<(u32, u32, String)> = jadpo_check::check(&db, project)
        .iter()
        .filter_map(|d| d.next.as_ref())
        .flat_map(|next| next.edits.iter())
        .map(|edit| (edit.span.start, edit.span.end, edit.replacement.clone()))
        .collect();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = source.to_string();
    for (start, end, text) in edits {
        out.replace_range(start as usize..end as usize, &text);
    }
    out
}

#[test]
fn the_fixes_write_the_type_and_drop_the_label() {
    assert_eq!(
        fixed(&format!(
            "{STATUS}type Thing = Input {{\n    status: Status.open\n    again: Status(\"open\")\n}}\n"
        )),
        format!("{STATUS}type Thing = Input {{\n    status: Status\n    again: Status\n}}\n")
    );
    assert_eq!(
        fixed("type Test = Enum {\n    a: \"A\"\n    b\n}\n"),
        "type Test = Enum {\n    a\n    b\n}\n"
    );
}
