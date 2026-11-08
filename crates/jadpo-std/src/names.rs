//! A name in code as the registry writes it (area 19, "Registry names"):
//! one rule, shared by `names.registry_part` and the registry's own names
//! (`jadpo_registry::names::ScopedName::of_code`), so what a Jadpo
//! application works out and what the registry holds never differ.

/// `name`, a name in code, as a part of a registry name: lower case, with
/// a `-` before each capital but the first. `AcmeCouriers` is
/// `acme-couriers`, `OpenAi` is `open-ai`, and `BritishGrid` is
/// `british-grid`. Anything else is kept as it is, lower-cased: whether the
/// result is a name the registry takes is the registry's to say.
pub fn registry_part(name: &str) -> String {
    let mut part = String::with_capacity(name.len() + 4);
    for (at, letter) in name.char_indices() {
        if letter.is_ascii_uppercase() && at > 0 {
            part.push('-');
        }
        part.push(letter.to_ascii_lowercase());
    }
    part
}

#[cfg(test)]
mod tests {
    use super::registry_part;

    #[test]
    fn each_capital_but_the_first_starts_a_word() {
        assert_eq!(registry_part("AcmeCouriers"), "acme-couriers");
        assert_eq!(registry_part("OpenAi"), "open-ai");
        assert_eq!(registry_part("Postcodes"), "postcodes");
        assert_eq!(registry_part("UkAddressFormat"), "uk-address-format");
        assert_eq!(registry_part(""), "");
    }
}
