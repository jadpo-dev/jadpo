//! `jadpo test --watch` (area 13, "Running tests"): reruns, on every save,
//! the tests the change reaches, through `jadpo_test::watch`, and writes
//! each run as the CLI design language draws it: a muted rule naming the
//! time, what changed and how many tests reach it, then `jadpo test`'s own
//! reports and footer, and a live line saying it is watching. The screen
//! is never cleared, so scrollback is a record of every save.
//!
//! [`Watcher`] turns each step into the text both streams get, so every
//! state can be tested without a terminal or a clock; [`serve`] is the
//! loop that finds saves and writes the frames. With `--json`, each step
//! is one envelope on one line, as streaming commands write them, and
//! nothing goes to stderr. Ctrl-C ends the session.

use std::io::Write;

use jadpo_term::{
    Block, Change, ChangeMark, ColorChoice, Environment, Fact, Json, Screen, Stream, Style,
    envelope,
};
use jadpo_test::Options;
use jadpo_test::watch::{Edit, EditKind, Event, Poller, Watch};

use crate::test::{reply, view};

/// What `jadpo test --watch` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchArgs {
    pub options: Options,
    pub json: bool,
    pub choice: ColorChoice,
}

/// What one step writes: the live waiting line erased first when one is
/// showing, then the results on stdout, then the waiting line on stderr.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Frame {
    pub erase: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Clears the line the cursor is on, where the live waiting line is.
const ERASE: &str = "\r\x1b[2K";

/// A watch session and what it has put on the screen.
#[derive(Debug)]
pub struct Watcher {
    watch: Watch,
    out: Style,
    err: Style,
    json: bool,
    /// A waiting line is showing on stderr and can be erased.
    live: bool,
    /// The waiting line last written where it can't be erased.
    shown: Option<String>,
}

impl Watcher {
    /// The first line, written before anything slow: bold
    /// `jadpo test --watch`, then when it started. Nothing in `--json`.
    pub fn header(args: &WatchArgs, env: &Environment, time: &str) -> Frame {
        if args.json {
            return Frame::default();
        }
        let out = Style::detect(env, false, args.choice, Stream::Stdout);
        let screen = Screen::new().with(Block::header(
            "jadpo test --watch",
            [format!("started {time}")],
        ));
        Frame {
            erase: false,
            stdout: screen.render(&out),
            stderr: String::new(),
        }
    }

    /// Loads the project and runs the tests asked for. A mistake in what to
    /// run comes back as the output the command ends with, and its exit
    /// code.
    pub fn start(
        args: &WatchArgs,
        env: &Environment,
        db: jadpo_db::Database,
    ) -> Result<(Watcher, Frame), (Frame, u8)> {
        let (watch, event) = match Watch::start_with(args.options.clone(), db) {
            Ok(started) => started,
            Err(ran) => {
                let output =
                    crate::run::finish(Some("test"), reply(ran), env, args.json, args.choice);
                return Err((
                    Frame {
                        erase: false,
                        stdout: output.stdout,
                        stderr: output.stderr,
                    },
                    output.code,
                ));
            }
        };
        let mut watcher = Watcher {
            watch,
            out: Style::detect(env, false, args.choice, Stream::Stdout),
            err: Style::detect(env, false, args.choice, Stream::Stderr),
            json: args.json,
            live: false,
            shown: None,
        };
        let frame = watcher.frame(event, None);
        Ok((watcher, frame))
    }

    /// Runs what `edits` reach. `None` when the saves changed nothing the
    /// project holds, such as a save of the same text.
    pub fn update(&mut self, edits: Vec<Edit>, time: &str) -> Option<Frame> {
        let event = self.watch.apply(edits)?;
        Some(self.frame(event, Some(time)))
    }

    pub fn watch(&self) -> &Watch {
        &self.watch
    }

    /// The frame for one step; `time` is `None` for the first run.
    fn frame(&mut self, event: Event, time: Option<&str>) -> Frame {
        if self.json {
            return Frame {
                erase: false,
                stdout: json_line(event, time.is_none()),
                stderr: String::new(),
            };
        }
        let mut screen = Screen::new();
        if let Some(time) = time {
            screen.push(rule(&event, time));
        }
        if let Some(ran) = event.ran {
            for block in view(ran).0.into_blocks() {
                screen.push(block);
            }
        }
        let waiting = Screen::new().with(key_bar(event.files, event.failing));
        let waiting = waiting.render(&self.err);
        let waiting = waiting.trim_end_matches('\n');
        // A blank line before every block of results: the live line's own
        // blank line stays when the line is erased.
        let lead = if self.live { "" } else { "\n" };
        let erase = self.live;
        let stdout = format!("{lead}{}", screen.render(&self.out));
        let stderr = if self.err.mode == jadpo_term::Mode::Rich {
            self.live = true;
            format!("\n{waiting}")
        } else if self.shown.as_deref() != Some(waiting) {
            self.shown = Some(waiting.to_string());
            format!("{waiting}\n")
        } else {
            String::new()
        };
        Frame {
            erase,
            stdout,
            stderr,
        }
    }
}

