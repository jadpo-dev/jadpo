//! One PostgreSQL server for a whole run of the gate
//! (docs/compiler-runtime.md §2.7): the server
//! [`jadpo_test::postgres::Server`] finds or starts, its admin URL printed
//! on one line, kept until standard input closes, then stopped and its
//! folder removed. `tools/verify.py` starts it once and gives every step
//! the URL through `JADPO_TEST_DATABASE_URL`, so no test binary or `jadpo
//! test` starts a server of its own.
//!
//! Each line it reads asks for a new, empty database, named after the
//! line, and is answered with that database's `postgresql://` URL on a
//! line of its own, or `error: …`: the database the docs check gives a
//! `sh run database` block (docs/compiler-runtime.md §2.7, "Command
//! examples").

use std::io::{BufRead as _, Write as _};
use std::process::ExitCode;

use jadpo_test::postgres::{ADMIN_DATABASE, Server};

fn main() -> ExitCode {
    let server = match Server::start() {
        Ok(server) => server,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let mut stdout = std::io::stdout();
    if writeln!(stdout, "{}", server.url(ADMIN_DATABASE))
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return ExitCode::FAILURE;
    }
    // Whatever arrives, the end of input is the end of the run.
    for line in std::io::stdin().lock().lines() {
        let Ok(stem) = line else { break };
        let answer = server
            .new_database(stem.trim())
            .map(|name| server.address(&name))
            .unwrap_or_else(|error| format!("error: {error}"));
        if writeln!(stdout, "{answer}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            break;
        }
    }
    drop(server);
    ExitCode::SUCCESS
}
