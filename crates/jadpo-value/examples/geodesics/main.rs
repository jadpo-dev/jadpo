//! Area 20's geodesic measures against a sample of GeographicLib's test
//! set, as a program the gate (tools/verify.py) builds for
//! `wasm32-unknown-unknown` and runs under Node (tools/run_wasm.mjs), so
//! the answers are checked under WebAssembly as well as natively. Its exit
//! code is how many disagree, at most 255; 255 too when there were none to
//! check.
//!
//! Natively, `cargo run --release -p jadpo-value --example geodesics --
//! bench` times each measure instead (area 20's "Chosen in chunk 5.5u2"
//! records the figures).

mod check;

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().nth(1).as_deref() == Some("bench") {
        bench::all();
        return ExitCode::SUCCESS;
    }
    let (wrong, count) = check::mismatches();
    for mismatch in &wrong {
        eprintln!("{mismatch}");
    }
    if count == 0 {
        return ExitCode::from(u8::MAX);
    }
    ExitCode::from(u8::try_from(wrong.len()).unwrap_or(u8::MAX))
}

/// How long each measure takes, on representative shapes.
#[cfg(not(target_arch = "wasm32"))]
mod bench {
    use jadpo_std::standard::geo::{LINE_STRING, POLYGON};
    use jadpo_value::{Decimal, Value, geo};
    use std::time::Instant;

    fn point(latitude: &str, longitude: &str) -> Value {
        let number = |text: &str| Decimal::parse(text).unwrap_or(Decimal::new(0, 0));
        geo::point(number(latitude), number(longitude), Value::None)
    }

    /// A ring of `n` points about 5 km around central London, each
    /// coordinate to 6 places.
    fn patch(n: i128) -> Vec<Value> {
        // cos and sin of 2πk/n to 6 places, by whole-number rotation: no
        // float decides a coordinate.
        let mut points = Vec::new();
        for k in 0..n {
            let (c, s) = turn(k, n);
            let latitude = Decimal::new(51_507_200 + 45 * c / 1000, 6);
            let longitude = Decimal::new(-127_600 + 72 * s / 1000, 6);
            points.push(geo::point(latitude, longitude, Value::None));
        }
        let first = points.first().cloned().unwrap_or(Value::None);
        points.push(first);
        points
    }

    /// cos and sin of `k/n` of a turn, times 10⁶, from a polygon of `n`
    /// sides walked with whole numbers: near enough for a benchmark's
    /// shape.
    fn turn(k: i128, n: i128) -> (i128, i128) {
        // A quarter-turn table of 0.5° steps would be exact; a regular
        // polygon's corners from a parabola are close enough here.
        let t = (4 * k * 1_000_000 / n) % 4_000_000;
        let quadrant = t / 1_000_000;
        let x = t % 1_000_000;
        // sin on [0, 1] quarter turns ≈ x(2 - x) in millionths.
        let s = x * (2_000_000 - x) / 1_000_000;
        let c = (1_000_000 - x) * (1_000_000 + x) / 1_000_000;
        match quadrant {
            0 => (c, s),
            1 => (-s, c),
            2 => (-c, -s),
            _ => (s, -c),
        }
    }

    fn time(name: &str, op: &str, args: &[Value]) {
        let operation = jadpo_std::operation("geo", op).map(|operation| operation.op);
        let Some(operation) = operation else {
            return;
        };
        let rounds = 200;
        let start = Instant::now();
        for _ in 0..rounds {
            let _ = jadpo_value::run(operation, args, None, &mut |_| None);
        }
        let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(rounds);
        println!("{name}: {each:.0} µs");
    }

