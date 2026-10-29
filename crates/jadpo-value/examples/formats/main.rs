//! Area 19's hashing, encodings and JSON against fixed answers (NIST's
//! SHA-256 examples, RFC 4648's vectors and the one text `json.write`
//! gives a document), as a program the gate (tools/verify.py) builds for
//! `wasm32-unknown-unknown` and runs under Node (tools/checks/run_wasm.mjs), so
//! the bytes are checked under WebAssembly as well as natively. Its exit
//! code is how many checks disagree, at most 255; 255 too when there were
//! none to make.

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
