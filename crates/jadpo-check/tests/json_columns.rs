//! Area 5, "Storage types" (chunk 5.9b), in the checker: an application's
//! enum with data, object, list, set and map is stored whole as `jsonb`
//! holding its type's JSON, which the route codec writes and reads, so any
//! type made of what that JSON holds checks; a part with no stored form is
//! `data.field_without_json`, and a secret, a token, an entity or text for
//! people inside one is refused as it is in a column of its own. A query
//! neither compares nor sorts such a field, no unique rule or index covers
//! one, a lifecycle tests or sets one only as `none`, and a field's policy
//! covers its whole value.

use crate::support;

use jadpo_db::Layout;

const NONE: &[&str] = &[];

const TYPES: &str = r#"type Tag = Text {
    max_length: 20
}

type Heaps = Int {
    min: 0
    max: 100000
}

type Shelf = Text {
    pattern: "[A-Z][0-9]"
}

type Label = Object {
    line: Tag
    note: Tag?
    weight: Heaps
    limit: Heaps
    price: Money
    contact: Email
    spot: Point
    rule within_limit: weight <= limit
}

type Delivery = Enum {
    collect
    courier: {
        company: Tag
        weight: Heaps
    }
}

type Thread = Object {
    text: Tag
    replies: List<Thread>
}

type Pin = Object {
    name: Tag
}

type Arrived = Event {
    note: Tag
}

type LabelInput = Input {
    line: Tag
}

type Login = Object {
    hash: PasswordHash
}

type Signed = Object {
    password: Password
}

type Issued = Object {
    token: ApiToken
}

type Shown = Object {
    when: PresentationText
}

"#;