    pub fn all() {
        let london = point("51.5072", "-0.1276");
        time(
            "geo.distance, 1 km along a meridian",
            "distance",
            &[london.clone(), point("51.5162", "-0.1276")],
        );
        time(
            "geo.distance, 2 km across London",
            "distance",
            &[london.clone(), point("51.5155", "-0.1076")],
        );
        time(
            "geo.distance, London to Paris",
            "distance",
            &[london.clone(), point("48.8566", "2.3522")],
        );
        time(
            "geo.distance, London to Sydney",
            "distance",
            &[london.clone(), point("-33.8688", "151.2093")],
        );
        time(
            "geo.distance, nearly antipodal",
            "distance",
            &[
                point("8.226828747671", "0"),
                point("-8.516119211674", "178.688979582629"),
            ],
        );
        time(
            "geo.bearing, London to Paris",
            "bearing",
            &[london.clone(), point("48.8566", "2.3522")],
        );
        let line = Value::record(LINE_STRING, vec![("points".into(), Value::list(patch(10)))]);
        time("geo.length, a line of 11 points", "length", &[line]);
        let polygon = |ring: Vec<Value>| {
            Value::record(
                POLYGON,
                vec![
                    ("outer".into(), Value::list(ring)),
                    ("holes".into(), Value::list(Vec::new())),
                ],
            )
        };
        let square = ["0 0", "0 1", "1 1", "1 0", "0 0"]
            .iter()
            .map(|corner| {
                let (latitude, longitude) = corner.split_once(' ').unwrap_or(("0", "0"));
                point(latitude, longitude)
            })
            .collect();
        time(
            "geo.area, the 1° square at the equator",
            "area",
            &[polygon(square)],
        );
        time(
            "geo.area, a 100-point patch",
            "area",
            &[polygon(patch(100))],
        );
        // The predicates and shapes (chunk 5.5u3).
        let doorstep = point("51.51", "-0.12");
        time(
            "geo.contains, a point in a 100-point patch",
            "contains",
            &[polygon(patch(100)), doorstep.clone()],
        );
        time(
            "geo.contains, a point in a 1,000-point patch",
            "contains",
            &[polygon(patch(1000)), doorstep.clone()],
        );
        time(
            "geo.contains, a 100-point patch in another",
            "contains",
            &[polygon(patch(100)), polygon(patch(100))],
        );
        time(
            "geo.intersects, two 100-point patches",
            "intersects",
            &[polygon(patch(100)), polygon(patch(100))],
        );
        time(
            "geo.within, 5 km of a point",
            "within",
            &[
                doorstep.clone(),
                london.clone(),
                Value::Quantity(jadpo_value::Quantity {
                    kind: jadpo_std::QuantityKind::Length,
                    amount: 5_000_000_000,
                }),
            ],
        );
        time(
            "geo.centroid, a 100-point patch",
            "centroid",
            &[polygon(patch(100))],
        );
        time(
            "geo.bounding_box, a 100-point patch",
            "bounding_box",
            &[polygon(patch(100))],
        );
        validity("rings_valid, a 100-point patch", 100);
        validity("rings_valid, a 1,000-point patch", 1_000);
        validity("rings_valid, a 10,000-point outline", 10_000);
    }

    /// How long checking a ring of `n` points takes.
    fn validity(name: &str, n: i128) {
        let ring: Vec<(jadpo_std::Num, jadpo_std::Num)> = patch(n)
            .iter()
            .filter_map(|point| match point {
                Value::Record(record) => match (record.get("latitude"), record.get("longitude")) {
                    (Some(Value::Decimal(lat)), Some(Value::Decimal(lon))) => Some((
                        jadpo_std::Num {
                            mantissa: lat.mantissa,
                            scale: lat.scale,
                        },
                        jadpo_std::Num {
                            mantissa: lon.mantissa,
                            scale: lon.scale,
                        },
                    )),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let rounds = if n > 1000 { 3 } else { 20 };
        let start = Instant::now();
        for _ in 0..rounds {
            let _ = jadpo_std::sphere::rings_broken(&ring, &[]);
        }
        let each = start.elapsed().as_secs_f64() * 1e3 / f64::from(rounds);
        println!(
            "{name}: {each:.1} ms ({:?})",
            jadpo_std::sphere::rings_broken(&ring, &[])
        );
    }
}
