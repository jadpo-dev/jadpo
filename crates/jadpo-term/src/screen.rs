//! The grid (CLI design language, "Layout"): a screen is a list of blocks
//! with one blank line between them. A status symbol sits in column 0 and its
//! text starts in column 2; labels sit in columns 2–9 and values start in
//! column 12; lists share one set of columns across the screen; lines fit
//! 80 columns, prose wraps, values are cut with `…`, and commands, paths and
//! links are never cut.

use jadpo_diag::{Decider, Diagnostic, Severity};

use crate::detect::{Mode, Style, Symbols};
use crate::diagnostic::DiagnosticView;
use crate::palette::{Symbol, Token};
use crate::sources::Sources;

/// Where label values start.
const VALUE_COLUMN: usize = 12;
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
}

pub(crate) fn raw(text: impl Into<String>) -> Piece {
    Piece {
        text: text.into(),
        token: None,
        bold: false,
    }
}

pub(crate) fn bold(text: impl Into<String>) -> Piece {
    Piece {
        text: text.into(),
        token: None,
        bold: true,
    }
}

pub(crate) fn paint(token: Token, text: impl Into<String>) -> Piece {
    Piece {
        text: text.into(),
        token: Some(token),
        bold: false,
    }
}

pub(crate) fn strong(token: Token, text: impl Into<String>) -> Piece {
    Piece {
        text: text.into(),
        token: Some(token),
        bold: true,
    }
}

pub(crate) type Line = Vec<Piece>;

/// Display width. Every symbol Jadpo prints is one cell wide.
pub(crate) fn width(text: &str) -> usize {
    text.chars().count()
}

fn render_line(line: &Line, style: &Style) -> String {
    let mut out = String::new();
    for piece in line {
        let mut codes = Vec::new();
        if piece.bold && style.bold {
            codes.push("1".to_string());
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

/// Splits prose into lines of at most `max` cells. A word longer than a
/// line (a command, a path, a link) gets a line of its own, whole.
pub(crate) fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
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
fn symbols(style: &Style) -> Symbols {
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
/// column (subcommands or values) that drops below 80 columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub description: String,
    pub extra: Vec<String>,
}

impl Row {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Row {
        Row {
            name: name.into(),
            description: description.into(),
            extra: Vec::new(),
        }
    }

    pub fn extra<S: Into<String>>(mut self, extra: impl IntoIterator<Item = S>) -> Row {
        self.extra = extra.into_iter().map(Into::into).collect();
        self
    }
}

#[derive(Clone, Debug)]
pub enum Block {
    /// Bold `jadpo <command>`, muted facts joined by `·`, then prose.
    Header {
        title: String,
        facts: Vec<String>,
        text: Vec<String>,
    },
    /// A bold heading and rows on the screen's shared list grid.
    List {
        heading: String,
        rows: Vec<Row>,
    },
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
    /// The one next command: `→ command  reason`.
    Hint {
        command: String,
        reason: String,
    },
}

impl Block {
    pub fn header<S: Into<String>>(
        title: impl Into<String>,
        facts: impl IntoIterator<Item = S>,
    ) -> Block {
        Block::Header {
            title: title.into(),
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
        let with_extra = rows()
            .filter(|row| !row.extra.is_empty())
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

fn rich(style: &Style) -> bool {
    style.mode == Mode::Rich
}

fn block_lines(block: &Block, grid: &Grid, style: &Style) -> Vec<Line> {
    match block {
        Block::Header { title, facts, text } => {
            let mut first = vec![bold(title.as_str())];
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

fn list_row(row: &Row, grid: &Grid, style: &Style) -> Line {
    let name = format!("  {:<width$}", row.name, width = grid.name);
    let room = style.width.saturating_sub(width(&name));
    let extra = row.extra.join(separator(style));
    match grid.description {
        Some(column)
            if !row.extra.is_empty() && width(&name) + column + width(&extra) <= style.width =>
        {
            let description = format!("{:<column$}", row.description);
            vec![raw(name), raw(description), paint(Token::Muted, extra)]
        }
        _ => vec![raw(name), raw(cut(&row.description, room, style))],
    }
}

/// A label in columns 2–9 and its value from column 12, wrapped as prose.
/// The value's pieces are words joined by single spaces. Plain output keeps
/// area 14's `label: value` form on one line, so it can be searched.
pub(crate) fn field(label: &str, value: Vec<Piece>, style: &Style) -> Vec<Line> {
    if !rich(style) {
        let text: Vec<&str> = value.iter().map(|piece| piece.text.as_str()).collect();
        return vec![vec![raw(format!("  {label}: {}", text.join(" ")))]];
    }
    let pad = " ".repeat(VALUE_COLUMN.saturating_sub(2 + width(label)).max(2));
    let indent = 2 + width(label) + width(&pad);
    let room = style.width.saturating_sub(indent);
    let mut lines: Vec<Line> = vec![vec![raw("  "), paint(Token::Muted, label), raw(pad)]];
    let mut used = 0;
    for piece in value {
        for word in piece.text.split(' ').filter(|word| !word.is_empty()) {
            let last = lines.last_mut().expect("a line");
            if used > 0 && used + 1 + width(word) > room {
                lines.push(vec![raw(" ".repeat(indent))]);
                used = 0;
            } else if used > 0 {
                last.push(raw(" "));
                used += 1;
            }
            used += width(word);
            let last = lines.last_mut().expect("a line");
            last.push(Piece {
                text: word.to_string(),
                token: piece.token,
                bold: piece.bold,
            });
        }
    }
    lines
}

/// The headline: symbol and severity word in its colour and bold, two
/// spaces, the subject in bold. Plain output reads `error: subject`.
pub(crate) fn headline(severity: Severity, subject: &str, style: &Style) -> Line {
    let (symbol, token) = match severity {
        Severity::Error => (Symbol::Failure, Token::Failure),
        Severity::Warning => (Symbol::Warning, Token::Warning),
    };
    if rich(style) {
        let word = format!("{} {}", symbol.text(style.symbols), severity.word());
        vec![strong(token, word), raw("  "), bold(subject)]
    } else {
        vec![raw(format!("{}: {subject}", severity.word()))]
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
    fn wrap_keeps_long_words_whole() {
        assert_eq!(wrap("a bb ccc", 4), ["a bb", "ccc"]);
        assert_eq!(
            wrap("short https://example.test/a/very/long/link end", 10),
            ["short", "https://example.test/a/very/long/link", "end"]
        );
        assert_eq!(wrap("", 10), [""]);
    }

    #[test]
    fn cut_fits_exactly() {
        let style = Style::new(Mode::Rich);
        assert_eq!(cut("abcdef", 4, &style), "abc…");
        assert_eq!(cut("abc", 4, &style), "abc");
        assert_eq!(cut("abcdef", 4, &Style::new(Mode::Plain)), "a...");
    }
}
