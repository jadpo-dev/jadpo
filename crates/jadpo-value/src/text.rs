//! The `text` operations (area 19, "Text"), one function each for both
//! executors.
//!
//! Text is counted, cut and padded in Unicode code points, as `max_length`
//! counts it (area 2) and PostgreSQL's `char_length` does (area 19, "Code points"),
//! never in graphemes: `keep_first` can split an emoji with a skin tone,
//! and the text it gives is still valid. White space is Unicode's
//! `White_Space` property; case mapping, collation, normalisation and the
//! marks a slug drops are the pinned ICU4X release's Unicode and CLDR
//! data, so every target gives the same text.

use std::borrow::Cow;
use std::cmp::Ordering;

use icu_casemap::CaseMapper;
use icu_collator::Collator;
use icu_collator::options::CollatorOptions;
use icu_locale::LanguageIdentifier;
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use icu_properties::CodePointMapData;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup};

use crate::Fault;

/// How many code points `value` holds.
pub fn length(value: &str) -> i64 {
    i64::try_from(value.chars().count()).unwrap_or(i64::MAX)
}

/// `value` without Unicode white space at either end.
pub fn trim(value: &str) -> &str {
    // `str::trim` removes exactly the `White_Space` characters.
    value.trim()
}

/// `value` without Unicode white space at its start.
pub fn trim_start(value: &str) -> &str {
    value.trim_start()
}

/// `value` without Unicode white space at its end.
pub fn trim_end(value: &str) -> &str {
    value.trim_end()
}

/// `value` in lower case: Unicode's default case mapping, or `locale`'s
/// (a BCP 47 tag) when one is given, so `İ` becomes `i` in Turkish.
pub fn lower(value: &str, locale: Option<&str>) -> Result<String, Fault> {
    let id = language(locale)?;
    Ok(CaseMapper::new()
        .lowercase_to_string(value, &id)
        .into_owned())
}

/// `value` in upper case, as [`lower`] maps it: `i` becomes `İ` in
/// Turkish, and `ß` becomes `SS`, so the result can be longer.
pub fn upper(value: &str, locale: Option<&str>) -> Result<String, Fault> {
    let id = language(locale)?;
    Ok(CaseMapper::new()
        .uppercase_to_string(value, &id)
        .into_owned())
}

/// The language case mapping follows: the root's without a locale.
fn language(locale: Option<&str>) -> Result<LanguageIdentifier, Fault> {
    match locale {
        None => Ok(LanguageIdentifier::UNKNOWN),
        Some(tag) => crate::present::locale_of(tag).map(|locale| locale.id),
    }
}

/// Whether `part` appears in `value`, code point for code point.
pub fn contains(value: &str, part: &str) -> bool {
    value.contains(part)
}

/// `value` with each code point simply case-folded (CaseFolding.txt's
/// `C` and `S` entries): one code point for one, so `ß` stays `ß` and the
/// text keeps its length. What `contains_ignoring_case` and
/// `equals_ignoring_case` compare, and what a query's search finds
/// (area 6, "Searching inside text").
pub fn folded(value: &str) -> String {
    let mapper = CaseMapper::new();
    value.chars().map(|c| mapper.simple_fold(c)).collect()
}

/// Whether `part` is in `value` when both are [`folded`].
pub fn contains_ignoring_case(value: &str, part: &str) -> bool {
    folded(value).contains(&folded(part))
}

/// Whether `value` and `other` are equal when both are [`folded`].
pub fn equals_ignoring_case(value: &str, other: &str) -> bool {
    folded(value) == folded(other)
}

/// The code-point position `part` first starts at in `value`, from 0.
pub fn index_of(value: &str, part: &str) -> Option<i64> {
    value.find(part).map(|at| length(&value[..at]))
}

/// The code-point position `part` last starts at in `value`, from 0;
/// `""` is last at the end.
pub fn last_index_of(value: &str, part: &str) -> Option<i64> {
    value.rfind(part).map(|at| length(&value[..at]))
}

