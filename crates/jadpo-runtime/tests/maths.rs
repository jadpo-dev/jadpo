//! Area 19's maths on `Decimal` (chunk 5.5e), natively: every operation
//! against the reference values the gate also checks under WebAssembly
//! (crates/jadpo-value/examples/decimal_maths), through the one
//! implementation both executors call.

#[path = "../../jadpo-value/examples/decimal_maths/check.rs"]
mod check;

#[test]
fn every_operation_gives_the_reference_digits() {
    let (wrong, count) = check::mismatches();
    assert!(count > 1000, "{count} reference values");
    assert!(
        wrong.is_empty(),
        "{} of {count}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
