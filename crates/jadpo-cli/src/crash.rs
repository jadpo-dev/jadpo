//! What `jadpo` prints when it hits a bug of its own: an internal error,
//! not a problem in the project it was run on.
//!
//! The screen names only things Jadpo owns: the command that was run, the
//! version, the platform and the place in Jadpo's own source where it
//! stopped. It never prints the panic message, which could quote the
//! project's source, and it never prints the arguments, which could hold a
//! path or a value.
//!
//! The same crash is written to a file in the user's cache
//! (`~/.cache/jadpo/crashes/<timestamp>.txt`) with the version, the
//! platform, the arguments, the panic message and a backtrace. That file
//! can quote the project, so the screen says to read it before attaching
//! it. Nothing is sent anywhere.

use std::io::Write;
use std::path::{Path, PathBuf};

use jadpo_term::{Block, ColorChoice, Environment, Screen, Stream, Style};

use crate::help::VERSION;

/// The issue form a report goes to.
const ISSUE_FORM: &str = "https://github.com/jadpo-dev/jadpo/issues/new?template=bug.yml";

/// What the crash screen may say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    /// The command that was run, such as `check`; none for a bare `jadpo`.
    pub command: Option<String>,
    /// The place in Jadpo's own source, `crates/<crate>/src/<file>:<line>`.
    pub location: Option<String>,
    /// The crash file, when it could be written.
    pub file: Option<String>,
}

impl Crash {
    /// The command is the first argument that is not a flag, and only when
    /// it is a word, so a path or a value never gets through.
    pub fn from_run(args: &[String], location: Option<String>) -> Crash {
        let command = args
            .iter()
            .find(|arg| !arg.starts_with('-'))
            .filter(|arg| {
                !arg.is_empty()
                    && arg.len() <= 24
                    && arg.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            })
            .cloned();
        Crash {
            command,
            location,
            file: None,
        }
    }

    fn ran(&self) -> String {
        match &self.command {
            Some(command) => format!("jadpo {command}"),
            None => "jadpo".to_string(),
        }
    }

    fn platform() -> String {
        format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
    }

    /// The issue form with the title, version and command filled in.
    pub fn report_link(&self) -> String {
        format!(
            "{ISSUE_FORM}&title={}&version={}",
            encode(&format!("Internal error in {}", self.ran())),
            encode(&format!("{VERSION} {}", Crash::platform())),
        )
    }

    pub fn screen(&self) -> Screen {
        let mut fields = vec![
            ("ran".to_string(), self.ran()),
            (
                "version".to_string(),
                format!("jadpo {VERSION} · {}", Crash::platform()),
            ),
        ];
        if let Some(location) = &self.location {
            fields.push(("in".to_string(), location.clone()));
        }
        // The next steps, in order, each with its path or link whole on a
        // line of its own so it copies without a break.
        let mut steps = Vec::new();
        if let Some(file) = &self.file {
            steps.push("1. Read the crash file. It can quote your project.".to_string());
            steps.push(format!("   {file}"));
            steps.push("2. Report it with this link, attaching the file.".to_string());
        } else {
            steps.push("Report it with this link.".to_string());
        }
        steps.push(format!("   {}", self.report_link()));
        Screen::new()
            .with(Block::Failed {
                subject: "This is a bug in Jadpo, not in your code".to_string(),
                fields,
            })
            .with(Block::section("What to do next", steps))
    }

    pub fn render(&self, env: &Environment, choice: ColorChoice) -> String {
        self.screen()
            .render(&Style::detect(env, false, choice, Stream::Stderr))
    }
}

/// Percent-encodes everything but unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A path inside Jadpo's own workspace, or nothing: a panic in a
/// dependency or a generated file says nothing useful and could name a
/// user's directory.
fn own_location(file: &str, line: u32) -> Option<String> {
    let file = file.replace('\\', "/");
    let start = file.find("crates/")?;
    let path = &file[start..];
    path.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/_-.".contains(c))
        .then(|| format!("{path}:{line}"))
}

/// The arguments as the crash file keeps them. `config` takes values, so
/// only its subcommand is kept.
fn command_line(args: &[String]) -> String {
    let mut words = vec!["jadpo".to_string()];
    let secret = args.first().is_some_and(|first| first == "config");
    for (at, arg) in args.iter().enumerate() {
        words.push(if secret && at > 1 {
            "…".into()
        } else {
            arg.clone()
        });
        if secret && at > 1 {
            break;
        }
    }
    words.join(" ")
}

/// `2026-10-30T14-02-31Z`, UTC, from seconds since 1970.
fn timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Civil date from days since 1970 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// How many crash files are kept.
const KEPT: usize = 20;

/// Writes the crash file and returns its path; none when the cache can't
/// be written, which the screen then leaves out.
fn write_file(
    env: &Environment,
    args: &[String],
    location: Option<&str>,
    message: &str,
    backtrace: &str,
    now: u64,
) -> Option<PathBuf> {
    let folder = crate::package_cache::home(env)?
        .join("jadpo")
        .join("crashes");
    std::fs::create_dir_all(&folder).ok()?;
    let path = folder.join(format!("{}.txt", timestamp(now)));
    let text = format!(
        "jadpo crash report\n\
         version: {VERSION}\n\
         platform: {}\n\
         command line: {}\n\
         location: {}\n\
         panic: {message}\n\n\
         backtrace:\n{backtrace}\n",
        Crash::platform(),
        command_line(args),
        location.unwrap_or("unknown"),
    );
    std::fs::write(&path, text).ok()?;
    prune(&folder);
    Some(path)
}

