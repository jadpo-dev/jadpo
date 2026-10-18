//! A person's name and a postal address for people (area 19, "People and
//! contact", chunk 5.5l): each in the order the reader's locale gives,
//! since neither has a single text form.
//!
//! A name goes through CLDR's person-name patterns (UTS #35, "Person
//! Names", from `person_names_data`): medium length, formal, referring to
//! the person, with the fields a name hasn't left out. ICU4X's own
//! formatter is still experimental and leaves literals behind, so the
//! patterns are applied here.
//! The name has no locale of its own, so it is given one as the
//! specification derives it: the reader's, when the name is written in
//! the reader's script, and otherwise the likeliest locale for the name's
//! script. That locale decides the order, so a name in Latin script is
//! "Ada Lovelace" to an English reader and "Lovelace Ada" to a Hungarian
//! one, and a name in Japanese script is family name first to both.
//!
//! An address goes through libaddressinput's format for its country
//! (`jadpo_std::standard::addresses_data`): the country's own format for a
//! reader of one of its languages and its Latin-script format, where it has
//! one, for anyone else; one line per line, each street line its own; the
//! literal text between fields only where the fields on both sides are
//! there; and the country's CLDR name last when the reader's locale is
//! another country's, as an address posted from abroad has it.

use icu_locale::{Locale, LocaleExpander};
use icu_properties::props::Script;
use icu_properties::{CodePointMapData, PropertyNamesShort};
use jadpo_std::standard::person_names_data::{PERSON_NAMES, PersonNameData, ROOT};

use super::locale_of;
use crate::{Fault, Record, Value};

/// The text of a record's field, `None` when it is `none` or missing.
fn text<'r>(record: &'r Record, name: &str) -> Option<&'r str> {
    match record.get(name) {
        Some(Value::Text(text)) => Some(text),
        _ => None,
    }
}

/// A locale with its likely script and region filled in.
fn maximized(mut locale: Locale) -> Locale {
    LocaleExpander::new_extended().maximize(&mut locale.id);
    locale
}

/// The script a name is written in, as a locale's script subtag: that of
/// its first letter with one, the family name looked at first.
fn name_script(parts: &[&str]) -> Option<icu_locale::subtags::Script> {
    let scripts = CodePointMapData::<Script>::new();
    let found = parts
        .iter()
        .flat_map(|part| part.chars())
        .map(|c| scripts.get(c))
        .find(|script| {
            !matches!(
                *script,
                Script::Common | Script::Inherited | Script::Unknown
            )
        })?;
    PropertyNamesShort::<Script>::new().get_locale_script(found)
}

/// Whether two scripts write one language's names: the same, or Japanese
/// beside the kanji and kana it is made of.
fn compatible(a: icu_locale::subtags::Script, b: icu_locale::subtags::Script) -> bool {
    let japanese = ["Hani", "Hira", "Kana", "Jpan"];
    a == b || (japanese.contains(&a.as_str()) && japanese.contains(&b.as_str()))
}

/// CLDR's person-name data for a maximized locale: its language with its
/// script, then its language, then root's.
fn data_for(locale: &Locale) -> &'static PersonNameData {
    let language = locale.id.language.as_str();
    let with_script = locale
        .id
        .script
        .map(|script| format!("{language}-{}", script.as_str()));
    with_script
        .iter()
        .map(String::as_str)
        .chain([language])
        .find_map(|id| PERSON_NAMES.iter().find(|data| data.locale == id))
        .unwrap_or(&ROOT)
}

/// Whether `data`'s locale writes the names of `name_locale` family name
/// first (UTS #35, "Derive the name order"): its language listed with
/// those, or, not listed with either, `und` listed with them.
fn surname_first(data: &PersonNameData, name_locale: &Locale) -> bool {
    let language = name_locale.id.language.as_str();
    let listed = |list: &str| list.split(' ').any(|item| item == language);
    if listed(data.surname_first) {
        return true;
    }
    if listed(data.given_first) {
        return false;
    }
    data.surname_first.split(' ').any(|item| item == "und")
}

