//! Tables and alert lines (CLI design language, "Lists and tables"): a
//! table sits at column 2 with muted lower-case headers, no borders, at
//! least two spaces between columns, numbers right-aligned and state as a
//! symbol plus a word. A row too wide for the screen has its cut column's
//! muted rest cut with `…`; when even that can't fit, every row becomes a
//! block of its own, its first cell bold and the rest as labelled fields,
//! so nothing is lost below 80 columns.

use crate::detect::Style;
use crate::palette::{Symbol, Token};
use crate::screen::{Line, bold, cut, field, paint, raw, rich, strong, symbols, width, wrap};

/// One cell: an optional state symbol in its own colour, its text in the
/// body colour, and a muted rest after two spaces, aligned down the
/// column (`✓ succeeded  12:00:00 · 1.2 s`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub symbol: Option<Symbol>,
    pub text: String,
    pub muted: String,
}

impl Cell {
    /// Body text.
    pub fn text(text: impl Into<String>) -> Cell {
        Cell {
            text: text.into(),
            ..Cell::default()
        }
    }

    /// Muted text: a time, a count, a value that is secondary.
    pub fn muted(text: impl Into<String>) -> Cell {
        Cell {
            muted: text.into(),
            ..Cell::default()
        }
    }

    /// A state: its symbol and word.
    pub fn state(symbol: Symbol, word: impl Into<String>) -> Cell {
        Cell {
            symbol: Some(symbol),
            text: word.into(),
            muted: String::new(),
        }
    }

    /// The cell with a muted rest after its text.
    pub fn then(mut self, muted: impl Into<String>) -> Cell {
        self.muted = muted.into();
        self
    }

    /// The symbol and text, as wide as they are.
    fn head(&self, style: &Style) -> String {
        match self.symbol {
            Some(symbol) if self.text.is_empty() => symbol.text(symbols(style)).to_string(),
            Some(symbol) => format!("{} {}", symbol.text(symbols(style)), self.text),
            None => self.text.clone(),
        }
    }
}

/// A table's columns and rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// A bold heading at column 0 above the header row: a group's name.
    pub heading: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
    /// The columns whose values are right-aligned: numbers.
    pub right: Vec<usize>,
    /// The column whose muted rest is cut first when a row is too wide.
    pub cut: Option<usize>,
}

impl Table {
    pub fn new<S: Into<String>>(headers: impl IntoIterator<Item = S>) -> Table {
        Table {
            headers: headers.into_iter().map(Into::into).collect(),
            ..Table::default()
        }
    }

    pub fn row(mut self, cells: Vec<Cell>) -> Table {
        self.rows.push(cells);
        self
    }

    pub fn right(mut self, column: usize) -> Table {
        self.right.push(column);
        self
    }

    pub fn cut(mut self, column: usize) -> Table {
        self.cut = Some(column);
        self
    }