/// `── 14:02:31 · saved <file> · 6 tests reach it ──`, or with several
/// files or a long path, a count in the rule and the change list below.
fn rule(event: &Event, time: &str) -> Block {
    let reach = event.reached.map(|reached| match reached {
        0 => "nothing to rerun".to_string(),
        1 => "1 test reaches it".to_string(),
        n => format!("{n} tests reach it"),
    });
    let mut long = Vec::new();
    let mut short = vec![time.to_string()];
    match event.edits.as_slice() {
        [only] => {
            long = vec![
                time.to_string(),
                format!("{} {}", only.kind.verb(), only.path),
            ];
            short.push(format!("{} 1 file", only.kind.verb()));
        }
        edits => short.push(format!("{} files changed", edits.len())),
    }
    if let Some(reach) = reach {
        if !long.is_empty() {
            long.push(reach.clone());
        }
        short.push(reach);
    }
    let changes = event
        .edits
        .iter()
        .map(|edit| Change {
            mark: match edit.kind {
                EditKind::Added => ChangeMark::Added,
                EditKind::Saved => ChangeMark::Changed,
                EditKind::Removed => ChangeMark::Removed,
            },
            path: edit.path.clone(),
        })
        .collect();
    Block::Rule {
        long,
        short,
        changes,
        plain: "watch".to_string(),
    }
}

/// `watching 41 files · 1 failing  ctrl-c quit`.
fn key_bar(files: usize, failing: usize) -> Block {
    let mut facts = vec![Fact::plain(format!(
        "{files} {}",
        if files == 1 { "file" } else { "files" }
    ))];
    if failing > 0 {
        facts.push(Fact::problem(format!("{failing} failing")));
    }
    Block::KeyBar {
        label: "watching".to_string(),
        facts,
        keys: vec![("ctrl-c".to_string(), "quit".to_string())],
    }
}

/// One step as one envelope on one line: `jadpo test --json`'s data with a
/// `watch` object saying what changed and how many tests it reached.
fn json_line(event: Event, first: bool) -> String {
    let changed = Json::array(event.edits.iter().map(|edit| {
        Json::object([
            ("path", Json::from(&edit.path)),
            ("change", edit.kind.verb().into()),
        ])
    }));
    let watch = Json::object([
        ("event", if first { "start" } else { "change" }.into()),
        ("changed", changed),
        ("reached", event.reached.into()),
        ("failing", event.failing.into()),
        ("files", event.files.into()),
    ]);
    let (data, diagnostics, sources) = match event.ran {
        Some(ran) => {
            let (_, data, diagnostics, sources) = view(ran);
            (data, diagnostics, sources)
        }
        None => (
            Json::object([("tests", Json::Array(Vec::new()))]),
            Vec::new(),
            Default::default(),
        ),
    };
    let data = match data {
        Json::Object(mut entries) => {
            entries.push(("watch".to_string(), watch));
            Json::Object(entries)
        }
        other => Json::object([("result", other), ("watch", watch)]),
    };
    envelope(Some("test"), Some(data), &diagnostics, &sources).to_line()
}

/// Writes a frame: the erase first, so results replace the waiting line.
pub fn write(frame: &Frame, out: &mut dyn Write, err: &mut dyn Write) {
    if frame.erase {
        let _ = err.write_all(ERASE.as_bytes());
        let _ = err.flush();
    }
    let _ = out.write_all(frame.stdout.as_bytes());
    let _ = out.flush();
    let _ = err.write_all(frame.stderr.as_bytes());
    let _ = err.flush();
}

/// Runs a session on the process's own streams until it is stopped with
/// Ctrl-C; the exit code when it ends on its own, which only a mistake in
/// what to run does.
pub fn serve(args: &WatchArgs, env: &Environment) -> u8 {
    let offset = local_offset();
    serve_to(
        args,
        env,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
        &|| clock(offset),
        &|| false,
    )
}

/// [`serve`] on any streams, with any clock, until `stop` says so.
pub fn serve_to(
    args: &WatchArgs,
    env: &Environment,
    out: &mut dyn Write,
    err: &mut dyn Write,
    clock: &dyn Fn() -> String,
    stop: &dyn Fn() -> bool,
) -> u8 {
    write(&Watcher::header(args, env, &clock()), out, err);
    let (mut watcher, frame) = match Watcher::start(args, env, jadpo_db::Database::default()) {
        Ok(started) => started,
        Err((frame, code)) => {
            write(&frame, out, err);
            return code;
        }
    };
    let mut poller = Poller::new(watcher.watch().root(), watcher.watch().is_project());
    write(&frame, out, err);
    while let Some(edits) = poller.wait(stop) {
        if let Some(frame) = watcher.update(edits, &clock()) {
            write(&frame, out, err);
        }
    }
    0
}

/// The local time zone's offset from UTC in seconds, asked of `date` once:
/// the standard library has no time zones, and a rule's time is only read
/// by people. UTC when `date` can't say.
fn local_offset() -> i64 {
    let Ok(output) = std::process::Command::new("date").arg("+%z").output() else {
        return 0;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    let (sign, digits) = match text.split_at_checked(1) {
        Some(("+", digits)) => (1, digits),
        Some(("-", digits)) => (-1, digits),
        _ => return 0,
    };
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return 0;
    }
    let hours: i64 = digits[..2].parse().unwrap_or(0);
    let minutes: i64 = digits[2..].parse().unwrap_or(0);
    sign * (hours * 3600 + minutes * 60)
}

/// The time of day now as `14:02:31`.
fn clock(offset: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    let seconds = (now + offset).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}
