//! Diagnostics on the grid: headline, location, code frame, then area 14's
//! fields in its order (`reason`, `next`, `impact`, `rule`, `help`), with
//! the names in use after `next` when a rename needs them (area 1).
//!
//! An agent's next step that carries edits shows them under `fix`, after
//! `next` (chunk 10.9a, designed with impeccable's shape pass against the
//! CLI design language, run unattended, so its assumptions are marked):
//!
//! - *Job.* An agent in a failing check reads each error once and applies
//!   its repair without opening a docs page or the file. Mode: operate.
//!   The repair is what `--json` already gives as `next.edits`; the text
//!   shows the same edit as the lines it changes.
//! - `fix` is a change list, so it uses `-` and `+` in their colours,
//!   indented under the label as `in use` is, never at column 0: each
//!   changed line's number, muted, its mark, then the line as written, the
//!   lines' common indent left out. The old lines come first, then the new.
//!   An edit to another file than the error's names that file first, muted
//!   as a path in a list is. A line no longer there has `-` alone; a line
//!   added has `+` alone.
//! - At most eight lines are shown (assumption: an automatic fix touches a
//!   line or two; a longer one is a move, which `jadpo fix` makes), then
//!   `… N more lines`; `--json` holds the whole edit.
//! - A step said again (same rule, the same next step of more than a
//!   line) reads ``As the first `rule` above``, as a repeated reason does,
//!   keeping its owner; its own `fix` stays, since its place differs.

use jadpo_diag::{Decider, Diagnostic, Label};

use crate::detect::{Mode, Style};
use crate::palette::{Symbol, Token};
use crate::screen::{
    Line, VALUE_COLUMN, decider, field, headline, link, paint, raw, strong, visible, width,
};
use crate::sources::{Position, Sources, line_text, shown_path};

/// One mark under a source line.
#[derive(Clone, Debug)]
struct Mark {
    /// 0-based character column.
    column: usize,
    length: usize,
    label: String,
    primary: bool,
}

#[derive(Clone, Debug)]
struct FrameLine {
    number: usize,
    text: String,
    marks: Vec<Mark>,
}

/// A diagnostic with its positions resolved against the sources.
#[derive(Clone, Debug)]
pub struct DiagnosticView {
    message: String,
    location: Option<String>,
    frame: Vec<FrameLine>,
    reason: Option<String>,
    next: Option<(String, Decider)>,
    in_use: Vec<(String, String)>,
    impact: Option<String>,
    rule: String,
    help: String,
    /// It says what an earlier diagnostic of its rule says, so it points
    /// back instead of giving the reason and the link again.
    repeat: bool,
    /// Its next step is an earlier one's, word for word, so it points back.
    next_repeat: bool,
    /// The lines its edits change (chunk 10.9a).
    fix: Vec<FixLine>,
}

/// One line of a `fix` field.
#[derive(Clone, Debug)]
enum FixLine {
    /// The file the following lines are in, when it isn't the error's.
    File(String),
    /// A line as it was (`-`) or as the edit leaves it (`+`).
    Line {
        number: usize,
        added: bool,
        text: String,
    },
    /// How many lines were left out.
    More(usize),
}

/// The most lines a `fix` field shows.
const FIX_LINES: usize = 8;

impl DiagnosticView {
    pub fn new(diagnostic: &Diagnostic, sources: &dyn Sources) -> DiagnosticView {
        DiagnosticView::from_root(diagnostic, sources, None)
    }