    /// The width of each column's symbol and text, and of the whole cell.
    fn widths(&self, style: &Style) -> (Vec<usize>, Vec<usize>) {
        let columns = self.headers.len();
        let mut heads = vec![0; columns];
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate().take(columns) {
                heads[i] = heads[i].max(width(&cell.head(style)));
            }
        }
        let mut whole = vec![0; columns];
        for (i, header) in self.headers.iter().enumerate() {
            whole[i] = width(header);
        }
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate().take(columns) {
                whole[i] = whole[i].max(cell_width(cell, heads[i], style));
            }
        }
        (heads, whole)
    }

    pub(crate) fn lines(&self, style: &Style) -> Vec<Line> {
        let (heads, mut whole) = self.widths(style);
        let total =
            |whole: &[usize]| 2 + whole.iter().sum::<usize>() + 2 * whole.len().saturating_sub(1);
        if total(&whole) > style.width
            && let Some(column) = self.cut
        {
            let others = total(&whole) - whole[column];
            let room = style.width.saturating_sub(others);
            // The cut column keeps at least its header and its heads.
            let least = heads[column].max(width(&self.headers[column])) + 4;
            if room >= least {
                whole[column] = room;
            }
        }
        if total(&whole) > style.width {
            return self.blocks(style);
        }
        let mut lines = Vec::new();
        if let Some(heading) = &self.heading {
            lines.push(vec![bold(heading.as_str())]);
        }
        let mut header = vec![raw("  ")];
        for (i, name) in self.headers.iter().enumerate() {
            let text = self.align(i, name, whole[i]);
            header.push(paint(Token::Muted, text));
            if i + 1 < self.headers.len() {
                header.push(raw("  "));
            }
        }
        lines.push(header);
        for row in &self.rows {
            let mut line = vec![raw("  ")];
            for (i, cell) in row.iter().enumerate().take(self.headers.len()) {
                let last = i + 1 == self.headers.len();
                line.extend(self.cell(i, cell, heads[i], whole[i], last, style));
                if !last {
                    line.push(raw("  "));
                }
            }
            lines.push(line);
        }
        lines
    }

    fn align(&self, column: usize, text: &str, room: usize) -> String {
        if self.right.contains(&column) {
            format!("{text:>room$}")
        } else {
            format!("{text:<room$}")
        }
    }

    fn cell(
        &self,
        column: usize,
        cell: &Cell,
        head: usize,
        room: usize,
        last: bool,
        style: &Style,
    ) -> Vec<crate::screen::Piece> {
        let mut pieces = Vec::new();
        let right = self.right.contains(&column);
        let own = width(&cell.head(style));
        let bare = cell.text.is_empty() && cell.symbol.is_none();
        // The muted rest, cut to what is left of the column.
        let (pad, muted) = if cell.muted.is_empty() {
            (0, String::new())
        } else if bare {
            (0, cut(&cell.muted, room, style))
        } else {
            let pad = head - own + 2;
            (pad, cut(&cell.muted, room.saturating_sub(own + pad), style))
        };
        let used = own + pad + width(&muted);
        if right {
            pieces.push(raw(" ".repeat(room.saturating_sub(used))));
        }
        if let Some(symbol) = cell.symbol {
            pieces.push(paint(
                symbol.token().unwrap_or(Token::Muted),
                symbol.text(symbols(style)),
            ));
            if !cell.text.is_empty() {
                pieces.push(raw(" "));
            }
        }
        pieces.push(raw(cell.text.as_str()));
        if !muted.is_empty() {
            pieces.push(raw(" ".repeat(pad)));
            pieces.push(paint(Token::Muted, muted));
        }
        if !right && !last {
            pieces.push(raw(" ".repeat(room.saturating_sub(used))));
        }
        pieces
    }

    /// Each row as a block of its own: the first cell bold, then the rest
    /// as fields labelled by their headers.
    fn blocks(&self, style: &Style) -> Vec<Line> {
        let mut lines = Vec::new();
        if let Some(heading) = &self.heading {
            lines.push(vec![bold(heading.as_str())]);
            lines.push(Vec::new());
        }
        for (r, row) in self.rows.iter().enumerate() {
            if r > 0 {
                lines.push(Vec::new());
            }
            let mut cells = row.iter();
            if let Some(first) = cells.next() {
                lines.push(vec![bold(plain_cell(first, style))]);
            }
            for (cell, header) in cells.zip(self.headers.iter().skip(1)) {
                // A label is the header without what it says in brackets.
                let label = header.split(" (").next().unwrap_or(header);
                lines.extend(field(label, vec![raw(plain_cell(cell, style))], style));
            }
        }
        lines
    }
}

/// A cell's width: its head padded to the column's, two spaces and its
/// muted rest.
fn cell_width(cell: &Cell, head: usize, style: &Style) -> usize {
    let own = width(&cell.head(style));
    if cell.muted.is_empty() {
        own
    } else if cell.text.is_empty() && cell.symbol.is_none() {
        width(&cell.muted)
    } else {
        head + 2 + width(&cell.muted)
    }
}

fn plain_cell(cell: &Cell, style: &Style) -> String {
    let head = cell.head(style);
    match (head.is_empty(), cell.muted.is_empty()) {
        (_, true) => head,
        (true, false) => cell.muted.clone(),
        (false, false) => format!("{head}  {}", cell.muted),
    }
}

