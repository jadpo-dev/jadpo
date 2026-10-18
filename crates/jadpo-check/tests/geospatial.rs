//! Area 19, the geospatial types (chunk 5.5s), in the checker: a point
//! built by its names, its latitude and longitude kept in range where a
//! literal writes them; an application's own `{ lat, lng }` refused as the
//! standard `Point` unless declined; a geometry has no single text form,
//! since one would hold an axis order; a bounding box's sides ordered; and
//! storing a geometry waits on the geospatial area, while the cells, tiles
//! and projected coordinates store today.

use crate::support;

use jadpo_db::{Database, Layout};

const NAMES: &str = "type Wire = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_point_is_built_by_its_names_within_range() {
    let point = |latitude: &str, longitude: &str| {
        rules(&format!(
            "function depot() -> Point {{\n    return Point {{\n        latitude: {latitude}\n        longitude: {longitude}\n        altitude: none\n    }}\n}}\n"
        ))
    };
    assert_eq!(point("51.5072", "-0.1276"), NONE);
    assert_eq!(point("-90.0", "180.0"), NONE, "a pole and the antimeridian");
    assert_eq!(
        point("90.5", "0.0"),
        ["type.invalid_literal"],
        "a latitude past the pole"
    );
    assert_eq!(
        point("0.0", "-180.5"),
        ["type.invalid_literal"],
        "a longitude past the antimeridian"
    );
    assert_eq!(
        rules(
            "function summit() -> Point {\n    return Point {\n        latitude: 27.9881\n        longitude: 86.925\n        altitude: 8848.86metre\n    }\n}\n"
        ),
        NONE,
        "a height above the ellipsoid, as a `Length`"
    );
    assert_eq!(
        rules("function north(at: Point) -> Latitude {\n    return at.latitude\n}\n"),
        NONE
    );
    assert_eq!(
        rules("function north(at: Point) -> Latitude {\n    return at.lat\n}\n"),
        ["type.unknown_field"],
        "a point's parts are named in full"
    );
}

#[test]
fn an_application_s_own_point_is_the_standard_point() {
    for fields in [
        "lat: Decimal\n    lng: Decimal",
        "latitude: Decimal\n    longitude: Decimal",
        "lon: Longitude\n    lat: Latitude",
        "lat: Int\n    long: Int",
    ] {
        let source =
            format!("/// Where a van was.\ntype VanPosition = Object {{\n    {fields}\n}}\n");
        assert_eq!(rules(&source), ["type.standard_exists"], "{fields}");
    }
    let problems = support::problems(
        Layout::Loose,
        &[(
            "main.jadpo",
            "/// Where a van was.\ntype VanPosition = Object {\n    lat: Decimal\n    lng: Decimal\n}\n",
        )],
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem
                .contains("`VanPosition` is the standard `Point` under another name")),
        "{problems:?}"
    );
    // Declined, with the reason review reads; and a third field is
    // another shape.
    assert_eq!(
        rules(
            "/// A cell on the warehouse floor plan.\ndecline type.standard_exists: \"Shelf rows and columns, not WGS84 degrees\"\ntype ShelfCell = Object {\n    lat: Int\n    lng: Int\n}\n"
        ),
        NONE
    );
    assert_eq!(
        rules(
            "/// A van's position and speed.\ntype VanFix = Object {\n    lat: Decimal\n    lng: Decimal\n    speed: Decimal\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_geometry_has_no_text_of_its_own() {
    assert_eq!(
        rules("function sent(at: Point) -> Wire {\n    return Wire(text.from(at))\n}\n"),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(
            "function typed(raw: Wire) fails Unreadable -> Polygon {\n    var area: Polygon = attempt text.parse(raw, as: Polygon) {\n        malformed: Unreadable\n    }\n    return area\n}\n"
        ),
        ["type.mismatch"]
    );
    // A latitude is a decimal, read and written as one.
    assert_eq!(
        rules("function sent(north: Latitude) -> Wire {\n    return Wire(text.from(north))\n}\n"),
        NONE
    );
}

#[test]
fn a_bounding_box_s_south_is_at_or_below_its_north() {
    let area = |south: &str, north: &str| {
        rules(&format!(
            "function search() -> BoundingBox {{\n    return BoundingBox {{\n        south: {south}\n        west: -0.51\n        north: {north}\n        east: 0.33\n    }}\n}}\n"
        ))
    };
    assert_eq!(area("51.28", "51.69"), NONE);
    assert_eq!(area("51.28", "51.28"), NONE, "a box with no height");
    assert_eq!(area("51.69", "51.28"), ["type.invalid_literal"]);
}

/// The rules a project of `source` gives, checked whole.
fn checked(source: &str) -> Vec<String> {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

#[test]
fn storing_a_geometry_waits_on_the_geospatial_area() {
    for ty in ["Point", "Polygon", "Geometry", "BoundingBox", "Circle"] {
        assert_eq!(
            checked(&format!(
                "entity Depot {{\n    id: Uuid\n    at: {ty}\n}}\n"
            )),
            ["toolchain.not_built_yet"],
            "{ty}"
        );
    }
    assert_eq!(
        checked(
            "entity Depot {\n    id: Uuid\n    latitude: Latitude\n    longitude: Longitude\n    cell: H3Cell\n    tile: MapTile\n    grid: EastingNorthing\n}\n"
        ),
        NONE
    );
}
