//! The commands built so far. Each receives a [`Call`] and returns a
//! [`Reply`]; argument counts are checked from the table before it runs.

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::Json;

use crate::completion;

pub use crate::acceptance::acceptance;
pub use crate::ai::ai;
pub use crate::api::api;
pub use crate::build::build;
pub use crate::calls::calls;
pub use crate::check::check;
pub use crate::config::config;
pub use crate::declines::declines;
pub use crate::deliveries::deliveries;
pub use crate::dev::dev;
pub use crate::email::email;
pub use crate::fix::fix;
pub use crate::fmt::fmt;
use crate::help::{self, VERSION};
pub use crate::migrate::migrate;
pub use crate::new::new;
pub use crate::package::package;
pub use crate::records::records;
pub use crate::routes::routes;
use crate::run::{Call, Reply, Session, both, either, quoted, unknown_topic};
pub use crate::runs::runs;
pub use crate::schedules::{schedules, why};
pub use crate::sql::sql;
use crate::suggest::nearest;
pub use crate::suggestions::suggest;
use crate::table::{self, SHELLS};
pub use crate::test::test;
pub use crate::translations::translations;

pub fn help(call: &Call) -> Reply {
    match call.arguments.first() {
        None => help::reference(),
        Some(word) => match (table::find(word), jadpo_diag::find(word)) {
            (Some(command), _) => help::command(command),
            (None, Some(rule)) => help::rule(rule),
            (None, None) => Reply::failed(unknown_topic(word)),
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
