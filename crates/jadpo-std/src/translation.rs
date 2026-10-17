//! Translations as messages (area 10, "Translations as messages"): the
//! wording each language gives a message, an enum or an email subject, read
//! into [`Wording`] while compiling and carried to the runtime as a
//! [`TranslationPlan`], which `jadpo-value` writes in the caller's locale.
//!
//! A wording is a plain string whose only syntax is `{field}`; a literal
//! brace is doubled, `{{` writing `{` and `}}` writing `}`. Where a
//! message has an enum field or a `PluralCount` field, a language gives one
//! wording for each value of the enum or each plural form it needs, and
//! the field picks which is written.

/// What a parameter of a translation call is, which decides how its value
/// is written into the wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParamKind {
    /// A value built on `Text` or `PresentationText`: written as it is.
    Text,
    /// A value built on `Int` or `Decimal`: written the locale's way.
    Number,
    /// A `PluralCount`: picks the plural form, and `{count}` writes it the
    /// locale's way.
    Plural,
    /// An enum's value: picks which wording is written, and is not itself
    /// a placeholder.
    Choice,
}

impl ParamKind {
    /// How the kind reads in an error.
    pub fn word(self) -> &'static str {
        match self {
            ParamKind::Text => "text",
            ParamKind::Number => "a number",
            ParamKind::Plural => "a plural count",
            ParamKind::Choice => "an enum",
        }
    }

    /// Whether a wording may write the parameter as `{name}`.
    pub fn is_placeholder(self) -> bool {
        !matches!(self, ParamKind::Choice)
    }
}

/// How a client app formats a failure's client field before it fills the
/// field's placeholder (area 10, "Failures for client apps"): the field's
/// type decides it, and the export says it for the placeholders that need
/// one. A text field needs none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ClientFormat {
    /// An `Int`.
    Number,
    /// A `Decimal`.
    Decimal,
    /// A `CalendarDate`.
    Date,
    /// An `Instant`, shown in the reader's own zone by the client, or a
    /// `ZonedTime`, in the zone it carries.
    DateTime,
    /// A `Duration`.
    Duration,
}

impl ClientFormat {
    /// The word the export writes: `number`, `decimal`, `date`,
    /// `date_time` or `duration`.
    pub fn word(self) -> &'static str {
        match self {
            ClientFormat::Number => "number",
            ClientFormat::Decimal => "decimal",
            ClientFormat::Date => "date",
            ClientFormat::DateTime => "date_time",
            ClientFormat::Duration => "duration",
        }
    }
}

/// One of CLDR's plural categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// The cardinal plural forms the locale at `locale` (its place among
/// `Locale`'s variants) needs, from Unicode CLDR 48, in the order
/// [`PluralCategory::ALL`] lists them. `jadpo-value` holds the table to ICU4X's
/// own on every build.
pub fn plural_categories(locale: u32) -> Vec<PluralCategory> {
    let mask = PLURAL_MASKS
        .get(locale as usize)
        .copied()
        .unwrap_or(0b10_0000);
    PluralCategory::ALL
        .into_iter()
        .filter(|category| mask & category.bit() != 0)
        .collect()
}

/// The mask of a list of categories, as [`PLURAL_MASKS`] holds them.
pub fn plural_mask(categories: &[PluralCategory]) -> u8 {
    categories
        .iter()
        .fold(0, |mask, category| mask | category.bit())
}

/// A piece of a wording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    /// `{field}`.
    Field(String),
}

/// A wording: its pieces in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Template {
    pub pieces: Vec<Piece>,
}

impl Template {
    /// The wording as it is written in a language file: `{field}` for a
    /// field and a doubled brace for a literal one. The export for client
    /// apps carries it, so a client reads the same syntax.
    pub fn written(&self) -> String {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(text) => {
                    for c in text.chars() {
                        match c {
                            '{' => out.push_str("{{"),
                            '}' => out.push_str("}}"),
                            other => out.push(other),
                        }
                    }
                }
                Piece::Field(name) => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            }
        }
        out
    }

    /// The fields the wording writes, in order, repeats included.
    pub fn fields(&self) -> impl Iterator<Item = &str> {
        self.pieces.iter().filter_map(|piece| match piece {
            Piece::Field(name) => Some(name.as_str()),
            Piece::Text(_) => None,
        })
    }
}

/// Where a `{field}` stands in the text a wording was read from: a byte
/// range, braces included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldUse {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

/// What is wrong with a wording: where, and the words for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateError {
    pub start: usize,
    pub end: usize,
    /// The short words under the carets.
    pub label: String,
    /// The sentence saying why.
    pub reason: String,
    /// The one change that fixes it.
    pub next: String,
}

fn error(start: usize, end: usize, label: &str, reason: &str, next: &str) -> TemplateError {
    TemplateError {
        start,
        end: end.max(start + 1),
        label: label.to_string(),
        reason: reason.to_string(),
        next: next.to_string(),
    }
}

