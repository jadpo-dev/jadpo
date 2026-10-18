//! Area 20's interop (chunk 5.5u6), in the checker: an application's own
//! list of points is the standard `LineString` or `Polygon`, unless
//! declined; `geo.from_wkt` gives a `Geometry` and fails `malformed` or
//! `out_of_range`, which its `attempt` maps; `geo.to_wkt` writes any
//! geometry and nothing else.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Exported = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

fn problems(source: &str) -> Vec<String> {
    support::problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn an_application_s_own_list_of_points_is_a_line_string_or_a_polygon() {
    for (fields, extra) in [
        ("points: List<Point>", ""),
        ("coordinates: List<Point>", ""),
        // A named list of points.
        (
            "points: Stops",
            "/// A van's stops.\ntype Stops = List<Point> {\n    min_length: 2\n}\n\n",
        ),
    ] {
        let source =
            format!("{extra}/// A van's round.\ntype VanRound = Object {{\n    {fields}\n}}\n");
        assert_eq!(rules(&source), ["type.standard_exists"], "{fields}");
        let found = problems(&source);
        assert!(
            found.iter().any(|problem| problem.contains(
                "`VanRound` is the standard `LineString` or `Polygon` under another name"
            )),
            "{found:?}"
        );
    }
    // A list of the application's own point: both are the standard types.
    let own = "/// Where a van was.\ntype VanPosition = Object {\n    lat: Decimal\n    lng: Decimal\n}\n\n/// A van's round.\ntype VanRound = Object {\n    points: List<VanPosition>\n}\n";
    assert_eq!(rules(own), ["type.standard_exists", "type.standard_exists"]);
    // Declined, with the reason review reads.
    assert_eq!(
        rules(
            "/// Pins a person dropped, in no order.\ndecline type.standard_exists: \"Pins in no order, not a path or a ring\"\ntype DroppedPins = Object {\n    points: List<Point>\n}\n"
        ),
        NONE
    );
}

#[test]
fn other_lists_are_other_shapes() {
    for fields in [
        // Another field beside the list.
        "points: List<Point>\n    name: Exported",
        // Not a list of points.
        "points: List<Decimal>",
        "points: Set<Point>",
        "points: List<Point?>",
        // Not one of the list's names.
        "stops: List<Point>",
        // Optional.
        "points: List<Point>?",
    ] {
        let source = format!("/// A van's round.\ntype VanRound = Object {{\n    {fields}\n}}\n");
        assert_eq!(rules(&source), NONE, "{fields}");
    }
}

#[test]
fn wkt_is_read_into_a_geometry_whose_conditions_are_mapped() {
    let read = |arms: &str| {
        rules(&format!(
            "function shape(cell: Exported) fails Unreadable -> Geometry {{\n    return attempt geo.from_wkt(cell) {{\n{arms}    }}\n}}\n"
        ))
    };
    assert_eq!(
        read("        malformed: Unreadable\n        out_of_range: Unreadable\n"),
        NONE
    );
    assert_eq!(read("        invalid: Unreadable\n"), NONE);
    assert_ne!(
        read("        malformed: Unreadable\n"),
        NONE,
        "out_of_range unmapped"
    );
    assert_ne!(
        rules("function shape(cell: Exported) -> Geometry {\n    return geo.from_wkt(cell)\n}\n"),
        NONE,
        "a call that can fail, outside `attempt`"
    );
    assert_ne!(
        rules(
            "function shape(cell: Exported) fails Unreadable -> Polygon {\n    return attempt geo.from_wkt(cell) {\n        invalid: Unreadable\n    }\n}\n"
        ),
        NONE,
        "a `Geometry`, not a `Polygon`"
    );
}

#[test]
fn any_geometry_is_written_as_wkt_and_nothing_else() {
    for ty in [
        "Point",
        "LineString",
        "Polygon",
        "MultiPoint",
        "MultiLineString",
        "MultiPolygon",
        "Geometry",
    ] {
        assert_eq!(
            rules(&format!(
                "function sent(shape: {ty}) -> Exported {{\n    return Exported(geo.to_wkt(shape))\n}}\n"
            )),
            NONE,
            "{ty}"
        );
    }
    for ty in ["BoundingBox", "Circle", "Exported"] {
        assert_eq!(
            rules(&format!(
                "function sent(shape: {ty}) -> Exported {{\n    return Exported(geo.to_wkt(shape))\n}}\n"
            )),
            ["type.mismatch"],
            "{ty}"
        );
    }
}
