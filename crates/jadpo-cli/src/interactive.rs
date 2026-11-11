//! `jadpo test --interactive` (area 13, "Interactive mode"): a keyboard
//! view of every test in the app, grouped by domain and file with each
//! test's last result, to search, run a file, one test or everything,
//! rerun failures, read a failure's whole report and turn watching on and
//! off. It is built on the same [`Watch`] session as `jadpo test --watch`,
//! one long-lived database that rechecks only what changed, so a run from
//! the view costs what a save does.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, run unattended, so its assumptions are marked):
//!
//! - *Job.* A person at a terminal finds a test, runs it, reads why it
//!   failed, fixes the code and runs it again without retyping a path.
//!   Mode: operate. Agents never reach it: it refuses `--json` and a
//!   missing terminal, naming the flags that do the same job.
//! - *Screen.* The alternate screen, redrawn whole on every key with no
//!   animation, and given back on quit, where the last run's footer is
//!   printed so the scrollback keeps the outcome. From the top: the
//!   command header (`jadpo test --interactive · 12 tests · 4 files`, with
//!   `watching` and `3 match` when they apply); the list; the footer; the
//!   key bar.
//! - *List.* Domains as bold group headings (`tests` and `shared` for the
//!   folders outside `domains/`), each file under its domain by its path
//!   within it, at column 2, with what its tests did last on the right
//!   (`1 failed · 2 passed · 1 not run`, the failures in their colour);
//!   each test under its file at column 4 with its state as a symbol:
//!   `✓` passed, `✗` failed, `○` not run, `●` running. The selected row has
//!   `›` in column 0 and its name in accent bold; the list scrolls to keep
//!   it in sight. Each test's last result comes from the project's record
//!   of its last run (assumption: a test that passed is shown as passed
//!   even if its code changed since, as `--failed` trusts the record).
//! - *Footer.* The last run's footer exactly as `jadpo test` prints it;
//!   before any run, `○ 12 tests  1 failed last time`; while a run is under
//!   way, `● Running 3 tests`, the chosen rows marked `●`. Above it, one
//!   muted notice line when something needs saying: a save under watch
//!   (`14:02:31 · saved <file> · 3 tests reach it`), or why a key did
//!   nothing (`No test is failing`).
//! - *Keys.* The design language's one map: `enter` runs the selected file
//!   or test, `a` all, `f` the failing ones, `o` opens the selected test's
//!   report, `w` watches (`w stop` while watching), `/` searches, `esc`
//!   goes back, `q` and ctrl-c quit; arrows (and `j`, `k`) move. The key
//!   bar lists only the keys live now: `f` while a test is failing, `o`
//!   when the selected test has a report.
//! - *Search.* `/` turns the key bar into a prompt (`/ remind  enter run
//!   esc clear`); typing filters as it goes: a file whose path matches
//!   keeps all its tests, otherwise only the tests whose names match;
//!   `enter` runs the selection and keeps the filter, `esc` clears it.
//!   Nothing matching says so in the list.
//! - *Report.* `o` shows the failure's report exactly as `jadpo test`
//!   prints it, scrolled with the arrows; `enter` runs that test again,
//!   `esc` goes back. A build with errors shows its errors the same way,
//!   with `a` to run everything again once they are fixed.
//! - *Watching.* `w` turns on what `--watch` does: each save runs the tests
//!   it reaches and updates the list; `--interactive --watch` starts with
//!   it on.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{
    Block, ColorChoice, Environment, Fact, Mode, Outcome, Screen, Stream, Style, Symbol, Token,
};
use jadpo_test::watch::{Edit, Event, Pick, Poller, Watch};
use jadpo_test::{Options, Ran, Test};

use crate::keys::{Key, Keys, Raw};

/// What `jadpo test --interactive` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractiveArgs {
    pub options: Options,
    /// `--watch`: start with watching on.
    pub watch: bool,
    pub choice: ColorChoice,
}

