//! Ordinary point opt-in preserves raw helpers; this is not all-consumer time proof.
use jadpo_core::{analyze_sources, derive_target};
use jadpo_syntax::SourceFile;
use std::path::PathBuf;
const SOURCE: &str =
    include_str!("../../../../tests/runtime/fixtures/temporal-instant-sidecar.jadpo");
fn outputs(source: &str) -> (String, String) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_owned();
    let p = analyze_sources(vec![SourceFile::new(
        root.join("point.jadpo"),
        source.to_owned(),
    )])
    .unwrap();
    let errors: Vec<_> = p
        .syntax
        .diagnostics()
        .chain(p.semantics.diagnostics.iter())
        .chain(p.entity_model.diagnostics.iter())
        .chain(p.typing.diagnostics.iter())
        .chain(p.policy.diagnostics.iter())
        .chain(p.failures.diagnostics.iter())
        .filter(|d| d.severity == jadpo_diagnostics::Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let output = derive_target(&root, &p).unwrap();
    let body = |path| {
        output
            .iter()
            .find(|a| a.relative_path == path)
            .unwrap()
            .contents
            .clone()
    };
    (body("target/app.ts"), body("target/persistence.ts"))
}
#[test]
fn ordinary_point_has_explicit_read_and_decode_context() {
    let (app, persistence) = outputs(SOURCE);
    assert!(app.contains("persistence.query_required_TimedRecord_by_bucket(input.bucket, true)"));
    assert!(app.contains("validate_TimedRecord(value, \"database.TimedRecord\", true)"));
    assert!(persistence
        .contains("query_optional_TimedRecord_by_bucket(value: unknown, pointContext = false)"));
    assert!(persistence.contains("CASE WHEN pg_typeof(\"instant\")::text = 'timestamp with time zone' THEN EXTRACT(EPOCH FROM \"instant\"::text::timestamptz)::text ELSE NULL END".replace('"', "\\\"").as_str()));
    let start = persistence
        .find("async query_optional_TimedRecord_by_bucket")
        .unwrap();
    let method = &persistence[start
        ..persistence[start..]
            .find("async query_required_TimedRecord_by_bucket")
            .unwrap()
            + start];
    assert!(
        method.find("capturePointInstants(").unwrap()
            < method
                .find("const visible = await policyFilterRows")
                .unwrap()
    );
    assert!(method.find("query.cardinality").unwrap() < method.find("return visible[0]").unwrap());
    assert!(persistence.contains("if (micros % 1000n !== 0n) return fail();"));
    assert!(persistence.contains("context.postgres !== (postgres !== null)"));
}
#[test]
fn nullable_and_nominal_instant_keep_physical_kind_checks() {
    let source = SOURCE
        .replace(
            "entity TimedRecord {",
            "type StoredMoment = Instant\n\nentity TimedRecord {",
        )
        .replace("instant: Instant", "instant: StoredMoment");
    let (app, persistence) = outputs(&source);
    assert!(app.contains("decodePointInstant(object, \"TimedRecord\", \"optional_instant\", true"));
    assert!(persistence.contains("if (info.type !== \"timestamp with time zone\") return fail();"));
    assert!(
        persistence.find("info.type !==").unwrap()
            < persistence.find("info.value === null").unwrap()
    );
}
#[test]
fn every_authored_alias_collision_is_allocated_without_new_reserved_names() {
    let source=SOURCE.replace("entity TimedRecord {", "entity TimedRecord {\n jadpo_point_epoch_instant: Text\n jadpo_point_epoch_instant_1: Text\n jadpo_point_type_instant: Text");
    let (_, persistence) = outputs(&source);
    assert!(persistence.contains("jadpo_point_epoch_instant_2"));
    assert!(persistence.contains("jadpo_point_type_instant_1"));
}
#[test]
fn no_ordinary_point_keeps_legacy_decoders_and_private_helpers() {
    let source = "entity Plain { id: Uuid identity instant: Instant }\nroute GET /plain { auth: none output: Text action: { return \"yes\" } }";
    let (app, persistence) = outputs(&source);
    assert!(!app.contains("decodePointInstant"));
    assert!(!persistence.contains("capturePointInstants"));
}

#[test]
fn ordinary_optional_read_and_many_legacy_use_distinct_contexts() {
    let source = "entity Moment { id: Uuid identity at: Instant } input Lookup { id: Moment.id } action find(input: Lookup) -> Moment? { return attempt query optional Moment { where: id == input.id } }";
    let (app, persistence) = outputs(source);
    assert!(app.contains("persistence.query_optional_Moment_by_id(input.id, true)"));
    assert!(app.contains("validate_Moment(value, \"database.Moment\", true)"));
    assert!(persistence.contains("pointContext = false"));
    assert!(persistence.contains("WHERE \\\"id\\\" = ?1 LIMIT 2"));
    let source = include_str!("../../../../tests/runtime/fixtures/temporal-roundtrip.jadpo");
    let (app, _) = outputs(source);
    assert!(app.contains("persistence.query_many_TemporalRecord_by_instant_order_by_id_asc"));
    assert!(!app.contains("map((value: unknown) => validate_TemporalRecord(value, \"database.TemporalRecord\", true))"));
}
