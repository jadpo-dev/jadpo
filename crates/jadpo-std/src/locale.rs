//! The locales an application supports (area 10, "Locales"): declared
//! once in `app.jadpo`, read by the checker and carried to the runtime as
//! a [`LocalePlan`].

use crate::zones::LOCALES;

/// What `locales { default: ... supported: [...] }` says, as places among
/// `Locale`'s variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalePlan {
    /// The locale a choice from a header falls back to.
    pub default: u32,
    /// The ones formatting and translations accept, in the order listed.
    pub supported: Vec<u32>,
    /// The supported locales whose translations are not finished, which
    /// fall back where wording is missing (`in_progress:`).
    pub in_progress: Vec<u32>,
}

impl LocalePlan {
    pub fn supports(&self, ordinal: u32) -> bool {
        self.supported.contains(&ordinal)
    }
}

/// The place of the variant `variant` (`en_gb`) among `Locale`'s variants.
pub fn locale_ordinal(variant: &str) -> Option<u32> {
    LOCALES
        .iter()
        .position(|(name, _)| *name == variant)
        .and_then(|at| u32::try_from(at).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_knows_what_it_supports() {
        let en = locale_ordinal("en_gb").unwrap_or(0);
        let fr = locale_ordinal("fr_fr").unwrap_or(0);
        let plan = LocalePlan {
            default: en,
            supported: vec![en],
            in_progress: Vec::new(),
        };
        assert!(plan.supports(en));
        assert!(!plan.supports(fr));
        assert_eq!(locale_ordinal("klingon"), None);
    }
}