/// Whether `name` can name a field a wording writes: a lower_snake_case
/// name.
fn is_field_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Reads a wording, and where each of its fields stands.
pub fn parse_template(source: &str) -> Result<(Template, Vec<FieldUse>), TemplateError> {
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut uses: Vec<FieldUse> = Vec::new();
    let mut text = String::new();
    let mut at = 0;
    while at < chars.len() {
        let Some(&(offset, c)) = chars.get(at) else {
            break;
        };
        match c {
            '{' if chars.get(at + 1).is_some_and(|(_, next)| *next == '{') => {
                text.push('{');
                at += 2;
            }
            '}' if chars.get(at + 1).is_some_and(|(_, next)| *next == '}') => {
                text.push('}');
                at += 2;
            }
            '}' => {
                return Err(error(
                    offset,
                    offset + 1,
                    "a closing brace with no opening one",
                    "A wording's only syntax is `{field}`, so a brace that isn't part of one is doubled to write it: `}}` writes `}`.",
                    "Write `}}` for a literal brace, or add the `{` it closes",
                ));
            }
            '{' => {
                let close = chars
                    .get(at + 1..)
                    .unwrap_or_default()
                    .iter()
                    .position(|(_, next)| matches!(next, '}' | '{'));
                let close = close.map(|i| at + 1 + i);
                // The brace and the name after it, for an error to point at.
                let name_end = offset
                    + 1
                    + source[offset + 1..]
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
                        .map(char::len_utf8)
                        .sum::<usize>();
                let reaches = close.map_or(source.len(), |close| {
                    chars.get(close).map_or(source.len(), |(at, _)| *at)
                });
                if source[offset + 1..reaches].contains(',') {
                    return Err(error(
                        offset,
                        name_end,
                        "not a field",
                        "A wording's only syntax is `{field}`: there is no ICU MessageFormat, so a plural or a choice is one wording for each plural form or each value of the enum, written as members, and a number is written the locale's way without a style.",
                        "Write the field's name alone between the braces, and give each form its own wording",
                    ));
                }
                let Some(close) = close else {
                    return Err(error(
                        offset,
                        name_end,
                        "this brace is never closed",
                        "A `{` starts a field, `{field}`, and a literal brace is doubled: `{{` writes `{`.",
                        "Close the field with `}`, or write `{{` for a literal brace",
                    ));
                };
                let Some(&(end_offset, closer)) = chars.get(close) else {
                    break;
                };
                if closer == '{' {
                    return Err(error(
                        offset,
                        name_end,
                        "this brace is never closed",
                        "A `{` starts a field, `{field}`, and a literal brace is doubled: `{{` writes `{`.",
                        "Close the field with `}`, or write `{{` for a literal brace",
                    ));
                }
                let name = &source[offset + 1..end_offset];
                if name.is_empty() {
                    return Err(error(
                        offset,
                        end_offset + 1,
                        "no field named",
                        "A field is written with its name between braces, `{name}`.",
                        "Write the field's name between the braces, or `{{}}` for a literal pair of braces",
                    ));
                }
                if !is_field_name(name) {
                    return Err(error(
                        offset,
                        end_offset + 1,
                        "not a field's name",
                        "A field's name is lower_snake_case: lower-case letters, digits and underscores, starting with a letter, with nothing else between the braces.",
                        "Write the field's name between the braces, or `{{` for a literal brace",
                    ));
                }
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                pieces.push(Piece::Field(name.to_string()));
                uses.push(FieldUse {
                    name: name.to_string(),
                    start: offset,
                    end: end_offset + 1,
                });
                at = close + 1;
            }
            other => {
                text.push(other);
                at += 1;
            }
        }
    }
    if !text.is_empty() {
        pieces.push(Piece::Text(text));
    }
    Ok((Template { pieces }, uses))
}

/// What a language says for one message, enum or email subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wording {
    /// One wording.
    Plain(Template),
    /// One wording for each value of an enum, by the value's name.
    Variants(Vec<(String, Template)>),
    /// One wording for each plural form the language needs.
    Forms(Vec<(PluralCategory, Template)>),
}

/// One parameter of a translation call: a field of the message, or the
/// enum's value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TranslationParam {
    pub name: String,
    pub kind: ParamKind,
}

/// One message, enum or email subject: the typed call it becomes and its
/// wording in each locale that has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationEntry {
    /// `<domain>.<name>`, or `shared.<name>`, as the call is written.
    pub key: String,
    /// By name; a call gives each as an argument after its `locale:`.
    pub params: Vec<TranslationParam>,
    /// The wording in each locale, by the locale's place among `Locale`'s
    /// variants. A supported locale that is complete has one; an
    /// in-progress locale has one only where its language file words this
    /// message in full.
    pub wordings: Vec<(u32, Wording)>,
}

impl TranslationEntry {
    pub fn wording(&self, locale: u32) -> Option<&Wording> {
        self.wordings
            .iter()
            .find(|(at, _)| *at == locale)
            .map(|(_, wording)| wording)
    }
}