/// Why the view can't open here: `--json`, or no terminal to draw in.
pub fn refusal(env: &Environment, json: bool, choice: ColorChoice) -> Option<Diagnostic> {
    if json {
        return Some(
            Diagnostic::new(
                &rule::CLI_TEST_ARGUMENTS,
                "`jadpo test --interactive` can't write JSON",
            )
            .reason("It is a keyboard view for a person at a terminal, so there is no document to write.")
            .next("Leave out `--interactive`; `jadpo test --json`, with a path, `--name`, `--kind` or `--failed`, does the same job"),
        );
    }
    let rich = Style::detect(env, false, choice, Stream::Stdout).mode == Mode::Rich;
    if !(env.stdin_terminal && env.stdout_terminal && rich) {
        return Some(needs_a_terminal(
            "It is a keyboard view redrawn on every key, so its input and output must be a terminal it can draw in, not a pipe or plain output.",
        ));
    }
    None
}

fn needs_a_terminal(reason: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::CLI_TEST_ARGUMENTS,
        "`jadpo test --interactive` needs a terminal",
    )
    .reason(reason)
    .next("Use `jadpo test` with a path, `--name`, `--kind` or `--failed`, or `jadpo test --watch`, which work anywhere")
}

/// A test's last result, as the list shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    NotRun,
    Passed,
    Failed,
    Running,
}

/// What the view shows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Page {
    List,
    /// A failed test's report, by key, or with none the errors that stopped
    /// the last run; scrolled down this many lines.
    Report {
        key: Option<String>,
        scroll: usize,
    },
}

/// One line of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    /// A domain, or a folder outside `domains/`.
    Group(String),
    /// A test file, by its project-relative path.
    File(String),
    /// A test, by its place in the view's tests.
    Test(usize),
}

/// What a key asks the session to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Draw again; nothing else.
    Redraw,
    /// Run these tests.
    Run(Pick),
    /// Watching was turned on (`true`) or off.
    Watch(bool),
    Quit,
}

/// The keyboard view's state, drawn by [`View::lines`].
#[derive(Debug)]
pub struct View {
    watch: Watch,
    style: Style,
    tests: Vec<Test>,
    states: BTreeMap<String, State>,
    /// What each test was before a run marked it running.
    before: BTreeMap<String, State>,
    reports: BTreeMap<String, Screen>,
    /// The errors that stopped the last run, as `jadpo test` shows them.
    errors: Option<Screen>,
    page: Page,
    query: String,
    typing: bool,
    /// The selected row, among the rows that can be selected.
    cursor: usize,
    /// The first list row on screen.
    top: usize,
    watching: bool,
    /// The last run's footer, when the session has run anything.
    footer: Option<Block>,
    /// How many tests are running, while a run is under way.
    running: Option<usize>,
    notice: Option<String>,
    /// Files changed since the test list was last read.
    stale: bool,
}

impl View {
    /// Loads the project and reads every test and its last result; the
    /// errors are shown at once when the project doesn't check.
    pub fn open(args: &InteractiveArgs, style: Style) -> Result<View, Ran> {
        let mut watch = Watch::open(args.options.clone(), jadpo_db::Database::default())?;
        watch.adopt_last_failures();
        let (failed, passed) = watch.last_results();
        let mut view = View {
            watch,
            style,
            tests: Vec::new(),
            states: BTreeMap::new(),
            before: BTreeMap::new(),
            reports: BTreeMap::new(),
            errors: None,
            page: Page::List,
            query: String::new(),
            typing: false,
            cursor: 0,
            top: 0,
            watching: args.watch,
            footer: None,
            running: None,
            notice: None,
            stale: false,
        };
        match view.watch.tests() {
            Ok(tests) => view.tests = tests,
            Err(ran) => view.stopped(ran),
        }
        for test in &view.tests {
            let state = if failed.contains(&test.key) {
                State::Failed
            } else if passed.contains(&test.key) {
                State::Passed
            } else {
                State::NotRun
            };
            view.states.insert(test.key.clone(), state);
        }
        Ok(view)
    }

    /// The session underneath, for the paths to watch.
    pub fn watch(&self) -> &Watch {
        &self.watch
    }

    pub fn watching(&self) -> bool {
        self.watching
    }

