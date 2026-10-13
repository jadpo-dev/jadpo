//! The commands built so far. Each receives a [`Call`] and returns a
//! [`Reply`]; argument counts are checked from the table before it runs.

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::Json;

use crate::completion;

pub use crate::check::check;
pub use crate::fmt::fmt;
use crate::help::{self, VERSION};
use crate::run::{Call, Reply, Session, both, either, quoted, unknown_command};
use crate::suggest::nearest;
use crate::table::{self, SHELLS};

pub fn help(call: &Call) -> Reply {
    match call.arguments.first() {
        None => help::reference(),
        Some(word) => match table::find(word) {
            Some(command) => help::command(command),
            None => Reply::failed(unknown_command(word)),
        },
    }
}

/// `jadpo lsp`: the binary serves the editor on standard input and output
/// once this returns.
pub fn lsp(call: &Call) -> Reply {
    Reply::Session(Session::Lsp(
        call.cwd
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
    ))
}

pub fn version(_call: &Call) -> Reply {
    Reply::raw(
        format!("jadpo {VERSION}\n"),
        Json::object([("version", VERSION.into())]),
    )
}

pub fn completions(call: &Call) -> Reply {
    let names: Vec<String> = SHELLS.iter().map(|shell| shell.name.to_string()).collect();
    let Some(shell) = call.arguments.first() else {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_COMPLETIONS_ARGUMENTS,
                "`jadpo completions` needs a shell",
            )
            .reason(format!(
                "Each shell gets its own script: {}.",
                either(&names)
            ))
            .next("Add the shell, for example `jadpo completions zsh`"),
        );
    };
    match completion::script(shell) {
        Some(script) => Reply::raw(
            script,
            Json::object([("shell", shell.as_str().into()), ("script", script.into())]),
        ),
        None => {
            let diagnostic = Diagnostic::new(
                &rule::CLI_COMPLETIONS_ARGUMENTS,
                format!("`{shell}` isn't a shell jadpo completes"),
            );
            let reason = format!("Jadpo writes completion scripts for {}.", both(&names));
            Reply::failed(
                match nearest(shell, SHELLS.iter().map(|shell| shell.name)) {
                    Some(near) => diagnostic
                        .reason(format!("{reason} Did you mean `{near}`?"))
                        .next(format!("Use `{near}` instead of `{shell}`")),
                    None => diagnostic
                        .reason(reason)
                        .next(format!("Use {}", either(&quoted(&names)))),
                },
            )
        }
    }
}
