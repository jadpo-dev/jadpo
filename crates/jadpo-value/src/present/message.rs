//! A translation's wording in a locale (area 10, "Translations as
//! messages"): the pieces of a plain string written out with the arguments a
//! call gave, over the same CLDR data as `text.format`. A plural form is
//! decided by CLDR's plural rules for the locale, a number is written with
//! the locale's digits, grouping and decimal separator, so the interpreter,
//! a native deployable and WebAssembly write the same text.
//!
//! A locale whose translations are in progress falls back where it has no
//! wording: to its regional sibling, then the default locale. The words
//! come from the locale that has them and are written in that locale, so
//! the sentence stays in one language.

use fixed_decimal::Decimal as Fixed;
use icu_decimal::DecimalFormatter;
use icu_plurals::{PluralCategory as Category, PluralRules};
use jadpo_std::LocalePlan;
use jadpo_std::translation::{
    ParamKind, Piece, PluralCategory, Template, TranslationEntry, Wording,
};
use writeable::Writeable;

use super::locale_of;
use crate::{Fault, Value};

/// `translations.<domain>.<message>(locale:, ...)`: the entry's wording in
/// the locale given (or what an in-progress locale falls back to), with
/// `args` in the order of the entry's parameters.
pub fn translate(
    entry: &TranslationEntry,
    locale: &Value,
    args: &[Value],
    plan: &LocalePlan,
) -> Result<Value, Fault> {
    let Value::Variant(variant) = locale else {
        return Err(Fault::Shape);
    };
    if &*variant.ty != jadpo_std::LOCALE {
        return Err(Fault::Shape);
    }
    if args.len() != entry.params.len() {
        return Err(Fault::Shape);
    }
    let (source, wording) = crate::locale::fallback_chain(variant.ordinal, plan)
        .into_iter()
        .find_map(|at| Some((at, entry.wording(at)?)))
        .ok_or(Fault::Shape)?;
    let tag = jadpo_std::LOCALES
        .get(source as usize)
        .map(|(_, tag)| *tag)
        .ok_or(Fault::Shape)?;
    let named: Vec<(&str, ParamKind, &Value)> = entry
        .params
        .iter()
        .zip(args)
        .map(|(param, value)| (param.name.as_str(), param.kind, value))
        .collect();
    // The pseudo-locale has no wording of its own unless a file gives some:
    // it writes the default locale's, accented and lengthened.
    let pseudo = LocalePlan::is_pseudo(variant.ordinal) && source != variant.ordinal;
    let mut writer = Writer::new(tag, &named)?;
    writer.pseudo = pseudo;
    let mut out = String::new();
    match wording {
        Wording::Plain(template) => writer.write(template, None, &mut out)?,
        Wording::Variants(variants) => {
            let chosen = named
                .iter()
                .find_map(|(_, kind, value)| match (kind, value) {
                    (ParamKind::Choice, Value::Variant(chosen)) => Some(chosen.clone()),
                    _ => None,
                })
                .ok_or(Fault::Shape)?;
            let template = variants
                .iter()
                .find(|(name, _)| **name == *chosen.name)
                .map(|(_, template)| template)
                .ok_or(Fault::Shape)?;
            writer.write(template, Some(&chosen.fields), &mut out)?;
        }
        Wording::Forms(forms) => {
            let count = named
                .iter()
                .find(|(_, kind, _)| *kind == ParamKind::Plural)
                .map(|(_, _, value)| to_fixed(value))
                .ok_or(Fault::Shape)??;
            let category = writer.category(&count)?;
            let template = forms
                .iter()
                .find(|(form, _)| *form == category)
                .or_else(|| {
                    forms
                        .iter()
                        .find(|(form, _)| *form == PluralCategory::Other)
                })
                .map(|(_, template)| template)
                .ok_or(Fault::Shape)?;
            writer.write(template, None, &mut out)?;
        }
    }
    if pseudo {
        out = super::pseudo::frame(&out);
    }
    Ok(Value::text(&out))
}

/// Writes wordings in one locale, building each formatter when first
/// needed.
struct Writer<'a> {
    locale: icu_locale::Locale,
    args: &'a [(&'a str, ParamKind, &'a Value)],
    numbers: Option<DecimalFormatter>,
    cardinal: Option<PluralRules>,
    /// Whether the wording's own text is written accented (the values it
    /// writes are not).
    pseudo: bool,
}