pub fn starts_with(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
}

pub fn ends_with(value: &str, suffix: &str) -> bool {
    value.ends_with(suffix)
}

/// `value` with every `find` replaced by `with`, left to right. `find` is
/// never empty, which the checker proves.
pub fn replace(value: &str, find: &str, with: &str) -> Result<String, Fault> {
    if find.is_empty() {
        return Err(Fault::Shape);
    }
    Ok(value.replace(find, with))
}

/// `value` cut at each `separator`: at least one piece, `""` giving
/// `[""]`, and separators side by side giving `""` between them.
/// `separator` is never empty, which the checker proves.
pub fn split<'a>(value: &'a str, separator: &str) -> Result<Vec<&'a str>, Fault> {
    if separator.is_empty() {
        return Err(Fault::Shape);
    }
    Ok(value.split(separator).collect())
}

/// The pieces with `separator` between them.
pub fn join<'a>(pieces: impl IntoIterator<Item = &'a str>, separator: &str) -> String {
    let mut out = String::new();
    for (at, piece) in pieces.into_iter().enumerate() {
        if at > 0 {
            out.push_str(separator);
        }
        out.push_str(piece);
    }
    out
}

/// A count the checker proves is never negative.
fn count(n: i64) -> Result<usize, Fault> {
    usize::try_from(n).map_err(|_| Fault::Shape)
}

/// Up to `count` code points from position `from`, cut short at the end.
pub fn slice(value: &str, from: i64, count_of: i64) -> Result<String, Fault> {
    let (from, count_of) = (count(from)?, count(count_of)?);
    Ok(value.chars().skip(from).take(count_of).collect())
}

/// At most `n` code points from the start.
pub fn keep_first(value: &str, n: i64) -> Result<String, Fault> {
    Ok(value.chars().take(count(n)?).collect())
}

/// At most `n` code points from the end.
pub fn keep_last(value: &str, n: i64) -> Result<String, Fault> {
    let n = count(n)?;
    let total = value.chars().count();
    Ok(value.chars().skip(total.saturating_sub(n)).collect())
}

/// `value` without its first `n` code points; empty when `n` is longer.
pub fn drop_first(value: &str, n: i64) -> Result<String, Fault> {
    Ok(value.chars().skip(count(n)?).collect())
}

/// `value` without its last `n` code points; empty when `n` is longer.
pub fn drop_last(value: &str, n: i64) -> Result<String, Fault> {
    let n = count(n)?;
    let total = value.chars().count();
    Ok(value.chars().take(total.saturating_sub(n)).collect())
}

/// `value` with `fill`, one character, added at the start until it is
/// `length` code points long; never cut, so text already that long, or
/// a `length` below its own, is given back as it is.
pub fn pad_start(value: &str, length: i64, fill: &str) -> Result<String, Fault> {
    let mut chars = fill.chars();
    let (Some(fill), None) = (chars.next(), chars.next()) else {
        return Err(Fault::Shape);
    };
    let has = value.chars().count();
    let wanted = usize::try_from(length).unwrap_or(0);
    let mut out = String::new();
    for _ in has..wanted {
        out.push(fill);
    }
    out.push_str(value);
    Ok(out)
}

/// `value` with `fill`, one character, added at the end until it is
/// `length` code points long; never cut, as [`pad_start`].
pub fn pad_end(value: &str, length: i64, fill: &str) -> Result<String, Fault> {
    let mut chars = fill.chars();
    let (Some(fill), None) = (chars.next(), chars.next()) else {
        return Err(Fault::Shape);
    };
    let has = value.chars().count();
    let wanted = usize::try_from(length).unwrap_or(0);
    let mut out = String::from(value);
    for _ in has..wanted {
        out.push(fill);
    }
    Ok(out)
}

/// What a cut text ends in: one code point, counted in its length.
pub const ELLIPSIS: char = '\u{2026}';

