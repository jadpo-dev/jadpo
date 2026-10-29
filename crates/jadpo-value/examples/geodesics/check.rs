//! Area 20's geodesic measures (chunk 5.5u2) against a sample of
//! GeographicLib's published test set, `reference.tsv` beside this file,
//! which tools/references/geodtest_sample.py writes from `GeodTest-short.dat` and
//! says how. Shared by this example, which the gate runs under
//! WebAssembly, and the native test (crates/jadpo-runtime/tests/geo.rs),
//! so both targets are held to the same answers, each call made through
//! `jadpo_value::run` as both executors make it.
//!
//! Each line is one geodesic from point 1 to point 2, its distance exact
//! and its azimuth at point 1 exact, point 2 given to 10⁻¹⁸ degrees, and
//! the area between it and the equator to the square millimetre. Each is
//! checked:
//!
//! - `geo.distance` is the reference distance rounded half to even to the
//!   micrometre, either way when the reference is within a nanometre of a
//!   tie;
//! - `geo.bearing` is the reference azimuth, from 0 to 360, to its 9
//!   places, within half a unit of the 9th place plus how far rounding
//!   point 2 to 10⁻¹⁸ degrees can turn it, `1.6 × 10⁻¹³ m / m₁₂` radians,
//!   `m₁₂` the reference reduced length; a geodesic whose `m₁₂` is below a
//!   metre (its end near the point conjugate to its start, where the
//!   direction hardly changes where it ends) isn't checked;
//! - `geo.area` of the polygon from point 1 down its meridian to the
//!   equator, along it, and up to point 2 is the reference area's size,
//!   within the reference's square millimetre, half of `Area`'s last place,
//!   and how far moving point 2 by `1.6 × 10⁻¹³ m` can change the area:
//!   that distance times 10⁷ m for point 2's own meridian, and for the
//!   geodesic, that distance times its length while its arc is under 90°
//!   (no point along it moves further than its end), or else `c²` (the
//!   authalic radius squared) times the turn of the azimuths at both ends,
//!   `2 × 1.6 × 10⁻¹³ m / m₁₂`; again unchecked for an `m₁₂` below a
//!   metre.

use jadpo_std::standard::geo::POLYGON;
use jadpo_value::{Decimal, Fault, Value, geo, run};

pub const REFERENCE: &str = include_str!("reference.tsv");

/// FNV-1a over every answer's difference from the reference, line by line:
/// a target giving one digit differently from another gives a different
/// digest, so the native test and the WebAssembly run, each held to this,
/// give the same digits, not only answers within the tolerances.
pub const DIGEST: u64 = 0x6f78_9ea3_6ed2_b94a;

/// How far rounding point 2 to 10⁻¹⁸ degrees can move it, in metres: a
/// degree of latitude is under 111.7 km, so each coordinate's 5 × 10⁻¹⁹
/// degrees is under 5.6 × 10⁻¹⁴ m, and both together under 1.6 × 10⁻¹³ m
/// with room.
const NUDGE: f64 = 1.6e-13;

/// WGS84's authalic radius squared, in square metres: the area a turn of
/// a geodesic's azimuth by a radian sweeps.
const AUTHALIC: f64 = 40_589_732_499_314.76;

/// A reference number as a decimal: GeodTest writes `.5` and `-.5`.
fn number(text: &str) -> Option<Decimal> {
    let fixed = if let Some(rest) = text.strip_prefix("-.") {
        format!("-0.{rest}")
    } else if let Some(rest) = text.strip_prefix('.') {
        format!("0.{rest}")
    } else {
        text.to_string()
    };
    Decimal::parse(&fixed)
}

/// `value` as a whole number of `10^-places`, cut toward zero.
fn units(value: Decimal, places: u32) -> i128 {
    if value.scale <= places {
        value.mantissa * 10i128.pow(places - value.scale)
    } else {
        value.mantissa / 10i128.pow(value.scale - places)
    }
}

fn point(latitude: Decimal, longitude: Decimal) -> Value {
    geo::point(latitude, longitude, Value::None)
}

/// What one line gave, against the reference, each difference in units
/// of its last checked place: picometres, 10⁻¹² degrees and square
/// micrometres. `None` where the line isn't checked for that measure.
pub struct Measured {
    pub category: String,
    pub line: String,
    pub distance: i128,
    pub bearing: Option<i128>,
    pub bearing_allowed: i128,
    pub area: Option<i128>,
    pub area_allowed: i128,
}

