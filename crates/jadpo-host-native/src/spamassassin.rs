//! SpamAssassin on this machine, for the dev inbox's health score (area
//! 16, "A mail health score"; chunk 7.3c3). `jadpo dev` hands each mail it
//! catches, as the SMTP driver writes it, to `spamassassin` when it is
//! installed and reads back its report; the inbox reads that through its
//! own seam, so its tests never run a real one.
//!
//! It runs with its network tests off (`-L`), creating no preferences
//! (`-x`) and learning nothing (`bayes_auto_learn 0`), so nothing about
//! the mail leaves the machine or changes the user's own filter. The mail
//! goes to the process's standard input only; what it prints is returned,
//! never logged, and its standard error is thrown away.

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use jadpo_runtime::LocalFuture;

/// The program's name.
pub const PROGRAM: &str = "spamassassin";

/// How long one mail may take before the run is given up.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The arguments every run is given: local tests only, the report added
/// to the mail it prints back, no preferences file made, nothing learnt.
pub const ARGUMENTS: [&str; 4] = ["-L", "-t", "-x", "--cf=bayes_auto_learn 0"];

/// Why a run gave no report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// It couldn't be started.
    Start(String),
    /// It exited with a status other than 0, or was stopped by a signal.
    Exited(Option<i32>),
    /// It didn't finish within [`TIMEOUT`].
    TimedOut,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Start(why) => write!(f, "it couldn't be started: {why}"),
            RunError::Exited(Some(code)) => write!(f, "it exited with status {code}"),
            RunError::Exited(None) => f.write_str("it was stopped before it finished"),
            RunError::TimedOut => {
                write!(f, "it didn't finish within {} seconds", TIMEOUT.as_secs())
            }
        }
    }
}

/// Where `spamassassin` is on `path` (a `PATH` value), if anywhere.
pub fn find(path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .map(|dir| dir.join(PROGRAM))
        .find(|candidate| executable(candidate))
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

/// Runs `program` on `message`, waiting at most `timeout`: what it
/// printed.
pub fn run(program: &Path, message: &[u8], timeout: Duration) -> Result<String, RunError> {
    let mut child = Command::new(program)
        .args(ARGUMENTS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| RunError::Start(error.to_string()))?;
    // The mail goes in on one thread while its report is read on another,
    // so a large mail can't fill both pipes at once.
    let writer = child.stdin.take().map(|mut stdin| {
        let message = message.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&message);
        })
    });
    let reader = child.stdout.take().map(|mut stdout| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.read_to_end(&mut out);
            out
        })
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                return Err(RunError::Start(error.to_string()));
            }
        }
    };
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let out = reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    if !status.success() {
        return Err(RunError::Exited(status.code()));
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// [`run`] on tokio's blocking pool, so the worker's thread goes on
/// serving while it runs.
pub fn run_later(
    program: PathBuf,
    message: Vec<u8>,
) -> LocalFuture<'static, Result<String, RunError>> {
    Box::pin(async move {
        match tokio::task::spawn_blocking(move || run(&program, &message, TIMEOUT)).await {
            Ok(done) => done,
            Err(_) => Err(RunError::Exited(None)),
        }
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn script(name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("jadpo-spamassassin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(PROGRAM);
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn it_is_found_on_the_path_and_given_the_mail_with_network_tests_off() {
        // Echoes its arguments, then the mail it was given.
        let program = script("echo", "printf '%s\\n' \"$*\"\ncat\n");
        let dir = program.parent().unwrap().to_path_buf();
        let path = std::env::join_paths([PathBuf::from("/nowhere"), dir]).unwrap();
        assert_eq!(find(Some(&path)), Some(program.clone()));
        assert_eq!(find(Some(OsStr::new("/nowhere"))), None);
        assert_eq!(find(None), None);
        let out = run(
            &program,
            b"Subject: Notes\r\n\r\nThe weekly notes\r\n",
            TIMEOUT,
        )
        .unwrap();
        assert_eq!(
            out,
            "-L -t -x --cf=bayes_auto_learn 0\nSubject: Notes\r\n\r\nThe weekly notes\r\n"
        );
    }

    #[test]
    fn a_failed_or_slow_run_says_so() {
        let failing = script("fails", "cat > /dev/null\nexit 3\n");
        assert_eq!(run(&failing, b"x", TIMEOUT), Err(RunError::Exited(Some(3))));
        let slow = script("slow", "sleep 5\n");
        assert_eq!(
            run(&slow, b"x", Duration::from_millis(100)),
            Err(RunError::TimedOut)
        );
        assert!(matches!(
            run(Path::new("/nowhere/spamassassin"), b"x", TIMEOUT),
            Err(RunError::Start(_))
        ));
    }
}