/// A pattern with its given name and family name in place: a field the
/// name hasn't is left out with the text written beside it, unless a field
/// it has stands on each side; spaces between the parts are `space`.
fn filled(pattern: &str, given: &str, family: Option<&str>, space: &str) -> String {
    // The pattern's pieces: `Some(value)` for a field, its value empty
    // when the name hasn't it, and `None` for the text between.
    let mut pieces: Vec<(Option<&str>, &str)> = Vec::new();
    let mut rest = pattern;
    while !rest.is_empty() {
        match (rest.find('{'), rest.find('}')) {
            (Some(0), Some(close)) => {
                let field = &rest[1..close];
                let value = match field.split('-').next() {
                    Some("given") if field == "given" => given,
                    Some("surname") if field == "surname" => family.unwrap_or(""),
                    _ => "",
                };
                pieces.push((Some(value), ""));
                rest = &rest[close + 1..];
            }
            (Some(open), _) => {
                pieces.push((None, &rest[..open]));
                rest = &rest[open..];
            }
            (None, _) => {
                pieces.push((None, rest));
                rest = "";
            }
        }
    }
    let present = |piece: &(Option<&str>, &str)| piece.0.is_some_and(|value| !value.is_empty());
    let mut out = String::new();
    for (at, piece) in pieces.iter().enumerate() {
        match piece {
            (Some(value), _) => out.push_str(value),
            (None, literal) => {
                let before = pieces.iter().take(at).any(present);
                let after = pieces.iter().skip(at + 1).any(present);
                if before && after {
                    out.push_str(literal);
                }
            }
        }
    }
    // Runs of spaces the left-out fields leave become one, then the
    // locale's own.
    let joined = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if space == " " {
        joined
    } else {
        joined.replace(' ', space)
    }
}

/// A `PersonName` for people in the locale tagged `tag`
/// (`FormatKey::PersonName`).
pub fn person_name(value: &Value, tag: &str) -> Result<String, Fault> {
    let Value::Record(record) = value else {
        return Err(Fault::Shape);
    };
    let given = text(record, "given").ok_or(Fault::Shape)?;
    let family = text(record, "family");
    let reader = maximized(locale_of(tag)?);
    // The name's locale (UTS #35, "Derive the name locale"): the reader's
    // when the name is in the reader's script, else the likeliest for the
    // name's script, whose own data then formats it ("Switch the
    // formatting locale").
    let written: Vec<&str> = family.into_iter().chain([given]).collect();
    let name_locale = match (name_script(&written), reader.id.script) {
        (Some(script), Some(theirs)) if !compatible(script, theirs) => {
            let mut locale = Locale::UNKNOWN;
            locale.id.script = Some(script);
            maximized(locale)
        }
        _ => reader,
    };
    let data = data_for(&name_locale);
    let pattern = if surname_first(data, &name_locale) {
        data.surname_pattern
    } else {
        data.given_pattern
    };
    Ok(filled(pattern, given, family, data.native_space))
}

/// One piece of an address format: a field's letter or text written
/// between fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Piece<'f> {
    Field(char),
    Literal(&'f str),
}

/// A format's lines, each its pieces: `%n` ends a line, `%X` is field `X`.
fn lines_of(format: &str) -> Vec<Vec<Piece<'_>>> {
    let mut lines = vec![Vec::new()];
    let mut rest = format;
    while !rest.is_empty() {
        match rest.find('%') {
            Some(0) => {
                let mut chars = rest[1..].chars();
                match chars.next() {
                    Some('n') => lines.push(Vec::new()),
                    Some(letter) => {
                        if let Some(line) = lines.last_mut() {
                            line.push(Piece::Field(letter));
                        }
                    }
                    None => break,
                }
                rest = chars.as_str();
            }
            Some(at) => {
                if let Some(line) = lines.last_mut() {
                    line.push(Piece::Literal(&rest[..at]));
                }
                rest = &rest[at..];
            }
            None => {
                if let Some(line) = lines.last_mut() {
                    line.push(Piece::Literal(rest));
                }
                rest = "";
            }
        }
    }
    lines
}

