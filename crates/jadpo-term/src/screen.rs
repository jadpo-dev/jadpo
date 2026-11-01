//! The grid (CLI design language, "Layout"): a screen is a list of blocks
//! with one blank line between them. A status symbol sits in column 0 and its
//! text starts in column 2; labels sit in columns 2–9 and values start in
//! column 12; lists share one set of columns across the screen; lines fit
//! 80 columns, prose wraps, values are cut with `…`, and commands, paths and
//! links are never cut.

use jadpo_diag::{Decider, Diagnostic};

use crate::detect::{Mode, Style, Symbols};
use crate::diagnostic::DiagnosticView;
use crate::palette::{Symbol, Token};
use crate::report::Report;
use crate::sources::Sources;

/// Where label values start.
pub(crate) const VALUE_COLUMN: usize = 12;
/// The narrowest name column in a list, so descriptions start at column 16.
const MIN_NAME_WIDTH: usize = 12;
/// The narrowest description column when a list has a third column.
const MIN_DESCRIPTION_WIDTH: usize = 28;

/// A run of text with one look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    text: String,
    token: Option<Token>,
    bold: bool,
    /// A link's address, underlined where text has a look (CLI design
    /// language, "Type") and always printed in full.
    underline: bool,
}

impl Piece {
    pub(crate) fn new(text: impl Into<String>, token: Option<Token>, bold: bool) -> Piece {
        Piece {
            text: text.into(),
            token,
            bold,
            underline: false,
        }
    }
}

pub(crate) fn raw(text: impl Into<String>) -> Piece {
    Piece::new(text, None, false)
}

pub(crate) fn bold(text: impl Into<String>) -> Piece {
    Piece::new(text, None, true)
}

pub(crate) fn paint(token: Token, text: impl Into<String>) -> Piece {
    Piece::new(text, Some(token), false)
}

pub(crate) fn strong(token: Token, text: impl Into<String>) -> Piece {
    Piece::new(text, Some(token), true)
}

/// A link: its address, underlined in rich output.
pub(crate) fn link(address: impl Into<String>) -> Piece {
    Piece {
        underline: true,
        ..Piece::new(address, None, false)
    }
}

pub(crate) type Line = Vec<Piece>;

