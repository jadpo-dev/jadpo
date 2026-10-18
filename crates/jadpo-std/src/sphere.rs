//! Geometry on the sphere with great-circle edges (area 20): whether one
//! shape covers another and whether two share a point, whether a
//! polygon's rings are valid, and how far a shape reaches, for the `geo`
//! operations in `jadpo-value` and for the rule `rings_valid`, which the
//! checker reads a polygon literal by and the runtime runs on every
//! polygon built.
//!
//! Edges are great circles on a sphere of latitude and longitude, as
//! PostGIS's `geography` draws them for `ST_Covers` and `ST_Intersects`
//! (area 20, question 1). Every test is exact: points are directions in
//! whole numbers, from sines and cosines in the fixed module's working
//! values, and every predicate is the sign of a polynomial in them worked
//! out in whole numbers as wide as it needs ([`vector`]). No float decides
//! anything, so the interpreter, native builds and wasm32 give one answer,
//! and a point exactly on an edge (a corner, a meridian, the equator) is
//! found on it: inside includes the edge. Only the choice of a ring's
//! inside, the smaller side, is made from an area in working values, which
//! is exact unless the two sides are within about 10⁻³⁵ of each other.

mod bounds;
mod ring;
mod shape;
mod sweep;
mod valid;
mod vector;
mod wide;

pub use bounds::{Latitude, Reach, box_holds, outward, reach, smallest_box};
pub use shape::{Area, Position, Shape, covers, intersects};
pub use valid::rings_broken;
pub use vector::unit_vector;

