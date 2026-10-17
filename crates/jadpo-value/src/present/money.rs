//! Money for people (area 19, "Money and payments"): `text.format` on an
//! amount, over ICU4X's currency formatter and the CLDR data compiled into
//! it, so the interpreter, native deployables and WebAssembly write the
//! same text. CLDR gives the locale's grouping and decimal separator, the
//! currency's places for display and where its symbol, code or name goes:
//! `£1,234.50` in `en-GB`, `1 234,50 €` in `fr-FR`.

use fixed_decimal::Decimal as Fixed;
use icu_decimal::DecimalFormatter;
use icu_experimental::dimension::currency::CurrencyType;
use icu_experimental::dimension::currency::formatter::CurrencyFormatter;
use icu_experimental::dimension::currency::options::{CurrencyFormatterOptions, CurrencyUsage};
use icu_experimental::dimension::provider::currency::displayname::CurrencyDisplaynameV1;
use icu_provider::prelude::*;
use jadpo_std::{MoneyDisplay, MoneyPrecision, MoneySign, Rounding};
use writeable::Writeable;

use super::locale_of;
use crate::standard::MoneyOptions;
use crate::{Decimal, Fault, Money};

/// An amount for people in the locale tagged `tag`, with `text.format`'s
/// options: `precision: MoneyPrecision.whole` rounds to whole major units,
/// a half away from zero (`£1,235`); `display:` writes the symbol (the
/// default), the code or the name; `sign: MoneySign.accounting` writes a
/// negative amount as the locale's accounts do (`(£5.00)`).
pub fn format(amount: Money, tag: &str, options: &MoneyOptions) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    let currency = CurrencyType::try_from_str(&amount.code().to_ascii_lowercase())
        .map_err(|_| Fault::Shape)?;
    let whole = options.precision == Some(MoneyPrecision::Whole);
    let value: Decimal = if whole {
        amount
            .major()
            .rescale(0, Rounding::HalfUp)
            .ok_or(Fault::DecimalOverflow)?
    } else {
        amount.major()
    };
    let fixed: Fixed = value.to_string().parse().map_err(|_| Fault::Shape)?;
    let usage = match options.sign {
        Some(MoneySign::Accounting) => CurrencyUsage::Accounting,
        Some(MoneySign::Standard) | None => CurrencyUsage::Standard,
    };
    let mut formatter_options = CurrencyFormatterOptions::default();
    formatter_options.usage = usage;
    let prefs = (&locale).into();
    let formatter = match options.display {
        Some(MoneyDisplay::Code) => {
            CurrencyFormatter::try_new_code(prefs, currency, formatter_options)
        }
        Some(MoneyDisplay::Name) => CurrencyFormatter::try_new_name(prefs, currency),
        Some(MoneyDisplay::Symbol) | None => {
            CurrencyFormatter::try_new_symbol(prefs, currency, formatter_options)
        }
    }
    .map_err(|_| Fault::Shape)?;
    let shown = formatter
        .format_fixed_decimal(&fixed)
        .write_to_string()
        .into_owned();
    if !whole {
        return Ok(shown);
    }
    // The currency formatter writes the currency's places for display
    // whatever it is given, so a whole amount's number is written without
    // them: the same digits, grouping and separators, as the locale writes
    // a whole number.
    let numbers = DecimalFormatter::try_new((&locale).into(), Default::default())
        .map_err(|_| Fault::Shape)?;
    let size = value
        .checked_neg()
        .filter(|_| value.mantissa < 0)
        .unwrap_or(value);
    let places = jadpo_std::money::exponent(amount.currency).unwrap_or(2);
    let padded: Fixed = size
        .rescale(places, Rounding::HalfEven)
        .ok_or(Fault::DecimalOverflow)?
        .to_string()
        .parse()
        .map_err(|_| Fault::Shape)?;
    let bare: Fixed = size.to_string().parse().map_err(|_| Fault::Shape)?;
    let padded = numbers.format(&padded).write_to_string().into_owned();
    let bare = numbers.format(&bare).write_to_string().into_owned();
    Ok(shown.replacen(&padded, &bare, 1))
}

/// A currency's CLDR display name in the locale tagged `tag`
/// (`FormatKey::CurrencyName`): "British Pound" for `GBP` in `en-GB`; the
/// code where CLDR names none.
pub fn currency_name(code: &str, tag: &str) -> Result<String, Fault> {
    let locale = locale_of(tag)?;
    let attributes = DataMarkerAttributes::try_from_str(code).map_err(|_| Fault::Shape)?;
    let data_locale: DataLocale = (&locale).into();
    let request = DataRequest {
        id: DataIdentifierBorrowed::for_marker_attributes_and_locale(attributes, &data_locale),
        ..Default::default()
    };
    let loaded: Option<DataResponse<CurrencyDisplaynameV1>> =
        DataProvider::<CurrencyDisplaynameV1>::load(&icu_experimental::provider::Baked, request)
            .allow_identifier_not_found()
            .map_err(|_| Fault::Shape)?;
    Ok(match loaded {
        Some(response) => {
            let name: &str = response.payload.get();
            name.to_string()
        }
        None => code.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amount(code: &str, minor: i64) -> Money {
        Money {
            currency: jadpo_std::money::ordinal_of_code(code).unwrap(),
            minor,
        }
    }

    fn shown(code: &str, minor: i64, tag: &str, options: MoneyOptions) -> String {
        format(amount(code, minor), tag, &options).unwrap()
    }

    #[test]
    fn amounts_are_written_as_the_locale_writes_them() {
        let plain = MoneyOptions::default();
        assert_eq!(shown("GBP", 1050, "en-GB", plain), "£10.50");
        assert_eq!(shown("GBP", 123_450, "en-GB", plain), "£1,234.50");
        assert_eq!(
            shown("EUR", 123_450, "fr-FR", plain),
            "1\u{202f}234,50\u{a0}€"
        );
        assert_eq!(shown("JPY", 500, "en-GB", plain), "JP¥500");
        assert_eq!(shown("BHD", 1250, "en-GB", plain), "BHD\u{a0}1.250");
        let whole = MoneyOptions {
            precision: Some(MoneyPrecision::Whole),
            ..plain
        };
        assert_eq!(shown("GBP", 123_450, "en-GB", whole), "£1,235");
        assert_eq!(shown("GBP", -123_450, "en-GB", whole), "-£1,235");
        let code = MoneyOptions {
            display: Some(MoneyDisplay::Code),
            ..plain
        };
        assert_eq!(shown("GBP", 1050, "en-GB", code), "GBP\u{a0}10.50");
        let name = MoneyOptions {
            display: Some(MoneyDisplay::Name),
            ..plain
        };
        assert_eq!(shown("GBP", 1050, "en-GB", name), "10.50 British pounds");
        let accounting = MoneyOptions {
            sign: Some(MoneySign::Accounting),
            ..plain
        };
        assert_eq!(shown("GBP", -500, "en-GB", accounting), "(£5.00)");
        assert_eq!(shown("GBP", -500, "en-GB", plain), "-£5.00");
    }

    #[test]
    fn currencies_have_cldr_names() {
        assert_eq!(currency_name("GBP", "en-GB").unwrap(), "British Pound");
        assert_eq!(currency_name("EUR", "fr-FR").unwrap(), "euro");
    }
}