    /// [`DiagnosticView::new`], its location under `root`, the project as
    /// the command line spelt it (`shown_path`).
    pub fn from_root(
        diagnostic: &Diagnostic,
        sources: &dyn Sources,
        root: Option<&str>,
    ) -> DiagnosticView {
        let mut location = None;
        let mut frame: Vec<FrameLine> = Vec::new();
        if let Some(primary) = &diagnostic.primary {
            let path = &primary.span.path;
            // An empty span with no label stands for the whole file: the
            // location is the path alone, with no frame.
            let whole_file = primary.span.start == primary.span.end && primary.text.is_empty();
            match sources.source(path).filter(|_| !whole_file) {
                Some(text) => {
                    let start = Position::of(text, primary.span.start);
                    location = Some(format!(
                        "{}:{}:{}",
                        shown_path(root, path),
                        start.line,
                        start.column
                    ));
                    let labels = std::iter::once((primary, true)).chain(
                        diagnostic
                            .secondary
                            .iter()
                            .filter(|label| &label.span.path == path)
                            .map(|label| (label, false)),
                    );
                    for (label, is_primary) in labels {
                        add_mark(&mut frame, text, label, is_primary);
                    }
                    frame.sort_by_key(|line| line.number);
                }
                None => location = Some(shown_path(root, path)),
            }
        }
        DiagnosticView {
            message: diagnostic.message.clone(),
            location,
            frame,
            reason: diagnostic.reason.clone(),
            next: diagnostic
                .next
                .as_ref()
                .map(|next| (next.text.clone(), next.decides)),
            in_use: diagnostic.in_use.clone(),
            impact: diagnostic.impact.clone(),
            rule: diagnostic.rule.id.to_string(),
            help: diagnostic.help(),
            repeat: diagnostic.repeat_of.is_some(),
            next_repeat: diagnostic.next_repeat_of.is_some(),
            fix: fix_lines(diagnostic, sources, root),
        }
    }

    pub(crate) fn lines(&self, style: &Style) -> Vec<Line> {
        let rich = style.mode == Mode::Rich;
        let mut lines = vec![headline(&self.message, style)];
        if let Some(location) = &self.location {
            lines.push(vec![raw(if rich {
                format!("  {location}")
            } else {
                format!(" --> {location}")
            })]);
        }
        lines.extend(self.frame_lines(style));
        if let Some(reason) = &self.reason {
            lines.extend(field("reason", vec![raw(reason.as_str())], style));
        } else if self.repeat {
            let said = format!("As the first `{}` above", self.rule);
            lines.extend(field("reason", vec![raw(said.as_str())], style));
        }
        if let Some((text, decides)) = &self.next {
            let said = format!("As the first `{}` above", self.rule);
            let text = if self.next_repeat {
                said.as_str()
            } else {
                text.as_str()
            };
            lines.extend(field("next", vec![raw(text), decider(*decides)], style));
        }
        lines.extend(self.fix_lines(style));
        lines.extend(self.in_use_lines(style));
        if let Some(impact) = &self.impact {
            lines.extend(field("impact", vec![raw(impact.as_str())], style));
        }
        lines.extend(field("rule", vec![raw(self.rule.as_str())], style));
        if !self.repeat {
            lines.extend(field("help", vec![link(self.help.as_str())], style));
        }
        lines
    }

    /// The names in use, one per line in two columns, name and file: in
    /// plain output under `in use:`, indented as the frame's text is; in
    /// rich output in the value column, the files muted as paths in lists
    /// are.
    fn in_use_lines(&self, style: &Style) -> Vec<Line> {
        if self.in_use.is_empty() {
            return Vec::new();
        }
        let rich = style.mode == Mode::Rich;
        let widest = self
            .in_use
            .iter()
            .map(|(name, _)| width(name))
            .max()
            .unwrap_or(0);
        let row = |(name, path): &(String, String)| -> Line {
            let pad = " ".repeat(widest - width(name) + 2);
            if path.is_empty() {
                return vec![paint(Token::Muted, name.as_str())];
            }
            if rich {
                vec![
                    raw(format!("{name}{pad}")),
                    paint(Token::Muted, path.as_str()),
                ]
            } else {
                vec![raw(format!("{name}{pad}{path}"))]
            }
        };
        if !rich {
            let mut lines = vec![vec![raw("  in use:")]];
            for entry in &self.in_use {
                let mut line = vec![raw("    ")];
                line.extend(row(entry));
                lines.push(line);
            }
            return lines;
        }
        let mut lines = Vec::new();
        for (index, entry) in self.in_use.iter().enumerate() {
            let mut line = if index == 0 {
                vec![
                    raw("  "),
                    paint(Token::Muted, "in use"),
                    raw(" ".repeat(VALUE_COLUMN - 2 - width("in use"))),
                ]
            } else {
                vec![raw(" ".repeat(VALUE_COLUMN))]
            };
            line.extend(row(entry));
            lines.push(line);
        }
        lines
    }

