//! Translations (area 10, "Translations"): the messages in
//! `translations/<locale>.json`, written in ICU MessageFormat, read into
//! [`Message`] while compiling and carried to the runtime as a
//! [`TranslationPlan`], which `jadpo-value` formats in the caller's locale.
//!
//! A message is text with placeholders: `{name}` for text, `{count, number}`
//! for a number, `{when, date, short}` and `{when, time}` for a moment,
//! `{count, plural, one {# item} other {# items}}` for a count through the
//! locale's CLDR plural rules, `{rank, selectordinal, ...}` for the same
//! with ordinal rules, and `{kind, select, a {...} other {...}}` for a
//! choice of text. An apostrophe quotes a brace only where it comes right
//! before one (`'{'`), so "Don't" needs nothing, and `''` is an apostrophe.

use crate::format::FormatLength;

/// What a placeholder takes, which decides the type of the argument a call
/// of the message must give it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParamKind {
    /// `{name}` and the choice of a `select`: a value built on `Text`.
    Text,
    /// `{n, number}` and the count of a `plural`: a value built on `Int`
    /// or `Decimal`.
    Number,
    /// `{when, date}`: a `CalendarDate` or a `ZonedTime`.
    Date,
    /// `{when, time}`, alone or beside `date`: a `ZonedTime`.
    Time,
}

impl ParamKind {
    /// How the kind reads in an error: "text", "a number".
    pub fn word(self) -> &'static str {
        match self {
            ParamKind::Text => "text",
            ParamKind::Number => "a number",
            ParamKind::Date => "a date",
            ParamKind::Time => "a time",
        }
    }

    /// The kind a placeholder used as both is, if there is one: a moment
    /// shown as a date and as a time needs a time.
    pub fn merge(self, other: ParamKind) -> Option<ParamKind> {
        match (self, other) {
            (a, b) if a == b => Some(a),
            (ParamKind::Date, ParamKind::Time) | (ParamKind::Time, ParamKind::Date) => {
                Some(ParamKind::Time)
            }
            _ => None,
        }
    }
}

/// How `{n, number}` is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumberStyle {
    /// The locale's number format, with its digits, grouping and decimal
    /// separator.
    Decimal,
    /// Rounded to a whole number first.
    Integer,
}

/// One of CLDR's plural categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub const ALL: [PluralCategory; 6] = [
        PluralCategory::Zero,
        PluralCategory::One,
        PluralCategory::Two,
        PluralCategory::Few,
        PluralCategory::Many,
        PluralCategory::Other,
    ];

    pub fn word(self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }

    pub fn from_word(word: &str) -> Option<PluralCategory> {
        PluralCategory::ALL
            .into_iter()
            .find(|category| category.word() == word)
    }
}

/// What a plural's case matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Selector {
    /// `=0`: exactly this value, before any offset.
    Exact(i64),
    Category(PluralCategory),
}

/// A case of a plural.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluralCase {
    pub selector: Selector,
    pub message: Message,
}

/// A piece of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// `{name}`.
    Argument(String),
    /// `{name, number}` and `{name, number, integer}`.
    Number { name: String, style: NumberStyle },
    /// `{name, date, short}`.
    Date { name: String, length: FormatLength },
    /// `{name, time, short}`.
    Time { name: String, length: FormatLength },
    /// `{name, plural, ...}` and `{name, selectordinal, ...}`.
    Plural {
        name: String,
        ordinal: bool,
        /// Taken off the count before the category is chosen and before
        /// `#` shows it (`offset:1`).
        offset: i64,
        cases: Vec<PluralCase>,
        other: Message,
    },
    /// `{name, select, ...}`: the case whose word the text is, else `other`.
    Select {
        name: String,
        cases: Vec<(String, Message)>,
        other: Message,
    },
    /// `#` inside a plural: the count, formatted as a number.
    Count,
}

/// A message: its pieces in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub pieces: Vec<Piece>,
}