    /// What `key` does where the view is now.
    pub fn press(&mut self, key: Key) -> Effect {
        self.notice = None;
        if key == Key::Interrupt {
            return Effect::Quit;
        }
        if let Page::Report { key: shown, scroll } = &self.page {
            let shown = shown.clone();
            let scroll = *scroll;
            return match key {
                Key::Escape | Key::Char('o') => {
                    self.page = Page::List;
                    Effect::Redraw
                }
                Key::Up | Key::Char('k') => self.scroll(scroll.saturating_sub(1)),
                Key::Down | Key::Char('j') => self.scroll(scroll + 1),
                Key::PageUp => self.scroll(scroll.saturating_sub(10)),
                Key::PageDown => self.scroll(scroll + 10),
                Key::Enter => match shown {
                    Some(key) => Effect::Run(Pick::Only(BTreeSet::from([key]))),
                    None => Effect::Run(Pick::All),
                },
                Key::Char('a') => Effect::Run(Pick::All),
                Key::Char('q') => Effect::Quit,
                _ => Effect::Redraw,
            };
        }
        if self.typing {
            match key {
                Key::Escape => {
                    self.typing = false;
                    self.query.clear();
                    self.cursor = 0;
                }
                Key::Enter => {
                    self.typing = false;
                    return self.run_selected();
                }
                Key::Backspace => {
                    self.query.pop();
                    self.cursor = 0;
                }
                Key::Up => self.cursor = self.cursor.saturating_sub(1),
                Key::Down => self.cursor += 1,
                Key::Char(c) => {
                    self.query.push(c);
                    self.cursor = 0;
                }
                _ => {}
            }
            return Effect::Redraw;
        }
        match key {
            Key::Char('q') => return Effect::Quit,
            Key::Up | Key::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            Key::Down | Key::Char('j') => self.cursor += 1,
            Key::PageUp => self.cursor = self.cursor.saturating_sub(10),
            Key::PageDown => self.cursor += 10,
            Key::Enter => return self.run_selected(),
            Key::Char('a') => return Effect::Run(Pick::All),
            Key::Char('f') => {
                if self.watch.failing().is_empty() {
                    self.notice = Some("No test is failing".to_string());
                } else {
                    return Effect::Run(Pick::Failing);
                }
            }
            Key::Char('o') => self.open_report(),
            Key::Char('w') => {
                self.watching = !self.watching;
                return Effect::Watch(self.watching);
            }
            Key::Char('/') => self.typing = true,
            Key::Escape => {
                self.query.clear();
                self.cursor = 0;
            }
            _ => {}
        }
        // Moving past the last row stays on it.
        self.cursor = self.cursor.min(self.selectable().len().saturating_sub(1));
        Effect::Redraw
    }

    fn scroll(&mut self, to: usize) -> Effect {
        if let Page::Report { scroll, .. } = &mut self.page {
            *scroll = to;
        }
        Effect::Redraw
    }

    /// The selected row, among those that can be selected.
    fn selected(&self) -> Option<Row> {
        let selectable = self.selectable();
        let at = self.cursor.min(selectable.len().checked_sub(1)?);
        selectable.into_iter().nth(at)
    }

    fn selectable(&self) -> Vec<Row> {
        self.rows()
            .into_iter()
            .filter(|row| !matches!(row, Row::Group(_)))
            .collect()
    }

    /// `enter`: the selected file's tests that are shown, or the selected
    /// test.
    fn run_selected(&mut self) -> Effect {
        let keys: BTreeSet<String> = match self.selected() {
            Some(Row::Test(index)) => BTreeSet::from([self.tests[index].key.clone()]),
            Some(Row::File(file)) => self
                .rows()
                .into_iter()
                .filter_map(|row| match row {
                    Row::Test(index) if self.tests[index].file == file => {
                        Some(self.tests[index].key.clone())
                    }
                    _ => None,
                })
                .collect(),
            _ => return Effect::Redraw,
        };
        Effect::Run(Pick::Only(keys))
    }

