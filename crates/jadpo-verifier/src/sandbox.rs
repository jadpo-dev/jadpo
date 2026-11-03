//! Running a program that can reach nothing it isn't given.
//!
//! On Linux, where the verifier is served, this is [bubblewrap] under
//! `prlimit`: new user, network, process, IPC, hostname and cgroup
//! namespaces, every capability dropped, the system's programs read-only,
//! a private `/tmp` held in memory with its own size limit, and only the
//! folders a job names. Starting them as programs rather than calling the
//! kernel from here keeps this crate free of `unsafe`, and bubblewrap is
//! far better reviewed than anything written for this purpose would be.
//!
//! A Mac has `sandbox-exec`, so a contributor's tests exercise the same
//! behaviour: no network, no reading the home folder or another
//! verification's, writes only to its own folder. It shares a process
//! table and has no memory limit, so it is never served with
//! ([`Sandbox::is_isolating`]).
//!
//! Every variable of this process is left behind, the credential included.
//!
//! [bubblewrap]: https://github.com/containers/bubblewrap

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// What a run may use. Each is a hard limit it can't raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub cpu_seconds: u64,
    pub wall: Duration,
    /// Its address space, on Linux.
    pub memory_bytes: u64,
    /// The largest file it may write.
    pub file_bytes: u64,
    /// What its temporary folder may hold, on Linux.
    pub tmp_bytes: u64,
    /// Processes and threads of its user, on Linux.
    pub processes: u64,
    pub open_files: u64,
    /// What it may write to standard output, and to standard error.
    pub output_bytes: usize,
}

/// One program to run.
#[derive(Clone, Debug)]
pub struct Job {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Its variables, the only ones it has, beside `HOME` and `TMPDIR`.
    pub env: Vec<(String, String)>,
    pub read_only: Vec<PathBuf>,
    pub writable: Vec<PathBuf>,
    /// An empty folder of its own; on Linux its temporary folder is one in
    /// memory instead, and this one isn't used.
    pub tmp: PathBuf,
    /// Whether it has this machine's network: only ever for `git` reaching
    /// one checked address.
    pub network: bool,
    pub stdin: Option<Vec<u8>>,
    pub limits: Limits,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ended {
    Exited(i32),
    /// Stopped by a signal: its CPU limit, say, or a crash.
    Signalled,
    TimedOut,
    OutputTooLarge,
}

/// What a run gave.
#[derive(Clone, Debug)]
pub struct Finished {
    pub ended: Ended,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// The sandbox this machine has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sandbox {
    Bubblewrap { bwrap: PathBuf, prlimit: PathBuf },
    SandboxExec { sandbox_exec: PathBuf },
}

/// Where a system tool is looked for: never `PATH`, which a person's shell
/// or a container's environment sets.
const TOOL_FOLDERS: &[&str] = &["/usr/bin", "/bin", "/usr/local/bin", "/opt/homebrew/bin"];

/// A system tool, found in [`TOOL_FOLDERS`].
pub fn tool(name: &str) -> Option<PathBuf> {
    TOOL_FOLDERS
        .iter()
        .map(|folder| Path::new(folder).join(name))
        .find(|path| path.is_file())
}

/// How long a sandbox's own check may take.
const CHECK: Duration = Duration::from_secs(20);

impl Sandbox {
    /// This machine's sandbox, checked by running in it; why not, when it
    /// has none that works. Nothing is ever run without one.
    pub fn detect() -> Result<Sandbox, String> {
        let sandbox = if cfg!(target_os = "linux") {
            let missing = |name: &str| format!("`{name}` isn't installed");
            Sandbox::Bubblewrap {
                bwrap: tool("bwrap").ok_or_else(|| missing("bwrap"))?,
                prlimit: tool("prlimit").ok_or_else(|| missing("prlimit"))?,
            }
        } else if cfg!(target_os = "macos") {
            Sandbox::SandboxExec {
                sandbox_exec: tool("sandbox-exec")
                    .ok_or_else(|| "`sandbox-exec` isn't there".to_string())?,
            }
        } else {
            return Err("only Linux and macOS have a sandbox this can use".to_string());
        };
        sandbox.check()?;
        Ok(sandbox)
    }

    /// Whether this sandbox separates a package's code from this process
    /// enough to serve the registry with.
    pub fn is_isolating(&self) -> bool {
        matches!(self, Sandbox::Bubblewrap { .. })
    }

