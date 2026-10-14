//! A failed test's report (CLI design language, "A failed test report";
//! area 13, "What a failed test reports"), on the grid every error shares:
//! the headline (`✗ failed  <test>`, plain `FAIL  <test>`) and location,
//! then groups of items with one blank line between groups. An item is the
//! assertion with each part's value hung beneath it, power-assert style; a
//! labelled value; or a one-line code frame. Rich values are cut at the
//! screen's width; plain output keeps them whole, for logs and searching.

use crate::detect::{Mode, Style};
use crate::palette::{Symbol, Token};
use crate::screen::{Line, Piece, bold, cut, paint, raw, strong, visible, width, wrap};

/// Where label values start, as for diagnostics.
const VALUE_COLUMN: usize = 12;

/// One failed test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// What failed: the test's name.
    pub subject: String,
    /// `file:line` of the failure in the test.
    pub location: Option<String>,
    /// Blocks of items, a blank line between each.
    pub groups: Vec<Vec<Item>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Diagram(Diagram),
    Field(Field),
    Frame(Frame),
}

/// Source lines with values hung beneath them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagram {
    pub lines: Vec<DiagramLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramLine {
    /// The line as written, without its indentation.
    pub source: String,
    pub marks: Vec<Mark>,
    /// Whether the statement goes on past this line, which is then shown
    /// ending in a muted `…`.
    pub continues: bool,
}

/// A value hung under the character at `column` (0-based, in characters
/// of `source`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub column: usize,
    pub value: String,
    pub tone: Option<Token>,
}

/// A label and its lines of value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub label: String,
    pub lines: Vec<Text>,
}

impl Field {
    pub fn new(label: impl Into<String>, lines: Vec<Text>) -> Field {
        Field {
            label: label.into(),
            lines,
        }
    }

    /// A label with one value in the terminal's own colour.
    pub fn value(label: impl Into<String>, value: impl Into<String>) -> Field {
        Field::new(label, vec![Text::Value(vec![(value.into(), None)])])
    }
}

/// One line of a field's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    /// A sentence: wrapped, never cut.
    Prose(String),
    /// Runs of text, each with its colour: cut to fit.
    Value(Vec<(String, Option<Token>)>),
    /// A command to run: never cut; on a line of its own when it doesn't
    /// fit beside its label.
    Command(String),
    /// Callables a call travelled through, outermost first, joined by
    /// muted arrows.
    Path(Vec<String>),
    /// Two columns, the first aligned across the field's lines: a frame's
    /// `file:line` and its callable.
    Pair(String, String),
}

impl Text {
    /// A value in one colour.
    pub fn value_in(token: Token, text: impl Into<String>) -> Text {
        Text::Value(vec![(text.into(), Some(token))])
    }
}

/// One line of source with carets under a span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub line: u32,
    pub source: String,
    /// 0-based character column where the carets start.
    pub column: usize,
    pub length: usize,
    pub label: String,
}

fn rich(style: &Style) -> bool {
    style.mode == Mode::Rich
}

/// Plain output has no symbols.
fn symbols(style: &Style) -> crate::detect::Symbols {
    if rich(style) {
        style.symbols
    } else {
        crate::detect::Symbols::Ascii
    }
}

impl Report {
    pub(crate) fn lines(&self, style: &Style) -> Vec<Line> {
        let mut lines = vec![if rich(style) {
            vec![
                strong(
                    Token::Failure,
                    format!("{} failed", Symbol::Failure.text(style.symbols)),
                ),
                raw("  "),
                bold(self.subject.as_str()),
            ]
        } else {
            vec![raw(format!("FAIL  {}", self.subject))]
        }];
        if let Some(location) = &self.location {
            lines.push(vec![raw(if rich(style) {
                format!("  {location}")
            } else {
                format!(" --> {location}")
            })]);
        }
        for group in &self.groups {
            lines.push(Vec::new());
            for item in group {
                match item {
                    Item::Diagram(diagram) => lines.extend(diagram_lines(diagram, style)),
                    Item::Field(field) => lines.extend(field_lines(field, style)),
                    Item::Frame(frame) => lines.extend(frame_lines(frame, style)),
                }
            }
        }
        lines
    }
}

fn bar(style: &Style) -> &'static str {
    if rich(style) {
        Symbol::Gutter.text(style.symbols)
    } else {
        "|"
    }
}

/// Each source line, then rows of its values: the rightmost value still
/// to place goes on each row, with every value to its left that ends
/// before the next column still waiting; a waiting column shows a bar.
fn diagram_lines(diagram: &Diagram, style: &Style) -> Vec<Line> {
    let mut lines = Vec::new();
    for line in &diagram.lines {
        let source = visible(&line.source);
        let mut shown = vec![raw(format!("  {source}"))];
        if line.continues {
            shown.push(paint(
                Token::Muted,
                format!(" {}", Symbol::Cut.text(symbols(style))),
            ));
        }
        lines.push(shown);
        // Columns as the line is shown, hidden characters spelt out.
        let mut pending: Vec<(usize, &Mark)> = line
            .marks
            .iter()
            .map(|mark| {
                let before: String = line.source.chars().take(mark.column).collect();
                (width(&visible(&before)), mark)
            })
            .collect();
        pending.sort_by_key(|(column, _)| *column);
        pending.dedup_by_key(|(column, _)| *column);
        if pending.is_empty() {
            continue;
        }
        lines.push(bars_line(&pending, style));
        while !pending.is_empty() {
            let mut placed: Vec<(usize, String, Option<Token>)> = Vec::new();
            for i in (0..pending.len()).rev() {
                let (column, mark) = pending[i];
                let limit = pending.get(i + 1).map(|(next, _)| *next);
                let value = visible(&mark.value);
                let shown = match limit {
                    None if rich(style) => {
                        cut(&value, style.width.saturating_sub(2 + column).max(1), style)
                    }
                    _ => value,
                };
                let fits = limit.is_none_or(|next| column + width(&shown) < next);
                if fits {
                    placed.push((column, shown, mark.tone));
                }
            }
            placed.reverse();
            lines.push(row_line(&pending, &placed, style));
            pending.retain(|(column, _)| !placed.iter().any(|(at, _, _)| at == column));
        }
    }
    lines
}

