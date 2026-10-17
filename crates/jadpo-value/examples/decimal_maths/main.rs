//! Area 19's maths on `Decimal` against its reference values, as a program
//! the gate (tools/verify.py) builds for `wasm32-unknown-unknown` and runs
//! under Node (tools/run_wasm.mjs), so the digits are checked under
//! WebAssembly as well as natively. Its exit code is how many values
//! disagree, at most 255; 255 too when there were none to check.

mod check;

use std::process::ExitCode;

fn main() -> ExitCode {
    let (wrong, count) = check::mismatches();
    for mismatch in &wrong {
        eprintln!("{mismatch}");
    }
    if count == 0 {
        return ExitCode::from(u8::MAX);
    }
    ExitCode::from(u8::try_from(wrong.len()).unwrap_or(u8::MAX))
}