/// A `PostalAddress` for people in the locale tagged `tag`
/// (`FormatKey::PostalAddress`), one line per line.
pub fn postal_address(value: &Value, tag: &str) -> Result<String, Fault> {
    let Value::Record(record) = value else {
        return Err(Fault::Shape);
    };
    let country = match record.get("country") {
        Some(Value::Variant(variant)) => jadpo_std::standard_type(&variant.ty)
            .and_then(|entry| entry.codes())
            .and_then(|codes| codes.get(variant.ordinal as usize))
            .map(|(_, code)| *code)
            .ok_or(Fault::Shape)?,
        _ => return Err(Fault::Shape),
    };
    let written_lines: Vec<&str> = match record.get("lines") {
        Some(Value::List(lines)) => lines
            .iter()
            .filter_map(|line| match line {
                Value::Text(text) if !text.trim().is_empty() => Some(&**text),
                _ => None,
            })
            .collect(),
        _ => return Err(Fault::Shape),
    };
    let reader = maximized(locale_of(tag)?);
    let (own, latin, languages) = jadpo_std::standard::addresses_data::ADDRESS_FORMATS
        .iter()
        .find(|(code, ..)| *code == country)
        .map_or(
            (jadpo_std::standard::addresses_data::DEFAULT_FORMAT, "", ""),
            |(_, own, latin, languages)| (*own, *latin, *languages),
        );
    // The country's own format for a reader of its languages, its Latin
    // one for anyone else.
    let reads_its_own = languages.is_empty()
        || languages
            .split('~')
            .any(|language| language.split('-').next() == Some(reader.id.language.as_str()));
    let format = if !reads_its_own && !latin.is_empty() {
        latin
    } else if own.is_empty() {
        jadpo_std::standard::addresses_data::DEFAULT_FORMAT
    } else {
        own
    };
    let field = |letter: char| -> Vec<&str> {
        match letter {
            'A' => written_lines.clone(),
            'C' => text(record, "locality").into_iter().collect(),
            'S' => text(record, "region").into_iter().collect(),
            'Z' => text(record, "postal_code").into_iter().collect(),
            _ => Vec::new(),
        }
    };
    let mut out: Vec<String> = Vec::new();
    for pieces in lines_of(format) {
        let filled: Vec<bool> = pieces
            .iter()
            .map(|piece| match piece {
                Piece::Field(letter) => !field(*letter).is_empty(),
                Piece::Literal(_) => false,
            })
            .collect();
        let fields = pieces
            .iter()
            .filter(|piece| matches!(piece, Piece::Field(_)))
            .count();
        let mut line = String::new();
        for (at, piece) in pieces.iter().enumerate() {
            match piece {
                // A street's lines are lines of their own.
                Piece::Field(letter) => line.push_str(&field(*letter).join("\n")),
                // Text between fields only where the fields beside it are
                // there: before the first, only if the next is; after the
                // last, only if that one is.
                Piece::Literal(text) => {
                    let is_field = |piece: &&Piece<'_>| matches!(piece, Piece::Field(_));
                    let before = pieces.iter().take(at).rposition(|piece| is_field(&piece));
                    let after = pieces
                        .iter()
                        .skip(at + 1)
                        .position(|piece| is_field(&piece))
                        .map(|offset| at + 1 + offset);
                    let there = |at: usize| filled.get(at).copied().unwrap_or(false);
                    let keep = match (before, after) {
                        _ if fields == 0 => true,
                        (Some(before), Some(after)) => there(before) && there(after),
                        (None, Some(after)) => there(after),
                        (Some(before), None) => there(before),
                        (None, None) => true,
                    };
                    if keep {
                        line.push_str(text);
                    }
                }
            }
        }
        for part in line.split('\n') {
            let part = part.trim();
            if !part.is_empty() {
                out.push(part.to_string());
            }
        }
    }
    // The country, last, for a reader in another country.
    if reader.id.region.map(|region| region.as_str().to_string()) != Some(country.to_string()) {
        let name = super::names::region(country, tag)?.unwrap_or_else(|| country.to_string());
        out.push(name);
    }
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(given: &str, family: Option<&str>) -> Value {
        Value::record(
            "PersonName",
            vec![
                ("given".into(), Value::text(given)),
                ("family".into(), family.map_or(Value::None, Value::text)),
            ],
        )
    }

    fn address(
        lines: &[&str],
        locality: Option<&str>,
        region: Option<&str>,
        postal_code: Option<&str>,
        country: &str,
    ) -> Value {
        let entry = jadpo_std::standard_type("Country").unwrap();
        let codes = entry.codes().unwrap();
        let at = codes.iter().position(|(_, code)| *code == country).unwrap();
        let optional = |part: Option<&str>| part.map_or(Value::None, Value::text);
        Value::record(
            "PostalAddress",
            vec![
                (
                    "lines".into(),
                    Value::list(lines.iter().map(|line| Value::text(line)).collect()),
                ),
                ("locality".into(), optional(locality)),
                ("region".into(), optional(region)),
                ("postal_code".into(), optional(postal_code)),
                (
                    "country".into(),
                    Value::variant("Country", at as u32, codes[at].0, Vec::new()),
                ),
            ],
        )
    }

    #[test]
    fn a_name_is_in_the_order_its_script_and_the_reader_give() {
        let ada = name("Ada", Some("Lovelace"));
        assert_eq!(person_name(&ada, "en-GB"), Ok("Ada Lovelace".to_string()));
        assert_eq!(person_name(&ada, "hu-HU"), Ok("Lovelace Ada".to_string()));
        let taro = name("太郎", Some("山田"));
        assert_eq!(person_name(&taro, "ja-JP"), Ok("山田太郎".to_string()));
        assert_eq!(
            person_name(&name("Sukarno", None), "en-GB"),
            Ok("Sukarno".to_string())
        );
    }

    #[test]
    fn an_address_is_in_its_country_s_order_for_the_reader() {
        let tokyo = address(
            &["丸の内2-7-2"],
            Some("千代田区"),
            Some("東京都"),
            Some("100-8994"),
            "JP",
        );
        assert_eq!(
            postal_address(&tokyo, "ja-JP"),
            Ok("〒100-8994\n東京都千代田区\n丸の内2-7-2".to_string())
        );
        assert_eq!(
            postal_address(&tokyo, "en-GB"),
            Ok("丸の内2-7-2\n千代田区, 東京都\n100-8994\nJapan".to_string())
        );
        let downing = address(
            &["10 Downing Street"],
            Some("London"),
            None,
            Some("SW1A 2AA"),
            "GB",
        );
        assert_eq!(
            postal_address(&downing, "en-GB"),
            Ok("10 Downing Street\nLondon\nSW1A 2AA".to_string())
        );
        assert_eq!(
            postal_address(&downing, "de-DE"),
            Ok("10 Downing Street\nLondon\nSW1A 2AA\nVereinigtes Königreich".to_string())
        );
        let white_house = address(
            &["1600 Pennsylvania Avenue NW"],
            Some("Washington"),
            Some("DC"),
            Some("20500"),
            "US",
        );
        assert_eq!(
            postal_address(&white_house, "en-US"),
            Ok("1600 Pennsylvania Avenue NW\nWashington DC 20500".to_string())
        );
        let berlin = address(
            &["Platz der Republik 1"],
            Some("Berlin"),
            None,
            Some("11011"),
            "DE",
        );
        assert_eq!(
            postal_address(&berlin, "de-DE"),
            Ok("Platz der Republik 1\n11011 Berlin".to_string())
        );
        assert_eq!(
            postal_address(&berlin, "en-US"),
            Ok("Platz der Republik 1\n11011 Berlin\nGermany".to_string())
        );
    }

    #[test]
    fn literals_go_with_the_fields_beside_them() {
        let lines = lines_of("〒%Z%n%S%C%n%A");
        assert_eq!(lines[0], vec![Piece::Literal("〒"), Piece::Field('Z')]);
        let no_region = address(&["2-7-2"], Some("Chiyoda-ku"), None, None, "JP");
        assert_eq!(
            postal_address(&no_region, "en-GB"),
            Ok("2-7-2\nChiyoda-ku\nJapan".to_string())
        );
    }
}