/// `value` when it is at most `length` code points, else its first
/// `length - 1` and [`ELLIPSIS`], or `""` for a `length` of 0: never
/// longer than `length` or than `value`.
pub fn truncate(value: &str, length: i64) -> Result<String, Fault> {
    let length = count(length)?;
    if value.chars().count() <= length {
        return Ok(value.to_string());
    }
    if length == 0 {
        return Ok(String::new());
    }
    let mut out: String = value.chars().take(length - 1).collect();
    out.push(ELLIPSIS);
    Ok(out)
}

/// `value` in Unicode's normalisation form `form`, `nfc` or `nfkc`
/// (`NormalForm`'s variants), by the pinned ICU4X release's data.
pub fn normalise(value: &str, form: &str) -> Result<String, Fault> {
    Ok(match form {
        "nfc" => ComposingNormalizerBorrowed::new_nfc()
            .normalize(value)
            .into_owned(),
        "nfkc" => ComposingNormalizerBorrowed::new_nfkc()
            .normalize(value)
            .into_owned(),
        _ => return Err(Fault::Shape),
    })
}

/// `value` as a URL's path segment: lowercase ASCII letters, digits and
/// single `-`. Accents are dropped (the marks Unicode's canonical
/// decomposition separates from their letters), every other run of
/// characters becomes one `-`, and none is left at either end; it may
/// be empty. ASCII only (question 26).
pub fn slug(value: &str) -> String {
    let decomposed = DecomposingNormalizerBorrowed::new_nfd().normalize(value);
    let categories = CodePointMapData::<GeneralCategory>::new();
    let mut out = String::new();
    let mut gap = false;
    for c in decomposed.chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        } else if !GeneralCategoryGroup::Mark.contains(categories.get(c)) {
            gap = true;
        }
    }
    out
}

/// `value` in Unicode's composed form, NFC: what text decoded at a
/// boundary is turned into, so text that looks equal is equal (areas 2
/// and 7). Text already composed, which is nearly all of it, is borrowed.
pub fn composed(value: &str) -> Cow<'_, str> {
    ComposingNormalizerBorrowed::new_nfc().normalize(value)
}