    /// What it is, for the serving screen.
    pub fn name(&self) -> &'static str {
        match self {
            Sandbox::Bubblewrap { .. } => "bubblewrap",
            Sandbox::SandboxExec { .. } => "sandbox-exec",
        }
    }

    /// Runs a small program in it and confirms it is cut off.
    fn check(&self) -> Result<(), String> {
        let tmp = std::env::temp_dir().join(format!("jadpo-verifier-check-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).map_err(|error| error.to_string())?;
        let shell = tool("sh").ok_or_else(|| "there is no `sh`".to_string())?;
        let script = match self {
            Sandbox::Bubblewrap { .. } => "cat /proc/net/dev",
            Sandbox::SandboxExec { .. } => "echo lo:",
        };
        let job = Job {
            program: shell,
            args: vec![OsString::from("-c"), OsString::from(script)],
            env: vec![("PATH".to_string(), "/usr/bin:/bin".to_string())],
            read_only: Vec::new(),
            writable: Vec::new(),
            tmp: tmp.clone(),
            network: false,
            stdin: None,
            limits: Limits {
                cpu_seconds: 10,
                wall: CHECK,
                memory_bytes: 1 << 30,
                file_bytes: 1 << 20,
                tmp_bytes: 1 << 20,
                processes: 4096,
                open_files: 64,
                output_bytes: 1 << 16,
            },
        };
        let done = self.run(&job);
        let _ = std::fs::remove_dir_all(&tmp);
        let done = done.map_err(|error| error.to_string())?;
        if done.ended != Ended::Exited(0) {
            return Err(format!(
                "it couldn't start a program: {}",
                String::from_utf8_lossy(&done.stderr).trim()
            ));
        }
        let text = String::from_utf8_lossy(&done.stdout);
        let interfaces: Vec<&str> = text
            .lines()
            .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim()))
            .collect();
        if interfaces != ["lo"] {
            return Err(format!(
                "a program in it still has network interfaces: {}",
                interfaces.join(", ")
            ));
        }
        Ok(())
    }

    /// Runs `job` to its end or its limits.
    pub fn run(&self, job: &Job) -> std::io::Result<Finished> {
        let mut command = match self {
            Sandbox::Bubblewrap { bwrap, prlimit } => bubblewrap(bwrap, prlimit, job),
            Sandbox::SandboxExec { sandbox_exec } => seatbelt(sandbox_exec, job),
        };
        command
            .stdin(if job.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let (Some(mut pipe), Some(bytes)) = (child.stdin.take(), job.stdin.clone()) {
            std::thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
            });
        }
        let overflowed = Arc::new(AtomicBool::new(false));
        let stdout = read_within(child.stdout.take(), job.limits.output_bytes, &overflowed);
        let stderr = read_within(child.stderr.take(), job.limits.output_bytes, &overflowed);
        let started = Instant::now();
        let ended = loop {
            if let Some(status) = child.try_wait()? {
                break match status.code() {
                    // Bubblewrap passes on a signal as 128 and its number.
                    Some(code) if code <= 128 => Ended::Exited(code),
                    _ => Ended::Signalled,
                };
            }
            if overflowed.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                break Ended::OutputTooLarge;
            }
            if started.elapsed() >= job.limits.wall {
                let _ = child.kill();
                let _ = child.wait();
                break Ended::TimedOut;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let ended = if overflowed.load(Ordering::SeqCst) {
            Ended::OutputTooLarge
        } else {
            ended
        };
        Ok(Finished {
            ended,
            stdout: collected(stdout),
            stderr: collected(stderr),
        })
    }
}

/// Reads a pipe up to `most` bytes on a thread of its own, setting `over`
/// when there is more.
fn read_within(
    pipe: Option<impl Read + Send + 'static>,
    most: usize,
    over: &Arc<AtomicBool>,
) -> mpsc::Receiver<Vec<u8>> {
    let (send, receive) = mpsc::channel();
    let over = over.clone();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        if let Some(mut pipe) = pipe {
            let mut piece = [0u8; 65536];
            while let Ok(read) = pipe.read(&mut piece) {
                if read == 0 {
                    break;
                }
                if kept.len() + read > most {
                    over.store(true, Ordering::SeqCst);
                    break;
                }
                kept.extend_from_slice(&piece[..read]);
            }
        }
        let _ = send.send(kept);
    });
    receive
}

/// What a pipe's thread read, not waiting for long on a pipe that something
/// left behind still holds open.
fn collected(pipe: mpsc::Receiver<Vec<u8>>) -> Vec<u8> {
    pipe.recv_timeout(Duration::from_secs(5))
        .unwrap_or_default()
}

