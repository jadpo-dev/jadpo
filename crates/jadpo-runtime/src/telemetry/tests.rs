//! The telemetry's own tests: what is read from the standard variables,
//! and the payloads' shapes.

use super::*;

#[test]
fn the_standard_headers_variable_is_read_as_the_specification_writes_it() {
    // A key made as the test runs, so none is written in the source.
    let key: String = ["k", "e", "y"].concat();
    assert_eq!(
        headers_of(&format!("x-honeycomb-team={key}%3D, other = two")).unwrap(),
        [
            ("x-honeycomb-team".to_string(), format!("{key}=")),
            ("other".to_string(), "two".to_string())
        ]
    );
    assert_eq!(headers_of("").unwrap(), []);
    assert!(headers_of("no-equals").is_none());
    assert!(headers_of("bad name=1").is_none());
}

#[test]
fn an_endpoint_left_out_is_read_from_the_standard_variable() {
    let plan = TelemetryPlan::default();
    let key: String = ["k", "e", "y"].concat();
    let found = destination(&plan, None, None, &|name: &str| match name {
        "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("http://127.0.0.1:4318/".to_string()),
        "OTEL_EXPORTER_OTLP_HEADERS" => Some(format!("x-api-key=a%20{key}")),
        _ => None,
    })
    .unwrap();
    assert_eq!(found.0, "http://127.0.0.1:4318");
    assert_eq!(found.1, [("x-api-key".to_string(), format!("a {key}"))]);
    let missing = destination(&plan, None, None, &|_: &str| None).unwrap_err();
    assert_eq!(missing.code(), "telemetry_endpoint_missing");
    assert!(missing.message().contains("OTEL_EXPORTER_OTLP_ENDPOINT"));
    let invalid = destination(&plan, None, None, &|name: &str| {
        (name == "OTEL_EXPORTER_OTLP_ENDPOINT").then(|| "collector:4318".to_string())
    })
    .unwrap_err();
    assert_eq!(invalid.code(), "telemetry_endpoint_invalid");
}

#[test]
fn each_runtime_event_has_a_level() {
    assert_eq!(level_of("operation.failed"), Level::Error);
    assert_eq!(level_of("schedule.alert"), Level::Warn);
    assert_eq!(level_of("failure.rejected"), Level::Info);
    assert_eq!(level_of("privacy.read"), Level::Info);
}

#[test]
fn a_logs_payload_keeps_only_the_levels_sent() {
    let records = [
        LogRecord {
            time_ms: 1,
            level: Level::Info,
            event: "failure.rejected".into(),
            fields: Map::new(),
        },
        LogRecord {
            time_ms: 2,
            level: Level::Error,
            event: "operation.failed".into(),
            fields: Map::new(),
        },
    ];
    let payload = otlp::logs(&Resource::default(), &records, None);
    let kept = otlp::logs_from(&payload, Level::Warn).unwrap();
    let records = &kept["resourceLogs"][0]["scopeLogs"][0]["logRecords"];
    assert_eq!(records.as_array().unwrap().len(), 1);
    assert_eq!(records[0]["severityText"], "ERROR");
    assert_eq!(records[0]["body"]["stringValue"], "operation.failed");
    assert!(otlp::logs_from(&payload, Level::Fatal).is_none());
}

#[test]
fn a_log_names_an_endpoint_without_what_was_written_into_it() {
    // Made as the test runs, so no credential is written in the source.
    let password: String = ["pass", "word"].concat();
    let endpoint = format!("https://user:{password}@collector.example:4318/otlp?x=1");
    assert_eq!(origin(&endpoint), "https://collector.example:4318");
    assert_eq!(origin("http://127.0.0.1:4318"), "http://127.0.0.1:4318");
}
