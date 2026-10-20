//! The built-in encodings a provider package's field names with `wire:`
//! (area 11, "Decoding"; chunk 7.2c2), as JSON turned into JSON: what the
//! provider sends made into what the contract writes for the field's type,
//! which the field's own codec then reads and checks, and the contract's
//! JSON for a value made into what the provider takes. Each refusal is the
//! rule the value broke, never the value.

use jadpo_std::provider::WireEncoding;

/// What the provider sent for a field in `encoding`, as the contract's
/// JSON for its type; `currency` is the code of a `Wire.minor_units`
/// amount's one currency. The rule broken when it isn't one.
pub(super) fn read(
    encoding: WireEncoding,
    currency: Option<&str>,
    json: &serde_json::Value,
    written: Option<&str>,
) -> Result<serde_json::Value, &'static str> {
    use serde_json::Value as Json;
    let text = |json: &Json| match json {
        Json::String(text) => Ok(text.clone()),
        _ => Err("type"),
    };
    let whole = |json: &Json| match json {
        Json::Number(number) => number.as_i64().ok_or("type"),
        _ => Err("type"),
    };
    // An instant past the years an `Instant` holds breaks its range.
    let instant = |millis: Option<i64>| {
        millis
            .and_then(|millis| crate::temporal::instant(millis).ok())
            .map(|millis| Json::String(jadpo_std::format_instant(millis)))
            .ok_or("max")
    };
    match encoding {
        WireEncoding::UnixSeconds => instant(whole(json)?.checked_mul(1000)),
        WireEncoding::UnixMilliseconds => instant(Some(whole(json)?)),
        WireEncoding::Iso8601 => {
            let millis = jadpo_std::instant_millis(&text(json)?).ok_or("format")?;
            instant(Some(millis))
        }
        WireEncoding::DayMonthYear => {
            let written = text(json)?;
            let parts: Vec<&str> = written.split('/').collect();
            match parts.as_slice() {
                [day, month, year]
                    if day.len() == 2
                        && month.len() == 2
                        && year.len() == 4
                        && parts
                            .iter()
                            .all(|part| part.bytes().all(|b| b.is_ascii_digit())) =>
                {
                    Ok(Json::String(format!("{year}-{month}-{day}")))
                }
                _ => Err("format"),
            }
        }
        WireEncoding::Text => Ok(Json::String(text(json)?)),
        // A number as the document wrote it, where that is known, so no
        // place is lost to a float (chunk 5.8c).
        WireEncoding::Number => match json {
            Json::Number(number) => Ok(Json::String(
                written.map_or_else(|| number.to_string(), str::to_string),
            )),
            _ => Err("type"),
        },
        WireEncoding::CommaSeparated => {
            let joined = text(json)?;
            if joined.is_empty() {
                return Ok(Json::Array(Vec::new()));
            }
            Ok(Json::Array(
                joined
                    .split(',')
                    .map(|item| Json::String(item.to_string()))
                    .collect(),
            ))
        }
        WireEncoding::Base64 => {
            let bytes = crate::encoding::from_base64(&text(json)?).ok_or("format")?;
            String::from_utf8(bytes)
                .map(Json::String)
                .map_err(|_| "format")
        }
        WireEncoding::JsonText => serde_json::from_str(&text(json)?).map_err(|_| "format"),
        WireEncoding::MinorUnits => {
            let amount = whole(json)?;
            let mut money = serde_json::Map::new();
            money.insert("amount".into(), Json::from(amount));
            money.insert(
                "currency".into(),
                Json::String(currency.unwrap_or_default().to_string()),
            );
            Ok(Json::Object(money))
        }
    }
}

/// `written`, the contract's JSON for a field's value, as the provider
/// takes it in `encoding`; the rule broken when it can't be.
pub(super) fn write(encoding: WireEncoding, written: &str) -> Result<String, &'static str> {
    use serde_json::Value as Json;
    let json: Json = serde_json::from_str(written).map_err(|_| "type")?;
    let string = |text: &str| {
        let mut out = String::new();
        super::write_string(text, &mut out);
        out
    };
    let text = || match &json {
        Json::String(text) => Ok(text.clone()),
        _ => Err("type"),
    };
    match encoding {
        WireEncoding::UnixSeconds | WireEncoding::UnixMilliseconds => {
            let millis = jadpo_std::instant_millis(&text()?).ok_or("format")?;
            Ok(if encoding == WireEncoding::UnixSeconds {
                millis.div_euclid(1000).to_string()
            } else {
                millis.to_string()
            })
        }
        WireEncoding::DayMonthYear => {
            let date = text()?;
            let parts: Vec<&str> = date.split('-').collect();
            match parts.as_slice() {
                [year, month, day] => Ok(string(&format!("{day}/{month}/{year}"))),
                _ => Err("format"),
            }
        }
        WireEncoding::Text => match &json {
            Json::String(_) => Ok(written.to_string()),
            Json::Number(number) => Ok(string(&number.to_string())),
            _ => Err("type"),
        },
        // A number written as the contract's text, `"2.50"`, sent as the
        // JSON number of those digits, every place kept.
        WireEncoding::Number => match &json {
            Json::Number(_) => Ok(written.to_string()),
            Json::String(digits) if is_number(digits) => Ok(digits.clone()),
            _ => Err("type"),
        },
        WireEncoding::CommaSeparated => {
            let Json::Array(items) = &json else {
                return Err("type");
            };
            let mut joined = Vec::with_capacity(items.len());
            for item in items {
                let item = match item {
                    Json::String(text) => text.clone(),
                    Json::Number(number) => number.to_string(),
                    _ => return Err("type"),
                };
                // An item holding a comma would be read back as two.
                if item.contains(',') {
                    return Err("comma_separated");
                }
                joined.push(item);
            }
            Ok(string(&joined.join(",")))
        }
        WireEncoding::Base64 => Ok(string(&crate::encoding::to_base64(text()?.as_bytes()))),
        WireEncoding::JsonText => Ok(string(written)),
        WireEncoding::Iso8601 => Ok(written.to_string()),
        WireEncoding::MinorUnits => match json.get("amount") {
            Some(Json::Number(amount)) => Ok(amount.to_string()),
            _ => Err("type"),
        },
    }
}

