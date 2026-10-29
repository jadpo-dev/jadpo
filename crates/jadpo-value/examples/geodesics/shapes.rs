//! Area 20's predicates and shapes (chunk 5.5u3) against PostGIS's answers,
//! `shapes.tsv` beside this file, which tools/references/geo_cases.py writes from a
//! PostGIS server and says how: `geo.intersects`, `contains` (with a
//! `BoundingBox` and a `Circle`), `within` (with `distance:`), `centroid`
//! and `bounding_box`, each called through `jadpo_value::run` as both
//! executors call it. Shared by this example, which the gate runs under
//! WebAssembly, and the native test (crates/jadpo-runtime/tests/
//! geodesics.rs), so both targets are held to the same answers and, by a
//! digest of every answer, to the same digits.
//!
//! A yes or no must be PostGIS's, or the page's where the file says the two
//! part (tools/references/geo_cases.py lists them and why). A centroid must be within 10⁻⁹ of a
//! degree of `ST_Centroid(x, false)`'s, measured as the angle between the
//! two, so a pole's longitude doesn't count; a box's sides within 10⁻⁶ of
//! a degree of the box round PostGIS's points every 100 m along the shape.

use jadpo_std::standard::geo::{BOUNDING_BOX, CIRCLE};
use jadpo_value::{Decimal, Fault, Quantity, Value, geo, run};

pub const CASES: &str = include_str!("shapes.tsv");

/// FNV-1a over every answer Jadpo gave, case by case, so a target giving
/// one digit differently from another gives a different digest.
pub const DIGEST: u64 = 0x461b_2aa5_a9bc_5a5d;

/// How far a centroid may be from PostGIS's, in degrees.
const CENTROID: f64 = 1e-9;

/// How far a box's side may be from the box round PostGIS's points.
const SIDE: f64 = 1e-6;

/// A GeoJSON geometry, read as JSON is.
fn geometry(text: &str) -> Result<Value, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("`{text}` isn't JSON: {e}"))?;
    geo::read("Geometry", &json).map_err(|e| format!("`{text}` isn't a geometry: {e:?}"))
}

fn decimal(text: &str) -> Result<Decimal, String> {
    Decimal::parse(text).ok_or_else(|| format!("`{text}` isn't a decimal"))
}

/// Metres as a `Length`, in micrometres.
fn length(metres: &str) -> Result<Value, String> {
    let amount = metres
        .parse::<i128>()
        .map_err(|_| format!("`{metres}` isn't whole metres"))?;
    Ok(Value::Quantity(Quantity {
        kind: jadpo_std::QuantityKind::Length,
        amount: amount * 1_000_000,
    }))
}

/// A container: a `box:` or a `circle:`, or a geometry.
fn container(text: &str) -> Result<Value, String> {
    if let Some(sides) = text.strip_prefix("box:") {
        let sides: Vec<&str> = sides.split(',').collect();
        let [south, west, north, east] = sides[..] else {
            return Err(format!("`{text}` isn't four sides"));
        };
        return Ok(Value::record(
            BOUNDING_BOX,
            vec![
                ("south".into(), Value::Decimal(decimal(south)?)),
                ("west".into(), Value::Decimal(decimal(west)?)),
                ("north".into(), Value::Decimal(decimal(north)?)),
                ("east".into(), Value::Decimal(decimal(east)?)),
            ],
        ));
    }
    if let Some(rest) = text.strip_prefix("circle:") {
        let (centre, radius) = rest
            .rsplit_once(',')
            .ok_or_else(|| format!("`{text}` isn't a centre and a radius"))?;
        let centre = match geometry(centre)? {
            Value::Variant(variant) => variant
                .fields
                .first()
                .map(|(_, inner)| inner.clone())
                .ok_or("an empty geometry")?,
            other => other,
        };
        return Ok(Value::record(
            CIRCLE,
            vec![
                ("centre".into(), centre),
                ("radius".into(), length(radius)?),
            ],
        ));
    }
    geometry(text)
}

fn call(name: &str, args: &[Value]) -> Result<Value, String> {
    let operation =
        jadpo_std::operation("geo", name).ok_or_else(|| format!("`geo.{name}` isn't one"))?;
    run(operation.op, args, None, &mut |_| None)
        .map_err(|fault: Fault| format!("geo.{name} faulted: {fault:?}"))
}

