//! A translation's message in a locale (area 10, "Translations"): the
//! pieces of an ICU MessageFormat message written out with the arguments a
//! call gave, over the same CLDR data as `temporal.format`. A plural is
//! decided by CLDR's plural rules for the locale, a number is written with
//! the locale's digits, grouping and decimal separator, and a date or time
//! with the locale's patterns, so the interpreter, a native deployable and
//! WebAssembly write the same text.

use fixed_decimal::{Decimal as Fixed, SignedRoundingMode, UnsignedRoundingMode};
use icu_decimal::DecimalFormatter;
use icu_plurals::{PluralCategory as Category, PluralRules};
use jadpo_std::translation::{
    Message, NumberStyle, Piece, PluralCategory, Selector, TranslationKey,
};
use jadpo_std::{BuiltinFormat, FormatPart, TimeFormatSpec};
use writeable::Writeable;

use super::{format, locale_of, moment_of};
use crate::{Decimal, Fault, Value};

/// `translations.<key>(locale:, ...)`: the key's message in the locale
/// given, with `args` in the order of the key's parameters.
pub fn translate(key: &TranslationKey, locale: &Value, args: &[Value]) -> Result<Value, Fault> {
    let Value::Variant(variant) = locale else {
        return Err(Fault::Shape);
    };
    if &*variant.ty != jadpo_std::LOCALE {
        return Err(Fault::Shape);
    }
    let message = key.message(variant.ordinal).ok_or(Fault::Shape)?;
    let tag = jadpo_std::LOCALES
        .get(variant.ordinal as usize)
        .map(|(_, tag)| *tag)
        .ok_or(Fault::Shape)?;
    if args.len() != key.params.len() {
        return Err(Fault::Shape);
    }
    let named: Vec<(&str, &Value)> = key
        .params
        .iter()
        .map(|param| param.name.as_str())
        .zip(args)
        .collect();
    let mut out = String::new();
    Writer::new(tag, &named)?.write(message, None, &mut out)?;
    Ok(Value::text(&out))
}

/// Writes messages in one locale, building each formatter when first
/// needed.
struct Writer<'a> {
    tag: &'a str,
    locale: icu_locale::Locale,
    args: &'a [(&'a str, &'a Value)],
    numbers: Option<DecimalFormatter>,
    cardinal: Option<PluralRules>,
    ordinal: Option<PluralRules>,
}

/// The count a `#` shows inside a plural.
type Count<'a> = Option<&'a Fixed>;