/// Whether `text` is a number as JSON writes one: an optional `-`, digits,
/// and places after a `.`.
fn is_number(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let (whole, places) = digits.split_once('.').unwrap_or((digits, "0"));
    !whole.is_empty()
        && !places.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && places.bytes().all(|b| b.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_one(encoding: WireEncoding, json: &str) -> Result<String, &'static str> {
        let json: serde_json::Value = serde_json::from_str(json).expect("json");
        read(encoding, Some("GBP"), &json, None).map(|json| json.to_string())
    }

    /// What a provider sends in each encoding becomes the contract's JSON,
    /// and the contract's JSON is sent back as the provider takes it.
    #[test]
    fn each_encoding_reads_and_writes() {
        use WireEncoding as W;
        assert_eq!(
            read_one(W::UnixSeconds, "1792400400"),
            Ok("\"2026-10-19T09:00:00Z\"".into())
        );
        assert_eq!(
            read_one(W::UnixMilliseconds, "1792400400250"),
            Ok("\"2026-10-19T09:00:00.250Z\"".into())
        );
        assert_eq!(
            read_one(W::Iso8601, "\"2026-10-19T10:00:00+01:00\""),
            Ok("\"2026-10-19T09:00:00Z\"".into())
        );
        assert_eq!(
            read_one(W::DayMonthYear, "\"21/10/2026\""),
            Ok("\"2026-10-21\"".into())
        );
        assert_eq!(read_one(W::Text, "\"42\""), Ok("\"42\"".into()));
        assert_eq!(read_one(W::Number, "2.5"), Ok("\"2.5\"".into()));
        assert_eq!(
            read_one(W::CommaSeparated, "\"a,b\""),
            Ok("[\"a\",\"b\"]".into())
        );
        assert_eq!(read_one(W::CommaSeparated, "\"\""), Ok("[]".into()));
        assert_eq!(read_one(W::Base64, "\"aGk=\""), Ok("\"hi\"".into()));
        assert_eq!(
            read_one(W::JsonText, "\"{\\\"a\\\":1}\""),
            Ok("{\"a\":1}".into())
        );
        assert_eq!(
            read_one(W::MinorUnits, "1250"),
            Ok("{\"amount\":1250,\"currency\":\"GBP\"}".into())
        );
        // What doesn't fit breaks a rule, never showing the value.
        assert_eq!(read_one(W::UnixSeconds, "\"soon\""), Err("type"));
        assert_eq!(read_one(W::Iso8601, "\"soon\""), Err("format"));
        assert_eq!(read_one(W::DayMonthYear, "\"2026-10-21\""), Err("format"));
        assert_eq!(read_one(W::Base64, "\"%%\""), Err("format"));
        assert_eq!(read_one(W::MinorUnits, "12.5"), Err("type"));

        assert_eq!(
            write(W::UnixSeconds, "\"2026-10-19T09:00:00.999Z\""),
            Ok("1792400400".into())
        );
        assert_eq!(
            write(W::UnixMilliseconds, "\"2026-10-19T09:00:00.250Z\""),
            Ok("1792400400250".into())
        );
        assert_eq!(
            write(W::DayMonthYear, "\"2026-10-21\""),
            Ok("\"21/10/2026\"".into())
        );
        assert_eq!(write(W::Text, "42"), Ok("\"42\"".into()));
        assert_eq!(write(W::Number, "\"2.50\""), Ok("2.50".into()));
        assert_eq!(
            write(W::CommaSeparated, "[\"a\",\"b\"]"),
            Ok("\"a,b\"".into())
        );
        assert_eq!(
            write(W::CommaSeparated, "[\"a,b\"]"),
            Err("comma_separated")
        );
        assert_eq!(write(W::Base64, "\"hi\""), Ok("\"aGk=\"".into()));
        assert_eq!(
            write(W::JsonText, "{\"a\":1}"),
            Ok("\"{\\\"a\\\":1}\"".into())
        );
        assert_eq!(
            write(W::MinorUnits, "{\"amount\":1250,\"currency\":\"GBP\"}"),
            Ok("1250".into())
        );
    }
}