/// The rules a project of [`TYPES`] and `source` gives, checked whole,
/// its storage with it.
fn checked(source: &str) -> Vec<String> {
    let db = jadpo_db::Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

/// An entity holding one field of type `ty`.
fn holding(ty: &str) -> String {
    format!("entity Parcel {{\n    id: Uuid\n    held: {ty}\n}}\n")
}

#[test]
fn every_structured_type_its_json_holds_is_stored() {
    for ty in [
        "Label",
        "Label?",
        "Delivery",
        "Delivery?",
        "List<Tag>",
        "List<Label>",
        "Set<Tag>",
        "Map<Tag, Heaps>",
        "Map<Uuid, Delivery>",
        "Map<CalendarDate, List<Heaps>>",
        "Map<Currency, Money>",
        "Thread",
        "List<Map<Tag, Set<Heaps>>>",
        "Feature<Point, Pin>",
        // A password's hash is kept on the server, inside a stored value as
        // in a column of its own.
        "Login",
        // The standard collections, as chunk 5.5w2b stored them.
        "RichText",
        "LocalisedText?",
    ] {
        assert_eq!(checked(&holding(ty)), NONE, "{ty}");
    }
}

#[test]
fn a_part_with_no_stored_json_form_is_refused() {
    for (ty, rule) in [
        // An event is emitted, an input arrives with a request: a record
        // keeps an `Object`.
        ("Arrived", "data.field_without_json"),
        ("List<Arrived>", "data.field_without_json"),
        ("LabelInput", "data.field_without_json"),
        ("Map<Label, Heaps>", "data.field_without_json"),
        ("Map<Delivery, Heaps>", "data.field_without_json"),
        // Refused inside a stored value as in a column of its own.
        ("Signed", "type.secret_exposed"),
        ("List<Issued>", "type.auth_credential_stored"),
        ("Map<Tag, ApiToken>", "type.auth_credential_stored"),
        ("Shown", "type.presentation_text_persistence"),
    ] {
        assert_eq!(checked(&holding(ty)), [rule], "{ty}");
    }
    let entity = "entity Shipper {\n    id: Uuid\n}\n\nentity Parcel {\n    id: Uuid\n    shippers: List<Shipper>\n}\n";
    assert_eq!(checked(entity), ["data.entity_in_field"]);
}

#[test]
fn a_query_neither_compares_nor_sorts_a_stored_value() {
    let entity =
        "entity Parcel {\n    id: Uuid\n    tags: List<Tag>\n    delivery: Delivery\n}\n\n";
    assert_eq!(
        checked(&format!(
            "{entity}query tagged(tags: List<Tag>) -> List<Parcel> {{\n    from: Parcel\n    where: Parcel.tags == tags\n    order: [Parcel.id asc]\n    limit: 20\n}}\n"
        )),
        ["toolchain.not_built_yet"],
        "compared"
    );
    assert_eq!(
        checked(&format!(
            "{entity}query couriered() -> List<Parcel> {{\n    from: Parcel\n    where: Parcel.delivery == Delivery.collect\n    order: [Parcel.id asc]\n    limit: 20\n}}\n"
        )),
        ["toolchain.not_built_yet"],
        "a variant of an enum with data"
    );
    assert_eq!(
        checked(&format!(
            "{entity}query sorted() -> List<Parcel> {{\n    from: Parcel\n    order: [Parcel.tags asc]\n    limit: 20\n}}\n"
        )),
        ["query.order_field"],
        "sorted"
    );
    // Read whole, as any field is.
    assert_eq!(
        checked(&format!(
            "{entity}query parcels() -> List<Parcel> {{\n    from: Parcel\n    order: [Parcel.id asc]\n    limit: 20\n}}\n"
        )),
        NONE
    );
}

#[test]
fn the_database_works_out_no_stored_value_in_one_step() {
    let entity = "type ParcelNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\nentity Parcel {\n    id: Uuid\n    label: Label\n    spare: Label?\n}\n\n";
    assert_eq!(
        checked(&format!(
            "{entity}action copy_label(parcel_id: Parcel.id) fails ParcelNotFound -> Parcel {{\n    return attempt Parcel.update(parcel_id) {{\n        set: {{\n            spare: Parcel.label\n        }}\n    }}\n}}\n"
        )),
        ["toolchain.not_built_yet"],
        "an update by `id` copying one stored value into another"
    );
    // A value the action is given is written through the codec.
    assert_eq!(
        checked(&format!(
            "{entity}action relabel(parcel_id: Parcel.id, label: Label) fails ParcelNotFound -> Parcel {{\n    return attempt Parcel.update(parcel_id) {{\n        set: {{\n            spare: label\n        }}\n    }}\n}}\n"
        )),
        NONE
    );
}

#[test]
fn no_unique_rule_or_index_covers_a_stored_value() {
    assert_eq!(
        checked(
            "entity Parcel {\n    id: Uuid\n    tags: List<Tag> {\n        unique: true\n    }\n}\n"
        ),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        checked(
            "entity Parcel {\n    id: Uuid\n    shelf: Shelf\n    tags: List<Tag>\n\n    unique: {\n        shelf_tags: [shelf, tags]\n    }\n}\n"
        ),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        checked(
            "entity Parcel {\n    id: Uuid\n    label: Label\n\n    indexes: {\n        by_label: [label]\n    }\n}\n"
        ),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        checked(
            "entity Parcel {\n    id: Uuid\n    about: RichText {\n        unique: true\n    }\n}\n"
        ),
        ["toolchain.not_built_yet"],
        "a standard collection too"
    );
}

#[test]
fn a_lifecycle_tests_and_sets_a_stored_value_only_as_none() {
    let entity = |lifecycle: &str| {
        checked(&format!(
            "entity Parcel {{\n    id: Uuid\n    delivery: Delivery?\n    shelf: Shelf\n\n    lifecycle: {{\n{lifecycle}    }}\n}}\n"
        ))
    };
    assert_eq!(
        entity(
            "        visible: delivery != none\n        transitions: {\n            hand_over: {\n                when: delivery != none\n                set: {\n                    delivery: none\n                }\n            }\n        }\n"
        ),
        NONE,
        "`none` is SQL's `NULL`"
    );
    assert_eq!(
        entity("        visible: delivery == Delivery.collect\n"),
        ["data.lifecycle_condition"]
    );
    assert_eq!(
        entity(
            "        transitions: {\n            collect: {\n                set: {\n                    delivery: Delivery.collect\n                }\n            }\n        }\n"
        ),
        ["data.lifecycle_value"]
    );
    assert_eq!(
        entity(
            "        initial: {\n            delivery: Delivery.collect\n        }\n        transitions: {\n            hand_over: {\n                set: {\n                    delivery: none\n                }\n            }\n        }\n"
        ),
        ["data.lifecycle_value"]
    );
}

#[test]
fn a_fields_policy_covers_its_whole_stored_value() {
    assert_eq!(
        checked(
            "entity Parcel {\n    id: Uuid\n    label: Label {\n        policy: FieldAccess.server_only\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n"
        ),
        NONE
    );
    // A field of the object has no policy of its own.
    assert_eq!(
        checked(
            "type Note = Object {\n    line: Text {\n        policy: FieldAccess.server_only\n    }\n}\n\nentity Parcel {\n    id: Uuid\n    note: Note\n}\n"
        ),
        ["type.unknown_setting"]
    );
}
