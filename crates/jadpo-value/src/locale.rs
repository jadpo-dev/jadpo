//! `locale.from_accept_language` (area 10, "Locales").

use icu_locale::{LanguageIdentifier, LocaleExpander};
use jadpo_std::LocalePlan;

use crate::Value;

/// The `Locale` variant at `ordinal`.
pub fn value(ordinal: u32) -> Value {
    let name = jadpo_std::LOCALES
        .get(ordinal as usize)
        .map_or("", |(name, _)| *name);
    Value::variant(jadpo_std::LOCALE, ordinal, name, Vec::new())
}

/// The way the text of the locale at `ordinal` runs, as a `TextDirection`
/// value (area 10, "Locales"): CLDR's likely scripts decide, so a locale
/// written in a right-to-left script such as `ar_eg`, `he_il` or `fa_ir`
/// reads right to left, and every other left to right.
pub fn direction(ordinal: u32) -> Value {
    let tag = jadpo_std::LOCALES
        .get(ordinal as usize)
        .map(|(_, tag)| tag.replace('_', "-"))
        .unwrap_or_default();
    let right_to_left = tag
        .parse::<LanguageIdentifier>()
        .ok()
        .is_some_and(|id| icu_locale::LocaleDirectionality::new_common().is_right_to_left(&id));
    let (ordinal, name) = if right_to_left {
        (1, jadpo_std::TextDirection::RightToLeft.name())
    } else {
        (0, jadpo_std::TextDirection::LeftToRight.name())
    };
    Value::variant(jadpo_std::TextDirection::NAME, ordinal, name, Vec::new())
}

/// The place, among `Locale`'s variants, of the supported locale `header`
/// asks for (area 10, "Locales").
///
/// The header is read as RFC 9110 reads `Accept-Language`: entries are
/// split at commas, a weight is `;q=` and up to three digits, an entry that
/// does not parse is skipped, a weight of zero excludes its tag and `*`
/// names nothing. The tags are then taken by descending weight, the
/// header's own order breaking ties, and the first that matches a supported
/// locale decides: an exact match (case and `-` or `_` aside), else a
/// supported locale of the same language, the one CLDR's likely subtags
/// give for the language (`en` is `en-US`) before any other, and any other
/// in the order `supported:` lists them. With none matching, or no header,
/// it is `default:`.
pub fn from_accept_language(header: Option<&str>, plan: &LocalePlan) -> u32 {
    let mut ranges = header.map(ranges).unwrap_or_default();
    // A stable sort, so equal weights keep the order the header gives.
    ranges.sort_by_key(|range| std::cmp::Reverse(range.1));
    ranges
        .iter()
        .find_map(|(tag, _)| supported_for(tag, plan))
        .unwrap_or(plan.default)
}

/// The language ranges of `header` that can match something, each with its
/// weight in thousandths, in the order written.
fn ranges(header: &str) -> Vec<(String, u16)> {
    header.split(',').filter_map(entry).collect()
}

/// One list entry: `language-range [ OWS ";" OWS "q=" qvalue ]`. `None`
/// for an empty one, a malformed one, `*` and a weight of zero.
fn entry(entry: &str) -> Option<(String, u16)> {
    let entry = entry.trim_matches(is_space);
    let (range, q) = match entry.split_once(';') {
        Some((range, rest)) => {
            let rest = rest.trim_start_matches(is_space);
            let value = rest
                .get(..2)
                .filter(|q| q.eq_ignore_ascii_case("q="))
                .and_then(|_| rest.get(2..))?;
            (range.trim_end_matches(is_space), q_value(value)?)
        }
        None => (entry, 1000),
    };
    if range == "*" || q == 0 || !is_range(range) {
        return None;
    }
    Some((range.replace('_', "-").to_ascii_lowercase(), q))
}

fn is_space(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// `qvalue = ( "0" [ "." 0*3DIGIT ] ) / ( "1" [ "." 0*3("0") ] )`, in
/// thousandths.
fn q_value(text: &str) -> Option<u16> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, fraction),
        None => (text, ""),
    };
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut thousandths: u16 = 0;
    for place in 0..3 {
        let digit = fraction
            .as_bytes()
            .get(place)
            .map_or(0, |b| u16::from(b - b'0'));
        thousandths = thousandths * 10 + digit;
    }
    match whole {
        "0" => Some(thousandths),
        "1" if thousandths == 0 => Some(1000),
        _ => None,
    }
}