    /// The `fix` field: under the label, each line indented as `in use`'s
    /// plain rows are, its number muted and its mark in its colour.
    fn fix_lines(&self, style: &Style) -> Vec<Line> {
        if self.fix.is_empty() {
            return Vec::new();
        }
        let rich = style.mode == Mode::Rich;
        let digits = self
            .fix
            .iter()
            .filter_map(|line| match line {
                FixLine::Line { number, .. } => Some(number.to_string().len()),
                _ => None,
            })
            .max()
            .unwrap_or(1);
        let mut lines = vec![if rich {
            vec![raw("  "), paint(Token::Muted, "fix")]
        } else {
            vec![raw("  fix:")]
        }];
        for line in &self.fix {
            lines.push(match line {
                FixLine::File(path) => vec![raw("    "), paint(Token::Muted, path.as_str())],
                FixLine::Line {
                    number,
                    added,
                    text,
                } => {
                    let (mark, token) = if *added {
                        ("+", Token::Success)
                    } else {
                        ("-", Token::Failure)
                    };
                    let mut line = vec![
                        raw("    "),
                        paint(Token::Muted, format!("{number:>digits$}")),
                        raw(" "),
                        paint(token, mark),
                    ];
                    if !text.is_empty() {
                        line.push(raw(format!(" {}", visible(text))));
                    }
                    line
                }
                FixLine::More(left) => vec![
                    raw("    "),
                    paint(
                        Token::Muted,
                        format!(
                            "{} {left} more {}",
                            Symbol::Cut.text(style.symbols),
                            if *left == 1 { "line" } else { "lines" }
                        ),
                    ),
                ],
            });
        }
        lines
    }

    fn frame_lines(&self, style: &Style) -> Vec<Line> {
        if self.frame.is_empty() {
            return Vec::new();
        }
        let rich = style.mode == Mode::Rich;
        let digits = self
            .frame
            .iter()
            .map(|line| line.number.to_string().len())
            .max()
            .unwrap_or(1);
        let token = Token::Failure;
        let bar = if rich {
            Symbol::Gutter.text(style.symbols)
        } else {
            "|"
        };
        // Rich frames sit at indent 2 with a muted gutter; plain frames are rustc's.
        let indent = if rich { "  " } else { "" };
        let gutter = |number: Option<usize>| -> Line {
            let number = number.map_or_else(String::new, |n| n.to_string());
            vec![
                raw(indent),
                paint(Token::Muted, format!("{number:>digits$} {bar}")),
            ]
        };
        let mut lines = vec![gutter(None)];
        for line in &self.frame {
            let mut source = gutter(Some(line.number));
            source.push(raw(format!(" {}", line.text)));
            lines.push(source);
            for mark in &line.marks {
                let mut under = gutter(None);
                under.push(raw(format!(" {}", " ".repeat(mark.column))));
                if mark.primary {
                    under.push(strong(token, "^".repeat(mark.length)));
                } else {
                    under.push(paint(Token::Muted, "-".repeat(mark.length)));
                }
                if !mark.label.is_empty() {
                    under.push(raw(format!(" {}", mark.label)));
                }
                lines.push(under);
            }
        }
        if rich {
            lines.push(gutter(None));
        }
        lines
    }
}

