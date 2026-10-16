//! Diagnostics on the grid: headline, location, code frame, then area 14's
//! fields in its order (`reason`, `next`, `impact`, `rule`, `help`), with
//! the names in use after `next` when a rename needs them (area 1).

use jadpo_diag::{Decider, Diagnostic, Label, Severity};

use crate::detect::{Mode, Style};
use crate::palette::{Symbol, Token};
use crate::screen::{
    Line, VALUE_COLUMN, decider, field, headline, paint, raw, strong, visible, width,
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
    severity: Severity,
    message: String,
    location: Option<String>,
    frame: Vec<FrameLine>,
    reason: Option<String>,
    next: Option<(String, Decider)>,
    in_use: Vec<(String, String)>,
    impact: Option<String>,
    rule: String,
    help: String,
}

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
            severity: diagnostic.severity(),
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
        }
    }

    pub(crate) fn lines(&self, style: &Style) -> Vec<Line> {
        let rich = style.mode == Mode::Rich;
        let mut lines = vec![headline(self.severity, &self.message, style)];
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
        }
        if let Some((text, decides)) = &self.next {
            lines.extend(field(
                "next",
                vec![raw(text.as_str()), decider(*decides)],
                style,
            ));
        }
        lines.extend(self.in_use_lines(style));
        if let Some(impact) = &self.impact {
            lines.extend(field("impact", vec![raw(impact.as_str())], style));
        }
        lines.extend(field("rule", vec![raw(self.rule.as_str())], style));
        lines.extend(field("help", vec![raw(self.help.as_str())], style));
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
        let token = match self.severity {
            Severity::Error => Token::Failure,
            Severity::Warning => Token::Warning,
        };
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