fn bubblewrap(bwrap: &Path, prlimit: &Path, job: &Job) -> Command {
    let limits = job.limits;
    let mut command = Command::new(prlimit);
    command.env_clear();
    command.args([
        "--core=0".to_string(),
        format!("--cpu={}", limits.cpu_seconds),
        format!("--as={}", limits.memory_bytes),
        format!("--fsize={}", limits.file_bytes),
        format!("--nofile={}", limits.open_files),
        format!("--nproc={}", limits.processes),
        "--".to_string(),
    ]);
    command.arg(bwrap);
    command.args([
        "--unshare-user",
        "--unshare-pid",
        "--unshare-ipc",
        "--unshare-uts",
        "--unshare-cgroup-try",
    ]);
    if !job.network {
        command.arg("--unshare-net");
    }
    command.args([
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--hostname",
        "verifier",
        "--clearenv",
    ]);
    // The system's programs and libraries, and nothing else of the host.
    command.args(["--ro-bind", "/usr", "/usr"]);
    for folder in ["/bin", "/sbin", "/lib", "/lib64"] {
        command.args(["--ro-bind-try", folder, folder]);
    }
    if job.network {
        // Certificates to check the git host's; no resolver, since `git` is
        // given the address.
        for folder in ["/etc/ssl", "/etc/ca-certificates", "/etc/pki"] {
            command.args(["--ro-bind-try", folder, folder]);
        }
    }
    command.args(["--proc", "/proc", "--dev", "/dev"]);
    command.args(["--size", &limits.tmp_bytes.to_string(), "--tmpfs", "/tmp"]);
    let mut bind = |flag: &str, path: &Path| {
        command.arg(flag).arg(path).arg(path);
    };
    bind("--ro-bind", &job.program);
    for path in &job.read_only {
        bind("--ro-bind", path);
    }
    for path in &job.writable {
        bind("--bind", path);
    }
    command.args(["--chdir", "/tmp"]);
    for (name, value) in &job.env {
        command.args(["--setenv", name, value]);
    }
    command.args(["--setenv", "HOME", "/tmp", "--setenv", "TMPDIR", "/tmp"]);
    command.arg("--").arg(&job.program).args(&job.args);
    command
}

/// `sandbox-exec`'s profile for `job`: later rules win.
fn profile(job: &Job) -> String {
    let quoted = |path: &Path| format!("{:?}", path.display().to_string());
    let mut text = String::from("(version 1)\n(allow default)\n");
    if !job.network {
        text.push_str("(deny network*)\n");
    }
    text.push_str("(deny file-write*)\n");
    // Every verification's folder, and the home folder; what this job is
    // given is let back in below.
    text.push_str("(deny file-read-data (regex #\"/jadpo-verifier-\"))\n");
    if let Some(home) = std::env::var_os("HOME") {
        text.push_str(&format!(
            "(deny file-read-data (subpath {}))\n",
            quoted(Path::new(&home))
        ));
    }
    text.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/dtracehelper\"))\n");
    text.push_str("(allow file-read* file-read-data (literal \"/\") (subpath \"/usr\"))\n");
    text.push_str(&format!(
        "(allow file-read* file-read-data (literal {}))\n",
        quoted(&job.program)
    ));
    for path in &job.read_only {
        text.push_str(&format!(
            "(allow file-read* file-read-data (subpath {}))\n",
            quoted(path)
        ));
    }
    for path in job.writable.iter().chain([&job.tmp]) {
        text.push_str(&format!(
            "(allow file-read* file-read-data file-write* (subpath {}))\n",
            quoted(path)
        ));
    }
    text
}

fn seatbelt(sandbox_exec: &Path, job: &Job) -> Command {
    let limits = job.limits;
    // sh's own limits, since nothing here sets them directly: its address
    // space can't be limited on macOS.
    let script = format!(
        "ulimit -c 0 && ulimit -t {} && ulimit -f {} && ulimit -n {} && exec \"$@\"",
        limits.cpu_seconds,
        limits.file_bytes / 512,
        limits.open_files
    );
    let mut command = Command::new("/bin/sh");
    command.env_clear();
    command.args(["-c", &script, "sh"]);
    command.arg(sandbox_exec);
    command.args(["-p", &profile(job)]);
    command.arg(&job.program).args(&job.args);
    command.envs(job.env.iter().map(|(name, value)| (name, value)));
    command.env("HOME", &job.tmp).env("TMPDIR", &job.tmp);
    command.current_dir(&job.tmp);
    command
}
