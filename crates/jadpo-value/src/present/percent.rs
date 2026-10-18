//! A percentage for people (area 19, "The standard operator table"):
//! `text.format` on a `Percentage` and `{rate, number, percent}` in a
//! message, over ICU4X's percent formatter and CLDR's percent pattern, so
//! the interpreter, native deployables and WebAssembly write the same
//! text: `12.5%` in `en-GB`, `20 %` in `de-DE`.

use fixed_decimal::Decimal as Fixed;
use icu_experimental::dimension::percent::formatter::PercentFormatter;
use writeable::Writeable;

use super::locale_of;
use crate::{Decimal, Fault};

/// The fraction `fraction` as a percentage in the locale tagged `tag`:
/// the locale's digits, grouping and decimal separator, every place the
/// value keeps as a percentage, and the `%` where the locale puts it.
pub fn percent(fraction: &Decimal, tag: &str) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    let (mantissa, places) = jadpo_std::percentage::as_percent(fraction.mantissa, fraction.scale);
    let fixed: Fixed = Decimal::new(mantissa, places)
        .to_string()
        .parse()
        .map_err(|_| Fault::Shape)?;
    let formatter = PercentFormatter::try_new((&locale).into(), Default::default())
        .map_err(|_| Fault::Shape)?;
    Ok(formatter.format(&fixed).write_to_string().into_owned())
}
