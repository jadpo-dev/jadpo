//! Rebind the exact production compiler input with names for CPU diagnosis.
//! The Python harness checks JavaScript and every noncustom Wasm section.
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("expected compiled input and a fresh output directory".into());
    }
    let bound = jadpo_codegen::workers::bind(
        Path::new(&args[0]),
        "kiosk",
        jadpo_codegen::workers::Names::Kept,
    )?;
    let output = Path::new(&args[1]);
    std::fs::create_dir(output)?;
    std::fs::write(output.join("kiosk.js"), bound.js)?;
    std::fs::write(output.join("kiosk.wasm"), bound.wasm)?;
    Ok(())
}
