//! `Money` and `Currency` (area 19, "Money and payments"):
//! an amount is a whole number of its currency's minor units, and each
//! currency's exponent, how many places its major unit has, comes from
//! ISO 4217. `Currency` is a built-in enum of the active ISO 4217 codes
//! (question 22), its variants the codes in lower case (`Currency.gbp`),
//! shipped with each release as the time-zone database is.

/// `Currency`'s name in Jadpo.
pub const CURRENCY: &str = "Currency";

/// `Money`'s fields on the wire and in storage, in order: the amount in
/// minor units and the currency's code (`{"amount": 1050, "currency":
/// "GBP"}`).
pub const MONEY_FIELDS: [&str; 2] = ["amount", "currency"];

/// Each active ISO 4217 code with an exponent (ISO 4217's list one, as of
/// this release), sorted by code: the variant's name and the code. Codes
/// with no minor unit (`N.A.` in the list: the precious metals, the SDR,
/// the bond-market units, `XSU`, `XUA`, `XTS` and `XXX`) hold no amount
/// in minor units and are left out. The codes ISO has withdrawn since
/// 2021 (`ANG`, `BGN`, `CUC`, `HRK`, `SLL`, `ZWL`, all in hundredths) are
/// among them, retired ([`CURRENCIES_RETIRED`], chunk 5.5l): stored data
/// and old messages still read, and new code that writes one is
/// `type.variant_retired`.
pub const CURRENCIES: &[(&str, &str)] = &[
    ("aed", "AED"),
    ("afn", "AFN"),
    // `all` lists an enum's variants (question 22), so Albania's lek takes
    // the trailing `_` a code that is a reserved word takes.
    ("all_", "ALL"),
    ("amd", "AMD"),
    ("ang", "ANG"),
    ("aoa", "AOA"),
    ("ars", "ARS"),
    ("aud", "AUD"),
    ("awg", "AWG"),
    ("azn", "AZN"),
    ("bam", "BAM"),
    ("bbd", "BBD"),
    ("bdt", "BDT"),
    ("bgn", "BGN"),
    ("bhd", "BHD"),
    ("bif", "BIF"),
    ("bmd", "BMD"),
    ("bnd", "BND"),
    ("bob", "BOB"),
    ("bov", "BOV"),
    ("brl", "BRL"),
    ("bsd", "BSD"),
    ("btn", "BTN"),
    ("bwp", "BWP"),
    ("byn", "BYN"),
    ("bzd", "BZD"),
    ("cad", "CAD"),
    ("cdf", "CDF"),
    ("che", "CHE"),
    ("chf", "CHF"),
    ("chw", "CHW"),
    ("clf", "CLF"),
    ("clp", "CLP"),
    ("cny", "CNY"),
    ("cop", "COP"),
    ("cou", "COU"),
    ("crc", "CRC"),
    ("cuc", "CUC"),
    ("cup", "CUP"),
    ("cve", "CVE"),
    ("czk", "CZK"),
    ("djf", "DJF"),
    ("dkk", "DKK"),
    ("dop", "DOP"),
    ("dzd", "DZD"),
    ("egp", "EGP"),
    ("ern", "ERN"),
    ("etb", "ETB"),
    ("eur", "EUR"),
    ("fjd", "FJD"),
    ("fkp", "FKP"),
    ("gbp", "GBP"),
    ("gel", "GEL"),
    ("ghs", "GHS"),
    ("gip", "GIP"),
    ("gmd", "GMD"),
    ("gnf", "GNF"),
    ("gtq", "GTQ"),
    ("gyd", "GYD"),
    ("hkd", "HKD"),
    ("hnl", "HNL"),
    ("hrk", "HRK"),
    ("htg", "HTG"),
    ("huf", "HUF"),
    ("idr", "IDR"),
    ("ils", "ILS"),
    ("inr", "INR"),
    ("iqd", "IQD"),
    ("irr", "IRR"),
    ("isk", "ISK"),
    ("jmd", "JMD"),
    ("jod", "JOD"),
    ("jpy", "JPY"),
    ("kes", "KES"),
    ("kgs", "KGS"),
    ("khr", "KHR"),
    ("kmf", "KMF"),
    ("kpw", "KPW"),
    ("krw", "KRW"),
    ("kwd", "KWD"),
    ("kyd", "KYD"),
    ("kzt", "KZT"),
    ("lak", "LAK"),
    ("lbp", "LBP"),
    ("lkr", "LKR"),
    ("lrd", "LRD"),
    ("lsl", "LSL"),
    ("lyd", "LYD"),
    ("mad", "MAD"),
    ("mdl", "MDL"),
    ("mga", "MGA"),
    ("mkd", "MKD"),
    ("mmk", "MMK"),
    ("mnt", "MNT"),
    ("mop", "MOP"),
    ("mru", "MRU"),
    ("mur", "MUR"),
    ("mvr", "MVR"),
    ("mwk", "MWK"),
    ("mxn", "MXN"),
    ("mxv", "MXV"),
    ("myr", "MYR"),
    ("mzn", "MZN"),
    ("nad", "NAD"),
    ("ngn", "NGN"),
    ("nio", "NIO"),
    ("nok", "NOK"),
    ("npr", "NPR"),
    ("nzd", "NZD"),
    ("omr", "OMR"),
    ("pab", "PAB"),
    ("pen", "PEN"),
    ("pgk", "PGK"),
    ("php", "PHP"),
    ("pkr", "PKR"),
    ("pln", "PLN"),
    ("pyg", "PYG"),
    ("qar", "QAR"),
    ("ron", "RON"),
    ("rsd", "RSD"),
    ("rub", "RUB"),
    ("rwf", "RWF"),
    ("sar", "SAR"),
    ("sbd", "SBD"),
    ("scr", "SCR"),
    ("sdg", "SDG"),
    ("sek", "SEK"),
    ("sgd", "SGD"),
    ("shp", "SHP"),
    ("sle", "SLE"),
    ("sll", "SLL"),
    ("sos", "SOS"),
    ("srd", "SRD"),
    ("ssp", "SSP"),
    ("stn", "STN"),
    ("svc", "SVC"),
    ("syp", "SYP"),
    ("szl", "SZL"),
    ("thb", "THB"),
    ("tjs", "TJS"),
    ("tmt", "TMT"),
    ("tnd", "TND"),
    ("top", "TOP"),
    ("try", "TRY"),
    ("ttd", "TTD"),
    ("twd", "TWD"),
    ("tzs", "TZS"),
    ("uah", "UAH"),
    ("ugx", "UGX"),
    ("usd", "USD"),
    ("usn", "USN"),
    ("uyi", "UYI"),
    ("uyu", "UYU"),
    ("uyw", "UYW"),
    ("uzs", "UZS"),
    ("ved", "VED"),
    ("ves", "VES"),
    ("vnd", "VND"),
    ("vuv", "VUV"),
    ("wst", "WST"),
    ("xaf", "XAF"),
    ("xcd", "XCD"),
    ("xcg", "XCG"),
    ("xof", "XOF"),
    ("xpf", "XPF"),
    ("yer", "YER"),
    ("zar", "ZAR"),
    ("zmw", "ZMW"),
    ("zwg", "ZWG"),
    ("zwl", "ZWL"),
];