    /// `o`: the selected test's report, when it has one.
    fn open_report(&mut self) {
        let Some(Row::Test(index)) = self.selected() else {
            self.notice = Some("Move to a failed test to open its report".to_string());
            return;
        };
        let key = self.tests[index].key.clone();
        if self.reports.contains_key(&key) {
            self.page = Page::Report {
                key: Some(key),
                scroll: 0,
            };
        } else if self.states.get(&key) == Some(&State::Failed) {
            self.notice = Some("It failed last time; run it with enter to see why".to_string());
        } else {
            self.notice = Some("Only a failed test has a report".to_string());
        }
    }

    /// Marks what `pick` runs as running, for the frame drawn while it
    /// runs.
    pub fn begin(&mut self, pick: &Pick) {
        let keys: BTreeSet<String> = match pick {
            Pick::All => self.tests.iter().map(|test| test.key.clone()).collect(),
            Pick::Failing => self.watch.failing().clone(),
            Pick::Only(keys) => keys.clone(),
            Pick::Reached | Pick::Again => BTreeSet::new(),
        };
        self.before.clear();
        for key in &keys {
            if let Some(state) = self.states.get_mut(key) {
                self.before.insert(key.clone(), *state);
                *state = State::Running;
            }
        }
        self.running = Some(keys.len());
    }

    /// Takes in `edits`, then runs what `pick` picks.
    pub fn run(&mut self, pick: Pick, edits: Vec<Edit>) {
        if !self.watch.sync(edits).is_empty() {
            self.stale = true;
        }
        let event = self.watch.run(pick);
        self.running = None;
        if let Some(event) = event {
            self.absorb(event);
        }
        self.settle();
    }

    /// Takes in a burst of saves while watching: the tests they reach run.
    pub fn saved(&mut self, edits: Vec<Edit>, time: &str) {
        let Some(event) = self.watch.apply(edits) else {
            return;
        };
        self.stale = true;
        let mut words = vec![time.to_string()];
        match event.edits.as_slice() {
            [only] => words.push(format!("{} {}", only.kind.verb(), only.path)),
            edits => words.push(format!("{} files changed", edits.len())),
        }
        match event.reached {
            Some(0) if event.fixed => words.push("errors fixed".to_string()),
            Some(0) => words.push("nothing to rerun".to_string()),
            Some(1) => words.push("1 test reaches it".to_string()),
            Some(n) => words.push(format!("{n} tests reach it")),
            None => {}
        }
        let separator = format!(" {} ", Symbol::Separator.text(self.style.symbols));
        let notice = words.join(&separator);
        if event.fixed && event.ran.is_none() {
            self.errors = None;
            if matches!(self.page, Page::Report { key: None, .. }) {
                self.page = Page::List;
            }
            self.relist();
        }
        self.absorb(event);
        self.settle();
        self.notice = Some(notice);
    }

    /// What a run did, into the list and the footer.
    fn absorb(&mut self, event: Event) {
        match event.ran {
            Some(Ran::Done(run)) => {
                for result in &run.results {
                    let key = &result.test.key;
                    if result.failure.is_some() {
                        self.states.insert(key.clone(), State::Failed);
                    } else {
                        self.states.insert(key.clone(), State::Passed);
                        self.reports.remove(key);
                    }
                    self.before.remove(key);
                }
                for (key, block) in jadpo_test::reports(&run) {
                    self.reports.insert(key, Screen::new().with(block));
                }
                self.footer = Some(jadpo_test::footer(&run));
                self.errors = None;
                // Errors fixed, or the test on the report passing now, is
                // back to the list.
                let gone = match &self.page {
                    Page::Report { key: None, .. } => true,
                    Page::Report { key: Some(key), .. } => !self.reports.contains_key(key),
                    Page::List => false,
                };
                if gone {
                    self.page = Page::List;
                }
                if self.stale {
                    self.relist();
                }
            }
            Some(ran) => self.stopped(ran),
            None => {}
        }
    }

    /// The tests chosen but not run go back to what they were.
    fn settle(&mut self) {
        for (key, state) in std::mem::take(&mut self.before) {
            if self.states.get(&key) == Some(&State::Running) {
                self.states.insert(key, state);
            }
        }
    }