/// A placeholder of a message and what it takes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Placeholder {
    pub name: String,
    pub kind: ParamKind,
}

/// A placeholder used as two kinds that no argument can be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub name: String,
    pub first: ParamKind,
    pub second: ParamKind,
}

impl Message {
    /// Every placeholder, once, by name, with the one kind its uses add up
    /// to, or the first conflict between two of its uses.
    pub fn placeholders(&self) -> Result<Vec<Placeholder>, Conflict> {
        let mut found: Vec<Placeholder> = Vec::new();
        self.collect(&mut found)?;
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }

    fn collect(&self, found: &mut Vec<Placeholder>) -> Result<(), Conflict> {
        for piece in &self.pieces {
            match piece {
                Piece::Text(_) | Piece::Count => {}
                Piece::Argument(name) => add(found, name, ParamKind::Text)?,
                Piece::Number { name, .. } => add(found, name, ParamKind::Number)?,
                Piece::Date { name, .. } => add(found, name, ParamKind::Date)?,
                Piece::Time { name, .. } => add(found, name, ParamKind::Time)?,
                Piece::Plural {
                    name,
                    cases,
                    other,
                    ..
                } => {
                    add(found, name, ParamKind::Number)?;
                    for case in cases {
                        case.message.collect(found)?;
                    }
                    other.collect(found)?;
                }
                Piece::Select { name, cases, other } => {
                    add(found, name, ParamKind::Text)?;
                    for (_, message) in cases {
                        message.collect(found)?;
                    }
                    other.collect(found)?;
                }
            }
        }
        Ok(())
    }
}

fn add(found: &mut Vec<Placeholder>, name: &str, kind: ParamKind) -> Result<(), Conflict> {
    if let Some(seen) = found.iter_mut().find(|p| p.name == name) {
        return match seen.kind.merge(kind) {
            Some(merged) => {
                seen.kind = merged;
                Ok(())
            }
            None => Err(Conflict {
                name: name.to_string(),
                first: seen.kind,
                second: kind,
            }),
        };
    }
    found.push(Placeholder {
        name: name.to_string(),
        kind,
    });
    Ok(())
}

/// What is wrong with a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageErrorKind {
    /// Not a message: a brace left open, an unknown argument type, a
    /// placeholder with no name.
    Syntax,
    /// A `plural`, `selectordinal` or `select` with no `other` case.
    OtherMissing,
}

/// Where and what: a byte range of the message and the words for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageError {
    pub kind: MessageErrorKind,
    pub start: usize,
    pub end: usize,
    /// The short words under the carets.
    pub label: String,
    /// The sentence saying why.
    pub reason: String,
    /// The one change that fixes it.
    pub next: String,
}

/// Reads a message.
pub fn parse(source: &str) -> Result<Message, MessageError> {
    let mut parser = Parser {
        chars: source.char_indices().collect(),
        at: 0,
        length: source.len(),
    };
    parser.message(false, false, 0)
}

struct Parser {
    chars: Vec<(usize, char)>,
    at: usize,
    length: usize,
}

type Parsed<T> = Result<T, MessageError>;