/// Every entry, in key order: what the runtime writes a translation call
/// from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranslationPlan {
    pub entries: Vec<TranslationEntry>,
}

impl TranslationPlan {
    /// The entry called `key` and its place.
    pub fn entry(&self, key: &str) -> Option<(u32, &TranslationEntry)> {
        let at = self.entries.iter().position(|entry| entry.key == key)?;
        Some((u32::try_from(at).ok()?, self.entries.get(at)?))
    }
}

/// The plural forms each locale needs, as masks over
/// [`PluralCategory::ALL`], indexed by the locale's place among `Locale`'s
/// variants. Generated from ICU4X's CLDR data and checked against it by
/// `jadpo-value`'s tests.
static PLURAL_MASKS: [u8; 533] = [
    34, 34, 34, 34, 34, 34, 34, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63,
    63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 63, 34, 34, 34, 34, 34, 32, 32, 58, 58, 58, 34, 34, 34,
    34, 34, 42, 42, 42, 50, 50, 50, 50, 50, 50, 34, 34, 58, 58, 35, 35, 63, 63, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 46, 46, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 50, 50, 50, 50, 50, 50, 50,
    50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50,
    50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50,
    50, 50, 50, 50, 50, 50, 50, 50, 62, 62, 62, 46, 46, 34, 34, 34, 34, 34, 34, 34, 34, 38, 38, 34,
    34, 34, 42, 42, 42, 46, 46, 50, 50, 34, 34, 34, 34, 32, 32, 32, 32, 34, 34, 50, 50, 50, 50, 50,
    32, 32, 32, 32, 34, 34, 34, 32, 32, 34, 34, 32, 32, 34, 34, 32, 32, 32, 32, 34, 34, 34, 34, 34,
    32, 32, 58, 58, 35, 35, 34, 34, 34, 34, 34, 34, 34, 34, 32, 32, 32, 32, 32, 32, 32, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 34, 58, 58,
    34, 34, 34, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 50, 32, 32, 32, 32, 34, 34, 42, 42,
    42, 58, 58, 58, 58, 58, 58, 58, 34, 34, 34, 32, 32, 32, 34, 34, 58, 58, 46, 46, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 42, 42, 42, 42, 42, 32, 32, 32, 32, 42, 34, 34, 34, 34, 34, 34, 34, 34, 34,
    34, 34, 34, 34, 34, 34, 34, 32, 32, 34, 34, 34, 34, 34, 34, 34, 34, 58, 58, 32, 32, 34, 34, 34,
    34, 34, 34, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32, 32,
    32, 32, 32, 34, 34,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn read(source: &str) -> (Template, Vec<FieldUse>) {
        match parse_template(source) {
            Ok(found) => found,
            Err(error) => panic!("{error:?}"),
        }
    }

    #[test]
    fn text_and_fields() {
        let (template, uses) = read("Hello {name}, your order {order_number}.");
        assert_eq!(
            template.pieces,
            vec![
                Piece::Text("Hello ".into()),
                Piece::Field("name".into()),
                Piece::Text(", your order ".into()),
                Piece::Field("order_number".into()),
                Piece::Text(".".into()),
            ]
        );
        assert_eq!(
            uses[0],
            FieldUse {
                name: "name".into(),
                start: 6,
                end: 12
            }
        );
        assert_eq!(
            template.fields().collect::<Vec<_>>(),
            ["name", "order_number"]
        );
    }

    #[test]
    fn doubled_braces_are_literal() {
        let (template, uses) = read("{{ {x} }}");
        assert_eq!(
            template.pieces,
            vec![
                Piece::Text("{ ".into()),
                Piece::Field("x".into()),
                Piece::Text(" }".into()),
            ]
        );
        assert_eq!(uses.len(), 1);
    }

    #[test]
    fn a_wording_with_nothing_special_is_one_text() {
        let (template, uses) = read("Don't panic; it's 'fine'.");
        assert_eq!(
            template.pieces,
            vec![Piece::Text("Don't panic; it's 'fine'.".into())]
        );
        assert!(uses.is_empty());
        assert_eq!(parse_template("").map(|(t, _)| t.pieces).ok(), Some(vec![]));
    }

    #[test]
    fn mistakes_say_where() {
        let at = |source: &str| parse_template(source).err().map(|e| (e.start, e.end));
        assert_eq!(at("a } b"), Some((2, 3)));
        assert_eq!(at("Hello {name"), Some((6, 11)));
        assert_eq!(at("Hello {name {x}"), Some((6, 11)));
        assert_eq!(at("{}"), Some((0, 2)));
        assert_eq!(at("{n, plural, one {x}}"), Some((0, 2)));
        assert_eq!(at("{Name}"), Some((0, 6)));
        assert_eq!(at("{ name }"), Some((0, 8)));
        assert_eq!(at("{0}"), Some((0, 3)));
    }
}