fn add_mark(frame: &mut Vec<FrameLine>, text: &str, label: &Label, primary: bool) {
    let start = Position::of(text, label.span.start);
    let end = Position::of(text, label.span.end);
    let source = line_text(text, start.line);
    // Columns as the line is shown, with hidden characters spelt out.
    let shown = |chars: usize| -> usize {
        width(&visible(&source.chars().take(chars).collect::<String>()))
    };
    let column = shown(start.column - 1);
    let length = if end.line == start.line {
        shown(end.column - 1).saturating_sub(column)
    } else {
        width(&visible(source)).saturating_sub(column)
    };
    let mark = Mark {
        column,
        length: length.max(1),
        label: label.text.clone(),
        primary,
    };
    match frame.iter_mut().find(|line| line.number == start.line) {
        Some(line) => line.marks.push(mark),
        None => frame.push(FrameLine {
            number: start.line,
            text: source.to_string(),
            marks: vec![mark],
        }),
    }
}

/// The lines an agent's next step's edits change, each file's edits
/// applied together: nothing for a person's step or one without edits.
fn fix_lines(diagnostic: &Diagnostic, sources: &dyn Sources, root: Option<&str>) -> Vec<FixLine> {
    let Some(next) = &diagnostic.next else {
        return Vec::new();
    };
    if next.edits.is_empty() || next.decides != Decider::Agent {
        return Vec::new();
    }
    let home = diagnostic
        .primary
        .as_ref()
        .map(|label| label.span.path.as_str());
    let mut paths: Vec<&str> = Vec::new();
    for edit in &next.edits {
        if !paths.contains(&edit.span.path.as_str()) {
            paths.push(edit.span.path.as_str());
        }
    }
    let mut out = Vec::new();
    for path in paths {
        let before = sources.source(path).unwrap_or("");
        let mut edits: Vec<&jadpo_diag::Edit> = next
            .edits
            .iter()
            .filter(|edit| edit.span.path == path)
            .collect();
        edits.sort_by_key(|edit| std::cmp::Reverse((edit.span.start, edit.span.end)));
        let mut after = before.to_string();
        for edit in edits {
            let (start, end) = (edit.span.start as usize, edit.span.end as usize);
            if start > end
                || end > after.len()
                || !after.is_char_boundary(start)
                || !after.is_char_boundary(end)
            {
                return Vec::new();
            }
            after.replace_range(start..end, &edit.replacement);
        }
        let old: Vec<&str> = before.lines().collect();
        let new: Vec<&str> = after.lines().collect();
        let mut top = 0;
        while top < old.len() && top < new.len() && old[top] == new[top] {
            top += 1;
        }
        let mut tail = 0;
        while tail < old.len() - top
            && tail < new.len() - top
            && old[old.len() - 1 - tail] == new[new.len() - 1 - tail]
        {
            tail += 1;
        }
        let removed = &old[top..old.len() - tail];
        let added = &new[top..new.len() - tail];
        if removed.is_empty() && added.is_empty() {
            continue;
        }
        if home != Some(path) {
            out.push(FixLine::File(crate::sources::shown_path(root, path)));
        }
        // The lines' common indent is left out, as it says nothing.
        let indent = removed
            .iter()
            .chain(added)
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);
        let cut = |line: &str| line.get(indent..).unwrap_or(line.trim_start()).to_string();
        for (index, line) in removed.iter().enumerate() {
            out.push(FixLine::Line {
                number: top + index + 1,
                added: false,
                text: cut(line),
            });
        }
        for (index, line) in added.iter().enumerate() {
            out.push(FixLine::Line {
                number: top + index + 1,
                added: true,
                text: cut(line),
            });
        }
    }
    let shown = out
        .iter()
        .filter(|line| matches!(line, FixLine::Line { .. }))
        .count();
    if shown > FIX_LINES {
        let mut kept = 0;
        out.retain(|line| match line {
            FixLine::Line { .. } => {
                kept += 1;
                kept <= FIX_LINES
            }
            _ => kept < FIX_LINES,
        });
        out.push(FixLine::More(shown - FIX_LINES));
    }
    out
}