fn syntax(start: usize, end: usize, label: &str, reason: &str, next: &str) -> MessageError {
    MessageError {
        kind: MessageErrorKind::Syntax,
        start,
        end: end.max(start),
        label: label.to_string(),
        reason: reason.to_string(),
        next: next.to_string(),
    }
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).map(|(_, c)| *c)
    }

    fn peek_at(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).map(|(_, c)| *c)
    }

    /// The byte offset of the next character, or the end.
    fn offset(&self) -> usize {
        self.chars.get(self.at).map_or(self.length, |(at, _)| *at)
    }

    /// The byte offset just past the previous character.
    fn offset_before(&self) -> usize {
        self.at
            .checked_sub(1)
            .and_then(|i| self.chars.get(i))
            .map_or(0, |(at, c)| at + c.len_utf8())
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.at += 1;
        }
        c
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
    }

    /// Pieces up to the `}` that closes a nested message (not consumed), or
    /// the end of the text.
    fn message(&mut self, nested: bool, in_plural: bool, opened: usize) -> Parsed<Message> {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut text = String::new();
        loop {
            let Some(c) = self.peek() else {
                if nested {
                    return Err(syntax(
                        opened,
                        opened + 1,
                        "never closed",
                        "Every `{` is closed by a `}`.",
                        "Add the `}` this `{` needs, or quote the brace as `'{'`",
                    ));
                }
                break;
            };
            match c {
                '\'' => self.quote(&mut text, in_plural),
                '{' => {
                    flush(&mut text, &mut pieces);
                    let piece = self.argument(in_plural)?;
                    pieces.push(piece);
                }
                '}' => {
                    if nested {
                        break;
                    }
                    let at = self.offset();
                    return Err(syntax(
                        at,
                        at + 1,
                        "no `{` before it",
                        "A `}` closes a `{`; this one closes nothing.",
                        "Remove the `}`, or quote it as `'}'`",
                    ));
                }
                '#' if in_plural => {
                    self.at += 1;
                    flush(&mut text, &mut pieces);
                    pieces.push(Piece::Count);
                }
                other => {
                    self.at += 1;
                    text.push(other);
                }
            }
        }
        flush(&mut text, &mut pieces);
        Ok(Message { pieces })
    }

    /// An apostrophe: `''` is one; one before a brace (or `#` in a plural)
    /// quotes up to the next single apostrophe; any other is itself.
    fn quote(&mut self, text: &mut String, in_plural: bool) {
        self.at += 1;
        match self.peek() {
            Some('\'') => {
                self.at += 1;
                text.push('\'');
            }
            Some('{' | '}') => self.quoted(text),
            Some('#') if in_plural => self.quoted(text),
            _ => text.push('\''),
        }
    }

    fn quoted(&mut self, text: &mut String) {
        while let Some(c) = self.bump() {
            if c == '\'' {
                if self.peek() == Some('\'') {
                    self.at += 1;
                    text.push('\'');
                    continue;
                }
                return;
            }
            text.push(c);
        }
    }

    /// `{name}`, `{name, type}` or `{name, type, style or cases}`; the
    /// `{` is next.
    fn argument(&mut self, in_plural: bool) -> Parsed<Piece> {
        let open = self.offset();
        self.at += 1;
        self.skip_space();
        let name = self.name()?;
        self.skip_space();
        match self.peek() {
            Some('}') => {
                self.at += 1;
                return Ok(Piece::Argument(name));
            }
            Some(',') => self.at += 1,
            _ => {
                return Err(syntax(
                    open,
                    open + 1,
                    "never closed",
                    "After its name a placeholder is closed by `}` or continues with `,` and a type.",
                    "Close the placeholder with `}`, or add a type such as `, number`",
                ));
            }
        }
        self.skip_space();
        let type_start = self.offset();
        let word = self.word();
        let type_end = self.offset_before();
        self.skip_space();
        let piece = match word.as_str() {
            "number" => {
                let style = self.style(open)?;
                let style = match style.as_deref() {
                    None => NumberStyle::Decimal,
                    Some("integer") => NumberStyle::Integer,
                    Some(other) => {
                        let end = self.offset_before();
                        return Err(syntax(
                            type_start,
                            end,
                            "style not supported",
                            &format!(
                                "`number` takes no style, or `integer`; `{other}` is not one of them."
                            ),
                            "Write `{name, number}` or `{name, number, integer}`",
                        ));
                    }
                };
                Piece::Number { name, style }
            }
            kind @ ("date" | "time") => {
                let style = self.style(open)?;
                let length = match style.as_deref() {
                    None | Some("medium") => FormatLength::Medium,
                    Some("short") => FormatLength::Short,
                    Some("long") => FormatLength::Long,
                    Some("full") => FormatLength::Full,
                    Some(other) => {
                        let end = self.offset_before();
                        return Err(syntax(
                            type_start,
                            end,
                            "style not supported",
                            &format!(
                                "`{kind}` takes `short`, `medium`, `long` or `full`, or no style; `{other}` is not one of them."
                            ),
                            &format!("Use `short`, `medium`, `long` or `full` after `{kind},`"),
                        ));
                    }
                };
                if kind == "date" {
                    Piece::Date { name, length }
                } else {
                    Piece::Time { name, length }
                }
            }
            kind @ ("plural" | "selectordinal") => {
                self.expect_comma(open, kind)?;
                self.plural(open, name, kind == "selectordinal")?
            }
            "select" => {
                self.expect_comma(open, "select")?;
                self.select(open, name, in_plural)?
            }
            "" => {
                return Err(syntax(
                    open,
                    open + 1,
                    "no type",
                    "After the comma a placeholder names its type.",
                    "Write the type, one of `number`, `date`, `time`, `plural`, `selectordinal` or `select`",
                ));
            }
            other => {
                return Err(syntax(
                    type_start,
                    type_end,
                    "no such type",
                    &format!(
                        "`{other}` is not an argument type; the types are `number`, `date`, `time`, `plural`, `selectordinal` and `select`."
                    ),
                    "Use one of `number`, `date`, `time`, `plural`, `selectordinal` or `select`",
                ));
            }
        };
        Ok(piece)
    }

    /// A placeholder's name: lower_snake_case, as an argument's is.
    fn name(&mut self) -> Parsed<String> {
        let start = self.offset();
        let word = self.word();
        let end = self.offset_before();
        let Some(first) = word.chars().next() else {
            return Err(syntax(
                start,
                start + 1,
                "no name",
                "A placeholder is a name in braces, `{order_number}`: the call gives each one an argument of the same name.",
                "Write the placeholder's name between the braces",
            ));
        };
        let snake = first.is_ascii_lowercase()
            && word
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if snake {
            return Ok(word);
        }
        let reason = if first.is_ascii_digit() {
            format!(
                "`{word}` is a number, but placeholders are named: the call gives each one an argument of the same name, as `order_number: ...`."
            )
        } else {
            format!(
                "`{word}` is not lower_snake_case, and the call gives each placeholder an argument of the same name."
            )
        };
        Err(syntax(
            start,
            end,
            "not a name",
            &reason,
            "Name the placeholder in lower_snake_case, such as `{order_number}`",
        ))
    }

    /// Letters, digits and `_`.
    fn word(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                out.push(c);
                self.at += 1;
            } else {
                break;
            }
        }
        out
    }

    /// After a type: `}`, or `,` a style and `}`.
    fn style(&mut self, open: usize) -> Parsed<Option<String>> {
        match self.peek() {
            Some('}') => {
                self.at += 1;
                Ok(None)
            }
            Some(',') => {
                self.at += 1;
                self.skip_space();
                let start = self.offset();
                let mut style = String::new();
                while let Some(c) = self.peek() {
                    if c == '}' || c == '{' {
                        break;
                    }
                    style.push(c);
                    self.at += 1;
                }
                if self.peek() != Some('}') {
                    return Err(syntax(
                        open,
                        open + 1,
                        "never closed",
                        "Every `{` is closed by a `}`.",
                        "Close the placeholder with `}`",
                    ));
                }
                self.at += 1;
                let style = style.trim().to_string();
                if style.is_empty() {
                    return Err(syntax(
                        start,
                        start + 1,
                        "no style",
                        "A comma after the type is followed by a style.",
                        "Write a style after the comma, or remove the comma",
                    ));
                }
                Ok(Some(style))
            }
            _ => Err(syntax(
                open,
                open + 1,
                "never closed",
                "After its type a placeholder is closed by `}`, or continues with `,` and a style.",
                "Close the placeholder with `}`",
            )),
        }
    }

    fn expect_comma(&mut self, open: usize, kind: &str) -> Parsed<()> {
        if self.peek() == Some(',') {
            self.at += 1;
            self.skip_space();
            return Ok(());
        }
        Err(syntax(
            open,
            open + 1,
            "no cases",
            &format!("`{kind}` is followed by a comma and its cases."),
            &format!("Write the cases after `{kind},`"),
        ))
    }

    /// The cases of a `plural` or `selectordinal`, up to its closing `}`.
    fn plural(&mut self, open: usize, name: String, ordinal: bool) -> Parsed<Piece> {
        let mut offset = 0;
        if self.starts_with("offset:") {
            let start = self.offset();
            self.at += "offset:".len();
            self.skip_space();
            let digits = self.word();
            match digits.parse::<i64>() {
                Ok(value) if value >= 0 => offset = value,
                _ => {
                    return Err(syntax(
                        start,
                        self.offset_before(),
                        "not a whole number",
                        "`offset:` is the whole number taken off the count before its case is chosen.",
                        "Write a whole number after `offset:`, such as `offset:1`",
                    ));
                }
            }
            self.skip_space();
        }
        let mut cases: Vec<PluralCase> = Vec::new();
        let mut other: Option<Message> = None;
        loop {
            self.skip_space();
            match self.peek() {
                Some('}') => {
                    self.at += 1;
                    break;
                }
                None => {
                    return Err(syntax(
                        open,
                        open + 1,
                        "never closed",
                        "Every `{` is closed by a `}`.",
                        "Add the `}` this `{` needs",
                    ));
                }
                _ => {}
            }
            let start = self.offset();
            let selector = self.selector()?;
            let end = self.offset_before();
            let word = match selector {
                Selector::Exact(n) => format!("={n}"),
                Selector::Category(c) => c.word().to_string(),
            };
            if (other.is_some() && selector == Selector::Category(PluralCategory::Other))
                || cases.iter().any(|c| c.selector == selector)
            {
                return Err(syntax(
                    start,
                    end,
                    "listed twice",
                    &format!("A plural has one case for `{word}`."),
                    &format!("Remove the second `{word}` case"),
                ));
            }
            let body = self.body(start, end, &word, true)?;
            match selector {
                Selector::Category(PluralCategory::Other) => other = Some(body),
                selector => cases.push(PluralCase {
                    selector,
                    message: body,
                }),
            }
        }
        let close = self.offset_before();
        let Some(other) = other else {
            return Err(other_missing(
                open,
                close,
                if ordinal { "selectordinal" } else { "plural" },
                &name,
            ));
        };
        Ok(Piece::Plural {
            name,
            ordinal,
            offset,
            cases,
            other,
        })
    }

    /// The cases of a `select`, up to its closing `}`.
    fn select(&mut self, open: usize, name: String, in_plural: bool) -> Parsed<Piece> {
        let mut cases: Vec<(String, Message)> = Vec::new();
        let mut other: Option<Message> = None;
        loop {
            self.skip_space();
            match self.peek() {
                Some('}') => {
                    self.at += 1;
                    break;
                }
                None => {
                    return Err(syntax(
                        open,
                        open + 1,
                        "never closed",
                        "Every `{` is closed by a `}`.",
                        "Add the `}` this `{` needs",
                    ));
                }
                _ => {}
            }
            let start = self.offset();
            let word = self.word();
            let end = self.offset_before();
            if word.is_empty() {
                return Err(syntax(
                    start,
                    start + 1,
                    "not a case",
                    "A select's cases are words, each followed by its message in braces: `female {She} other {They}`.",
                    "Write the case's word, then its message in braces",
                ));
            }
            if (word == "other" && other.is_some()) || cases.iter().any(|(seen, _)| *seen == word) {
                return Err(syntax(
                    start,
                    end,
                    "listed twice",
                    &format!("A select has one case for `{word}`."),
                    &format!("Remove the second `{word}` case"),
                ));
            }
            let body = self.body(start, end, &word, in_plural)?;
            if word == "other" {
                other = Some(body);
            } else {
                cases.push((word, body));
            }
        }
        let close = self.offset_before();
        let Some(other) = other else {
            return Err(other_missing(open, close, "select", &name));
        };
        Ok(Piece::Select { name, cases, other })
    }

    /// A case's message in braces.
    fn body(&mut self, start: usize, end: usize, word: &str, in_plural: bool) -> Parsed<Message> {
        self.skip_space();
        if self.peek() != Some('{') {
            return Err(syntax(
                start,
                end.max(self.offset()),
                "no message",
                &format!("`{word}` is followed by its message in braces."),
                &format!("Write the message after `{word}`, as `{word} {{...}}`"),
            ));
        }
        let open = self.offset();
        self.at += 1;
        let message = self.message(true, in_plural, open)?;
        // `message` stops at the `}` that closes this one.
        self.at += 1;
        Ok(message)
    }

    /// `=3`, `one`, `other`.
    fn selector(&mut self) -> Parsed<Selector> {
        let start = self.offset();
        if self.peek() == Some('=') {
            self.at += 1;
            let negative = self.peek() == Some('-');
            if negative {
                self.at += 1;
            }
            let digits = self.word();
            let end = self.offset_before();
            return match digits.parse::<i64>() {
                Ok(n) => Ok(Selector::Exact(if negative { -n } else { n })),
                Err(_) => Err(syntax(
                    start,
                    end.max(start + 1),
                    "not a whole number",
                    "`=` is followed by the whole number the case matches exactly, `=0` or `=1`.",
                    "Write a whole number after `=`",
                )),
            };
        }
        let word = self.word();
        let end = self.offset_before();
        if word.is_empty() {
            return Err(syntax(
                start,
                start + 1,
                "not a case",
                "A plural's cases are `=0`, `=1` and so on, or one of `zero`, `one`, `two`, `few`, `many` and `other`, each followed by its message in braces.",
                "Write a case, then its message in braces",
            ));
        }
        match PluralCategory::from_word(&word) {
            Some(category) => Ok(Selector::Category(category)),
            None => Err(syntax(
                start,
                end,
                "no such case",
                &format!(
                    "`{word}` is not one of CLDR's plural categories, `zero`, `one`, `two`, `few`, `many` and `other`; an exact count is written `=1`."
                ),
                "Use one of the six categories, or `=` and a number",
            )),
        }
    }

    fn starts_with(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, c)| self.peek_at(i) == Some(c))
    }
}