/// A comparison of text by `locale`'s collation, its CLDR rules at their
/// default strength, for `list.sort_by`'s `locale:` (question 26).
pub fn collation(locale: &str) -> Result<impl Fn(&str, &str) -> Ordering, Fault> {
    let locale = crate::present::locale_of(locale)?;
    let collator = Collator::try_new((&locale).into(), CollatorOptions::default())
        .map_err(|_| Fault::Shape)?;
    Ok(move |a: &str, b: &str| collator.compare(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_cuts_in_code_points() {
        // "é" written as `e` and a combining acute is two code points.
        assert_eq!(length("cafe\u{301}"), 5);
        assert_eq!(length("café"), 4);
        assert_eq!(length("👍🏽"), 2);
        assert_eq!(length("面包"), 2);
        assert_eq!(keep_first("👍🏽ok", 1).unwrap(), "👍");
        assert_eq!(keep_last("cafe\u{301}", 1).unwrap(), "\u{301}");
        assert_eq!(drop_last("面包店", 1).unwrap(), "面包");
        assert_eq!(slice("面包店", 1, 9).unwrap(), "包店");
    }

    #[test]
    fn slugs_are_ascii() {
        assert_eq!(slug("  Crème Brûlée!! "), "creme-brulee");
        assert_eq!(slug("Pain---au  chocolat"), "pain-au-chocolat");
        assert_eq!(slug("面包"), "");
        assert_eq!(slug("Straße 9"), "stra-e-9");
    }

    #[test]
    fn locale_case_and_order() {
        assert_eq!(upper("istanbul", Some("tr-TR")).unwrap(), "İSTANBUL");
        assert_eq!(upper("istanbul", None).unwrap(), "ISTANBUL");
        assert_eq!(lower("İ", Some("tr-TR")).unwrap(), "i");
        assert_eq!(lower("İ", None).unwrap(), "i\u{307}");
        assert_eq!(upper("straße", None).unwrap(), "STRASSE");
        let swedish = collation("sv-SE").unwrap();
        let german = collation("de-DE").unwrap();
        assert_eq!(swedish("ö", "z"), Ordering::Greater);
        assert_eq!(german("ö", "z"), Ordering::Less);
    }

    #[test]
    fn ignores_case_by_simple_folding() {
        assert!(contains_ignoring_case("Buy MILK", "milk"));
        assert!(contains_ignoring_case("ÉCLAIR", "éclair"));
        assert!(contains_ignoring_case("ΟΔΟΣ", "οδος"));
        assert!(contains_ignoring_case("\u{212a}elvin", "kelvin"));
        assert!(contains_ignoring_case("anything", ""));
        assert!(!contains_ignoring_case("Straße", "STRASSE"));
        assert!(!contains_ignoring_case("50% off", "5000"));
        assert!(equals_ignoring_case("Rye Loaf", "rYE lOAF"));
        assert!(!equals_ignoring_case("Rye", "Rye loaf"));
        assert_eq!(folded("İ"), "İ");
    }

    /// The ASCII letters another code point folds to, which a query's
    /// search pattern matches with `_` (`jadpo_std::search::substring`):
    /// only `k` (the Kelvin sign) and `s` (`ſ`), in the pinned Unicode
    /// data.
    #[test]
    fn only_k_and_s_are_folded_to_from_outside_ascii() {
        let mapper = CaseMapper::new();
        let mut targets: Vec<char> = (0x80..=0x10ffff_u32)
            .filter_map(char::from_u32)
            .map(|c| mapper.simple_fold(c))
            .filter(char::is_ascii)
            .collect();
        targets.sort_unstable();
        targets.dedup();
        assert_eq!(targets, ['k', 's']);
        assert_eq!(
            jadpo_std::search::FOLDED_INTO_ASCII,
            &['k', 's'],
            "the search pattern's list"
        );
        // Every ASCII letter folds to its lower case, and nothing else.
        for c in (0..0x80_u8).map(char::from) {
            assert_eq!(mapper.simple_fold(c), c.to_ascii_lowercase());
        }
    }

    #[test]
    fn finds_trims_pads_and_cuts_in_code_points() {
        assert_eq!(index_of("rye and rye", "rye"), Some(0));
        assert_eq!(last_index_of("rye and rye", "rye"), Some(8));
        assert_eq!(index_of("面包店", "店"), Some(2));
        assert_eq!(index_of("rye", "oat"), None);
        assert_eq!(index_of("rye", ""), Some(0));
        assert_eq!(last_index_of("rye", ""), Some(3));
        assert_eq!(last_index_of("aaa", "aa"), Some(1));
        assert_eq!(trim_start("\u{3000} rye "), "rye ");
        assert_eq!(trim_end(" rye\u{a0}"), " rye");
        assert_eq!(pad_end("rye", 6, ".").unwrap(), "rye...");
        assert_eq!(pad_end("rye", -1, ".").unwrap(), "rye");
        assert_eq!(truncate("Seeded sourdough", 10).unwrap(), "Seeded so…");
        assert_eq!(truncate("Rye", 3).unwrap(), "Rye");
        assert_eq!(truncate("Rye", 0).unwrap(), "");
        assert_eq!(truncate("Rye", 1).unwrap(), "…");
        assert_eq!(normalise("cafe\u{301}", "nfc").unwrap(), "café");
        assert_eq!(normalise("\u{fb01}ne", "nfkc").unwrap(), "fine");
        assert_eq!(normalise("\u{fb01}ne", "nfc").unwrap(), "\u{fb01}ne");
    }

    #[test]
    fn composes() {
        assert_eq!(composed("cafe\u{301}"), "café");
        assert!(matches!(composed("plain"), Cow::Borrowed(_)));
    }
}
