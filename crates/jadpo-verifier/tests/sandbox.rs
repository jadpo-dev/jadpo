//! What a package's code can't do from the sandbox. Each probe is a shell
//! script standing in for code that has escaped the language: what matters
//! is what the process can reach, whatever it runs. Where a probe's refusal
//! could mean the probe is broken, the same probe runs outside first.

use std::ffi::OsString;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use jadpo_verifier::sandbox::{Ended, Finished, Job, Limits, Sandbox};

use super::support::{Dir, sandbox, write};

const BASH: &str = "/bin/bash";

fn limits() -> Limits {
    Limits {
        cpu_seconds: 20,
        wall: Duration::from_secs(30),
        memory_bytes: 1 << 31,
        file_bytes: 1 << 26,
        tmp_bytes: 1 << 26,
        processes: 4096,
        open_files: 256,
        output_bytes: 1 << 20,
    }
}

fn job(tmp: &Path, script: &str) -> Job {
    Job {
        program: PathBuf::from(BASH),
        args: vec![OsString::from("-c"), OsString::from(script)],
        env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
        read_only: Vec::new(),
        writable: Vec::new(),
        tmp: tmp.to_path_buf(),
        network: false,
        stdin: None,
        limits: limits(),
    }
}

fn run(sandbox: &Sandbox, job: &Job) -> Finished {
    sandbox.run(job).expect("it runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Runs `script` with no sandbox, as the control.
fn outside(script: &str) -> bool {
    Command::new(BASH)
        .args(["-c", script])
        .status()
        .expect("bash")
        .success()
}

#[test]
fn it_runs_a_program_and_reads_what_it_writes() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-runs");
    let done = run(&sandbox, &job(dir.path(), "echo out; echo err >&2; exit 3"));
    assert_eq!(done.ended, Ended::Exited(3));
    assert_eq!(text(&done.stdout), "out\n");
    assert_eq!(text(&done.stderr), "err\n");
}

#[test]
fn it_gives_a_program_what_it_is_given_on_standard_input() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-stdin");
    let mut piped = job(dir.path(), "cat");
    piped.stdin = Some(b"one line\n".to_vec());
    assert_eq!(text(&run(&sandbox, &piped).stdout), "one line\n");
}

#[test]
fn there_is_no_network_while_verifying() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-network");
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let probe = format!("exec 3<>/dev/tcp/127.0.0.1/{port}");
    assert!(outside(&probe), "the probe connects from outside");
    listener.set_nonblocking(true).expect("not blocking");
    while listener.accept().is_ok() {}

    let done = run(&sandbox, &job(dir.path(), &probe));
    assert_ne!(done.ended, Ended::Exited(0), "{}", text(&done.stderr));
    assert!(
        listener.accept().is_err(),
        "the sandboxed probe reached this machine's loopback"
    );
    // Nor anywhere further: a public address, and a name to look up.
    for probe in [
        "exec 3<>/dev/tcp/1.1.1.1/443",
        "exec 3<>/dev/tcp/github.com/443",
        "exec 3<>/dev/tcp/169.254.169.254/80",
    ] {
        let done = run(&sandbox, &job(dir.path(), probe));
        assert_ne!(done.ended, Ended::Exited(0), "{probe}");
    }
}

#[test]
fn on_linux_the_only_interface_is_its_own_loopback() {
    let Some(sandbox) = sandbox() else { return };
    if !sandbox.is_isolating() {
        return;
    }
    let dir = Dir::new("sandbox-interfaces");
    let done = run(&sandbox, &job(dir.path(), "cat /proc/net/dev"));
    let names: Vec<String> = text(&done.stdout)
        .lines()
        .skip(2)
        .filter_map(|line| line.split(':').next())
        .map(|name| name.trim().to_string())
        .collect();
    assert_eq!(names, ["lo"]);
}

#[test]
fn it_can_be_given_network_for_fetching() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-with-network");
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let mut fetch = job(dir.path(), &format!("exec 3<>/dev/tcp/127.0.0.1/{port}"));
    fetch.network = true;
    assert_eq!(run(&sandbox, &fetch).ended, Ended::Exited(0));
}

#[test]
fn it_can_read_nothing_of_the_host_it_isnt_given() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-read");
    let secret = Dir::new("sandbox-secret");
    write(secret.path(), "secret", "a secret");
    let path = secret.path().join("secret");
    let probe = format!("cat '{}'", path.display());
    assert!(outside(&probe), "the probe reads it from outside");
    let done = run(&sandbox, &job(dir.path(), &probe));
    assert_ne!(done.ended, Ended::Exited(0));
    assert!(!text(&done.stdout).contains("a secret"));

    // Given, it can be read, so the refusal above is the sandbox's.
    let mut given = job(dir.path(), &probe);
    given.read_only = vec![secret.path().to_path_buf()];
    assert_eq!(text(&run(&sandbox, &given).stdout), "a secret");
}

#[test]
fn it_can_write_only_to_its_own_temporary_folder() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-write");
    let input = Dir::new("sandbox-input");
    write(input.path(), "package.jadpo", "as committed");
    let file = input.path().join("package.jadpo");
    let mut change = job(
        dir.path(),
        &format!(
            "echo forged > '{}'; touch '{}/new'",
            file.display(),
            input.path().display()
        ),
    );
    change.read_only = vec![input.path().to_path_buf()];
    run(&sandbox, &change);
    assert_eq!(
        std::fs::read_to_string(&file).expect("the file"),
        "as committed"
    );
    assert!(!input.path().join("new").exists());

    let wrote = run(
        &sandbox,
        &job(
            dir.path(),
            "echo mine > \"$TMPDIR/file\" && cat \"$TMPDIR/file\"",
        ),
    );
    assert_eq!(text(&wrote.stdout), "mine\n");
}