    /// A run that ran nothing, and why, shown as `jadpo test` shows it.
    fn stopped(&mut self, ran: Ran) {
        let (screen, ..) = crate::test::view(ran);
        let blocks = screen.clone().into_blocks();
        self.footer = blocks.last().cloned();
        if blocks.len() > 1 {
            self.errors = Some(screen);
            self.page = Page::Report {
                key: None,
                scroll: 0,
            };
        }
    }

    /// Reads the test list again after files changed: tests added and
    /// removed.
    fn relist(&mut self) {
        let Ok(tests) = self.watch.tests() else {
            return;
        };
        self.stale = false;
        let keys: BTreeSet<&str> = tests.iter().map(|test| test.key.as_str()).collect();
        self.states.retain(|key, _| keys.contains(key.as_str()));
        self.reports.retain(|key, _| keys.contains(key.as_str()));
        for test in &tests {
            self.states.entry(test.key.clone()).or_insert(State::NotRun);
        }
        self.tests = tests;
    }

    /// The list's rows, filtered by the search.
    fn rows(&self) -> Vec<Row> {
        let query = self.query.to_lowercase();
        let mut rows = Vec::new();
        let mut group: Option<String> = None;
        let mut file: Option<&str> = None;
        for (index, test) in self.tests.iter().enumerate() {
            let shown = query.is_empty()
                || test.file.to_lowercase().contains(&query)
                || test.name.to_lowercase().contains(&query);
            if !shown {
                continue;
            }
            let (name, _) = place(&test.file);
            if group.as_deref() != Some(name.as_str()) {
                if !name.is_empty() {
                    rows.push(Row::Group(name.clone()));
                }
                group = Some(name);
            }
            if file != Some(test.file.as_str()) {
                rows.push(Row::File(test.file.clone()));
                file = Some(test.file.as_str());
            }
            rows.push(Row::Test(index));
        }
        rows
    }

    fn paint(&self, token: Token, text: &str) -> String {
        token.painted(text, &self.style)
    }

    fn bold(&self, text: &str) -> String {
        if self.style.bold {
            format!("\x1b[1m{text}\x1b[22m")
        } else {
            text.to_string()
        }
    }

    fn symbol(&self, symbol: Symbol) -> String {
        let text = symbol.text(self.style.symbols);
        match symbol.token() {
            Some(token) => self.paint(token, text),
            None => text.to_string(),
        }
    }

