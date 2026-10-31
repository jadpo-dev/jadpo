//! Area 12, "A payload is the contract's JSON" (chunk 5.9b): every event
//! type is numbered in the plan's JSON table, by the identity key the
//! outbox's row records, so its payload is written and read through the
//! one JSON codec, as a request's body is, with the type's rules applied
//! on read; a field stored as JSON keeps its own number beside it.

use crate::support;

use jadpo_interp::Value;

const SOURCE: &str = r#"type Tag = Text {
    max_length: 20
}

type Heaps = Int {
    min: 0
    max: 100000
}

type ParcelSent = Event {
    parcel_id: Uuid
    price: Money
    weight: Heaps
    tags: List<Tag>
    sent_at: Instant
}

type ParcelLost = Event {
    parcel_id: Uuid
}

entity Parcel {
    id: Uuid
    tags: List<Tag>
}
"#;

const SENT: &str = r#"{"parcel_id":"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e","price":{"amount":1050,"currency":"GBP"},"weight":250,"tags":["fragile","ice"],"sent_at":"2026-10-19T09:00:00Z"}"#;

#[test]
fn every_event_type_is_numbered_in_the_json_table() {
    let program = support::main(SOURCE);
    let table = program.plan().json();
    let keys: Vec<&str> = table
        .events
        .iter()
        .map(|event| event.key.as_str())
        .collect();
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(
        keys.iter().any(|key| key.ends_with("ParcelSent"))
            && keys.iter().any(|key| key.ends_with("ParcelLost")),
        "{keys:?}"
    );
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "in key order");
    for event in &table.events {
        assert_eq!(table.event(&event.key), Some(event.number));
        assert!(table.codec_of(event.number).is_some(), "{}", event.key);
    }
    // Each type numbered once: the stored field's own number is another.
    let mut numbers: Vec<u32> = table.events.iter().map(|event| event.number).collect();
    numbers.dedup();
    assert_eq!(numbers.len(), 2);
    assert!(table.types.len() >= 3, "the field's number beside them");
}

#[test]
fn an_events_payload_is_its_contract_json_read_with_its_rules() {
    let program = support::main(SOURCE);
    let table = program.plan().json();
    let sent = table
        .events
        .iter()
        .find(|event| event.key.ends_with("ParcelSent"))
        .expect("numbered")
        .number;
    let app = program.app();
    let payload = app.json.read(sent, SENT).expect("reads");
    let Value::Record(record) = &payload else {
        panic!("a record: {payload:?}");
    };
    assert_eq!(&*record.ty, "ParcelSent");
    // Written back exactly, its money in minor units with its currency.
    match app.json.write(sent, &payload) {
        Ok(jadpo_runtime::SqlValue::Json(text)) => assert_eq!(text, SENT),
        other => panic!("{other:?}"),
    }
    // Its rules hold as it is read, named by the field and the rule,
    // never the value.
    let heavy = SENT.replace("\"weight\":250", "\"weight\":250000");
    let why = app.json.read(sent, &heavy).expect_err("past `max`");
    assert!(
        why.contains("`max`") && why.contains("`weight`") && !why.contains("250000"),
        "{why}"
    );
    let long = SENT.replace("\"ice\"", "\"a tag far longer than twenty\"");
    let why = app.json.read(sent, &long).expect_err("past `max_length`");
    assert!(
        why.contains("`max_length`") && !why.contains("twenty"),
        "{why}"
    );
}