/// `▲ alert  <subject>`, a line each, wrapped at column 2.
pub(crate) fn alert_lines(subjects: &[String], style: &Style) -> Vec<Line> {
    let mut lines = Vec::new();
    for subject in subjects {
        if !rich(style) {
            lines.push(vec![raw(format!("alert: {subject}"))]);
            continue;
        }
        let word = format!("{} alert", Symbol::Warning.text(style.symbols));
        let lead = width(&word) + 2;
        let mut wrapped = wrap(subject, style.width.saturating_sub(lead)).into_iter();
        let first = wrapped.next().unwrap_or_default();
        lines.push(vec![strong(Token::Warning, word), raw("  "), bold(first)]);
        for rest in wrapped {
            lines.push(vec![raw("  "), bold(rest)]);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::Mode;
    use crate::screen::{Block, Screen};

    fn table() -> Table {
        Table::new(["schedule", "last run (UTC)", "next"])
            .cut(1)
            .row(vec![
                Cell::text("chore_census"),
                Cell::state(Symbol::Running, "running").then("11:50:10 · 13 min so far"),
                Cell::text("12:15"),
            ])
            .row(vec![
                Cell::text("nightly_invoices"),
                Cell::state(Symbol::Failure, "failed").then("02:00:04 · provider_unavailable"),
                Cell::text("2026-10-20 02:00"),
            ])
            .row(vec![
                Cell::text("tidy_drafts"),
                Cell::state(Symbol::Skipped, "none yet"),
                Cell::state(Symbol::Skipped, ""),
            ])
    }

    fn rich() -> Style {
        Style {
            bold: false,
            ..Style::new(Mode::Rich)
        }
    }

    #[test]
    fn a_table_aligns_its_columns_and_the_rest_of_each_cell() {
        let mut style = rich();
        style.width = 90;
        let text = Screen::new().with(Block::Table(table())).render(&style);
        assert_eq!(
            text,
            "  schedule          last run (UTC)                               next
  chore_census      ● running   11:50:10 · 13 min so far         12:15
  nightly_invoices  ✗ failed    02:00:04 · provider_unavailable  2026-10-20 02:00
  tidy_drafts       – none yet                                   –
"
        );
    }

    #[test]
    fn a_wide_row_cuts_the_cut_column_then_becomes_blocks() {
        let mut style = rich();
        let text = Screen::new().with(Block::Table(table())).render(&style);
        assert!(
            text.contains(
                "  nightly_invoices  ✗ failed    02:00:04 · provider_unavailab…  2026-10-20 02:00\n"
            ),
            "{text}"
        );
        assert!(
            text.lines().all(|line| line.chars().count() <= 80),
            "{text}"
        );
        style.width = 40;
        let text = Screen::new().with(Block::Table(table())).render(&style);
        assert!(
            text.starts_with("chore_census\n  last run  ● running 11:50:10 · 13 min\n            so far\n  next      12:15\n\nnightly_invoices\n"),
            "{text}"
        );
    }

    #[test]
    fn numbers_are_right_aligned() {
        let text = Screen::new()
            .with(Block::Table(
                Table::new(["due", "took"])
                    .right(1)
                    .row(vec![Cell::text("12:00"), Cell::muted("1.2 s")])
                    .row(vec![Cell::text("11:45"), Cell::muted("41 s")]),
            ))
            .render(&Style::new(Mode::Plain));
        assert_eq!(text, "  due     took\n  12:00  1.2 s\n  11:45   41 s\n");
    }

    #[test]
    fn alerts_read_the_same_in_plain_output() {
        let alerts = Block::Alerts(vec!["`census` has failed 3 times in a row".into()]);
        let screen = Screen::new().with(alerts);
        assert_eq!(
            screen.render(&rich()),
            "▲ alert  `census` has failed 3 times in a row\n"
        );
        assert_eq!(
            screen.render(&Style::new(Mode::Plain)),
            "alert: `census` has failed 3 times in a row\n"
        );
    }
}