/// Removes the oldest crash files past [`KEPT`]; their names sort by time.
fn prune(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    names.sort();
    let extra = names.len().saturating_sub(KEPT);
    for old in names.into_iter().take(extra) {
        let _ = std::fs::remove_file(old);
    }
}

/// Replaces the default panic message with the crash screen. Call it once,
/// from `main`.
pub fn install(args: Vec<String>, env: Environment) {
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .and_then(|at| own_location(at.file(), at.line()));
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|m| (*m).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no message".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let mut crash = Crash::from_run(&args, location.clone());
        crash.file = write_file(&env, &args, location.as_deref(), &message, &backtrace, now)
            .map(|path| path.display().to_string());
        let text = crash.render(&env, ColorChoice::Auto);
        let _ = std::io::stderr().write_all(text.as_bytes());
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Environment {
        Environment::empty().with_var("JADPO_PLAIN", "1")
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_screen_says_it_is_jadpo_and_links_the_form() {
        let crash = Crash::from_run(
            &args(&["check", "--json"]),
            own_location("/home/me/jadpo/crates/jadpo-check/src/lib.rs", 12),
        );
        let text = crash.render(&plain(), ColorChoice::Never);
        assert!(
            text.contains("This is a bug in Jadpo, not in your code"),
            "{text}"
        );
        assert!(text.contains(&format!("jadpo {VERSION}")));
        assert!(text.contains("crates/jadpo-check/src/lib.rs:12"));
        assert!(text.contains(&format!(
            "{ISSUE_FORM}&title=Internal%20error%20in%20jadpo%20check"
        )));
        assert!(!text.contains("/home/me"));
    }

    #[test]
    fn only_a_command_word_is_named() {
        for leak in ["/Users/me/secret-app", "token=abc123", "My App", ""] {
            let crash = Crash::from_run(&args(&[leak]), None);
            assert_eq!(crash.command, None, "{leak}");
            let text = crash.render(&plain(), ColorChoice::Never);
            assert!(!text.contains(leak) || leak.is_empty(), "{text}");
        }
        assert_eq!(
            Crash::from_run(&args(&["--color=never", "test"]), None).command,
            Some("test".to_string())
        );
    }

    #[test]
    fn a_location_outside_jadpo_is_dropped() {
        assert_eq!(own_location("/rustc/abc/library/core/src/x.rs", 3), None);
        let kept = own_location("/home/my app/crates/x/src/a.rs", 3).unwrap();
        assert_eq!(kept, "crates/x/src/a.rs:3");
    }

    #[test]
    fn rich_output_has_the_same_words() {
        let crash = Crash::from_run(&args(&["build"]), None);
        let env = Environment::empty()
            .with_terminals(true, true, true)
            .with_var("TERM", "xterm-256color");
        let rich = crash.render(&env, ColorChoice::Always);
        assert!(rich.contains("This is a bug in Jadpo, not in your code"));
    }

    #[test]
    fn the_crash_file_holds_the_arguments_and_the_message_but_the_screen_does_not() {
        let cache = std::env::temp_dir().join(format!("jadpo-crash-{}", std::process::id()));
        let env = plain().with_var("XDG_CACHE_HOME", cache.to_str().unwrap());
        let a = args(&["check", "--json"]);
        let path = write_file(
            &env,
            &a,
            Some("crates/x/src/a.rs:3"),
            "boom: secret_name",
            "frames",
            1_793_368_951,
        )
        .expect("written");
        assert!(
            path.ends_with("jadpo/crashes/2026-10-30T14-02-31Z.txt"),
            "{path:?}"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(&format!("version: {VERSION}")));
        assert!(text.contains("command line: jadpo check --json"));
        assert!(text.contains("panic: boom: secret_name"));
        assert!(text.contains("backtrace:\nframes"));
        let mut crash = Crash::from_run(&a, None);
        crash.file = Some(path.display().to_string());
        let screen = crash.render(&env, ColorChoice::Never);
        assert!(screen.contains("2026-10-30T14-02-31Z.txt"), "{screen}");
        assert!(screen.contains("1. Read the crash file"), "{screen}");
        assert!(
            screen.contains("2. Report it with this link, attaching the file."),
            "{screen}"
        );
        // The path and the link each sit whole on a line of their own.
        assert!(
            screen
                .lines()
                .any(|l| l.trim() == path.display().to_string()),
            "{screen}"
        );
        assert!(
            screen.lines().any(|l| l.trim() == crash.report_link()),
            "{screen}"
        );
        assert!(!screen.contains("secret_name"));
        let _ = std::fs::remove_dir_all(&cache);
    }

    #[test]
    fn a_config_value_never_reaches_the_file() {
        let line = command_line(&args(&["config", "set", "STRIPE_KEY", "sk_live_x"]));
        assert_eq!(line, "jadpo config set …");
    }

    #[test]
    fn a_dev_or_test_crash_reads_like_any_other() {
        for command in ["dev", "test"] {
            let crash = Crash::from_run(
                &args(&[command]),
                Some("crates/jadpo-interp/src/lib.rs:9".into()),
            );
            let text = crash.render(&plain(), ColorChoice::Never);
            assert!(text.contains(&format!("jadpo {command}")), "{text}");
            assert!(
                text.contains("This is a bug in Jadpo, not in your code"),
                "{text}"
            );
        }
    }
}
