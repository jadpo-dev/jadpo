//! Area 19's hashing, encodings and JSON (chunk 5.5g), natively: SHA-256
//! against NIST's examples, the encodings against RFC 4648's vectors, and
//! `json.write`'s exact text for one document, the same checks the gate
//! makes under WebAssembly (crates/jadpo-value/examples/formats), through
//! the one implementation both executors call.

#[path = "../../jadpo-value/examples/formats/check.rs"]
mod check;

#[test]
fn every_operation_gives_the_fixed_answers() {
    let (wrong, count) = check::mismatches();
    assert!(count > 50, "{count} checks");
    assert!(
        wrong.is_empty(),
        "{} of {count}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
