//! The document algebra and its printer (Wadler's "prettier printer", in
//! the form Prettier uses): text, line breaks that a group may keep flat,
//! indentation, and trailing comments held until the end of their line.

/// Spaces per indentation level.
pub const INDENT: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Doc {
    Text(String),
    /// A space when its group is flat, a line break otherwise.
    Line,
    /// Nothing when its group is flat, a line break otherwise.
    SoftLine,
    /// Always a line break; the groups around it break too.
    HardLine,
    Indent(Box<Doc>),
    Concat(Vec<Doc>),
    /// Flat if it fits on the line, broken otherwise.
    Group {
        doc: Box<Doc>,
        breaks: bool,
    },
    /// Printed at the end of the current line: a trailing comment.
    LineSuffix(String),
}

pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}

pub fn concat(docs: impl IntoIterator<Item = Doc>) -> Doc {
    let docs: Vec<Doc> = docs.into_iter().filter(|doc| !doc.is_empty()).collect();
    match docs.len() {
        0 => Doc::Concat(Vec::new()),
        1 => docs.into_iter().next().unwrap_or(Doc::Concat(Vec::new())),
        _ => Doc::Concat(docs),
    }
}

pub fn nil() -> Doc {
    Doc::Concat(Vec::new())
}

pub fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

pub fn group(doc: Doc) -> Doc {
    let breaks = doc.has_hard_line();
    Doc::Group {
        doc: Box::new(doc),
        breaks,
    }
}

impl Doc {
    pub fn is_empty(&self) -> bool {
        matches!(self, Doc::Concat(docs) if docs.is_empty())
            || matches!(self, Doc::Text(s) if s.is_empty())
    }

    fn has_hard_line(&self) -> bool {
        match self {
            Doc::HardLine => true,
            Doc::Indent(doc) => doc.has_hard_line(),
            Doc::Concat(docs) => docs.iter().any(Doc::has_hard_line),
            Doc::Group { breaks, .. } => *breaks,
            Doc::Text(_) | Doc::Line | Doc::SoftLine | Doc::LineSuffix(_) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

type Command<'a> = (usize, Mode, &'a Doc);

/// Prints `doc` in lines of at most `width` columns where it can.
pub fn print(doc: &Doc, width: usize) -> String {
    let mut out = String::new();
    let mut column = 0usize;
    let mut suffixes: Vec<&str> = Vec::new();
    let mut stack: Vec<Command<'_>> = vec![(0, Mode::Break, doc)];
    while let Some((level, mode, doc)) = stack.pop() {
        match doc {
            Doc::Text(s) => {
                // A trailing comment ends its line: what follows goes on
                // the next one, so the comment stays with its token and
                // formatting again gives the same text.
                if !suffixes.is_empty() && !s.is_empty() {
                    if s.trim().is_empty() {
                        continue;
                    }
                    for suffix in suffixes.drain(..) {
                        out.push_str(suffix);
                    }
                    trim_end(&mut out);
                    out.push('\n');
                    let pad = level * INDENT;
                    out.extend(std::iter::repeat_n(' ', pad));
                    column = pad;
                }
                out.push_str(s);
                column += s.chars().count();
            }
            Doc::Concat(docs) => {
                for doc in docs.iter().rev() {
                    stack.push((level, mode, doc));
                }
            }
            Doc::Indent(doc) => stack.push((level + 1, mode, doc)),
            Doc::Group { doc, breaks } => {
                let flat = (level, Mode::Flat, &**doc);
                let room = width as isize - column as isize;
                if mode == Mode::Flat || (!breaks && fits(flat, &stack, room)) {
                    stack.push(flat);
                } else {
                    stack.push((level, Mode::Break, doc));
                }
            }
            Doc::LineSuffix(s) => suffixes.push(s),
            Doc::Line | Doc::SoftLine | Doc::HardLine => {
                if mode == Mode::Flat && !matches!(doc, Doc::HardLine) {
                    if matches!(doc, Doc::Line) {
                        out.push(' ');
                        column += 1;
                    }
                    continue;
                }
                for suffix in suffixes.drain(..) {
                    out.push_str(suffix);
                }
                trim_end(&mut out);
                out.push('\n');
                let pad = level * INDENT;
                out.extend(std::iter::repeat_n(' ', pad));
                column = pad;
            }
        }
    }
    for suffix in suffixes {
        out.push_str(suffix);
    }
    trim_end(&mut out);
    out
}

fn trim_end(out: &mut String) {
    let trimmed = out.trim_end_matches(' ').len();
    out.truncate(trimmed);
}

/// Whether `next`, printed flat, and what follows it up to the next line
/// break fit in `room` columns. A trailing comment inside `next` fits only
/// at the end of its line; one after it ends the line there.
fn fits(next: Command<'_>, rest: &[Command<'_>], mut room: isize) -> bool {
    let mut stack = vec![next];
    let mut rest = rest.iter().rev();
    let mut suffix = false;
    let mut after = false;
    loop {
        let (level, mode, doc) = match stack.pop() {
            Some(command) => command,
            None => match rest.next() {
                Some(command) => {
                    after = true;
                    *command
                }
                None => return true,
            },
        };
        match doc {
            Doc::Text(s) => {
                if suffix && !s.is_empty() {
                    return false;
                }
                room -= s.chars().count() as isize;
                if room < 0 {
                    return false;
                }
            }
            Doc::Concat(docs) => {
                for doc in docs.iter().rev() {
                    stack.push((level, mode, doc));
                }
            }
            Doc::Indent(doc) => stack.push((level + 1, mode, doc)),
            Doc::Group { doc, breaks } => {
                let mode = if *breaks { Mode::Break } else { mode };
                stack.push((level, mode, doc));
            }
            Doc::LineSuffix(_) if after => return true,
            Doc::LineSuffix(_) => suffix = true,
            Doc::HardLine => return true,
            Doc::Line | Doc::SoftLine if mode == Mode::Break => return true,
            Doc::Line => room -= 1,
            Doc::SoftLine => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[&str]) -> Doc {
        let mut inner = vec![Doc::SoftLine];
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                inner.push(text(","));
                inner.push(Doc::Line);
            }
            inner.push(text(*item));
        }
        group(concat([
            text("f("),
            indent(concat(inner)),
            Doc::SoftLine,
            text(")"),
        ]))
    }

    #[test]
    fn a_group_stays_flat_when_it_fits() {
        assert_eq!(print(&list(&["a", "b"]), 20), "f(a, b)");
    }

    #[test]
    fn a_group_breaks_when_it_does_not() {
        assert_eq!(
            print(&list(&["alpha", "beta", "gamma"]), 12),
            "f(\n    alpha,\n    beta,\n    gamma\n)"
        );
    }

    #[test]
    fn text_after_a_trailing_comment_starts_a_new_line() {
        let doc = concat([text("{"), Doc::LineSuffix("  // TODO".into()), text("}")]);
        assert_eq!(print(&doc, 80), "{  // TODO\n}");
        let doc = concat([
            text("x +"),
            Doc::LineSuffix("  // the base".into()),
            text(" "),
            text("1"),
            Doc::LineSuffix("  // plus one".into()),
        ]);
        assert_eq!(print(&doc, 80), "x +  // the base\n1  // plus one");
    }

    #[test]
    fn a_trailing_comment_waits_for_the_end_of_its_line() {
        let doc = concat([
            text("a"),
            Doc::LineSuffix("  // note".into()),
            Doc::HardLine,
            text("b"),
        ]);
        assert_eq!(print(&doc, 80), "a  // note\nb");
    }
}