/// The payload of the enum value a wording is for, whose fields are
/// placeholders beside the call's own.
type Payload<'a> = Option<&'a [(std::rc::Rc<str>, Value)]>;

impl<'a> Writer<'a> {
    fn new(tag: &str, args: &'a [(&'a str, ParamKind, &'a Value)]) -> Result<Writer<'a>, Fault> {
        Ok(Writer {
            locale: locale_of(tag)?,
            args,
            numbers: None,
            cardinal: None,
            pseudo: false,
        })
    }

    fn write(
        &mut self,
        template: &Template,
        payload: Payload<'_>,
        out: &mut String,
    ) -> Result<(), Fault> {
        for piece in &template.pieces {
            match piece {
                Piece::Text(text) if self.pseudo => out.push_str(&super::pseudo::accent(text)),
                Piece::Text(text) => out.push_str(text),
                Piece::Field(name) => {
                    let found = self
                        .args
                        .iter()
                        .find(|(seen, kind, _)| seen == name && kind.is_placeholder())
                        .map(|(_, _, value)| *value)
                        .or_else(|| {
                            payload?
                                .iter()
                                .find(|(field, _)| &**field == name)
                                .map(|(_, value)| value)
                        })
                        .ok_or(Fault::Shape)?;
                    match found {
                        Value::Text(text) => out.push_str(text),
                        Value::Int(_) | Value::Decimal(_) => {
                            let shown = self.number(&to_fixed(found)?)?;
                            out.push_str(&shown);
                        }
                        _ => return Err(Fault::Shape),
                    }
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
    fn category(&mut self, count: &Fixed) -> Result<PluralCategory, Fault> {
        if self.cardinal.is_none() {
            let rules =
                PluralRules::try_new_cardinal((&self.locale).into()).map_err(|_| Fault::Shape)?;
            self.cardinal = Some(rules);
        }
        let rules = self.cardinal.as_ref().ok_or(Fault::Shape)?;
        Ok(category_of(rules.category_for(count)))
    }
}

fn category_of(category: Category) -> PluralCategory {
    match category {
        Category::Zero => PluralCategory::Zero,
        Category::One => PluralCategory::One,
        Category::Two => PluralCategory::Two,
        Category::Few => PluralCategory::Few,
        Category::Many => PluralCategory::Many,
        Category::Other => PluralCategory::Other,
    }
}

/// A number for people in the locale tagged `tag` (area 19, "Numbers"):
/// `text.format` on an `Int` or a `Decimal`, written as a translation's
/// `{n}` is, with the locale's digits, grouping and decimal separator, and
/// every place the value keeps.
pub fn number(value: &Value, tag: &str) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    let formatter = DecimalFormatter::try_new((&locale).into(), Default::default())
        .map_err(|_| Fault::Shape)?;
    Ok(formatter
        .format(&to_fixed(value)?)
        .write_to_string()
        .into_owned())
}

/// A number as ICU4X holds it, with the places it was written with.
fn to_fixed(value: &Value) -> Result<Fixed, Fault> {
    match value {
        Value::Int(n) => Ok(Fixed::from(*n)),
        Value::Decimal(decimal) => decimal.to_string().parse().map_err(|_| Fault::Shape),
        _ => Err(Fault::Shape),
    }
}

/// The plural forms ICU4X's CLDR data gives the locale tagged `tag`.
#[cfg(test)]
fn icu_categories(tag: &str) -> Vec<PluralCategory> {
    let Ok(locale) = locale_of(tag) else {
        return Vec::new();
    };
    let Ok(rules) = PluralRules::try_new_cardinal((&locale).into()) else {
        return Vec::new();
    };
    let mut found: Vec<PluralCategory> = rules.categories().map(category_of).collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::locale_ordinal;
    use jadpo_std::translation::{TranslationParam, parse_template};

    fn template(text: &str) -> Template {
        match parse_template(text) {
            Ok((template, _)) => template,
            Err(error) => panic!("{text}: {error:?}"),
        }
    }

    fn ordinal(variant: &str) -> u32 {
        locale_ordinal(variant).unwrap_or(0)
    }

    fn locale(variant: &str) -> Value {
        Value::variant(jadpo_std::LOCALE, ordinal(variant), variant, Vec::new())
    }

    fn plan(default: &str, supported: &[&str], in_progress: &[&str]) -> LocalePlan {
        LocalePlan {
            default: ordinal(default),
            supported: supported.iter().map(|v| ordinal(v)).collect(),
            in_progress: in_progress.iter().map(|v| ordinal(v)).collect(),
        }
    }

    fn entry(params: &[(&str, ParamKind)], wordings: Vec<(&str, Wording)>) -> TranslationEntry {
        TranslationEntry {
            key: "k".into(),
            params: params
                .iter()
                .map(|(name, kind)| TranslationParam {
                    name: (*name).into(),
                    kind: *kind,
                })
                .collect(),
            wordings: wordings
                .into_iter()
                .map(|(at, wording)| (ordinal(at), wording))
                .collect(),
        }
    }

    fn say(entry: &TranslationEntry, at: &str, plan: &LocalePlan, args: Vec<Value>) -> String {
        match translate(entry, &locale(at), &args, plan) {
            Ok(Value::Text(text)) => text.to_string(),
            other => format!("{other:?}"),
        }
    }

    fn forms(list: &[(PluralCategory, &str)]) -> Wording {
        Wording::Forms(list.iter().map(|(c, t)| (*c, template(t))).collect())
    }

    #[test]
    fn a_plain_wording_writes_its_fields() {
        let e = entry(
            &[("name", ParamKind::Text), ("n", ParamKind::Number)],
            vec![(
                "en_gb",
                Wording::Plain(template("Hi {name}, you have {n} things; {{done}}.")),
            )],
        );
        let p = plan("en_gb", &["en_gb"], &[]);
        assert_eq!(
            say(&e, "en_gb", &p, vec![Value::text("Ana"), Value::Int(1234)]),
            "Hi Ana, you have 1,234 things; {done}."
        );
    }

    #[test]
    fn a_plural_follows_the_locales_rules() {
        use PluralCategory::{Few, Many, One, Other};
        let count = [("count", ParamKind::Plural)];
        let e = entry(
            &count,
            vec![
                (
                    "en_gb",
                    forms(&[(One, "{count} item"), (Other, "{count} items")]),
                ),
                (
                    "fr_fr",
                    forms(&[
                        (One, "{count} article"),
                        (Many, "{count} d'articles"),
                        (Other, "{count} articles"),
                    ]),
                ),
                (
                    "ru",
                    forms(&[
                        (One, "{count} книга"),
                        (Few, "{count} книги"),
                        (Many, "{count} книг"),
                        (Other, "{count} книги"),
                    ]),
                ),
            ],
        );
        let p = plan("en_gb", &["en_gb", "fr_fr", "ru"], &[]);
        let n = |at: &str, n: i64| say(&e, at, &p, vec![Value::Int(n)]);
        assert_eq!(n("en_gb", 0), "0 items");
        assert_eq!(n("en_gb", 1), "1 item");
        assert_eq!(n("en_gb", 1234), "1,234 items");
        // French counts 0 and 1 as one, and groups with a narrow no-break space.
        assert_eq!(n("fr_fr", 0), "0 article");
        assert_eq!(n("fr_fr", 2), "2 articles");
        assert_eq!(n("fr_fr", 1_000_000), "1\u{202f}000\u{202f}000 d'articles");
        // Russian has four forms.
        assert_eq!(n("ru", 1), "1 книга");
        assert_eq!(n("ru", 3), "3 книги");
        assert_eq!(n("ru", 5), "5 книг");
        assert_eq!(n("ru", 21), "21 книга");
    }

    #[test]
    fn an_enums_value_picks_the_wording_and_its_fields_are_placeholders() {
        let e = entry(
            &[("status", ParamKind::Choice)],
            vec![(
                "en_gb",
                Wording::Variants(vec![
                    ("pending".into(), template("Waiting")),
                    ("held".into(), template("Held: {reason}")),
                ]),
            )],
        );
        let p = plan("en_gb", &["en_gb"], &[]);
        let held = Value::variant(
            "Status",
            1,
            "held",
            vec![("reason".into(), Value::text("no stock"))],
        );
        let pending = Value::variant("Status", 0, "pending", Vec::new());
        assert_eq!(say(&e, "en_gb", &p, vec![held]), "Held: no stock");
        assert_eq!(say(&e, "en_gb", &p, vec![pending]), "Waiting");
    }

    #[test]
    fn a_number_alone_is_written_as_a_wording_writes_it() {
        let d = Value::decimal("-1234.50").unwrap_or(Value::Unit);
        assert_eq!(number(&d, "en-GB").as_deref(), Ok("-1,234.50"));
        assert_eq!(number(&d, "de-DE").as_deref(), Ok("-1.234,50"));
        assert_eq!(number(&Value::Int(7), "en-GB").as_deref(), Ok("7"));
        assert_eq!(number(&Value::text("7"), "en-GB"), Err(Fault::Shape));
    }

    #[test]
    fn numbers_use_the_locales_digits_and_separators() {
        let e = entry(
            &[("n", ParamKind::Number)],
            vec![
                ("en_gb", Wording::Plain(template("{n}"))),
                ("de_de", Wording::Plain(template("{n}"))),
                ("ar_eg", Wording::Plain(template("{n}"))),
            ],
        );
        let p = plan("en_gb", &["en_gb", "de_de", "ar_eg"], &[]);
        let d = Value::decimal("1234.50").unwrap_or(Value::Unit);
        assert_eq!(say(&e, "en_gb", &p, vec![d.clone()]), "1,234.50");
        assert_eq!(say(&e, "de_de", &p, vec![d]), "1.234,50");
        assert_eq!(
            say(&e, "ar_eg", &p, vec![Value::Int(123)]),
            "\u{661}\u{662}\u{663}"
        );
    }

    #[test]
    fn an_in_progress_locale_falls_back_to_its_sibling_then_the_default() {
        let e = entry(
            &[],
            vec![
                ("en_gb", Wording::Plain(template("Hello"))),
                ("fr_fr", Wording::Plain(template("Bonjour"))),
            ],
        );
        let p = plan(
            "en_gb",
            &["en_gb", "fr_fr", "fr_ca", "pl_pl"],
            &["fr_ca", "pl_pl"],
        );
        // `fr_ca` has no wording: its sibling `fr_fr` has.
        assert_eq!(say(&e, "fr_ca", &p, vec![]), "Bonjour");
        // `pl_pl` has no sibling: the default.
        assert_eq!(say(&e, "pl_pl", &p, vec![]), "Hello");
        // Its own wording wins when it has any.
        let e = entry(
            &[],
            vec![
                ("en_gb", Wording::Plain(template("Hello"))),
                ("fr_ca", Wording::Plain(template("Allo"))),
                ("fr_fr", Wording::Plain(template("Bonjour"))),
            ],
        );
        assert_eq!(say(&e, "fr_ca", &p, vec![]), "Allo");
    }

    #[test]
    fn the_pseudo_locale_accents_and_lengthens_the_default_wording() {
        use PluralCategory::{One, Other};
        let p = plan("en_gb", &["en_gb", "fr_fr", "en_xa"], &["en_xa"]);
        let e = entry(
            &[("name", ParamKind::Text), ("n", ParamKind::Number)],
            vec![
                ("en_gb", Wording::Plain(template("Hi {name}, {n} things."))),
                ("fr_fr", Wording::Plain(template("Salut {name}"))),
            ],
        );
        // The values are not words: they are written as they are, in the
        // default locale's way.
        assert_eq!(
            say(&e, "en_xa", &p, vec![Value::text("Ana"), Value::Int(1234)]),
            "[Ĥí Ana, 1,234 ţĥíñĝš. ~~~~~~~~~]"
        );
        // A plural form is the default locale's rule.
        let count = [("count", ParamKind::Plural)];
        let e = entry(
            &count,
            vec![(
                "en_gb",
                forms(&[(One, "{count} item"), (Other, "{count} items")]),
            )],
        );
        assert_eq!(say(&e, "en_xa", &p, vec![Value::Int(1)]), "[1 íţéɱ ~~~]");
        assert_eq!(say(&e, "en_xa", &p, vec![Value::Int(2)]), "[2 íţéɱš ~~~]");
        // Its own wording, when it has any, is written as it is.
        let own = entry(
            &[],
            vec![
                ("en_gb", Wording::Plain(template("Hello"))),
                ("en_xa", Wording::Plain(template("Hullo"))),
            ],
        );
        assert_eq!(say(&own, "en_xa", &p, vec![]), "Hullo");
    }

    #[test]
    fn the_plural_forms_the_checker_knows_are_icus() {
        for (variant, tag) in jadpo_std::LOCALES {
            let at = ordinal(variant);
            assert_eq!(
                jadpo_std::translation::plural_categories(at),
                icu_categories(tag),
                "{tag}"
            );
        }
    }
}
