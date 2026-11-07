//! A UK postcode's parts (area 19, "A UK postcode's parts"): each `postal`
//! operation, one part of a `PostalCode` by Royal Mail's format, through
//! `jadpo_std::standard::postal::uk_parts`, the one reading the checker's
//! proof uses too. A postal code not in the format fails
//! `not_uk_postcode`.

use jadpo_std::RuleKey;
use jadpo_std::standard::postal::{UkParts, uk_parts};

use crate::{Fault, Value};

/// `postcode`'s parts, or `not_uk_postcode`.
fn parts(postcode: &Value) -> Result<UkParts, Fault> {
    let Value::Text(text) = postcode else {
        return Err(Fault::Shape);
    };
    uk_parts(text).ok_or(Fault::Broke(RuleKey::NotUkPostcode))
}

/// `postal.area(postcode)`.
pub fn area(postcode: &Value) -> Result<Value, Fault> {
    Ok(Value::text(&parts(postcode)?.area))
}

/// `postal.district(postcode)`.
pub fn district(postcode: &Value) -> Result<Value, Fault> {
    Ok(Value::text(&parts(postcode)?.district))
}

/// `postal.inward_code(postcode)`.
pub fn inward_code(postcode: &Value) -> Result<Value, Fault> {
    Ok(Value::text(&parts(postcode)?.inward_code))
}

/// `postal.outward_code(postcode)`.
pub fn outward_code(postcode: &Value) -> Result<Value, Fault> {
    Ok(Value::text(&parts(postcode)?.outward_code))
}

/// `postal.sector(postcode)`.
pub fn sector(postcode: &Value) -> Result<Value, Fault> {
    Ok(Value::text(&parts(postcode)?.sector))
}

/// `postal.sub_district(postcode)`: `none` where the outward code has
/// none.
pub fn sub_district(postcode: &Value) -> Result<Value, Fault> {
    Ok(parts(postcode)?
        .sub_district
        .as_deref()
        .map_or(Value::None, Value::text))
}

/// `postal.unit(postcode)`.
pub fn unit(postcode: &Value) -> Result<Value, Fault> {
    Ok(Value::text(&parts(postcode)?.unit))
}