    /// One block on its own, as its lines.
    fn block_lines(&self, block: Block) -> Vec<String> {
        Screen::new()
            .with(block)
            .render(&self.style)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The frame for a terminal `height` rows by `width` columns, one
    /// string per row, with their colour.
    pub fn lines(&mut self, height: usize, width: usize) -> Vec<String> {
        let width = width.clamp(20, jadpo_term::MAX_WIDTH);
        self.style.width = width;
        let mut out = vec![self.header(), String::new()];
        let bottom = self.bottom();
        let room = height.saturating_sub(out.len() + 1 + bottom.len()).max(1);
        let body = match self.page.clone() {
            Page::List => self.list(room, width),
            Page::Report { key, scroll } => {
                let screen = match &key {
                    Some(key) => self.reports.get(key).cloned(),
                    None => self.errors.clone(),
                };
                let lines: Vec<String> = screen
                    .map(|screen| screen.render(&self.style))
                    .unwrap_or_default()
                    .lines()
                    .map(str::to_string)
                    .collect();
                let scroll = scroll.min(lines.len().saturating_sub(room));
                if let Page::Report { scroll: kept, .. } = &mut self.page {
                    *kept = scroll;
                }
                lines.into_iter().skip(scroll).take(room).collect()
            }
        };
        let padding = room.saturating_sub(body.len());
        out.extend(body);
        out.extend(std::iter::repeat_n(String::new(), padding));
        out.push(String::new());
        out.extend(bottom);
        out
    }

    /// `jadpo test --interactive · 12 tests · 4 files`.
    fn header(&self) -> String {
        let files: BTreeSet<&str> = self.tests.iter().map(|test| test.file.as_str()).collect();
        let count = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
        let mut facts = vec![count(self.tests.len(), "test"), count(files.len(), "file")];
        match &self.page {
            Page::Report { key: Some(key), .. } => {
                if let Some(test) = self.tests.iter().find(|test| &test.key == key) {
                    facts = vec![test.name.clone()];
                }
            }
            Page::Report { key: None, .. } => facts = vec!["tests not run".to_string()],
            Page::List => {}
        }
        if self.watching {
            facts.push("watching".to_string());
        }
        if !self.query.is_empty() && self.page == Page::List {
            let shown = self
                .rows()
                .iter()
                .filter(|row| matches!(row, Row::Test(_)))
                .count();
            facts.push(format!("{shown} match"));
        }
        self.block_lines(Block::header("jadpo test --interactive", facts))
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    /// The notice, the footer and the key bar.
    fn bottom(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(notice) = &self.notice {
            out.push(format!("  {}", self.paint(Token::Muted, notice)));
        }
        out.push(self.footer_line());
        out.push(self.key_bar());
        out
    }

    fn footer_line(&self) -> String {
        if let Some(n) = self.running {
            let what = format!("Running {n} {}", if n == 1 { "test" } else { "tests" });
            return self
                .block_lines(Block::summary(Outcome::Running, what, []))
                .join(" ");
        }
        if let Some(footer) = &self.footer {
            return self.block_lines(footer.clone()).join(" ");
        }
        let failed = self
            .states
            .values()
            .filter(|state| **state == State::Failed)
            .count();
        let ran = self.states.values().any(|state| *state != State::NotRun);
        let n = self.tests.len();
        let fact = if failed > 0 {
            self.paint(Token::Failure, &format!("{failed} failed last time"))
        } else if ran {
            self.paint(Token::Muted, "none failed last time")
        } else {
            self.paint(Token::Muted, "none run yet")
        };
        format!(
            "{} {}  {fact}",
            self.symbol(Symbol::Pending),
            self.bold(&format!("{n} {}", if n == 1 { "test" } else { "tests" }))
        )
    }

    fn key_bar(&self) -> String {
        let pair = |key: &str, action: &str| (key.to_string(), action.to_string());
        if self.typing {
            let key = |key: &str, action: &str| {
                format!("{}{}", self.bold(key), self.paint(Token::Muted, action))
            };
            return format!(
                "{} {}  {}  {}",
                self.paint(Token::Accent, "/"),
                self.bold(&self.query),
                key("enter", " run"),
                key("esc", " clear"),
            );
        }
        let keys = match &self.page {
            Page::Report { key: Some(_), .. } => {
                vec![
                    pair("enter", "run again"),
                    pair("esc", "back"),
                    pair("q", "quit"),
                ]
            }
            Page::Report { key: None, .. } => {
                vec![pair("a", "run all"), pair("esc", "back"), pair("q", "quit")]
            }
            Page::List => {
                let mut keys = vec![pair("/", "search")];
                match self.selected() {
                    Some(Row::File(_)) => keys.push(pair("enter", "run file")),
                    Some(Row::Test(_)) => keys.push(pair("enter", "run test")),
                    _ => {}
                }
                keys.push(pair("a", "run all"));
                if !self.watch.failing().is_empty() {
                    keys.push(pair("f", "run failed"));
                }
                if let Some(Row::Test(index)) = self.selected()
                    && self.reports.contains_key(&self.tests[index].key)
                {
                    keys.push(pair("o", "open report"));
                }
                keys.push(pair("w", if self.watching { "stop" } else { "watch" }));
                keys.push(pair("q", "quit"));
                keys
            }
        };
        self.block_lines(Block::KeyBar {
            label: String::new(),
            facts: Vec::<Fact>::new(),
            keys,
        })
        .join(" ")
    }

    /// The list's rows that fit in `room`, scrolled to keep the selected
    /// one in sight.
    fn list(&mut self, room: usize, width: usize) -> Vec<String> {
        let rows = self.rows();
        if rows.is_empty() {
            let text = if self.query.is_empty() {
                "No tests yet: a test sits in <name>.test.jadpo beside what it tests".to_string()
            } else {
                format!("Nothing matches \"{}\"", self.query)
            };
            return vec![format!("  {}", self.paint(Token::Muted, &text))];
        }
        let selectable = rows
            .iter()
            .filter(|row| !matches!(row, Row::Group(_)))
            .count();
        self.cursor = self.cursor.min(selectable.saturating_sub(1));
        let selected = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !matches!(row, Row::Group(_)))
            .nth(self.cursor)
            .map_or(0, |(at, _)| at);
        if selected < self.top {
            // The group heading above a file stays with it.
            self.top = match selected.checked_sub(1).map(|above| &rows[above]) {
                Some(Row::Group(_)) => selected - 1,
                _ => selected,
            };
        } else if selected >= self.top + room {
            self.top = selected + 1 - room;
        }
        self.top = self.top.min(rows.len().saturating_sub(room));
        rows.iter()
            .enumerate()
            .skip(self.top)
            .take(room)
            .map(|(at, row)| self.row(row, at == selected, width))
            .collect()
    }

    fn row(&self, row: &Row, selected: bool, width: usize) -> String {
        let mark = if selected {
            self.symbol(Symbol::Selected)
        } else {
            " ".to_string()
        };
        let name = |text: &str| {
            if selected {
                self.bold(&self.paint(Token::Accent, text))
            } else {
                text.to_string()
            }
        };
        match row {
            Row::Group(group) => format!("  {}", self.bold(group)),
            Row::File(file) => {
                let (_, within) = place(file);
                let summary = self.file_summary(file);
                let summary_width: usize = summary
                    .iter()
                    .map(|(text, _)| text.chars().count())
                    .sum::<usize>()
                    + 3 * summary.len().saturating_sub(1);
                let path = cut(within, width.saturating_sub(2));
                let used = 2 + path.chars().count();
                let mut line = format!("{mark} {}", name(&path));
                // What its tests did, right-aligned, when it fits.
                if !summary.is_empty() && used + 2 + summary_width <= width {
                    let separator = format!(
                        " {} ",
                        self.paint(Token::Muted, Symbol::Separator.text(self.style.symbols))
                    );
                    let parts: Vec<String> = summary
                        .iter()
                        .map(|(text, token)| self.paint(*token, text))
                        .collect();
                    line.push_str(&" ".repeat(width - used - summary_width));
                    line.push_str(&parts.join(&separator));
                }
                line
            }
            Row::Test(index) => {
                let test = &self.tests[*index];
                let state = self.states.get(&test.key).copied().unwrap_or(State::NotRun);
                let symbol = match state {
                    State::Passed => Symbol::Success,
                    State::Failed => Symbol::Failure,
                    State::NotRun => Symbol::Pending,
                    State::Running => Symbol::Running,
                };
                format!(
                    "{mark}   {} {}",
                    self.symbol(symbol),
                    name(&cut(&test.name, width.saturating_sub(6)))
                )
            }
        }
    }

    /// `1 failed · 2 passed · 1 not run` for a file's tests.
    fn file_summary(&self, file: &str) -> Vec<(String, Token)> {
        let mut counts = [0usize; 4];
        for test in self.tests.iter().filter(|test| test.file == file) {
            let at = match self.states.get(&test.key) {
                Some(State::Failed) => 0,
                Some(State::Passed) => 1,
                Some(State::Running) => 2,
                _ => 3,
            };
            counts[at] += 1;
        }
        [
            ("failed", Token::Failure),
            ("passed", Token::Muted),
            ("running", Token::Accent),
            ("not run", Token::Muted),
        ]
        .into_iter()
        .zip(counts)
        .filter(|(_, n)| *n > 0)
        .map(|((word, token), n)| (format!("{n} {word}"), token))
        .collect()
    }

    /// The last run's footer, for the scrollback once the view closes.
    pub fn closing(&self) -> Option<String> {
        self.footer
            .clone()
            .map(|footer| Screen::new().with(footer).render(&self.style))
    }
}

/// A test file's group and its path within it:
/// `domains/north/actions/a.test.jadpo` is `north`'s
/// `actions/a.test.jadpo`, `tests/checkout.test.jadpo` is `tests`'s
/// `checkout.test.jadpo`, and a loose file has no group.
fn place(file: &str) -> (String, &str) {
    if let Some(rest) = file.strip_prefix("domains/")
        && let Some((domain, within)) = rest.split_once('/')
    {
        return (domain.to_string(), within);
    }
    match file.split_once('/') {
        Some((folder, within)) => (folder.to_string(), within),
        None => (String::new(), file),
    }
}

/// `text` cut to `room` characters with `…`.
fn cut(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_string();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// Enters the alternate screen and hides the cursor.
const ENTER: &str = "\x1b[?1049h\x1b[?25l";
/// Shows the cursor and leaves the alternate screen.
const LEAVE: &str = "\x1b[?25h\x1b[?1049l";

/// Draws `lines` over the whole screen.
fn draw(out: &mut dyn Write, lines: &[String]) -> std::io::Result<()> {
    let mut frame = String::from("\x1b[H");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            frame.push_str("\r\n");
        }
        frame.push_str(line);
        frame.push_str("\x1b[K");
    }
    frame.push_str("\x1b[J");
    out.write_all(frame.as_bytes())?;
    out.flush()
}

