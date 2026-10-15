//! Name forms (area 1): `UpperCamelCase` for types and declared contracts,
//! `lower_snake_case` for everything else, and the conversion between a
//! name and its file name.

/// `UpperCamelCase`: an upper-case letter first, then letters and digits.
pub fn is_upper_camel(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}

/// `lower_snake_case`: lower-case letters, digits and single underscores
/// between words, starting with a letter.
pub fn is_lower_snake(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.contains("__")
        && !name.ends_with('_')
}

/// Whether a name starts with an upper-case letter, the sign of a type.
pub fn starts_upper(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

use jadpo_db::name_words as words;
pub use jadpo_db::to_snake;

/// The `UpperCamelCase` form.
pub fn to_camel(name: &str) -> String {
    words(name)
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Whether an `UpperCamelCase` name survives the trip to its file name and
/// back: `VatRate` does, `VATRate` (back as `VatRate`) does not.
pub fn round_trips(name: &str) -> bool {
    to_camel(&to_snake(name)) == name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_are_recognised() {
        assert!(is_upper_camel("ServiceCredential") && is_upper_camel("Item2"));
        assert!(!is_upper_camel("service_credential") && !is_upper_camel("Order_Line"));
        assert!(is_lower_snake("order_total") && is_lower_snake("line2"));
        assert!(!is_lower_snake("orderTotal") && !is_lower_snake("order__total"));
        assert!(!is_lower_snake("_hidden") && !is_lower_snake("trailing_"));
    }

    #[test]
    fn names_convert_both_ways() {
        assert_eq!(to_snake("ServiceCredential"), "service_credential");
        assert_eq!(to_snake("HTTPStatus"), "http_status");
        assert_eq!(to_snake("orderTotal"), "order_total");
        assert_eq!(to_camel("http_status"), "HttpStatus");
        assert_eq!(to_camel("VATRate"), "VatRate");
        assert!(round_trips("VatRate") && round_trips("Item2"));
        assert!(!round_trips("VATRate") && !round_trips("TodoID"));
    }
}
