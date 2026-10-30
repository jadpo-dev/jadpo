use std::io::Write;
use std::process::ExitCode;

use jadpo_cli::Session;
use jadpo_term::Environment;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let env = Environment::from_process();
    jadpo_cli::crash::install(args.clone(), Environment::from_process());
    let spinner = jadpo_cli::spinner(&args, &env);
    let output = jadpo_cli::run(&args, &env);
    spinner.finish();
    // A closed pipe (`jadpo help | head`) is not an error worth reporting.
    let _ = std::io::stdout().write_all(output.stdout.as_bytes());
    let _ = std::io::stderr().write_all(output.stderr.as_bytes());
    match output.session {
        Some(Session::Lsp(cwd)) => match jadpo_lsp::stdio(&cwd) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                let _ = writeln!(std::io::stderr(), "jadpo lsp stopped: {error}");
                ExitCode::from(1)
            }
        },
        Some(Session::ConfigSet(args)) => {
            ExitCode::from(jadpo_cli::config::set(&args, &Environment::from_process()))
        }
        Some(Session::Dev(args)) => {
            ExitCode::from(jadpo_cli::dev::serve(&args, &Environment::from_process()))
        }
        Some(Session::Watch(args)) => {
            ExitCode::from(jadpo_cli::watch::serve(&args, &Environment::from_process()))
        }
        None => ExitCode::from(output.code),
    }
}