/// `Currency`'s retired variants (question 22): ISO 4217's list three
/// since 2021, which CLDR still names.
pub const CURRENCIES_RETIRED: &[(&str, &str)] = &[
    ("ang", ""),
    ("bgn", ""),
    ("cuc", ""),
    ("hrk", ""),
    ("sll", ""),
    ("zwl", ""),
];

/// The codes whose exponent isn't 2, with theirs: ISO 4217's minor units
/// for every other active code are hundredths.
const EXPONENTS: &[(&str, u32)] = &[
    ("BHD", 3),
    ("BIF", 0),
    ("CLF", 4),
    ("CLP", 0),
    ("DJF", 0),
    ("GNF", 0),
    ("IQD", 3),
    ("ISK", 0),
    ("JOD", 3),
    ("JPY", 0),
    ("KMF", 0),
    ("KRW", 0),
    ("KWD", 3),
    ("LYD", 3),
    ("OMR", 3),
    ("PYG", 0),
    ("RWF", 0),
    ("TND", 3),
    ("UGX", 0),
    ("UYI", 0),
    ("UYW", 4),
    ("VND", 0),
    ("VUV", 0),
    ("XAF", 0),
    ("XOF", 0),
    ("XPF", 0),
];

/// How many places the major unit of the currency coded `code` has: `0`
/// for `JPY`, `2` for `GBP`, `3` for `BHD`.
pub fn exponent_of(code: &str) -> u32 {
    EXPONENTS
        .iter()
        .find(|(listed, _)| *listed == code)
        .map_or(2, |(_, exponent)| *exponent)
}

