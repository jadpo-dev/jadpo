//! Area 19's maths on `Decimal` (chunk 5.5e) against its reference values,
//! `reference.tsv` beside this file, which tools/references/decimal_maths_reference.py
//! writes from Python's `decimal` module and says how. Shared by this
//! example, which the gate runs under WebAssembly, and the native test
//! (crates/jadpo-runtime/tests/maths.rs), so both targets are held to the
//! same digits, places and failures, each call made through
//! `jadpo_value::run` as both executors make it.

use jadpo_std::{Rounding, RuleKey};
use jadpo_value::{Fault, Value, run};

pub const REFERENCE: &str = include_str!("reference.tsv");

/// One line of the reference: the call, written out, and what it gave.
fn checked(line: &str) -> Result<Option<String>, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [name, arguments, scale, rounding, expected] = fields[..] else {
        return Err(format!("`{line}` doesn't have five fields"));
    };
    let operation = jadpo_std::operation("decimal", name)
        .ok_or_else(|| format!("`decimal.{name}` isn't an operation"))?;
    let mut args = Vec::new();
    let mut written: Vec<String> = Vec::new();
    for (at, argument) in arguments.split(' ').filter(|a| *a != "-").enumerate() {
        let value = if name == "power" && at == 1 {
            argument.parse().ok().map(Value::Int)
        } else {
            Value::decimal(argument)
        };
        args.push(value.ok_or_else(|| format!("`{argument}` isn't a number"))?);
        written.push(argument.to_string());
    }
    if scale != "-" {
        let places: i64 = scale
            .parse()
            .map_err(|_| format!("`{scale}` isn't a scale"))?;
        let ordinal = jadpo_std::variant_ordinal(Rounding::NAME, rounding)
            .ok_or_else(|| format!("`{rounding}` isn't a rounding"))?;
        args.push(Value::Int(places));
        args.push(Value::variant(
            Rounding::NAME,
            ordinal,
            rounding,
            Vec::new(),
        ));
        written.push(format!("scale: {scale}"));
        written.push(format!("rounding: Rounding.{rounding}"));
    }
    let gave = match run(operation.op, &args, None, &mut |_| None) {
        Ok(value) => value.to_string(),
        Err(Fault::DecimalOverflow) => "overflow".to_string(),
        Err(Fault::Broke(RuleKey::OutOfRange)) => "out_of_range".to_string(),
        Err(fault) => format!("{fault:?}"),
    };
    Ok((gave != expected).then(|| {
        format!(
            "decimal.{name}({}) gave {gave}; the reference is {expected}",
            written.join(", ")
        )
    }))
}

/// Each reference line the maths disagrees with, described, and how many
/// lines were checked.
pub fn mismatches() -> (Vec<String>, usize) {
    let mut wrong = Vec::new();
    let mut count = 0;
    for line in REFERENCE.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        count += 1;
        match checked(line) {
            Ok(None) => {}
            Ok(Some(mismatch)) | Err(mismatch) => wrong.push(mismatch),
        }
    }
    (wrong, count)
}
