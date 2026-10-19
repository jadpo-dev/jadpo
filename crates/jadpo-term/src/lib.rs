//! The CLI design language (docs/language/research/14-cli-design-language.md):
//! how everything `jadpo` prints looks, in rich, plain and JSON output.
//!
//! [`Style::detect`] decides a stream's mode, colour, palette and symbols from
//! the environment; a [`Screen`] of [`Block`]s renders on the shared grid for
//! that style; [`envelope`] wraps a command's data and diagnostics for `--json`.

mod detect;
mod diagnostic;
mod json;
mod palette;
mod report;
mod screen;
mod sources;
mod spinner;
mod table;

pub use detect::{ColorChoice, Depth, Environment, MAX_WIDTH, Mode, Stream, Style, Symbols, Theme};
pub use diagnostic::DiagnosticView;
pub use json::{Json, SCHEMA_VERSION, diagnostic_json, envelope};
pub use palette::{Symbol, Token};
pub use report::{Diagram, DiagramLine, Field, Frame, Item, Mark, Report, Text};
pub use screen::{Block, Change, ChangeGroup, ChangeMark, Fact, Outcome, Row, RowState, Screen};
pub use sources::{Position, Sources, line_text, shown_path};
pub use spinner::{SPINNER_DELAY, Spinner};
pub use table::{Cell, Table};