impl<'a> Writer<'a> {
    fn new(tag: &'a str, args: &'a [(&'a str, &'a Value)]) -> Result<Writer<'a>, Fault> {
        Ok(Writer {
            tag,
            locale: locale_of(tag)?,
            args,
            numbers: None,
            cardinal: None,
            ordinal: None,
        })
    }

    fn arg(&self, name: &str) -> Result<&'a Value, Fault> {
        self.args
            .iter()
            .find(|(seen, _)| *seen == name)
            .map(|(_, value)| *value)
            .ok_or(Fault::Shape)
    }

    fn write(&mut self, message: &Message, count: Count<'_>, out: &mut String) -> Result<(), Fault> {
        for piece in &message.pieces {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Argument(name) => match self.arg(name)? {
                    Value::Text(text) => out.push_str(text),
                    _ => return Err(Fault::Shape),
                },
                Piece::Number { name, style } => {
                    let mut number = to_fixed(self.arg(name)?)?;
                    if *style == NumberStyle::Integer {
                        number.round_with_mode(
                            0,
                            SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfEven),
                        );
                    }
                    out.push_str(&self.number(&number)?);
                }
                Piece::Date { name, length } => {
                    let moment = moment_of(self.arg(name)?, &Value::Unit)?;
                    let spec = TimeFormatSpec::Builtin(BuiltinFormat {
                        part: FormatPart::Date,
                        length: *length,
                    });
                    out.push_str(&format(moment, self.tag, &spec)?);
                }
                Piece::Time { name, length } => {
                    let moment = moment_of(self.arg(name)?, &Value::Unit)?;
                    if moment.zone.is_none() {
                        return Err(Fault::Shape);
                    }
                    let spec = TimeFormatSpec::Builtin(BuiltinFormat {
                        part: FormatPart::Time,
                        length: *length,
                    });
                    out.push_str(&format(moment, self.tag, &spec)?);
                }
                Piece::Count => match count {
                    Some(count) => out.push_str(&self.number(count)?),
                    None => return Err(Fault::Shape),
                },
                Piece::Plural {
                    name,
                    ordinal,
                    offset,
                    cases,
                    other,
                } => {
                    let value = self.arg(name)?;
                    let exact = cases.iter().find(|case| match case.selector {
                        Selector::Exact(n) => equals(value, n),
                        Selector::Category(_) => false,
                    });
                    let shown = to_fixed(&minus(value, *offset)?)?;
                    let chosen = match exact {
                        Some(case) => &case.message,
                        None => {
                            let category = self.category(*ordinal, &shown)?;
                            cases
                                .iter()
                                .find(|case| case.selector == Selector::Category(category))
                                .map_or(other, |case| &case.message)
                        }
                    };
                    self.write(chosen, Some(&shown), out)?;
                }
                Piece::Select { name, cases, other } => {
                    let Value::Text(word) = self.arg(name)? else {
                        return Err(Fault::Shape);
                    };
                    let chosen = cases
                        .iter()
                        .find(|(case, _)| **case == **word)
                        .map_or(other, |(_, message)| message);
                    self.write(chosen, count, out)?;
                }
            }
        }
        Ok(())
    }

    fn number(&mut self, number: &Fixed) -> Result<String, Fault> {
        if self.numbers.is_none() {
            let formatter = DecimalFormatter::try_new((&self.locale).into(), Default::default())
                .map_err(|_| Fault::Shape)?;
            self.numbers = Some(formatter);
        }
        let formatter = self.numbers.as_ref().ok_or(Fault::Shape)?;
        Ok(formatter.format(number).write_to_string().into_owned())
    }

    /// The CLDR plural category of a count in the locale.
    fn category(&mut self, ordinal: bool, count: &Fixed) -> Result<PluralCategory, Fault> {
        let rules = if ordinal {
            &mut self.ordinal
        } else {
            &mut self.cardinal
        };
        if rules.is_none() {
            let built = if ordinal {
                PluralRules::try_new_ordinal((&self.locale).into())
            } else {
                PluralRules::try_new_cardinal((&self.locale).into())
            };
            *rules = Some(built.map_err(|_| Fault::Shape)?);
        }
        let rules = rules.as_ref().ok_or(Fault::Shape)?;
        Ok(match rules.category_for(count) {
            Category::Zero => PluralCategory::Zero,
            Category::One => PluralCategory::One,
            Category::Two => PluralCategory::Two,
            Category::Few => PluralCategory::Few,
            Category::Many => PluralCategory::Many,
            Category::Other => PluralCategory::Other,
        })
    }
}

