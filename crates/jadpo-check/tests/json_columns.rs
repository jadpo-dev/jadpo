//! Area 5, "Storage types" (chunk 5.9b), in the checker: an application's
//! enum with data, object, list, set and map is stored whole as `jsonb`
//! holding its type's JSON, which the route codec writes and reads, so any
//! type made of what that JSON holds checks; a part with no stored form is
//! `data.field_without_json`, and a secret, a token, an entity or text for
//! people inside one is refused as it is in a column of its own. A query
//! compares such a field as its JSON but never sorts by one, a unique rule
//! or an index holds the digest of its JSON (chunk Q2), an update by `id`
//! copies one stored value into another, a lifecycle tests or sets one
//! only as `none`, and a field's policy covers its whole value.

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
fn a_query_compares_a_stored_value_as_its_json_and_never_sorts_one() {
    let entity =
        "entity Parcel {\n    id: Uuid\n    tags: List<Tag>\n    delivery: Delivery\n}\n\n";
    assert_eq!(
        checked(&format!(
            "{entity}query tagged(tags: List<Tag>) -> List<Parcel> {{\n    from: Parcel\n    where: Parcel.tags == tags\n    order: [Parcel.id asc]\n    limit: 20\n}}\n"
        )),
        NONE,
        "compared"
    );
    assert_eq!(
        checked(&format!(
            "{entity}query couriered() -> List<Parcel> {{\n    from: Parcel\n    where: Parcel.delivery == Delivery.collect\n    order: [Parcel.id asc]\n    limit: 20\n}}\n"
        )),
        NONE,
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
fn an_update_by_id_copies_one_stored_value_into_another() {
    let entity = "type ParcelNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\nentity Parcel {\n    id: Uuid\n    label: Label\n    spare: Label?\n    tags: List<Tag>\n    kept_tags: List<Tag>\n}\n\n";
    let update = |set: &str| {
        checked(&format!(
            "{entity}action change(parcel_id: Parcel.id, label: Label, tags: List<Tag>) fails ParcelNotFound -> Parcel {{\n    return attempt Parcel.update(parcel_id) {{\n        set: {{\n{set}        }}\n    }}\n}}\n"
        ))
    };
    assert_eq!(
        update("            spare: Parcel.label\n"),
        NONE,
        "the database copies a stored value as its JSON"
    );
    assert_eq!(
        update("            kept_tags: (Parcel.tags)\n            tags: tags\n"),
        NONE,
        "beside a value the action is given, written through the codec"
    );
    // A value the action is given is written through the codec.
    assert_eq!(update("            spare: label\n"), NONE);
    // The database works out nothing else from one: a comparison isn't a
    // copy.
    assert_eq!(
        update("            tags: list.reverse(Parcel.tags)\n"),
        ["data.update_by_id_value"],
        "anything but a copy needs the record loaded"
    );
}

/// The statements a project of [`TYPES`] and `source` migrates from no
/// schema, one to a line.
fn migrated(source: &str) -> String {
    let db = jadpo_db::Database::default();
    let text = format!("{TYPES}{source}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    let schema = jadpo_check::data::schema(&db, project);
    let sql = jadpo_graph::migration::migration_sql(
        &jadpo_graph::schema::Schema::default(),
        &schema,
        &|_| None,
    )
    .expect("a migration");
    sql.stores
        .values()
        .flatten()
        .map(|statement| statement.sql.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_unique_rule_or_an_index_holds_the_digest_of_a_stored_values_json() {
    let unique_tags =
        "entity Parcel {\n    id: Uuid\n    tags: List<Tag> {\n        unique: true\n    }\n}\n";
    assert_eq!(checked(unique_tags), NONE);
    // A B-tree's entry holds about 2.7 kB and a stored value up to 1 MiB,
    // so the key holds the value's digest, never the value.
    let sql = migrated(unique_tags);
    assert!(
        sql.contains(
            "CREATE UNIQUE INDEX \"parcel_tags_key\" ON \"parcel\" ((md5(\"tags\"::text)))"
        ),
        "{sql}"
    );
    assert!(!sql.contains("UNIQUE (\"tags\")"), "{sql}");
    let shelf_tags = "entity Parcel {\n    id: Uuid\n    shelf: Shelf\n    tags: List<Tag>\n\n    unique: {\n        shelf_tags: [shelf, tags]\n    }\n}\n";
    assert_eq!(checked(shelf_tags), NONE);
    let sql = migrated(shelf_tags);
    assert!(
        sql.contains(
            "CREATE UNIQUE INDEX \"parcel_shelf_tags_key\" ON \"parcel\" (\"shelf\", (md5(\"tags\"::text)))"
        ),
        "{sql}"
    );
    let by_delivery = "entity Parcel {\n    id: Uuid\n    shelf: Shelf\n    delivery: Delivery\n\n    indexes: {\n        by_delivery: [delivery]\n        by_shelf_delivery: [shelf, delivery]\n    }\n}\n";
    assert_eq!(checked(by_delivery), NONE);
    let sql = migrated(by_delivery);
    for expected in [
        "CREATE INDEX \"parcel_by_delivery_idx\" ON \"parcel\" ((md5(\"delivery\"::text)))",
        "CREATE INDEX \"parcel_by_shelf_delivery_idx\" ON \"parcel\" (\"shelf\", (md5(\"delivery\"::text)))",
    ] {
        assert!(sql.contains(expected), "{expected}\n{sql}");
    }
    assert_eq!(
        checked(
            "entity Parcel {\n    id: Uuid\n    about: RichText {\n        unique: true\n    }\n}\n"
        ),
        NONE,
        "a standard collection too"
    );
    // The snapshot records which columns the key holds as digests.
    let db = jadpo_db::Database::default();
    let text = format!("{TYPES}{shelf_tags}");
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &text)]);
    let schema = jadpo_check::data::schema(&db, project);
    let rule = &schema.stores["main"].tables["parcel"].unique["parcel_shelf_tags_key"];
    assert_eq!(rule.columns, ["shelf", "tags"]);
    assert_eq!(rule.digest, ["tags"]);
    let read = jadpo_graph::schema::Schema::read(&schema.to_document()).expect("snapshot");
    assert_eq!(read, **schema, "the snapshot reads back the same");
}

#[test]
fn a_unique_rule_takes_no_value_whose_json_spells_a_decimal_two_ways() {
    // A percentage is written with its places; a decimal's type always
    // fixes its `scale`.
    let entity = |part: &str, unique: &str| {
        checked(&format!(
            "type Heft = Decimal {{\n    scale: 2\n}}\n\ntype Reading = Object {{\n    {part}\n}}\n\nentity Meter {{\n    id: Uuid\n    shelf: Shelf\n    reading: Reading{unique}\n}}\n"
        ))
    };
    let own = " {\n        unique: true\n    }";
    let rule = "\n\n    unique: {\n        shelf_reading: [shelf, reading]\n    }";
    let share = "share: Percentage";
    assert_eq!(entity(share, own), ["data.unique_decimal_places"]);
    assert_eq!(entity(share, rule), ["data.unique_decimal_places"]);
    // With `scale:`, one value is written one way.
    assert_eq!(entity("heft: Heft", own), NONE);
    assert_eq!(entity("heft: Heft", rule), NONE);
    // An index only finds records, so it may hold one.
    assert_eq!(
        entity(
            share,
            "\n\n    indexes: {\n        by_reading: [reading]\n    }"
        ),
        NONE
    );
}

#[test]
fn a_json_index_is_postgresqls_gin_index_on_one_field_kept_as_json() {
    let index = |fields: &str| {
        format!(
            "entity Parcel {{\n    id: Uuid\n    shelf: Shelf\n    tags: List<Tag>\n    label: Label\n\n    indexes: {{\n        by_contents: {{\n            fields: [{fields}]\n            kind: IndexKind.json\n        }}\n    }}\n}}\n"
        )
    };
    assert_eq!(checked(&index("tags")), NONE);
    let sql = migrated(&index("tags"));
    assert!(
        sql.contains("CREATE INDEX \"parcel_by_contents_idx\" ON \"parcel\" USING gin (\"tags\")"),
        "{sql}"
    );
    assert_eq!(checked(&index("shelf")), ["type.setting_value"], "text");
    assert_eq!(
        checked(&index("tags, label")),
        ["type.setting_value"],
        "two fields"
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