/// A record's decimal field, as a float for comparing.
fn field(value: &Value, name: &str) -> Result<(Decimal, f64), String> {
    let Value::Record(record) = value else {
        return Err(format!("{value:?} isn't a record"));
    };
    match record.get(name) {
        Some(Value::Decimal(d)) => Ok((*d, d.to_string().parse().unwrap_or(f64::NAN))),
        other => Err(format!("`{name}` is {other:?}")),
    }
}

/// The angle between two positions in degrees.
fn apart((lat1, lon1): (f64, f64), (lat2, lon2): (f64, f64)) -> f64 {
    let v = |lat: f64, lon: f64| {
        let (lat, lon) = (lat.to_radians(), lon.to_radians());
        [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
    };
    let (a, b) = (v(lat1, lon1), v(lat2, lon2));
    let chord = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    (2.0 * (chord / 2.0).asin()).to_degrees()
}

/// One case: what Jadpo gave, as text for the digest, or a disagreement.
fn case(line: &str) -> Result<String, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [operation, label, a, b, extra, postgis, page] = fields[..] else {
        return Err(format!("`{line}` doesn't have seven fields"));
    };
    // Where the page's meaning and PostGIS part, the page's answer.
    let answer = match page.split_once(':') {
        Some((wanted, _)) => wanted,
        None => postgis,
    };
    let yes_or_no = |gave: Value| -> Result<String, String> {
        let gave = match gave {
            Value::Bool(true) => "t",
            Value::Bool(false) => "f",
            other => return Err(format!("{operation} {label}: gave {other:?}")),
        };
        if gave == answer {
            Ok(gave.to_string())
        } else {
            Err(format!(
                "{operation} {label}: gave {gave}, wanted {answer} (PostGIS {postgis})"
            ))
        }
    };
    match operation {
        "contains" | "intersects" | "within" => {
            yes_or_no(call(operation, &[geometry(a)?, geometry(b)?])?)
        }
        "within_distance" => yes_or_no(call(
            "within",
            &[geometry(a)?, geometry(b)?, length(extra)?],
        )?),
        "box" | "circle" => yes_or_no(call("contains", &[container(a)?, geometry(b)?])?),
        "centroid" => {
            let gave = call("centroid", &[geometry(a)?])?;
            let (lat, lat_f) = field(&gave, "latitude")?;
            let (lon, lon_f) = field(&gave, "longitude")?;
            let expected: Vec<f64> = answer.split(' ').filter_map(|n| n.parse().ok()).collect();
            let [plat, plon] = expected[..] else {
                return Err(format!("{operation} {label}: `{answer}` isn't two numbers"));
            };
            let off = apart((lat_f, lon_f), (plat, plon));
            if off > CENTROID {
                return Err(format!(
                    "{operation} {label}: gave {lat} {lon}, PostGIS {answer}, {off:e}° apart"
                ));
            }
            Ok(format!("{lat} {lon}"))
        }
        "bounding_box" => {
            let gave = call("bounding_box", &[geometry(a)?])?;
            let expected: Vec<f64> = answer.split(' ').filter_map(|n| n.parse().ok()).collect();
            let mut shown = Vec::new();
            for (name, wanted) in ["south", "west", "north", "east"].iter().zip(&expected) {
                let (side, side_f) = field(&gave, name)?;
                let mut off = (side_f - wanted).abs();
                if off > 180.0 {
                    off = (off - 360.0).abs();
                }
                if off > SIDE {
                    return Err(format!(
                        "{operation} {label}: its {name} is {side}, PostGIS's points {wanted}"
                    ));
                }
                shown.push(side.to_string());
            }
            Ok(shown.join(" "))
        }
        other => Err(format!("`{other}` isn't an operation the cases name")),
    }
}

fn fnv(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Each case Jadpo disagrees with PostGIS on, described, how many there
/// were, and the digest of Jadpo's answers; a digest other than
/// [`DIGEST`] is a disagreement too.
pub fn mismatches() -> (Vec<String>, usize, u64) {
    let mut wrong = Vec::new();
    let mut count = 0;
    let mut digest: u64 = 0xcbf2_9ce4_8422_2325;
    for line in CASES.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        count += 1;
        match case(line) {
            Ok(gave) => digest = fnv(digest, format!("{gave}\n").as_bytes()),
            Err(problem) => wrong.push(problem),
        }
    }
    if digest != DIGEST {
        wrong.push(format!(
            "the answers' digest is {digest:#018x}, not the {DIGEST:#018x} recorded"
        ));
    }
    (wrong, count, digest)
}