/// A number as ICU4X holds it, with the places it was written with.
fn to_fixed(value: &Value) -> Result<Fixed, Fault> {
    match value {
        Value::Int(n) => Ok(Fixed::from(*n)),
        Value::Decimal(decimal) => decimal.to_string().parse().map_err(|_| Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

/// Whether a number is exactly `n`.
fn equals(value: &Value, n: i64) -> bool {
    match value {
        Value::Int(seen) => *seen == n,
        Value::Decimal(decimal) => {
            10i128
                .checked_pow(decimal.scale)
                .and_then(|unit| unit.checked_mul(i128::from(n)))
                == Some(decimal.mantissa)
        }
        _ => false,
    }
}

/// A number less a plural's offset.
fn minus(value: &Value, offset: i64) -> Result<Value, Fault> {
    match value {
        Value::Int(n) => n.checked_sub(offset).map(Value::Int).ok_or(Fault::IntOverflow),
        Value::Decimal(decimal) => {
            let unit = 10i128
                .checked_pow(decimal.scale)
                .ok_or(Fault::DecimalOverflow)?;
            let mantissa = unit
                .checked_mul(i128::from(offset))
                .and_then(|taken| decimal.mantissa.checked_sub(taken))
                .ok_or(Fault::DecimalOverflow)?;
            Ok(Value::Decimal(Decimal::new(mantissa, decimal.scale)))
        }
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::locale_ordinal;
    use jadpo_std::translation::{ParamKind, TranslationParam, parse};

    fn key(messages: &[(&str, &str)], params: &[(&str, ParamKind)]) -> TranslationKey {
        TranslationKey {
            name: "k".into(),
            params: params
                .iter()
                .map(|(name, kind)| TranslationParam {
                    name: (*name).into(),
                    kind: *kind,
                })
                .collect(),
            messages: messages
                .iter()
                .map(|(locale, text)| {
                    (
                        locale_ordinal(locale).unwrap_or(0),
                        parse(text).unwrap_or_default(),
                    )
                })
                .collect(),
        }
    }

    fn locale(variant: &str) -> Value {
        Value::Variant(std::rc::Rc::new(crate::Variant {
            ty: jadpo_std::LOCALE.into(),
            name: variant.into(),
            ordinal: locale_ordinal(variant).unwrap_or(0),
            fields: Vec::new(),
        }))
    }

    fn say(messages: &[(&str, &str)], params: &[(&str, ParamKind)], at: &str, args: Vec<Value>) -> String {
        match translate(&key(messages, params), &locale(at), &args) {
            Ok(Value::Text(text)) => text.to_string(),
            other => format!("{other:?}"),
        }
    }

    const PLURAL: &str = "{count, plural, =0 {nothing} one {# item} other {# items}} in your cart";

    #[test]
    fn a_plural_follows_the_locales_rules() {
        let count = [("count", ParamKind::Number)];
        let m = [("en_gb", PLURAL)];
        assert_eq!(say(&m, &count, "en_gb", vec![Value::Int(0)]), "nothing in your cart");
        assert_eq!(say(&m, &count, "en_gb", vec![Value::Int(1)]), "1 item in your cart");
        assert_eq!(say(&m, &count, "en_gb", vec![Value::Int(1234)]), "1,234 items in your cart");
        // French counts 0 and 1 as one, and groups with a narrow no-break space.
        let fr = [("fr_fr", "{count, plural, one {# article} many {# d'articles} other {# articles}}")];
        assert_eq!(say(&fr, &count, "fr_fr", vec![Value::Int(0)]), "0 article");
        assert_eq!(say(&fr, &count, "fr_fr", vec![Value::Int(2)]), "2 articles");
        assert_eq!(say(&fr, &count, "fr_fr", vec![Value::Int(1_000_000)]), "1\u{202f}000\u{202f}000 d'articles");
        // Russian has four forms.
        let ru = [("ru", "{n, plural, one {# книга} few {# книги} many {# книг} other {# книги}}")];
        let n = [("n", ParamKind::Number)];
        assert_eq!(say(&ru, &n, "ru", vec![Value::Int(1)]), "1 книга");
        assert_eq!(say(&ru, &n, "ru", vec![Value::Int(3)]), "3 книги");
        assert_eq!(say(&ru, &n, "ru", vec![Value::Int(5)]), "5 книг");
        assert_eq!(say(&ru, &n, "ru", vec![Value::Int(21)]), "21 книга");
    }

    #[test]
    fn an_offset_is_taken_off_before_the_category_and_the_count() {
        let m = [("en_gb", "{n, plural, offset:1 =0 {nobody} =1 {just you} one {you and # other} other {you and # others}}")];
        let n = [("n", ParamKind::Number)];
        assert_eq!(say(&m, &n, "en_gb", vec![Value::Int(0)]), "nobody");
        assert_eq!(say(&m, &n, "en_gb", vec![Value::Int(1)]), "just you");
        assert_eq!(say(&m, &n, "en_gb", vec![Value::Int(2)]), "you and 1 other");
        assert_eq!(say(&m, &n, "en_gb", vec![Value::Int(4)]), "you and 3 others");
    }

    #[test]
    fn an_ordinal_has_its_own_rules() {
        let m = [("en_gb", "{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}")];
        let n = [("n", ParamKind::Number)];
        let say_n = |n_: i64| say(&m, &n, "en_gb", vec![Value::Int(n_)]);
        assert_eq!([say_n(1), say_n(2), say_n(3), say_n(4), say_n(11), say_n(22)], ["1st", "2nd", "3rd", "4th", "11th", "22nd"]);
    }

    #[test]
    fn numbers_use_the_locales_digits_and_separators() {
        let m = [("en_gb", "{n, number}"), ("de_de", "{n, number}"), ("ar_eg", "{n, number}")];
        let n = [("n", ParamKind::Number)];
        let d = Value::decimal("1234.50").unwrap_or(Value::Unit);
        assert_eq!(say(&m, &n, "en_gb", vec![d.clone()]), "1,234.50");
        assert_eq!(say(&m, &n, "de_de", vec![d.clone()]), "1.234,50");
        assert_eq!(say(&m, &n, "ar_eg", vec![Value::Int(123)]), "\u{661}\u{662}\u{663}");
        let whole = [("en_gb", "{n, number, integer}")];
        assert_eq!(say(&whole, &n, "en_gb", vec![Value::decimal("2.5").unwrap_or(Value::Unit)]), "2");
        assert_eq!(say(&whole, &n, "en_gb", vec![Value::decimal("3.5").unwrap_or(Value::Unit)]), "4");
    }

    #[test]
    fn a_select_picks_its_word_or_other() {
        let m = [("en_gb", "{kind, select, gift {a gift for {who}} other {an order for {who}}}")];
        let p = [("kind", ParamKind::Text), ("who", ParamKind::Text)];
        assert_eq!(say(&m, &p, "en_gb", vec![Value::text("gift"), Value::text("Ana")]), "a gift for Ana");
        assert_eq!(say(&m, &p, "en_gb", vec![Value::text("bulk"), Value::text("Ana")]), "an order for Ana");
    }

    #[test]
    fn a_plural_inside_a_select_keeps_its_count() {
        let m = [("en_gb", "{kind, select, gift {{n, plural, one {# gift} other {# gifts}}} other {{n, plural, one {# order} other {# orders}}}}")];
        let p = [("kind", ParamKind::Text), ("n", ParamKind::Number)];
        assert_eq!(say(&m, &p, "en_gb", vec![Value::text("gift"), Value::Int(2)]), "2 gifts");
    }

    #[test]
    fn a_date_and_a_time_are_the_locales() {
        let m = [("en_gb", "{when, date, long} at {when, time, short}"), ("fr_fr", "{when, date, long} à {when, time, short}")];
        let p = [("when", ParamKind::Time)];
        let zone = jadpo_std::variant_ordinal(jadpo_std::ZONE, "europe_paris").unwrap_or(0);
        let when = Value::Zoned(crate::Zoned { millis: 1_790_000_000_000, zone });
        let en = say(&m, &p, "en_gb", vec![when.clone()]);
        let fr = say(&m, &p, "fr_fr", vec![when]);
        assert!(en.contains(" at "), "{en}");
        assert!(fr.contains(" à "), "{fr}");
        assert_ne!(en, fr);
    }

    #[test]
    fn quoted_braces_and_apostrophes_are_text() {
        let m = [("en_gb", "Use '{name}' in {tool}; don't")];
        let p = [("tool", ParamKind::Text)];
        assert_eq!(say(&m, &p, "en_gb", vec![Value::text("Jadpo")]), "Use {name} in Jadpo; don't");
    }
}
