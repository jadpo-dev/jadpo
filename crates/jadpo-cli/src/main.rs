use std::io::Write;
use std::process::ExitCode;

use jadpo_cli::Session;
use jadpo_cli::launcher::Launch;
use jadpo_term::Environment;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let env = Environment::from_process();
    // First, so a panic in the launcher is caught too. A release handed
    // over to replaces this process (Unix) or runs as a child that prints
    // its own screen, so only one screen is ever printed.
    jadpo_cli::crash::install(args.clone(), Environment::from_process());
    // The release the project names runs the command, when it isn't this
    // one (area 14, "Which jadpo runs").
    match jadpo_cli::launcher::launch(&args, &env) {
        Launch::Here => {}
        Launch::Run { binary, version } => {
            return jadpo_cli::launcher::hand_over(&binary, &version);
        }
        Launch::Refused(output) => {
            let _ = std::io::stdout().write_all(output.stdout.as_bytes());
            let _ = std::io::stderr().write_all(output.stderr.as_bytes());
            return ExitCode::from(output.code);
        }
    }
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
