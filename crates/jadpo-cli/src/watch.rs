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
//!
//! In a terminal, keys run tests between saves (CLI design language,
//! "Keys"): `enter` reruns the tests the last run ran, `a` runs every test
//! asked for and `f` the ones failing now, each run under a rule naming
//! the key as a save's names the file; `q` and ctrl-c quit. The key bar
//! lists them, `f` only while a test is failing. Without a terminal on
//! standard input, or with `--json`, no key is read and the bar says
//! ctrl-c quits.

use std::io::Write;

use jadpo_term::{
    Block, Change, ChangeMark, ColorChoice, Environment, Fact, Json, Screen, Stream, Style,
    envelope, short_envelope, shown_path,
};
use jadpo_test::Options;
use jadpo_test::watch::{Edit, EditKind, Event, Pick, Poller, Watch};

use crate::keys::{Key, Keys, Raw};

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
    /// `--full`: each line is whole rather than short (chunk A14).
    full: bool,
    /// A waiting line is showing on stderr and can be erased.
    live: bool,
    /// The waiting line last written where it can't be erased.
    shown: Option<String>,
    /// Keys run tests: standard input is a terminal and the waiting line
    /// is drawn for one.
    keys: bool,
}

/// What a key pressed under `--watch` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Pressed {
    /// It ran tests, which this frame shows.
    Ran(Frame),
    /// `q` or ctrl-c: the session ends.
    Quit,
    /// Nothing: a key with no meaning here, or `f` with nothing failing.
    Nothing,
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
                let output = crate::run::finish(
                    Some("test"),
                    reply(ran),
                    env,
                    args.json,
                    args.choice,
                    &crate::run::Asked {
                        full: args.options.full,
                        started: None,
                    },
                );
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
        let err = Style::detect(env, false, args.choice, Stream::Stderr);
        let mut watcher = Watcher {
            watch,
            out: Style::detect(env, false, args.choice, Stream::Stdout),
            err,
            json: args.json,
            full: args.options.full,
            live: false,
            shown: None,
            keys: !args.json && env.stdin_terminal && err.mode == jadpo_term::Mode::Rich,
        };
        let frame = watcher.frame(event, None, None);
        Ok((watcher, frame))
    }

    /// Whether keys run tests in this session.
    pub fn reads_keys(&self) -> bool {
        self.keys
    }

    /// Reads no keys after all: the terminal couldn't be put in the mode
    /// that reads them.
    pub fn without_keys(&mut self) {
        self.keys = false;
    }

    /// Runs what `key` asks for: `enter` the tests the last run ran, `a`
    /// every test asked for, `f` the failing ones.
    pub fn press(&mut self, key: Key, time: &str) -> Pressed {
        if !self.keys {
            return Pressed::Nothing;
        }
        let (pick, label) = match key {
            Key::Char('q') | Key::Interrupt => return Pressed::Quit,
            Key::Enter => (Pick::Again, "rerun"),
            Key::Char('a') => (Pick::All, "run all"),
            Key::Char('f') => (Pick::Failing, "run failed"),
            _ => return Pressed::Nothing,
        };
        match self.watch.run(pick) {
            Some(event) => Pressed::Ran(self.frame(event, Some(time), Some(label))),
            None => Pressed::Nothing,
        }
    }

    /// Runs what `edits` reach. `None` when the saves changed nothing the
    /// project holds, such as a save of the same text.
    pub fn update(&mut self, edits: Vec<Edit>, time: &str) -> Option<Frame> {
        let event = self.watch.apply(edits)?;
        Some(self.frame(event, Some(time), None))
    }

    pub fn watch(&self) -> &Watch {
        &self.watch
    }

    /// The frame for one step; `time` is `None` for the first run, and
    /// `key` names the key that ran it.
    fn frame(&mut self, event: Event, time: Option<&str>, key: Option<&str>) -> Frame {
        if self.json {
            return Frame {
                erase: false,
                stdout: json_line(event, time.is_none(), self.full),
                stderr: String::new(),
            };
        }
        let mut screen = Screen::new();
        match (time, key) {
            (Some(time), Some(key)) => screen.push(key_rule(&event, time, key)),
            (Some(time), None) => {
                screen.push(rule(&event, time, self.watch.shown_root().as_deref()))
            }
            (None, _) => {}
        }
        match event.ran {
            Some(ran) => {
                for block in view(ran).0.into_blocks() {
                    screen.push(block);
                }
            }
            // Nothing reruns, but the errors the last look showed are gone.
            None if event.fixed => screen.push(Block::summary(
                jadpo_term::Outcome::Success,
                "errors fixed",
                [Fact::plain(
                    "no test reaches what changed since the last run",
                )],
            )),
            None => {}
        }
        let waiting = Screen::new().with(key_bar(event.files, event.failing, self.keys));
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
fn rule(event: &Event, time: &str, root: Option<&str>) -> Block {
    let reach = event.reached.map(|reached| match reached {
        0 if event.fixed => "errors fixed".to_string(),
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
                format!("{} {}", only.kind.verb(), shown_path(root, &only.path)),
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
            path: shown_path(root, &edit.path),
        })
        .collect();
    Block::Rule {
        long,
        short,
        changes,
        plain: "watch".to_string(),
    }
}

/// `── 14:02:31 · run all · 8 tests ──`: a run a key started, named by
/// the key's action and how many tests it ran.
fn key_rule(event: &Event, time: &str, key: &str) -> Block {
    let mut words = vec![time.to_string(), key.to_string()];
    if let Some(jadpo_test::Ran::Done(run)) = &event.ran {
        let n = run.results.len();
        words.push(format!("{n} {}", if n == 1 { "test" } else { "tests" }));
    }
    Block::Rule {
        long: words.clone(),
        short: words,
        changes: Vec::new(),
        plain: "watch".to_string(),
    }
}

/// `watching 41 files · 1 failing  enter rerun  a run all  f run failed
/// q quit`, or with no keys, `ctrl-c quit`.
fn key_bar(files: usize, failing: usize, keys: bool) -> Block {
    let mut facts = vec![Fact::plain(format!(
        "{files} {}",
        if files == 1 { "file" } else { "files" }
    ))];
    if failing > 0 {
        facts.push(Fact::problem(format!("{failing} failing")));
    }
    let pair = |key: &str, action: &str| (key.to_string(), action.to_string());
    let keys = if keys {
        let mut keys = vec![pair("enter", "rerun"), pair("a", "run all")];
        if failing > 0 {
            keys.push(pair("f", "run failed"));
        }
        keys.push(pair("q", "quit"));
        keys
    } else {
        vec![pair("ctrl-c", "quit")]
    };
    Block::KeyBar {
        label: "watching".to_string(),
        facts,
        keys,
    }
}

/// One step as one envelope on one line: `jadpo test --json`'s data with a
/// `watch` object saying what changed and how many tests it reached.
fn json_line(event: Event, first: bool, full: bool) -> String {
    // How long this step's tests took, when any ran.
    let took = match &event.ran {
        Some(jadpo_test::Ran::Done(run)) => Some(run.elapsed),
        _ => None,
    };
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
        ("fixed", event.fixed.into()),
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
    let line = if full {
        envelope(Some("test"), Some(data), &diagnostics, &sources)
    } else {
        short_envelope(Some("test"), Some(data), &diagnostics, &sources)
    };
    let line = match took {
        Some(took) => line.with_field_after(
            "ok",
            "durationMs",
            Json::Int(i64::try_from(took.as_millis()).unwrap_or(i64::MAX)),
        ),
        None => line,
    };
    line.to_line()
}

/// Writes a frame: the erase first, so results replace the waiting line.
/// An error writing results means nothing reads them any more (`| head`
/// has exited), and ends the session; the waiting line on stderr is only
/// for people, so its errors are ignored.
pub fn write(frame: &Frame, out: &mut dyn Write, err: &mut dyn Write) -> std::io::Result<()> {
    if frame.erase {
        let _ = err.write_all(ERASE.as_bytes());
        let _ = err.flush();
    }
    out.write_all(frame.stdout.as_bytes())?;
    out.flush()?;
    let _ = err.write_all(frame.stderr.as_bytes());
    let _ = err.flush();
    Ok(())
}

/// Runs a session on the process's own streams until it is stopped with
/// Ctrl-C or nothing reads its results any more; the exit code when it
/// ends on its own, which is 1 or 2 only for a mistake in what to run.
pub fn serve(args: &WatchArgs, env: &Environment) -> u8 {
    let offset = local_offset();
    // Keys are read only where the key bar offers them; the terminal is
    // given back as it was when the session ends.
    let reads = !args.json
        && env.stdin_terminal
        && Style::detect(env, false, args.choice, Stream::Stderr).mode == jadpo_term::Mode::Rich;
    let raw = if reads { Raw::enter() } else { None };
    let keys = raw.as_ref().map(|_| Keys::reading(std::io::stdin()));
    let code = serve_to(
        args,
        env,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
        &|| clock(offset),
        &|| false,
        keys.as_ref(),
    );
    drop(raw);
    code
}

/// [`serve`] on any streams, with any clock and keys, until `stop` says
/// so or a key quits.
pub fn serve_to(
    args: &WatchArgs,
    env: &Environment,
    out: &mut dyn Write,
    err: &mut dyn Write,
    clock: &dyn Fn() -> String,
    stop: &dyn Fn() -> bool,
    keys: Option<&Keys>,
) -> u8 {
    if write(&Watcher::header(args, env, &clock()), out, err).is_err() {
        return 0;
    }
    let (mut watcher, frame) = match Watcher::start(args, env, jadpo_db::Database::default()) {
        Ok(started) => started,
        Err((frame, code)) => {
            let _ = write(&frame, out, err);
            return code;
        }
    };
    if keys.is_none() {
        watcher.without_keys();
    }
    let mut poller = Poller::new(watcher.watch().root(), watcher.watch().is_project());
    if write(&frame, out, err).is_err() {
        return 0;
    }
    // A save while the first run was running happened before the poller's
    // first look: comparing every file's text with the database's catches
    // it, and prints nothing when nothing changed.
    let every = poller
        .paths()
        .map(|path| Edit::new(path, EditKind::Saved))
        .collect();
    if let Some(frame) = watcher.update(every, &clock())
        && write(&frame, out, err).is_err()
    {
        return 0;
    }
    let waiting = || stop() || keys.is_some_and(Keys::pending);
    loop {
        if let Some(edits) = poller.wait(&waiting) {
            if let Some(frame) = watcher.update(edits, &clock())
                && write(&frame, out, err).is_err()
            {
                return 0;
            }
            continue;
        }
        let Some(keys) = keys.filter(|_| !stop()) else {
            return 0;
        };
        // A save not yet looked at runs first, so the key's run sees it.
        let edits = poller.scan();
        if !edits.is_empty()
            && let Some(frame) = watcher.update(edits, &clock())
            && write(&frame, out, err).is_err()
        {
            return 0;
        }
        match watcher.press(keys.next(), &clock()) {
            Pressed::Quit => {
                // The live line ends, so the shell's prompt starts a line.
                if watcher.live {
                    let _ = err.write_all(b"\n");
                    let _ = err.flush();
                }
                return 0;
            }
            Pressed::Ran(frame) => {
                if write(&frame, out, err).is_err() {
                    return 0;
                }
            }
            Pressed::Nothing => {}
        }
    }
}

/// The time of day now, in the local zone, as `14:02:31`.
pub(crate) fn time_now() -> String {
    clock(local_offset())
}

/// The local time zone's offset from UTC in seconds, asked of `date` once:
/// the standard library has no time zones, and a rule's time is only read
/// by people. UTC when `date` can't say.
pub(crate) fn local_offset() -> i64 {
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
pub(crate) fn clock(offset: i64) -> String {
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