/// The measures for one reference line.
pub fn measured(line: &str) -> Result<Measured, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [
        category,
        at,
        lat1,
        lon1,
        azi1,
        lat2,
        lon2,
        s12,
        a12,
        m12,
        area12,
    ] = fields[..]
    else {
        return Err(format!("`{line}` doesn't have eleven fields"));
    };
    let read = |text: &str| number(text).ok_or_else(|| format!("`{text}` isn't a number"));
    let (lat1, lon1, azi1, lat2, lon2) = (
        read(lat1)?,
        read(lon1)?,
        read(azi1)?,
        read(lat2)?,
        read(lon2)?,
    );
    let (s12, a12, m12, area12) = (read(s12)?, read(a12)?, read(m12)?, read(area12)?);
    let from = point(lat1, lon1);
    let to = point(lat2, lon2);
    let operation = |name: &str| {
        jadpo_std::operation("geo", name).ok_or_else(|| format!("`geo.{name}` isn't an operation"))
    };
    let call = |name: &str, args: &[Value]| -> Result<Value, String> {
        run(operation(name)?.op, args, None, &mut |_| None)
            .map_err(|fault: Fault| format!("geo.{name} faulted: {fault:?}"))
    };
    let Value::Quantity(distance) = call("distance", &[from.clone(), to.clone()])? else {
        return Err("geo.distance didn't give a length".into());
    };
    let distance = distance.amount * 1_000_000 - units(s12, 12);

    // Conditioning: a reduced length below a metre leaves the direction
    // and the area at point 2's mercy.
    let m12 = (units(m12, 6) as f64 / 1e6).abs();
    let conditioned = m12 >= 1.0;
    let bearing_allowed = if conditioned {
        500 + (NUDGE / m12 * (180.0 / std::f64::consts::PI) * 1e12).ceil() as i128
    } else {
        0
    };
    let bearing = if conditioned {
        let Value::Decimal(gave) = call("bearing", &[from.clone(), to.clone()])? else {
            return Err("geo.bearing didn't give a decimal".into());
        };
        let mut expected = units(azi1, 12);
        if expected < 0 {
            expected += 360 * 10i128.pow(12);
        }
        let mut difference = units(gave, 12) - expected;
        // 360 and 0 are one direction.
        if difference > 180 * 10i128.pow(12) {
            difference -= 360 * 10i128.pow(12);
        } else if difference < -180 * 10i128.pow(12) {
            difference += 360 * 10i128.pow(12);
        }
        Some(difference)
    } else {
        None
    };

    // The polygon point 1, (0, lon1), (0, half way), (0, lon2), point 2:
    // its area is the area under the geodesic (lon1 is always 0).
    let half = Decimal::new(units(lon2, 19) / 2, 19);
    let zero = Decimal::new(0, 0);
    let ring = vec![
        from.clone(),
        point(zero, lon1),
        point(zero, half),
        point(zero, lon2),
        to.clone(),
        from,
    ];
    let polygon = Value::record(
        POLYGON,
        vec![
            ("outer".into(), Value::list(ring)),
            ("holes".into(), Value::list(Vec::new())),
        ],
    );
    let swing = if units(a12, 0) < 90 {
        units(s12, 0) as f64
    } else {
        2.0 * AUTHALIC / m12
    };
    let area_allowed = if conditioned {
        15 + (NUDGE * (swing + 1.0e7) * 1e7).ceil() as i128
    } else {
        0
    };
    let area = if conditioned {
        let Value::Quantity(gave) = call("area", &[polygon])? else {
            return Err("geo.area didn't give an area".into());
        };
        Some(gave.amount * 10 - units(area12, 7).abs())
    } else {
        None
    };
    Ok(Measured {
        category: category.to_string(),
        line: at.to_string(),
        distance,
        bearing,
        bearing_allowed,
        area,
        area_allowed,
    })
}

/// FNV-1a, 64 bits, over `bytes`, continuing from `hash`.
fn fnv(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Each reference line a measure disagrees with, described, and how many
/// lines were checked; a digest of the answers other than [`DIGEST`] is a
/// disagreement too.
pub fn mismatches() -> (Vec<String>, usize) {
    let mut wrong = Vec::new();
    let mut count = 0;
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    for line in REFERENCE.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        count += 1;
        match measured(line) {
            Ok(m) => {
                let answers = format!("{} {:?} {:?}\n", m.distance, m.bearing, m.area);
                digest = fnv(digest, answers.as_bytes());
                // Half a micrometre, and a nanometre for a reference that
                // ends within one of a tie.
                if m.distance.abs() > 501_000 {
                    wrong.push(format!(
                        "line {} ({}): geo.distance is {} pm off",
                        m.line, m.category, m.distance
                    ));
                }
                if let Some(off) = m.bearing
                    && off.abs() > m.bearing_allowed
                {
                    wrong.push(format!(
                        "line {} ({}): geo.bearing is {off} × 10⁻¹² degrees off, {} allowed",
                        m.line, m.category, m.bearing_allowed
                    ));
                }
                if let Some(off) = m.area
                    && off.abs() > m.area_allowed
                {
                    wrong.push(format!(
                        "line {} ({}): geo.area is {off} × 10⁻⁷ m² off, {} allowed",
                        m.line, m.category, m.area_allowed
                    ));
                }
            }
            Err(problem) => wrong.push(problem),
        }
    }
    if digest != DIGEST {
        wrong.push(format!(
            "the answers' digest is {digest:#018x}, not the {DIGEST:#018x} recorded"
        ));
    }
    (wrong, count)
}