/// `1*8ALPHA *( "-" 1*8alphanum )`, with `_` read as `-`: how browsers
/// write a language range, and what a client that writes `en_GB` means.
fn is_range(range: &str) -> bool {
    let mut parts = range.split(['-', '_']);
    let first = parts.next().unwrap_or("");
    (1..=8).contains(&first.len())
        && first.bytes().all(|b| b.is_ascii_alphabetic())
        && parts.all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|b| b.is_ascii_alphanumeric())
        })
}

/// The locales a wording is looked for in, in order, for a call in the
/// locale at `ordinal` (area 10, "Translations as messages"). A complete
/// locale has every wording itself, so only it. One whose translations are
/// in progress goes on to its regional siblings, the supported locales of
/// the same language, the one CLDR's likely subtags give for the language
/// first (`fr_fr` for `fr_ca`) and the rest in the order `supported:` lists
/// them, and then to the default locale.
pub fn fallback_chain(ordinal: u32, plan: &LocalePlan) -> Vec<u32> {
    let mut chain = vec![ordinal];
    if !plan.in_progress.contains(&ordinal) {
        return chain;
    }
    let tag_of = |at: u32| jadpo_std::LOCALES.get(at as usize).map(|(_, tag)| *tag);
    let language_of = |tag: &str| tag.split('-').next().unwrap_or(tag).to_ascii_lowercase();
    if let Some(own) = tag_of(ordinal) {
        let language = language_of(own);
        let expander = LocaleExpander::new_extended();
        let maximal = |text: &str| {
            let mut id = LanguageIdentifier::try_from_str(text).ok()?;
            expander.maximize(&mut id);
            Some((id.script, id.region))
        };
        let likely = maximal(&language);
        let mut siblings: Vec<u32> = plan
            .supported
            .iter()
            .copied()
            .filter(|&at| at != ordinal)
            .filter(|&at| tag_of(at).is_some_and(|tag| language_of(tag) == language))
            .collect();
        // A stable sort puts the likely one first and keeps the rest in
        // the order they are listed.
        siblings.sort_by_key(|&at| {
            let is_likely = likely.is_some() && tag_of(at).map(maximal).unwrap_or(None) == likely;
            !is_likely
        });
        chain.extend(siblings);
    }
    chain.push(plan.default);
    let mut seen = Vec::new();
    chain.retain(|at| {
        let first = !seen.contains(at);
        seen.push(*at);
        first
    });
    chain
}

