//! Ordinary many opt-in preserves visibility and shared default consumers.
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

fn many_source() -> String {
    SOURCE.replace("query find(input: TimedLookup) freshness: authoritative fails TimedMissing -> TimedRecord {\n  var record = attempt query required TimedRecord { where: bucket == input.bucket missing: TimedMissing }\n  return record\n }", "query find(input: TimedLookup) freshness: authoritative -> List<TimedRecord> {\n  return attempt query many TimedRecord { where: bucket == input.bucket order_by: id asc limit: 10 offset: 0 }\n }").replace("output: TimedRecord run: TimedRecord.find(input)", "output: List<TimedRecord> run: TimedRecord.find(input)")
}
#[test]
fn many_only_has_explicit_read_and_postvisibility_decode_context() {
    let (app, p) = outputs(&many_source());
    assert!(app.contains(
        "query_many_TimedRecord_by_bucket_order_by_id_asc_paginated(input.bucket, 10, 0, true)"
    ));
    assert!(app.contains("validate_TimedRecord(value, `database.TimedRecord[${index}]`, true)"));
    assert!(p.contains(
        "_paginated(value: unknown, limit: unknown, offset: unknown, instantContext = false)"
    ));
    let start = p
        .find("async query_many_TimedRecord_by_bucket_order_by_id_asc_paginated")
        .unwrap();
    let m = &p[start..start + p[start..].find("\n  },").unwrap()];
    assert!(m.find("capturePointInstants(").unwrap() < m.find("return policyFilterRows(").unwrap());
    assert!(!m.contains("query.cardinality"));
    assert!(m.contains("LIMIT $2 OFFSET $3"));
    assert!(m.contains("LIMIT ?2 OFFSET ?3"));
    assert!(p.contains("context.postgres !== (postgres !== null)"));
    assert!(p.contains("if (micros % 1000n !== 0n) return fail();"));
}
#[test]
fn unpaginated_many_and_nominal_nullable_fields_are_opted_in() {
    let source = many_source()
        .replace(" limit: 10 offset: 0", "")
        .replace(
            "entity TimedRecord {",
            "type StoredMoment = Instant\nentity TimedRecord {",
        )
        .replace("instant: Instant", "instant: StoredMoment");
    let (app, p) = outputs(&source);
    assert!(app.contains("query_many_TimedRecord_by_bucket_order_by_id_asc(input.bucket, true)"));
    assert!(app.contains("decodePointInstant(object, \"TimedRecord\", \"optional_instant\", true"));
    assert!(p.contains(
        "query_many_TimedRecord_by_bucket_order_by_id_asc(value: unknown, instantContext = false)"
    ));
    assert!(p.contains("query_optional_TimedRecord_by_bucket(value: unknown):"));
}
#[test]
fn many_aliases_remain_bounded_and_reserve_authored_names() {
    for size in [52, 63] {
        let f = "a".repeat(size);
        let (_, p) =
            outputs(&many_source().replace("\n instant: Instant", &format!("\n {f}: Instant")));
        assert!(p.contains("jadpo_point_epoch_3"));
        assert!(!p.contains(&format!("jadpo_point_epoch_{f}")));
    }
    let(_,p)=outputs(&many_source().replace("entity TimedRecord {", "entity TimedRecord {\n jadpo_point_epoch_6: Text\n jadpo_point_epoch_6_1: Text\n jadpo_point_type_6: Text"));
    assert!(p.contains("jadpo_point_epoch_6_2"));
    assert!(p.contains("jadpo_point_type_6_1"));
}
#[test]
fn no_ordinary_many_retains_existing_point_and_unselected_defaults() {
    let (app, p) = outputs(SOURCE);
    assert!(app.contains("query_required_TimedRecord_by_bucket(input.bucket, true)"));
    assert!(!p.contains("instantContext"));
    let(app,p)=outputs("entity Plain { id: Uuid identity at: Instant } route GET /plain { auth: none output: Text action: { return \"yes\" } }");
    assert!(!app.contains("decodePointInstant"));
    assert!(!p.contains("capturePointInstants"));
}
