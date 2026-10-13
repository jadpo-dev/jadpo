use std::io::Write;
use std::process::ExitCode;

use jadpo_term::Environment;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let output = jadpo_cli::run(&args, &Environment::from_process());
    // A closed pipe (`jadpo help | head`) is not an error worth reporting.
    let _ = std::io::stdout().write_all(output.stdout.as_bytes());
    let _ = std::io::stderr().write_all(output.stderr.as_bytes());
    ExitCode::from(output.code)
}