#[test]
fn nothing_a_run_leaves_reaches_the_next() {
    let Some(sandbox) = sandbox() else { return };
    let (first, second) = (Dir::new("sandbox-first"), Dir::new("sandbox-second"));
    let wrote = run(
        &sandbox,
        &job(
            first.path(),
            "echo forged > \"$TMPDIR/left\"; echo \"$TMPDIR\"",
        ),
    );
    assert_eq!(wrote.ended, Ended::Exited(0));
    let tmp = text(&wrote.stdout).trim().to_string();
    let next = run(
        &sandbox,
        &job(
            second.path(),
            &format!(
                "cat \"$TMPDIR/left\" || cat '{tmp}/left' || cat '{}/left'",
                first.path().display()
            ),
        ),
    );
    assert_ne!(next.ended, Ended::Exited(0));
    assert!(!text(&next.stdout).contains("forged"));
}

#[test]
fn it_is_given_no_variable_of_the_process_that_started_it() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-env");
    let done = run(&sandbox, &job(dir.path(), "/usr/bin/env"));
    let names: Vec<String> = text(&done.stdout)
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::to_string)
        .collect();
    for name in &names {
        assert!(
            matches!(
                name.as_str(),
                "PATH"
                    | "HOME"
                    | "TMPDIR"
                    | "PWD"
                    | "SHLVL"
                    | "_"
                    | "OLDPWD"
                    | "__CF_USER_TEXT_ENCODING"
            ),
            "`{name}` reached the sandbox: {names:?}"
        );
    }
}

#[test]
fn on_linux_it_can_see_no_other_process() {
    let Some(sandbox) = sandbox() else { return };
    if !sandbox.is_isolating() {
        return;
    }
    let dir = Dir::new("sandbox-processes");
    let parent = std::process::id();
    let done = run(
        &sandbox,
        &job(
            dir.path(),
            &format!("test -e /proc/{parent}/environ && echo seen; ls /proc | grep -c '^[0-9]'"),
        ),
    );
    let out = text(&done.stdout);
    assert!(!out.contains("seen") || parent < 4, "{out}");
    let count: u32 = out
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .parse()
        .expect("a count");
    assert!(count < 6, "it sees {count} processes");
    // And no secrets through /proc: pid 1 is the sandbox's own.
    let init = run(
        &sandbox,
        &job(dir.path(), "cat /proc/1/environ | tr '\\0' '\\n'"),
    );
    assert!(
        !text(&init.stdout).contains("CARGO"),
        "{}",
        text(&init.stdout)
    );
}

#[test]
fn it_is_stopped_at_its_time_limit() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-wall");
    let mut waits = job(dir.path(), "sleep 60");
    waits.limits.wall = Duration::from_millis(300);
    let started = Instant::now();
    let done = run(&sandbox, &waits);
    assert_eq!(done.ended, Ended::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_loop_is_stopped_at_its_cpu_limit() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-cpu");
    let mut spins = job(dir.path(), "while :; do :; done");
    spins.limits.cpu_seconds = 1;
    let done = run(&sandbox, &spins);
    assert!(
        !matches!(done.ended, Ended::Exited(0) | Ended::TimedOut),
        "{:?}",
        done.ended
    );
}

#[test]
fn too_much_output_stops_it() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-output");
    let mut floods = job(
        dir.path(),
        "while :; do echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; done",
    );
    floods.limits.output_bytes = 4096;
    let done = run(&sandbox, &floods);
    assert_eq!(done.ended, Ended::OutputTooLarge);
    assert!(done.stdout.len() <= 4096 + 65536, "{}", done.stdout.len());
}

#[test]
fn a_file_cant_grow_past_its_limit() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("sandbox-fsize");
    let mut grows = job(
        dir.path(),
        "dd if=/dev/zero of=\"$TMPDIR/big\" bs=1048576 count=8 2>/dev/null",
    );
    grows.limits.file_bytes = 1 << 20;
    assert_ne!(run(&sandbox, &grows).ended, Ended::Exited(0));
}

#[test]
fn on_linux_memory_is_limited() {
    let Some(sandbox) = sandbox() else { return };
    if !sandbox.is_isolating() {
        return;
    }
    let dir = Dir::new("sandbox-memory");
    let done = run(&sandbox, &job(dir.path(), "cat /proc/self/limits"));
    let limits = text(&done.stdout);
    let line = |name: &str| {
        limits
            .lines()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("{limits}"))
            .split_whitespace()
            .rev()
            .nth(2)
            .map(str::to_string)
    };
    assert_eq!(line("Max address space"), Some((1u64 << 31).to_string()));
    assert_eq!(line("Max processes"), Some("4096".to_string()));
    assert_eq!(line("Max core file size"), Some("0".to_string()));
    let mut balloons = job(
        dir.path(),
        "x=$(head -c 3221225472 /dev/zero | tr '\\0' 'x'); echo ${#x}",
    );
    balloons.limits.wall = Duration::from_secs(60);
    assert_ne!(run(&sandbox, &balloons).ended, Ended::Exited(0));
}

#[test]
fn only_linuxs_sandbox_is_good_enough_to_serve() {
    let Some(sandbox) = sandbox() else { return };
    assert_eq!(sandbox.is_isolating(), cfg!(target_os = "linux"));
}
