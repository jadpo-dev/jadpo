//! One PostgreSQL server for a whole run of the gate
//! (docs/compiler-runtime.md §2.7): the disposable server
//! [`jadpo_test::postgres::Server`] starts, its admin URL printed on one
//! line, kept until standard input closes, then stopped and its folder
//! removed. `tools/verify.py` starts it once and gives every step the URL
//! through `JADPO_TEST_DATABASE_URL`, so no test binary or `jadpo test`
//! starts a server of its own.

use std::io::{Read as _, Write as _};
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
    let _ = std::io::stdin().read_to_end(&mut Vec::new());
    drop(server);
    ExitCode::SUCCESS
}