/// Whether `raw` is an amount's canonical text, `GBP 10.50`: an active
/// currency's code, one space, and the amount in major units with exactly
/// the currency's places, as `jadpo_value::money::read` reads it; what a
/// `Money` setting's `default:` is checked against (chunk 5.5v5).
pub fn is_amount_text(raw: &str) -> bool {
    let Some((code, amount)) = raw.split_once(' ') else {
        return false;
    };
    let Some(exponent) = ordinal_of_code(code).and_then(exponent) else {
        return false;
    };
    let digits = amount.strip_prefix('-').unwrap_or(amount);
    let (whole, places) = digits.split_once('.').unwrap_or((digits, ""));
    let plain = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    !whole.is_empty()
        && plain(whole)
        && plain(places)
        && places.len() == exponent as usize
        && (exponent == 0 || digits.contains('.'))
}

/// The exponent of the currency at `ordinal` among `Currency`'s variants.
pub fn exponent(ordinal: u32) -> Option<u32> {
    CURRENCIES
        .get(ordinal as usize)
        .map(|(_, code)| exponent_of(code))
}

/// The ISO 4217 code of the currency at `ordinal`: `GBP`.
pub fn code(ordinal: u32) -> Option<&'static str> {
    CURRENCIES.get(ordinal as usize).map(|(_, code)| *code)
}

/// The variant's name of the currency at `ordinal`: `gbp`.
pub fn variant(ordinal: u32) -> Option<&'static str> {
    CURRENCIES.get(ordinal as usize).map(|(name, _)| *name)
}

/// Where the currency coded `code` stands among `Currency`'s variants,
/// ignoring case (question 22): `gbp` and `GBP` are one code.
pub fn ordinal_of_code(code: &str) -> Option<u32> {
    CURRENCIES
        .iter()
        .position(|(_, listed)| listed.eq_ignore_ascii_case(code))
        .and_then(|at| u32::try_from(at).ok())
}

/// Where the currency whose variant is `name` stands.
pub fn ordinal_of_variant(name: &str) -> Option<u32> {
    CURRENCIES
        .iter()
        .position(|(listed, _)| *listed == name)
        .and_then(|at| u32::try_from(at).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_sorted_and_named_for_their_variant() {
        let codes: Vec<&str> = CURRENCIES.iter().map(|(_, code)| *code).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(codes, sorted, "sorted by code, each once");
        for (name, code) in CURRENCIES {
            assert_eq!(code.len(), 3);
            assert!(code.bytes().all(|b| b.is_ascii_uppercase()), "{code}");
            assert_eq!(name.trim_end_matches('_'), code.to_ascii_lowercase());
        }
        for (code, _) in EXPONENTS {
            assert!(codes.contains(code), "{code} is an active code");
        }
    }

    #[test]
    fn exponents_are_iso_4217_s() {
        assert_eq!(exponent_of("GBP"), 2);
        assert_eq!(exponent_of("JPY"), 0);
        assert_eq!(exponent_of("BHD"), 3);
        assert_eq!(exponent_of("CLF"), 4);
        let gbp = ordinal_of_code("gbp").expect("GBP");
        assert_eq!(code(gbp), Some("GBP"));
        assert_eq!(variant(gbp), Some("gbp"));
        assert_eq!(exponent(gbp), Some(2));
        assert_eq!(ordinal_of_variant("jpy").and_then(exponent), Some(0));
        assert!(
            ordinal_of_code("HRK").is_some(),
            "withdrawn in 2023, still read"
        );
        assert_eq!(ordinal_of_code("XAU"), None, "no minor unit");
    }
}
