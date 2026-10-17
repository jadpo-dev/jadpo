//! The files `jadpo translations export` writes and `import` reads
//! (area 10, "Tooling"): XLIFF 2.0 and JSON, the two formats Crowdin and
//! Lokalise both exchange. This module is their syntax; what a unit is
//! and where it goes is `jadpo-check`'s, and the command's is
//! [`command`].

pub mod command;
mod json;
mod xliff;
pub mod xml;

/// The file format, by what a file starts with, not by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Xliff,
    Json,
}

impl Format {
    pub fn word(self) -> &'static str {
        match self {
            Format::Xliff => "xliff",
            Format::Json => "json",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Xliff => "xlf",
            Format::Json => "json",
        }
    }

    /// What a file says it is.
    pub fn detect(text: &str) -> Option<Format> {
        match text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .chars()
            .next()
        {
            Some('<') => Some(Format::Xliff),
            Some('{' | '[') => Some(Format::Json),
            _ => None,
        }
    }
}

/// One unit of wording going out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutUnit {
    /// The domain, `shared` or `jadpo`: the XLIFF file the unit is in.
    pub scope: String,
    /// The folder the scope's language files are in.
    pub folder: String,
    /// The id within the scope: `member` or `member.value`.
    pub id: String,
    /// Notes for the translator, by category: `description`, `context`,
    /// `placeholders` and `stale`.
    pub notes: Vec<(&'static str, String)>,
    pub source: String,
    pub target: Option<String>,
    /// A person accepted the target.
    pub reviewed: bool,
}

impl OutUnit {
    /// The id JSON files use and `import` resolves: the scope and the id.
    pub fn full_id(&self) -> String {
        format!("{}.{}", self.scope, self.id)
    }
}

/// A file going out: all the units for one target locale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutFile {
    /// BCP 47.
    pub source_language: String,
    pub target_language: Option<String>,
    pub units: Vec<OutUnit>,
}

pub fn write(format: Format, file: &OutFile) -> String {
    match format {
        Format::Xliff => xliff::write(file),
        Format::Json => json::write(file),
    }
}

/// One unit of wording coming in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InUnit {
    /// `scope.member` or `scope.member.value`.
    pub id: String,
    /// What it was translated from, when the file carries it.
    pub source: Option<String>,
    pub target: Option<String>,
    /// The bytes in the file that hold it, for an error to point at.
    pub start: usize,
    pub end: usize,
}

/// A file coming in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InFile {
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub units: Vec<InUnit>,
}

/// What is wrong with a file: where, and the words for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreadable {
    pub at: usize,
    pub why: String,
}

pub fn read(format: Format, text: &str) -> Result<InFile, Unreadable> {
    match format {
        Format::Xliff => xliff::read(text),
        Format::Json => json::read(text),
    }
}