fn flush(text: &mut String, pieces: &mut Vec<Piece>) {
    if !text.is_empty() {
        pieces.push(Piece::Text(std::mem::take(text)));
    }
}

fn other_missing(open: usize, close: usize, kind: &str, name: &str) -> MessageError {
    MessageError {
        kind: MessageErrorKind::OtherMissing,
        start: open,
        end: close.max(open + 1),
        label: "no `other` case".to_string(),
        reason: format!(
            "`{name}`'s {kind} must end with an `other` case: it is what is shown when no other case matches, which a language with more plural forms than the sentence was written with will need."
        ),
        next: format!("Add an `other {{...}}` case to the {kind} on `{name}`"),
    }
}

/// One parameter of a translation key's call: a placeholder of its
/// default-locale message.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TranslationParam {
    pub name: String,
    pub kind: ParamKind,
}

/// One key of `translations/`: the typed call it becomes and its message in
/// each supported locale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationKey {
    pub name: String,
    /// By name; a call gives each as an argument after its `locale:`.
    pub params: Vec<TranslationParam>,
    /// The message in each locale, by the locale's place among `Locale`'s
    /// variants.
    pub messages: Vec<(u32, Message)>,
}

impl TranslationKey {
    pub fn message(&self, locale: u32) -> Option<&Message> {
        self.messages
            .iter()
            .find(|(at, _)| *at == locale)
            .map(|(_, message)| message)
    }
}