/// The fixed module's working constants, worked out once, which the
/// measures on the sphere share.
pub fn working_unit() -> &'static crate::fixed::Unit {
    vector::unit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Num;

    fn at(latitude: &str, longitude: &str) -> Position {
        (
            Num::parse(latitude).expect("latitude"),
            Num::parse(longitude).expect("longitude"),
        )
    }

    /// A ring from `latitude longitude` pairs, closed.
    fn ring(corners: &[&str]) -> Vec<Position> {
        let mut out: Vec<Position> = corners
            .iter()
            .map(|corner| {
                let (latitude, longitude) = corner.split_once(' ').expect("a pair");
                at(latitude, longitude)
            })
            .collect();
        if let Some(first) = out.first().cloned() {
            out.push(first);
        }
        out
    }

    fn polygon(outer: &[&str], holes: &[&[&str]]) -> Shape {
        Shape {
            areas: vec![Area {
                outer: ring(outer),
                holes: holes.iter().map(|hole| ring(hole)).collect(),
            }],
            ..Shape::default()
        }
    }

    fn spot(latitude: &str, longitude: &str) -> Shape {
        Shape {
            points: vec![at(latitude, longitude)],
            ..Shape::default()
        }
    }

    fn line(points: &[&str]) -> Shape {
        let mut path = ring(points);
        path.pop();
        Shape {
            lines: vec![path],
            ..Shape::default()
        }
    }

    const SQUARE: &[&str] = &["0 0", "0 10", "10 10", "10 0"];

    #[test]
    fn a_point_inside_on_the_edge_or_outside() {
        let square = polygon(SQUARE, &[]);
        assert!(covers(&square, &spot("5", "5")));
        assert!(!covers(&square, &spot("-1", "5")));
        assert!(!covers(&square, &spot("5", "10.5")));
        // On a meridian edge, on the equator, at a corner: inside includes
        // the edge.
        assert!(covers(&square, &spot("3.25", "0")));
        assert!(covers(&square, &spot("0", "7.125")));
        assert!(covers(&square, &spot("10", "10")));
        // Wound the other way, the same, smaller side.
        let reversed = polygon(&["0 0", "10 0", "10 10", "0 10"], &[]);
        assert!(covers(&reversed, &spot("5", "5")));
        assert!(!covers(&reversed, &spot("-5", "5")));
    }

    #[test]
    fn edges_bow_towards_the_pole() {
        // Area 20, verified with PostGIS: a 20°-wide box from 50° N to
        // 60° N holds a point at 60.1° N and not one at 50.1° N.
        let band = polygon(&["50 -10", "50 10", "60 10", "60 -10"], &[]);
        assert!(covers(&band, &spot("60.1", "0")));
        assert!(!covers(&band, &spot("50.1", "0")));
    }

    #[test]
    fn the_antimeridian_and_the_poles() {
        let across = polygon(&["-10 170", "-10 -170", "10 -170", "10 170"], &[]);
        assert!(covers(&across, &spot("0", "180")));
        assert!(covers(&across, &spot("0", "-180")));
        assert!(covers(&across, &spot("5", "-175")));
        assert!(!covers(&across, &spot("0", "0")));
        assert!(!covers(&across, &spot("0", "165")));
        let cap = polygon(&["80 0", "80 90", "80 180", "80 -90"], &[]);
        assert!(covers(&cap, &spot("90", "0")));
        assert!(covers(&cap, &spot("90", "123")));
        assert!(covers(&cap, &spot("85", "-45")));
        assert!(!covers(&cap, &spot("75", "-45")));
        assert!(!covers(&cap, &spot("-90", "0")));
        let south = polygon(&["-70 0", "-70 -120", "-70 120"], &[]);
        assert!(covers(&south, &spot("-90", "77")));
        assert!(!covers(&south, &spot("90", "77")));
    }

    #[test]
    fn a_hole_is_outside_and_its_edge_inside() {
        let park = polygon(SQUARE, &[&["2 2", "4 2", "4 4", "2 4"]]);
        assert!(covers(&park, &spot("1", "1")));
        assert!(!covers(&park, &spot("3", "3")));
        assert!(covers(&park, &spot("3", "2")));
        assert!(covers(&park, &line(&["1 1", "1 9"])));
        assert!(!covers(&park, &line(&["1 1", "5 5"])));
        assert!(covers(&park, &polygon(&["5 5", "5 6", "6 6"], &[])));
        assert!(!covers(&park, &polygon(&["1 1", "1 6", "6 6", "6 1"], &[])));
        // Around the hole, with the same hole: covered.
        assert!(covers(
            &park,
            &polygon(
                &["1 1", "1 6", "6 6", "6 1"],
                &[&["2 2", "4 2", "4 4", "2 4"]]
            )
        ));
        assert!(covers(&park, &park));
        assert!(!covers(&park, &polygon(SQUARE, &[])));
        assert!(covers(&polygon(SQUARE, &[]), &park));
    }

    #[test]
    fn lines_cover_what_runs_along_them() {
        let meridian = line(&["0 5", "10 5"]);
        assert!(covers(&meridian, &spot("3", "5")));
        assert!(covers(&meridian, &line(&["2 5", "4 5"])));
        assert!(!covers(&meridian, &line(&["2 5", "11 5"])));
        assert!(!covers(&meridian, &spot("3", "5.000001")));
        assert!(!covers(&spot("3", "5"), &meridian));
        assert!(covers(&spot("3", "5"), &spot("3", "5")));
        let path = line(&["0 0", "0 5", "5 5"]);
        assert!(covers(&path, &line(&["0 2", "0 5", "3 5"])));
    }

    #[test]
    fn shapes_meet_or_not() {
        let square = polygon(SQUARE, &[]);
        assert!(intersects(&square, &spot("5", "5")));
        assert!(intersects(&square, &line(&["5 5", "20 20"])));
        assert!(!intersects(&square, &line(&["11 11", "20 20"])));
        assert!(intersects(
            &square,
            &polygon(&["10 10", "10 20", "20 20"], &[])
        ));
        assert!(intersects(
            &square,
            &polygon(&["-5 -5", "-5 15", "15 15", "15 -5"], &[])
        ));
        assert!(!intersects(
            &square,
            &polygon(&["20 20", "20 30", "30 30"], &[])
        ));
        let park = polygon(SQUARE, &[&["2 2", "4 2", "4 4", "2 4"]]);
        assert!(!intersects(
            &park,
            &polygon(&["2.5 2.5", "2.5 3.5", "3.5 3.5"], &[])
        ));
        assert!(intersects(
            &line(&["0 0", "10 10"]),
            &line(&["0 10", "10 0"])
        ));
        assert!(!intersects(&line(&["0 0", "1 1"]), &line(&["0 10", "1 9"])));
    }

    fn broken(outer: &[&str], holes: &[&[&str]]) -> Option<usize> {
        let shape = polygon(outer, holes);
        let area = shape.areas.first().expect("an area");
        rings_broken(&area.outer, &area.holes)
    }

    #[test]
    fn valid_rings_and_each_way_to_break_them() {
        assert_eq!(broken(SQUARE, &[]), None);
        assert_eq!(broken(SQUARE, &[&["2 2", "4 2", "4 4", "2 4"]]), None);
        // A bow tie crosses itself.
        assert_eq!(broken(&["0 0", "10 10", "10 0", "0 10"], &[]), Some(0));
        // Doubling back along an edge, and touching itself at a corner.
        assert_eq!(broken(&["0 0", "0 10", "0 5", "10 5"], &[]), Some(0));
        assert_eq!(
            broken(&["0 0", "0 10", "5 5", "10 10", "10 0", "5 5"], &[]),
            Some(0)
        );
        // A point repeated, and an edge to the opposite point.
        assert_eq!(broken(&["0 0", "0 10", "0 10", "10 10"], &[]), Some(0));
        assert_eq!(broken(&["10 0", "-10 180", "0 90"], &[]), Some(0));
        // Exactly half the globe: four points on the equator.
        assert_eq!(broken(&["0 0", "0 90", "0 180", "0 -90"], &[]), Some(0));
        // A hole outside, crossing the outer ring, touching it at a corner,
        // inside another, and touching another.
        assert_eq!(broken(SQUARE, &[&["20 20", "21 20", "21 21"]]), Some(1));
        assert_eq!(broken(SQUARE, &[&["5 5", "5 15", "6 15"]]), Some(1));
        assert_eq!(broken(SQUARE, &[&["0 0", "1 2", "2 1"]]), Some(1));
        assert_eq!(
            broken(
                SQUARE,
                &[&["1 1", "1 9", "9 9", "9 1"], &["2 2", "3 2", "3 3"]]
            ),
            Some(2)
        );
        assert_eq!(
            broken(SQUARE, &[&["1 1", "1 4", "4 4"], &["4 4", "6 4", "6 6"]]),
            Some(2)
        );
        // A ring that isn't closed, or has too few points.
        let open = vec![at("0", "0"), at("0", "1"), at("1", "1"), at("1", "0")];
        assert_eq!(rings_broken(&open, &[]), Some(0));
        let short = vec![at("0", "0"), at("0", "1"), at("0", "0")];
        assert_eq!(rings_broken(&short, &[]), Some(0));
        // Across the antimeridian and round a pole, valid.
        assert_eq!(
            broken(&["-10 170", "-10 -170", "10 -170", "10 170"], &[]),
            None
        );
        assert_eq!(broken(&["80 0", "80 90", "80 180", "80 -90"], &[]), None);
    }

    fn sides(shape: &Shape) -> (String, String, String, String) {
        let reach = reach(shape).expect("a reach");
        let (south, west, north, east) = smallest_box(&reach);
        let shown = |latitude: &Latitude, up: bool| match latitude {
            Latitude::Exact(n) => n.to_string(),
            Latitude::Worked(r) => outward(r, up).to_string(),
        };
        (
            shown(&south, false),
            west.to_string(),
            shown(&north, true),
            east.to_string(),
        )
    }

    #[test]
    fn a_box_holds_the_shape_bulges_included() {
        let s = |a: &str, b: &str, c: &str, d: &str| {
            (a.to_string(), b.to_string(), c.to_string(), d.to_string())
        };
        assert_eq!(sides(&polygon(SQUARE, &[])).1, "0");
        let band = polygon(&["50 -10", "50 10", "60 10", "60 -10"], &[]);
        let (south, west, north, east) = sides(&band);
        assert_eq!(
            (south.as_str(), west.as_str(), east.as_str()),
            ("50", "-10", "10")
        );
        // atan(tan 60° / cos 10°), rounded up to a double.
        assert_eq!(north, "60.37834812480451");
        assert_eq!(
            sides(&polygon(&["-10 170", "-10 -170", "10 -170", "10 170"], &[])).1,
            "170"
        );
        let cap = sides(&polygon(&["80 0", "80 90", "80 180", "80 -90"], &[]));
        assert_eq!(
            (cap.1.as_str(), cap.2.as_str(), cap.3.as_str()),
            ("-180", "90", "180")
        );
        assert_eq!(sides(&spot("90", "45")), s("90", "45", "90", "45"));
        assert_eq!(
            sides(&line(&["0 179", "0 -179"])),
            s("0", "179", "0", "-179")
        );
        let (s2, w2, n2, e2) = sides(&band);
        let held = |n: &str| Num::parse(n).expect("a number");
        assert!(box_holds(held(&s2), held(&w2), held(&n2), held(&e2), &band));
        assert!(!box_holds(
            held("50"),
            held("-10"),
            held("60"),
            held("10"),
            &band
        ));
        assert!(box_holds(
            held("-11"),
            held("169"),
            held("11"),
            held("-169"),
            &polygon(&["-10 170", "-10 -170", "10 -170", "10 170"], &[])
        ));
        assert!(!box_holds(
            held("-11"),
            held("-169"),
            held("11"),
            held("169"),
            &polygon(&["-10 170", "-10 -170", "10 -170", "10 170"], &[])
        ));
    }
}