/// The supported locale a tag (lower case, `-` separated) matches.
fn supported_for(tag: &str, plan: &LocalePlan) -> Option<u32> {
    let tag_of = |ordinal: u32| {
        jadpo_std::LOCALES
            .get(ordinal as usize)
            .map(|(_, tag)| *tag)
    };
    if let Some(exact) = plan
        .supported
        .iter()
        .copied()
        .find(|&o| tag_of(o).is_some_and(|t| t.eq_ignore_ascii_case(tag)))
    {
        return Some(exact);
    }
    let language = tag.split('-').next().unwrap_or(tag);
    let same_language: Vec<(u32, &str)> = plan
        .supported
        .iter()
        .filter_map(|&o| tag_of(o).map(|t| (o, t)))
        .filter(|(_, t)| {
            t.split('-')
                .next()
                .is_some_and(|l| l.eq_ignore_ascii_case(language))
        })
        .collect();
    let (&(first, _), rest) = same_language.split_first()?;
    if rest.is_empty() {
        return Some(first);
    }
    // The one CLDR's likely subtags give for the language alone.
    let expander = LocaleExpander::new_extended();
    let maximal = |text: &str| {
        let mut id = LanguageIdentifier::try_from_str(text).ok()?;
        expander.maximize(&mut id);
        Some((id.script, id.region))
    };
    let likely = maximal(language);
    if likely.is_some()
        && let Some(&(ordinal, _)) = same_language.iter().find(|(_, t)| maximal(t) == likely)
    {
        return Some(ordinal);
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::locale_ordinal;

    fn ord(variant: &str) -> u32 {
        locale_ordinal(variant).unwrap_or(u32::MAX)
    }

    fn plan(default: &str, supported: &[&str]) -> LocalePlan {
        LocalePlan {
            default: ord(default),
            supported: supported.iter().map(|v| ord(v)).collect(),
            in_progress: Vec::new(),
        }
    }

    fn pick(header: &str, default: &str, supported: &[&str]) -> &'static str {
        let chosen = from_accept_language(Some(header), &plan(default, supported));
        jadpo_std::LOCALES
            .get(chosen as usize)
            .map_or("?", |(name, _)| name)
    }

    const APP: &[&str] = &["en_gb", "en_us", "fr_fr"];

    #[test]
    fn no_header_is_the_default() {
        assert_eq!(
            from_accept_language(None, &plan("en_gb", APP)),
            ord("en_gb")
        );
        assert_eq!(pick("", "fr_fr", APP), "fr_fr");
        assert_eq!(pick("   ", "fr_fr", APP), "fr_fr");
    }

    #[test]
    fn an_exact_tag_wins_whatever_its_case_or_separator() {
        assert_eq!(pick("fr-FR", "en_gb", APP), "fr_fr");
        assert_eq!(pick("FR-fr", "en_gb", APP), "fr_fr");
        assert_eq!(pick("en-US", "en_gb", APP), "en_us");
        assert_eq!(pick("en_us", "en_gb", APP), "en_us");
    }

    #[test]
    fn tags_are_taken_by_descending_weight_then_header_order() {
        assert_eq!(pick("fr-FR;q=0.8, en-US;q=0.9", "en_gb", APP), "en_us");
        assert_eq!(pick("en-US;q=0.5, fr-FR", "en_gb", APP), "fr_fr");
        // Equal weights keep the order the header gives.
        assert_eq!(pick("fr-FR, en-US", "en_gb", APP), "fr_fr");
        assert_eq!(pick("en-US;q=0.7, fr-FR;q=0.7", "en_gb", APP), "en_us");
        // An omitted weight is 1.
        assert_eq!(pick("en-US;q=0.9, fr-FR;q=1", "en_gb", APP), "fr_fr");
        assert_eq!(pick("en-US;q=0.9, fr-FR;q=1.000", "en_gb", APP), "fr_fr");
    }

    #[test]
    fn the_first_tag_that_matches_wins_over_a_heavier_one_that_does_not() {
        assert_eq!(pick("de-DE, fr-FR;q=0.5", "en_gb", APP), "fr_fr");
        assert_eq!(pick("de, ja;q=0.9, fr;q=0.1", "en_gb", APP), "fr_fr");
    }

    #[test]
    fn a_weight_of_zero_excludes_the_tag() {
        assert_eq!(pick("fr-FR;q=0", "en_us", APP), "en_us");
        assert_eq!(pick("fr-FR;q=0.000, en-US;q=0.1", "en_gb", APP), "en_us");
    }

    #[test]
    fn a_malformed_entry_is_skipped_and_the_rest_read() {
        for header in [
            "fr-FR;q=1.5",
            "fr-FR;q=",
            "fr-FR;q=abc",
            "fr-FR;q=0.1234",
            "fr-FR;q=1.001",
            "fr-FR;q=-1",
            "fr-FR;x=1",
            "fr-FR;q=0.5;extra=1",
            "fr-FR q=0.5",
            "f r",
            "toolongtag-FR",
            "fr--FR",
            "-fr",
            "fr-",
            "123",
            "fr-FRANCE123",
            ";q=0.5",
            "\u{e9}-FR",
        ] {
            assert_eq!(
                pick(&format!("{header}, en-US;q=0.1"), "en_gb", APP),
                "en_us",
                "{header}"
            );
        }
    }

    #[test]
    fn spaces_and_empty_list_entries_are_allowed() {
        assert_eq!(pick(" fr-FR ; q=0.5 ,, ,en-US ", "en_gb", APP), "en_us");
        assert_eq!(pick("fr-FR\t;\tq=0.5,\ten-US;q=0.4", "en_gb", APP), "fr_fr");
        assert_eq!(pick("fr-FR;Q=0.1, en-US;q=0.2", "en_gb", APP), "en_us");
    }

    #[test]
    fn a_wildcard_names_nothing() {
        assert_eq!(pick("*", "fr_fr", APP), "fr_fr");
        assert_eq!(pick("*, en-US;q=0.1", "fr_fr", APP), "en_us");
        assert_eq!(pick("*;q=0.5", "en_gb", &["en_gb", "fr_fr"]), "en_gb");
    }

    #[test]
    fn nothing_usable_is_the_default() {
        assert_eq!(pick("de-DE, ja;q=0.5", "en_gb", APP), "en_gb");
        assert_eq!(pick("de-DE;q=0", "fr_fr", APP), "fr_fr");
        assert_eq!(pick("garbage", "fr_fr", APP), "fr_fr");
    }

    #[test]
    fn only_a_supported_locale_is_chosen() {
        // `fr_ca` is a locale, but not one this application supports; the
        // supported French answers for the language.
        assert_eq!(pick("fr-CA", "en_gb", APP), "fr_fr");
        // German is a locale too, and is not supported.
        assert_eq!(pick("de-DE", "en_gb", &["en_gb"]), "en_gb");
    }

    #[test]
    fn a_language_takes_the_supported_locale_of_its_language() {
        assert_eq!(pick("fr", "en_gb", APP), "fr_fr");
        assert_eq!(pick("fr-BE", "en_gb", APP), "fr_fr");
        assert_eq!(pick("fr-CH-1996", "en_gb", APP), "fr_fr");
        assert_eq!(pick("en", "fr_fr", &["fr_fr", "en_gb"]), "en_gb");
    }

    #[test]
    fn a_language_prefers_the_locale_cldr_likely_subtags_give_for_it() {
        // English means en-US, whatever order the application lists them in.
        assert_eq!(
            pick("en-AU", "fr_fr", &["en_gb", "fr_fr", "en_us"]),
            "en_us"
        );
        assert_eq!(pick("en", "fr_fr", &["en_gb", "en_us"]), "en_us");
        // Portuguese means pt-BR, French means fr-FR.
        assert_eq!(pick("pt", "en_gb", &["pt_pt", "pt_br"]), "pt_br");
        assert_eq!(pick("pt-AO", "en_gb", &["pt_pt", "pt_br"]), "pt_br");
        assert_eq!(pick("fr-CA", "en_gb", &["fr_be", "fr_fr"]), "fr_fr");
    }

    #[test]
    fn with_no_likely_locale_the_first_supported_one_of_the_language() {
        // Neither is what French means, so the first listed answers.
        assert_eq!(pick("fr", "en_gb", &["en_gb", "fr_ca", "fr_be"]), "fr_ca");
        assert_eq!(pick("fr", "en_gb", &["en_gb", "fr_be", "fr_ca"]), "fr_be");
    }

    #[test]
    fn an_exact_match_beats_a_likely_one() {
        assert_eq!(
            pick("en-GB", "fr_fr", &["en_us", "en_gb", "fr_fr"]),
            "en_gb"
        );
        assert_eq!(pick("pt-PT", "en_gb", &["pt_br", "pt_pt"]), "pt_pt");
    }

    #[test]
    fn a_locale_with_no_region_is_a_locale_like_any() {
        assert_eq!(pick("fr", "en_gb", &["en_gb", "fr"]), "fr");
        assert_eq!(pick("fr-FR", "en_gb", &["en_gb", "fr"]), "fr");
    }

    #[test]
    fn the_default_answers_even_when_the_header_asks_for_it_by_a_different_region() {
        assert_eq!(pick("en-AU", "en_gb", &["en_gb"]), "en_gb");
    }

    #[test]
    fn a_long_header_is_read_to_its_end() {
        let mut header = String::new();
        for n in 0..500 {
            header.push_str(&format!("x{n}, "));
        }
        header.push_str("fr-FR");
        assert_eq!(pick(&header, "en_gb", APP), "fr_fr");
    }
}