/// Every key, in name order: what the runtime formats a translation call
/// from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranslationPlan {
    pub keys: Vec<TranslationKey>,
}

impl TranslationPlan {
    /// The key called `name` and its place.
    pub fn key(&self, name: &str) -> Option<(u32, &TranslationKey)> {
        let at = self.keys.iter().position(|key| key.name == name)?;
        Some((u32::try_from(at).ok()?, self.keys.get(at)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(source: &str) -> Message {
        parse(source).unwrap_or_default()
    }

    fn names(source: &str) -> Vec<(String, ParamKind)> {
        ok(source)
            .placeholders()
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.name, p.kind))
            .collect()
    }

    #[test]
    fn text_and_placeholders() {
        assert_eq!(
            ok("Your order {order_number} has shipped.").pieces,
            vec![
                Piece::Text("Your order ".into()),
                Piece::Argument("order_number".into()),
                Piece::Text(" has shipped.".into()),
            ]
        );
        assert_eq!(ok("").pieces, vec![]);
    }

    #[test]
    fn apostrophes_quote_only_before_a_brace() {
        assert_eq!(ok("Don't").pieces, vec![Piece::Text("Don't".into())]);
        assert_eq!(ok("It''s").pieces, vec![Piece::Text("It's".into())]);
        assert_eq!(ok("'{'x'}'").pieces, vec![Piece::Text("{x}".into())]);
        assert_eq!(
            ok("a '{b}' c").pieces,
            vec![Piece::Text("a {b} c".into())]
        );
    }

    #[test]
    fn a_plural_has_cases_and_a_count() {
        let message = ok("{count, plural, =0 {none} one {# item} other {# items}}");
        let Some(Piece::Plural { cases, other, .. }) = message.pieces.first() else {
            panic!("not a plural: {message:?}");
        };
        assert_eq!(cases.len(), 2);
        assert_eq!(other.pieces, vec![Piece::Count, Piece::Text(" items".into())]);
        assert_eq!(names("{count, plural, one {#} other {#}}"), [("count".into(), ParamKind::Number)]);
    }

    #[test]
    fn placeholders_join_their_uses() {
        assert_eq!(
            names("{a} {n, number} {when, date} {when, time}"),
            [
                ("a".into(), ParamKind::Text),
                ("n".into(), ParamKind::Number),
                ("when".into(), ParamKind::Time),
            ]
        );
        assert_eq!(
            ok("{n} {n, number}").placeholders(),
            Err(Conflict {
                name: "n".into(),
                first: ParamKind::Text,
                second: ParamKind::Number
            })
        );
        assert_eq!(
            names("{who, select, a {{x}} other {{y}}}"),
            [
                ("who".into(), ParamKind::Text),
                ("x".into(), ParamKind::Text),
                ("y".into(), ParamKind::Text),
            ]
        );
    }

    #[test]
    fn a_plural_or_select_without_other_is_its_own_error() {
        let error = parse("{n, plural, one {x}}").err();
        assert_eq!(error.map(|e| e.kind), Some(MessageErrorKind::OtherMissing));
        let error = parse("{k, select, a {x}}").err();
        assert_eq!(error.map(|e| e.kind), Some(MessageErrorKind::OtherMissing));
    }

    #[test]
    fn syntax_errors_say_where() {
        let error = parse("Hello {name").err().unwrap_or_else(|| syntax(0, 0, "", "", ""));
        assert_eq!((error.kind, error.start), (MessageErrorKind::Syntax, 6));
        let error = parse("Hello {0}").err().unwrap_or_else(|| syntax(0, 0, "", "", ""));
        assert_eq!((error.start, error.end), (7, 8));
        assert!(parse("a } b").is_err());
        assert!(parse("{n, plurl, other {x}}").is_err());
        assert!(parse("{n, number, percent}").is_err());
        assert!(parse("{n, plural, lots {x} other {y}}").is_err());
        assert!(parse("{n, plural, one {x} one {y} other {z}}").is_err());
    }
}