/// Runs the view on the process's terminal until it is quit.
pub fn serve(args: &InteractiveArgs, env: &Environment) -> u8 {
    let style = Style::detect(env, false, args.choice, Stream::Stdout);
    let Some(raw) = Raw::enter() else {
        let refusal = needs_a_terminal("The terminal wouldn't read keys as they are pressed.");
        let shown = Screen::new().with(Block::diagnostic(&refusal, &()));
        let err = Style::detect(env, false, args.choice, Stream::Stderr);
        let _ = std::io::stderr().write_all(shown.render(&err).as_bytes());
        return 2;
    };
    let keys = Keys::reading(std::io::stdin());
    let offset = crate::watch::local_offset();
    let code = serve_to(
        args,
        style,
        &keys,
        &mut std::io::stdout(),
        &crate::keys::size,
        &|| crate::watch::clock(offset),
    );
    drop(raw);
    code
}

/// [`serve`] with any keys, output, terminal size and clock, until a key
/// quits.
pub fn serve_to(
    args: &InteractiveArgs,
    style: Style,
    keys: &Keys,
    out: &mut dyn Write,
    size: &dyn Fn() -> (usize, usize),
    clock: &dyn Fn() -> String,
) -> u8 {
    if out.write_all(ENTER.as_bytes()).is_err() {
        return 0;
    }
    let mut view = match View::open(args, style) {
        Ok(view) => view,
        Err(ran) => {
            let _ = out.write_all(LEAVE.as_bytes());
            let (screen, ..) = crate::test::view(ran);
            let _ = out.write_all(screen.render(&style).as_bytes());
            return 1;
        }
    };
    let mut poller = Poller::new(view.watch().root(), view.watch().is_project());
    let redraw = |view: &mut View, out: &mut dyn Write| {
        let (rows, columns) = size();
        draw(out, &view.lines(rows, columns))
    };
    loop {
        if redraw(&mut view, out).is_err() {
            break;
        }
        if view.watching()
            && let Some(edits) = poller.wait(&|| keys.pending())
        {
            view.saved(edits, &clock());
            continue;
        }
        match view.press(keys.next()) {
            Effect::Quit => break,
            Effect::Run(pick) => {
                view.begin(&pick);
                if redraw(&mut view, out).is_err() {
                    break;
                }
                view.run(pick, poller.scan());
            }
            Effect::Watch(true) => view.saved(poller.scan(), &clock()),
            Effect::Watch(false) | Effect::Redraw => {}
        }
    }
    let _ = out.write_all(LEAVE.as_bytes());
    if let Some(closing) = view.closing() {
        let _ = out.write_all(closing.as_bytes());
    }
    let _ = out.flush();
    0
}