/// Text as it may reach a terminal: control characters (escape sequences
/// among them) and the characters that reorder text right to left are
/// written as `\u{...}`, so source text can't restyle the terminal or show
/// code in another order than the compiler reads it.
pub fn visible(text: &str) -> String {
    if !text.chars().any(hidden) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if hidden(c) {
            out.push_str(&format!("\\u{{{:x}}}", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn hidden(c: char) -> bool {
    (c.is_control() && c != '\t')
        || matches!(
            c,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

/// Display width. Every symbol Jadpo prints is one cell wide.
pub(crate) fn width(text: &str) -> usize {
    text.chars().count()
}

fn render_line(line: &Line, style: &Style) -> String {
    let mut out = String::new();
    for piece in line {
        let piece = &Piece {
            text: visible(&piece.text),
            ..piece.clone()
        };
        let mut codes = Vec::new();
        if piece.bold && style.bold {
            codes.push("1".to_string());
        }
        // Underline, like bold, is a look rich output keeps without colour.
        if piece.underline && style.bold {
            codes.push("4".to_string());
        }
        if let (Some(token), Some(depth)) = (piece.token, style.color) {
            codes.push(token.sgr(depth, style.theme));
        }
        if codes.is_empty() || piece.text.is_empty() {
            out.push_str(&piece.text);
        } else {
            out.push_str(&format!("\x1b[{}m{}\x1b[0m", codes.join(";"), piece.text));
        }
    }
    out.trim_end_matches(' ').to_string()
}

/// Cuts `text` to `max` cells, ending with the cut symbol.
pub(crate) fn cut(text: &str, max: usize, style: &Style) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    let mark = Symbol::Cut.text(symbols(style));
    let keep = max.saturating_sub(width(mark));
    let kept: String = text.chars().take(keep).collect();
    format!("{}{mark}", kept.trim_end())
}

/// The words of prose for wrapping: split at spaces, except inside inline
/// code (`` `jadpo check <path>` ``), which stays one word with whatever is
/// attached to it, so a command is never broken across lines.
pub(crate) fn words(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_code = false;
    for (i, c) in text.char_indices() {
        match c {
            '`' => in_code = !in_code,
            ' ' if !in_code => {
                if i > start {
                    out.push(&text[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Splits prose into lines of at most `max` cells. A word longer than a
/// line (a command, a path, a link, inline code) gets a line of its own,
/// whole.
pub(crate) fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in words(text) {
        if !current.is_empty() && width(&current) + 1 + width(word) > max {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Plain output has no symbols; rich output uses the detected set.
pub(crate) fn symbols(style: &Style) -> Symbols {
    if style.mode == Mode::Rich {
        style.symbols
    } else {
        Symbols::Ascii
    }
}

/// Facts joined by the separator: ` · ` in rich Unicode output, `, ` otherwise.
pub(crate) fn separator(style: &Style) -> &'static str {
    if style.mode == Mode::Rich && style.symbols == Symbols::Unicode {
        " · "
    } else {
        ", "
    }
}

/// One row of a list: a name, a description, and an optional muted third
/// column (subcommands or values) that drops below 80 columns. A row with
/// a state shows its symbol in column 0, before the name, as a status
/// line does; its description is the state's word, so plain output, which
/// has no symbols, says the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub description: String,
    pub extra: Vec<String>,
    pub state: Option<RowState>,
}

/// The state a row's symbol shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    /// `✓`, in the success colour.
    Done,
    /// `✗`, in the failure colour.
    Failed,
    /// `–`, muted: not looked at.
    Skipped,
}

impl Row {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Row {
        Row {
            name: name.into(),
            description: description.into(),
            extra: Vec::new(),
            state: None,
        }
    }

    /// The row with a state symbol before its name.
    pub fn state(mut self, state: RowState) -> Row {
        self.state = Some(state);
        self
    }

    pub fn extra<S: Into<String>>(mut self, extra: impl IntoIterator<Item = S>) -> Row {
        self.extra = extra.into_iter().map(Into::into).collect();
        self
    }
}

/// How a command ended, for its summary footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
    /// Neither yet: work still going on, which the footer says how far has
    /// got (`jadpo config rotation`'s rows still to move).
    Running,
}

/// One fact in a summary footer: a count, usually. A problem count takes
/// the failure colour; the rest are muted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub text: String,
    /// The colour of a count that needs attention.
    pub tone: Option<Token>,
}

impl Fact {
    pub fn plain(text: impl Into<String>) -> Fact {
        Fact {
            text: text.into(),
            tone: None,
        }
    }

    pub fn problem(text: impl Into<String>) -> Fact {
        Fact {
            text: text.into(),
            tone: Some(Token::Failure),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Block {
    /// Bold `jadpo <command>`, muted facts joined by `·`, then prose.
    Header {
        heading: String,
        facts: Vec<String>,
        text: Vec<String>,
    },
    /// A bold heading and rows on the screen's shared list grid.
    List {
        heading: String,
        rows: Vec<Row>,
    },
    /// Paragraphs of prose at column 0, each wrapped to the screen's
    /// width, with a blank line between them: a page read in the terminal
    /// (`jadpo help <rule>`).
    Prose(Vec<String>),
    /// A bold heading and lines at column 2, never cut.
    Section {
        heading: String,
        lines: Vec<String>,
    },
    /// Labels in columns 2–9, values from column 12.
    Fields(Vec<(String, String)>),
    /// A description, then the command whole on its own line.
    Examples(Vec<(String, String)>),
    Diagnostic(Box<DiagnosticView>),
    /// A failed test (area 13's report).
    Report(Box<Report>),
    /// The last line: the outcome in its colour, then facts joined by `·`
    /// (`✗ check failed  1 error · 11 files`). Plain output reads
    /// `headline: facts`, or `plain: headline, facts` when the headline is
    /// a count that needs a word before it (`test failed: 1 failed, …`).
    Summary {
        outcome: Outcome,
        headline: String,
        facts: Vec<Fact>,
        plain: Option<String>,
    },
    /// The one next command: `→ command  reason`.
    Hint {
        command: String,
        reason: String,
    },
    /// A muted rule between runs that names what happened
    /// (`── 14:02:31 · saved <file> · 6 tests reach it ──`): the `long`
    /// facts when they fit on one line, otherwise the `short` ones and the
    /// change list below, so a path is never cut. Plain output reads
    /// `plain: facts`.
    Rule {
        long: Vec<String>,
        short: Vec<String>,
        changes: Vec<Change>,
        plain: String,
    },
    /// A status line (`✓ Wrote <path>`), its symbol in the outcome's
    /// colour, then indented rows: a name and what it is.
    Status {
        outcome: Outcome,
        text: String,
        rows: Vec<(String, String)>,
    },
    /// An indented change list grouped under bold headings, each line a
    /// `+ ~ -` mark and columns (`+ priority   Priority   new field`),
    /// after a muted note saying what was compared.
    Changes {
        note: Option<String>,
        groups: Vec<ChangeGroup>,
    },
    /// Something for a person to decide: `◆ decision  <subject>` in the
    /// decision colour, where it is, and the next step with its owner.
    Decision {
        subject: String,
        location: Option<String>,
        next: String,
    },
    /// A live line saying what a session is waiting for and the keys that
    /// work: a muted label and facts, then each key in bold and its action
    /// muted (`watching 41 files  ctrl-c quit`). Plain output reads
    /// `label: facts, press key to action`.
    KeyBar {
        label: String,
        facts: Vec<Fact>,
        keys: Vec<(String, String)>,
    },
    /// A table at column 2: muted lower-case headers, columns two spaces
    /// apart, each cell a state symbol, its text and a muted rest
    /// (`● running  12:03:10 · 40 s so far`). Under the screen's width a
    /// cut column's muted rest is cut with `…`; narrower still, each row
    /// becomes a block of its own, the headers as labels.
    Table(crate::table::Table),
    /// `▲ alert  <subject>`, one line each, in the warning colour; plain
    /// output reads `alert: subject`.
    Alerts(Vec<String>),
    /// `✗ failed  <subject>` and its fields on the grid; plain output
    /// reads `failed: subject`.
    Failed {
        subject: String,
        fields: Vec<(String, String)>,
    },
    /// An answer (area 17's `jadpo why`): the sentence in bold, wrapped,
    /// then its fields on the grid, the place that decided it last.
    Answer {
        headline: String,
        fields: Vec<(String, String)>,
    },
}

/// A heading and its change lines, each a mark and its columns.
pub type ChangeGroup = (String, Vec<(ChangeMark, Vec<String>)>);

/// One line of an indented change list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub mark: ChangeMark,
    pub path: String,
}

/// Added, changed or removed: `+`, `~` or `-`, in the success, warning or
/// failure colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeMark {
    Added,
    Changed,
    Removed,
}

impl ChangeMark {
    fn text(self) -> &'static str {
        match self {
            ChangeMark::Added => "+",
            ChangeMark::Changed => "~",
            ChangeMark::Removed => "-",
        }
    }

    fn token(self) -> Token {
        match self {
            ChangeMark::Added => Token::Success,
            ChangeMark::Changed => Token::Warning,
            ChangeMark::Removed => Token::Failure,
        }
    }
}

/// How many changes a rule lists before counting the rest.
const MAX_CHANGES: usize = 5;

impl Block {
    pub fn header<S: Into<String>>(
        heading: impl Into<String>,
        facts: impl IntoIterator<Item = S>,
    ) -> Block {
        Block::Header {
            heading: heading.into(),
            facts: facts.into_iter().map(Into::into).collect(),
            text: Vec::new(),
        }
    }

    /// Adds a paragraph under a header. Other blocks are returned unchanged.
    pub fn with_text(mut self, paragraph: impl Into<String>) -> Block {
        if let Block::Header { text, .. } = &mut self {
            text.push(paragraph.into());
        }
        self
    }

    pub fn list(heading: impl Into<String>, rows: Vec<Row>) -> Block {
        Block::List {
            heading: heading.into(),
            rows,
        }
    }

    pub fn prose<S: Into<String>>(paragraphs: impl IntoIterator<Item = S>) -> Block {
        Block::Prose(paragraphs.into_iter().map(Into::into).collect())
    }

    pub fn section<S: Into<String>>(
        heading: impl Into<String>,
        lines: impl IntoIterator<Item = S>,
    ) -> Block {
        Block::Section {
            heading: heading.into(),
            lines: lines.into_iter().map(Into::into).collect(),
        }
    }

    pub fn fields<L: Into<String>, V: Into<String>>(rows: Vec<(L, V)>) -> Block {
        Block::Fields(
            rows.into_iter()
                .map(|(label, value)| (label.into(), value.into()))
                .collect(),
        )
    }

    pub fn examples<D: Into<String>, C: Into<String>>(examples: Vec<(D, C)>) -> Block {
        Block::Examples(
            examples
                .into_iter()
                .map(|(text, command)| (text.into(), command.into()))
                .collect(),
        )
    }

    pub fn diagnostic(diagnostic: &Diagnostic, sources: &dyn Sources) -> Block {
        Block::Diagnostic(Box::new(DiagnosticView::new(diagnostic, sources)))
    }

    /// A diagnostic whose location is shown under `root`, the project as
    /// the command line spelt it, when the command ran outside it.
    pub fn diagnostic_from(
        diagnostic: &Diagnostic,
        sources: &dyn Sources,
        root: Option<&str>,
    ) -> Block {
        Block::Diagnostic(Box::new(DiagnosticView::from_root(
            diagnostic, sources, root,
        )))
    }

    pub fn summary(
        outcome: Outcome,
        headline: impl Into<String>,
        facts: impl IntoIterator<Item = Fact>,
    ) -> Block {
        Block::Summary {
            outcome,
            headline: headline.into(),
            facts: facts.into_iter().collect(),
            plain: None,
        }
    }

    /// The words plain output puts before a summary's headline. Other
    /// blocks are returned unchanged.
    pub fn with_plain_headline(mut self, words: impl Into<String>) -> Block {
        if let Block::Summary { plain, .. } = &mut self {
            *plain = Some(words.into());
        }
        self
    }

    pub fn report(report: Report) -> Block {
        Block::Report(Box::new(report))
    }

    pub fn hint(command: impl Into<String>, reason: impl Into<String>) -> Block {
        Block::Hint {
            command: command.into(),
            reason: reason.into(),
        }
    }
}

/// Column positions shared by every list on a screen.
struct Grid {
    name: usize,
    description: Option<usize>,
}

impl Grid {
    fn of(blocks: &[Block], style: &Style) -> Grid {
        let rows = || {
            blocks.iter().flat_map(|block| match block {
                Block::List { rows, .. } => rows.as_slice(),
                _ => &[],
            })
        };
        let name = rows()
            .map(|row| width(&row.name))
            .max()
            .unwrap_or(0)
            .max(MIN_NAME_WIDTH)
            + 2;
        // Only rows whose third column can fit beside their own
        // description set its position, so one long description doesn't
        // push the column away from where other screens put it.
        let with_extra = rows()
            .filter(|row| {
                let extra = width(&row.extra.join(separator(style)));
                !row.extra.is_empty()
                    && 2 + name + width(&row.description).max(MIN_DESCRIPTION_WIDTH) + 2 + extra
                        <= style.width
            })
            .map(|row| width(&row.description))
            .max();
        let description = match with_extra {
            Some(longest) if style.width >= crate::detect::MAX_WIDTH => {
                Some(longest.max(MIN_DESCRIPTION_WIDTH) + 2)
            }
            _ => None,
        };
        Grid { name, description }
    }
}

/// A whole screen of output.
#[derive(Clone, Debug, Default)]
pub struct Screen {
    blocks: Vec<Block>,
}

impl Screen {
    pub fn new() -> Screen {
        Screen::default()
    }

    pub fn with(mut self, block: Block) -> Screen {
        self.blocks.push(block);
        self
    }

    pub fn push(&mut self, block: Block) {
        self.blocks.push(block);
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Its blocks, to put on another screen.
    pub fn into_blocks(self) -> Vec<Block> {
        self.blocks
    }

    /// The screen as text for `style`, ending in a newline. JSON mode never
    /// renders screens; it renders as plain text if asked.
    pub fn render(&self, style: &Style) -> String {
        let grid = Grid::of(&self.blocks, style);
        let mut out = String::new();
        for (i, block) in self.blocks.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            for line in block_lines(block, &grid, style) {
                out.push_str(&render_line(&line, style));
                out.push('\n');
            }
        }
        out
    }
}

pub(crate) fn rich(style: &Style) -> bool {
    style.mode == Mode::Rich
}

fn block_lines(block: &Block, grid: &Grid, style: &Style) -> Vec<Line> {
    match block {
        Block::Header {
            heading,
            facts,
            text,
        } => {
            let mut first = vec![bold(heading.as_str())];
            if !facts.is_empty() {
                let joined = facts
                    .iter()
                    .map(|fact| format!("{}{fact}", separator(style)))
                    .collect::<String>();
                first.push(paint(Token::Muted, joined));
            }
            let mut lines = vec![first];
            for paragraph in text {
                lines.extend(
                    wrap(paragraph, style.width)
                        .into_iter()
                        .map(|line| vec![raw(line)]),
                );
            }
            lines
        }
        Block::List { heading, rows } => {
            let mut lines = vec![vec![bold(heading.as_str())]];
            for row in rows {
                lines.push(list_row(row, grid, style));
            }
            lines
        }
        Block::Prose(paragraphs) => {
            let mut lines = Vec::new();
            for (i, paragraph) in paragraphs.iter().enumerate() {
                if i > 0 {
                    lines.push(Vec::new());
                }
                lines.extend(
                    wrap(paragraph, style.width)
                        .into_iter()
                        .map(|line| vec![raw(line)]),
                );
            }
            lines
        }
        Block::Section {
            heading,
            lines: body,
        } => {
            let mut lines = vec![vec![bold(heading.as_str())]];
            lines.extend(body.iter().map(|line| vec![raw(format!("  {line}"))]));
            lines
        }
        Block::Fields(rows) => {
            let mut lines = Vec::new();
            for (label, value) in rows {
                lines.extend(field(label, vec![raw(value.as_str())], style));
            }
            lines
        }
        Block::Examples(examples) => {
            let mut lines = vec![vec![bold("Examples")]];
            for (i, (text, command)) in examples.iter().enumerate() {
                if i > 0 {
                    lines.push(Vec::new());
                }
                lines.extend(
                    wrap(text, style.width - 2)
                        .into_iter()
                        .map(|line| vec![raw(format!("  {line}"))]),
                );
                lines.push(vec![raw(format!("    {command}"))]);
            }
            lines
        }
        Block::Diagnostic(view) => view.lines(style),
        Block::Report(report) => report.lines(style),
        Block::Summary {
            outcome,
            headline,
            facts,
            plain,
        } => vec![summary_line(
            *outcome,
            headline,
            facts,
            plain.as_deref(),
            style,
        )],
        Block::Rule {
            long,
            short,
            changes,
            plain,
        } => rule_lines(long, short, changes, plain, style),
        Block::KeyBar { label, facts, keys } => vec![key_bar(label, facts, keys, style)],
        Block::Table(table) => table.lines(style),
        Block::Alerts(subjects) => crate::table::alert_lines(subjects, style),
        Block::Failed { subject, fields } => {
            let mut lines = if rich(style) {
                let word = format!("{} failed", Symbol::Failure.text(style.symbols));
                vec![vec![
                    strong(Token::Failure, word),
                    raw("  "),
                    bold(subject.as_str()),
                ]]
            } else {
                vec![vec![raw(format!("failed: {subject}"))]]
            };
            for (label, value) in fields {
                lines.extend(field(label, vec![raw(value.as_str())], style));
            }
            lines
        }
        Block::Answer { headline, fields } => {
            let mut lines: Vec<Line> = wrap(headline, style.width)
                .into_iter()
                .map(|line| vec![bold(line)])
                .collect();
            for (label, value) in fields {
                lines.extend(field(label, vec![raw(value.as_str())], style));
            }
            lines
        }
        Block::Status {
            outcome,
            text,
            rows,
        } => {
            let mut lines = Vec::new();
            if rich(style) {
                let (symbol, token) = match outcome {
                    Outcome::Success => (Symbol::Success, Token::Success),
                    Outcome::Failure => (Symbol::Failure, Token::Failure),
                    Outcome::Running => (Symbol::Running, Token::Accent),
                };
                lines.push(vec![
                    paint(token, symbol.text(style.symbols)),
                    raw(format!(" {text}")),
                ]);
            } else {
                lines.push(vec![raw(text.as_str())]);
            }
            let name = rows.iter().map(|(n, _)| width(n)).max().unwrap_or(0) + 2;
            for (row, what) in rows {
                lines.push(vec![
                    raw(format!("    {row:<name$}")),
                    paint(Token::Muted, what.as_str()),
                ]);
            }
            lines
        }
        Block::Changes { note, groups } => {
            let mut lines = Vec::new();
            if let Some(note) = note {
                lines.push(vec![raw("  "), paint(Token::Muted, note.as_str())]);
                lines.push(Vec::new());
            }
            let columns = groups
                .iter()
                .flat_map(|(_, rows)| rows.iter().map(|(_, cells)| cells.len()))
                .max()
                .unwrap_or(0);
            let widths: Vec<usize> = (0..columns)
                .map(|i| {
                    groups
                        .iter()
                        .flat_map(|(_, rows)| rows.iter().filter_map(|(_, cells)| cells.get(i)))
                        .map(|cell| width(cell))
                        .max()
                        .unwrap_or(0)
                })
                .collect();
            for (i, (heading, rows)) in groups.iter().enumerate() {
                if i > 0 {
                    lines.push(Vec::new());
                }
                lines.push(vec![raw("  "), bold(heading.as_str())]);
                for (mark, cells) in rows {
                    let mut line = vec![raw("    "), paint(mark.token(), mark.text()), raw(" ")];
                    for (j, cell) in cells.iter().enumerate() {
                        let last = j + 1 == cells.len();
                        let text = if last {
                            cell.clone()
                        } else {
                            format!("{cell:<w$}  ", w = widths[j])
                        };
                        line.push(if last && j > 0 {
                            paint(Token::Muted, text)
                        } else {
                            raw(text)
                        });
                    }
                    lines.push(line);
                }
            }
            lines
        }
        Block::Decision {
            subject,
            location,
            next,
        } => {
            let mut lines = Vec::new();
            if rich(style) {
                let word = format!("{} decision", Symbol::Decision.text(style.symbols));
                lines.push(vec![
                    strong(Token::Decision, word),
                    raw("  "),
                    bold(subject.as_str()),
                ]);
                if let Some(location) = location {
                    lines.push(vec![raw(format!("  {location}"))]);
                }
            } else {
                lines.push(vec![raw(format!("decision: {subject}"))]);
                if let Some(location) = location {
                    lines.push(vec![raw(format!(" --> {location}"))]);
                }
            }
            lines.extend(field(
                "next",
                vec![raw(next.as_str()), decider(Decider::Human)],
                style,
            ));
            lines
        }
        Block::Hint { command, reason } => {
            if rich(style) {
                let arrow = Symbol::Next.text(style.symbols);
                vec![vec![
                    paint(Token::Accent, format!("{arrow} {command}")),
                    raw("  "),
                    paint(Token::Muted, reason.as_str()),
                ]]
            } else {
                vec![vec![raw(format!("next: {command}  {reason}"))]]
            }
        }
    }
}

fn rule_lines(
    long: &[String],
    short: &[String],
    changes: &[Change],
    plain: &str,
    style: &Style,
) -> Vec<Line> {
    let joined = |facts: &[String]| facts.join(separator(style));
    let line = |facts: &[String]| -> String {
        if rich(style) {
            let end = Symbol::Rule.text(style.symbols);
            format!("{end} {} {end}", joined(facts))
        } else {
            format!("{plain}: {}", joined(facts))
        }
    };
    let whole = line(long);
    if !long.is_empty() && width(&whole) <= style.width {
        return vec![vec![paint(Token::Muted, whole)]];
    }
    let mut lines = vec![vec![paint(Token::Muted, line(short))]];
    for change in changes.iter().take(MAX_CHANGES) {
        if rich(style) {
            lines.push(vec![
                raw("  "),
                paint(change.mark.token(), change.mark.text()),
                raw(" "),
                paint(Token::Muted, change.path.as_str()),
            ]);
        } else {
            lines.push(vec![raw(format!(
                "  {} {}",
                change.mark.text(),
                change.path
            ))]);
        }
    }
    if changes.len() > MAX_CHANGES {
        lines.push(vec![
            raw("  "),
            paint(
                Token::Muted,
                format!("and {} more", changes.len() - MAX_CHANGES),
            ),
        ]);
    }
    lines
}

fn key_bar(label: &str, facts: &[Fact], keys: &[(String, String)], style: &Style) -> Line {
    if !rich(style) {
        let mut parts: Vec<String> = facts.iter().map(|fact| fact.text.clone()).collect();
        parts.extend(
            keys.iter()
                .map(|(key, action)| format!("press {key} to {action}")),
        );
        return vec![raw(format!("{label}: {}", parts.join(", ")))];
    }
    let mut line = vec![paint(Token::Muted, label)];
    for (i, fact) in facts.iter().enumerate() {
        let lead = if i == 0 { " " } else { separator(style) };
        line.push(paint(Token::Muted, lead));
        line.push(paint(fact.tone.unwrap_or(Token::Muted), fact.text.as_str()));
    }
    for (key, action) in keys {
        line.push(raw("  "));
        line.push(bold(key.as_str()));
        line.push(paint(Token::Muted, format!(" {action}")));
    }
    line
}

fn summary_line(
    outcome: Outcome,
    headline: &str,
    facts: &[Fact],
    plain: Option<&str>,
    style: &Style,
) -> Line {
    if !rich(style) {
        let mut facts: Vec<&str> = facts.iter().map(|fact| fact.text.as_str()).collect();
        let headline = match plain {
            Some(words) => {
                facts.insert(0, headline);
                words
            }
            None => headline,
        };
        return if facts.is_empty() {
            vec![raw(headline)]
        } else {
            vec![raw(format!("{headline}: {}", facts.join(", ")))]
        };
    }
    let (symbol, token) = match outcome {
        Outcome::Success => (Symbol::Success, Token::Success),
        Outcome::Failure => (Symbol::Failure, Token::Failure),
        Outcome::Running => (Symbol::Running, Token::Accent),
    };
    let mut line = vec![strong(
        token,
        format!("{} {headline}", symbol.text(style.symbols)),
    )];
    for (i, fact) in facts.iter().enumerate() {
        let token = fact.tone.unwrap_or(Token::Muted);
        if i == 0 {
            line.push(raw("  "));
            line.push(paint(token, fact.text.as_str()));
        } else if fact.tone.is_some() {
            line.push(paint(Token::Muted, separator(style)));
            line.push(paint(token, fact.text.as_str()));
        } else {
            line.push(paint(
                Token::Muted,
                format!("{}{}", separator(style), fact.text),
            ));
        }
    }
    line
}

fn list_row(row: &Row, grid: &Grid, style: &Style) -> Line {
    let name = format!("  {:<width$}", row.name, width = grid.name);
    let room = style.width.saturating_sub(width(&name));
    let extra = row.extra.join(separator(style));
    let mut line = match (row.state, rich(style)) {
        (Some(state), true) => {
            let (symbol, token) = match state {
                RowState::Done => (Symbol::Success, Token::Success),
                RowState::Failed => (Symbol::Failure, Token::Failure),
                RowState::Skipped => (Symbol::Skipped, Token::Muted),
            };
            vec![
                paint(token, symbol.text(style.symbols)),
                raw(format!(" {:<width$}", row.name, width = grid.name)),
            ]
        }
        _ => vec![raw(name.clone())],
    };
    match grid.description {
        Some(column)
            if !row.extra.is_empty()
                && width(&row.description) + 2 <= column
                && width(&name) + column + width(&extra) <= style.width =>
        {
            let description = format!("{:<column$}", row.description);
            line.extend([raw(description), paint(Token::Muted, extra)]);
        }
        _ => line.push(raw(cut(&row.description, room, style))),
    }
    line
}

/// A label in columns 2–9 and its value from column 12, wrapped as prose.
/// The value's pieces are words joined by single spaces. Plain output keeps
/// area 14's `label: value` form on one line, so it can be searched. A
/// value's own line breaks, as in a tool's lines a reason quotes, start a
/// line under the value with the line's own indent kept, in both forms;
/// blank ones are left out, so the error stays one block.
pub(crate) fn field(label: &str, value: Vec<Piece>, style: &Style) -> Vec<Line> {
    if !rich(style) {
        let text: Vec<&str> = value.iter().map(|piece| piece.text.as_str()).collect();
        let text = text.join(" ");
        let mut quoted = broken(&text);
        let first = quoted.next().unwrap_or_default();
        let indent = " ".repeat(4 + width(label));
        let mut lines = vec![vec![raw(format!("  {label}: {first}"))]];
        lines.extend(quoted.map(|line| vec![raw(format!("{indent}{line}"))]));
        return lines;
    }
    let pad = " ".repeat(VALUE_COLUMN.saturating_sub(2 + width(label)).max(2));
    let indent = 2 + width(label) + width(&pad);
    let room = style.width.saturating_sub(indent);
    let mut lines: Vec<Line> = vec![vec![raw("  "), paint(Token::Muted, label), raw(pad)]];
    // Columns taken after the indent; whether the line has a word yet;
    // whether one of the value's own line breaks waits for the next word.
    let mut used = 0;
    let mut fresh = true;
    let mut broke = false;
    for piece in value {
        for (at, part) in piece.text.split('\n').enumerate() {
            let part = part.strip_suffix('\r').unwrap_or(part);
            broke |= at > 0;
            if part.trim().is_empty() {
                continue;
            }
            if broke {
                let kept = width(part) - width(part.trim_start());
                lines.push(vec![raw(" ".repeat(indent + kept))]);
                (used, fresh, broke) = (kept, true, false);
            }
            for word in words(part) {
                if !fresh && used + 1 + width(word) > room {
                    lines.push(vec![raw(" ".repeat(indent))]);
                    used = 0;
                } else if !fresh {
                    lines.last_mut().expect("a line").push(raw(" "));
                    used += 1;
                }
                fresh = false;
                used += width(word);
                lines.last_mut().expect("a line").push(Piece {
                    text: word.to_string(),
                    ..piece.clone()
                });
            }
        }
    }
    lines
}

/// `text`'s lines, without blank ones (each line break, `\r\n` among them,
/// ends one).
fn broken(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim().is_empty())
}

/// The headline: symbol and `error` in the failure colour and bold, two
/// spaces, the subject in bold. Plain output reads `error: subject`. Every
/// finding is an error (area 1, "Declining a judgement call"), so there is
/// no other word.
pub(crate) fn headline(subject: &str, style: &Style) -> Line {
    if rich(style) {
        let word = format!("{} error", Symbol::Failure.text(style.symbols));
        vec![strong(Token::Failure, word), raw("  "), bold(subject)]
    } else {
        vec![raw(format!("error: {subject}"))]
    }
}

/// `(agent)` muted, `(human)` in the decision colour.
pub(crate) fn decider(decides: Decider) -> Piece {
    let token = match decides {
        Decider::Agent => Token::Muted,
        Decider::Human => Token::Decision,
    };
    paint(token, format!("({})", decides.word()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_characters_are_spelt_out() {
        assert_eq!(
            visible("a\u{1b}[31mb\u{202e}c\td"),
            "a\\u{1b}[31mb\\u{202e}c\td"
        );
        assert_eq!(visible("plain é"), "plain é");
    }

    #[test]
    fn wrap_keeps_long_words_whole() {
        assert_eq!(wrap("a bb ccc", 4), ["a bb", "ccc"]);
        assert_eq!(
            wrap("short https://example.test/a/very/long/link end", 10),
            ["short", "https://example.test/a/very/long/link", "end"]
        );
        assert_eq!(wrap("", 10), [""]);
        assert_eq!(
            wrap("give a file: `jadpo check <path>` now", 20),
            ["give a file:", "`jadpo check <path>`", "now"]
        );
    }

    #[test]
    fn cut_fits_exactly() {
        let style = Style::new(Mode::Rich);
        assert_eq!(cut("abcdef", 4, &style), "abc…");
        assert_eq!(cut("abc", 4, &style), "abc");
        assert_eq!(cut("abcdef", 4, &Style::new(Mode::Plain)), "a...");
    }
}