fn bars_line(pending: &[(usize, &Mark)], style: &Style) -> Line {
    let mut line = vec![raw("  ")];
    let mut at = 0;
    for (column, _) in pending {
        line.push(raw(" ".repeat(column - at)));
        line.push(paint(Token::Muted, bar(style)));
        at = column + 1;
    }
    line
}

fn row_line(
    pending: &[(usize, &Mark)],
    placed: &[(usize, String, Option<Token>)],
    style: &Style,
) -> Line {
    let mut line = vec![raw("  ")];
    let mut at = 0;
    for (column, _) in pending {
        if *column < at {
            continue;
        }
        line.push(raw(" ".repeat(column - at)));
        match placed.iter().find(|(placed_at, _, _)| placed_at == column) {
            Some((_, value, tone)) => {
                line.push(Piece::new(value.clone(), *tone, false));
                at = column + width(value);
            }
            None => {
                line.push(paint(Token::Muted, bar(style)));
                at = column + 1;
            }
        }
    }
    line
}

/// A label in columns 2–9 and its value from column 12; plain output
/// keeps `label: value`, continuation lines under the first value.
fn field_lines(field: &Field, style: &Style) -> Vec<Line> {
    let label = field.label.as_str();
    let (indent, first): (usize, Line) = if rich(style) {
        let pad = VALUE_COLUMN.saturating_sub(2 + width(label)).max(2);
        (
            2 + width(label) + pad,
            vec![raw("  "), paint(Token::Muted, label), raw(" ".repeat(pad))],
        )
    } else {
        let lead = format!("  {label}: ");
        (width(&lead), vec![raw(lead)])
    };
    let room = style.width.saturating_sub(indent);
    let pair_width = field
        .lines
        .iter()
        .filter_map(|text| match text {
            Text::Pair(left, _) => Some(width(left)),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let mut rows: Vec<Line> = Vec::new();
    for text in &field.lines {
        match text {
            Text::Prose(prose) => {
                if rich(style) {
                    rows.extend(wrap(prose, room).into_iter().map(|line| vec![raw(line)]));
                } else {
                    rows.push(vec![raw(prose.as_str())]);
                }
            }
            Text::Value(runs) => {
                let mut row = Vec::new();
                let mut used = 0;
                for (text, tone) in runs {
                    let text = if rich(style) {
                        cut(text, room.saturating_sub(used).max(1), style)
                    } else {
                        text.clone()
                    };
                    used += width(&text);
                    row.push(Piece::new(text, *tone, false));
                }
                rows.push(row);
            }
            Text::Command(command) => {
                let piece = if rich(style) {
                    paint(Token::Accent, command.as_str())
                } else {
                    raw(command.as_str())
                };
                if rich(style) && width(command) > room && rows.is_empty() {
                    // Commands are never cut: one that doesn't fit beside
                    // its label gets a line of its own, so it copies whole.
                    let mut lines = vec![vec![raw("  "), paint(Token::Muted, label)]];
                    lines.push(vec![raw("  "), piece]);
                    return lines;
                }
                rows.push(vec![piece]);
            }
            Text::Path(steps) => {
                let arrow = if rich(style) {
                    format!(" {} ", Symbol::Next.text(style.symbols))
                } else {
                    " -> ".to_string()
                };
                let mut row = Vec::new();
                for (i, step) in steps.iter().enumerate() {
                    if i > 0 {
                        row.push(paint(Token::Muted, arrow.as_str()));
                    }
                    row.push(raw(step.as_str()));
                }
                rows.push(row);
            }
            Text::Pair(left, right) => {
                rows.push(vec![
                    raw(format!("{left:<pair_width$}  ")),
                    raw(right.as_str()),
                ]);
            }
        }
    }
    let mut lines = Vec::new();
    for (i, row) in rows.into_iter().enumerate() {
        let mut line = if i == 0 {
            first.clone()
        } else {
            vec![raw(" ".repeat(indent))]
        };
        line.extend(row);
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(first);
    }
    lines
}

/// One line of source with carets, as a diagnostic's code frame: rich at
/// indent 2 with a muted gutter, plain as rustc prints it.
fn frame_lines(frame: &Frame, style: &Style) -> Vec<Line> {
    let number = frame.line.to_string();
    let digits = number.len();
    let indent = if rich(style) { "  " } else { "" };
    let bar = bar(style);
    let gutter = |number: &str| -> Line {
        vec![
            raw(indent),
            paint(Token::Muted, format!("{number:>digits$} {bar}")),
        ]
    };
    let source = visible(&frame.source);
    let before: String = frame.source.chars().take(frame.column).collect();
    let column = width(&visible(&before));
    let mut lines = vec![gutter("")];
    let mut code = gutter(&number);
    code.push(raw(format!(" {source}")));
    lines.push(code);
    let mut under = gutter("");
    under.push(raw(format!(" {}", " ".repeat(column))));
    under.push(strong(Token::Failure, "^".repeat(frame.length.max(1))));
    if !frame.label.is_empty() {
        under.push(raw(format!(" {}", frame.label)));
    }
    lines.push(under);
    if rich(style) {
        lines.push(gutter(""));
    }
    lines
}
