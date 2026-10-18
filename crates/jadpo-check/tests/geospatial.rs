//! Area 19, the geospatial types (chunk 5.5s), in the checker: a point
//! built by its names, its latitude and longitude kept in range where a
//! literal writes them; an application's own `{ lat, lng }` refused as the
//! standard `Point` unless declined; a geometry has no single text form,
//! since one would hold an axis order; a bounding box's sides ordered; and
//! every geometry stored, with a geographic index declared on one (area
//! 20, chunk 5.5u4).

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
fn every_geometry_is_stored() {
    // Area 20, "Storage" (chunk 5.5u4): a geometry in its `geography`
    // column, a box and a circle in named columns.
    for ty in [
        "Point",
        "LineString",
        "Polygon",
        "MultiPoint",
        "MultiLineString",
        "MultiPolygon",
        "Geometry",
        "BoundingBox",
        "Circle",
        "Point?",
    ] {
        assert_eq!(
            checked(&format!(
                "entity Depot {{\n    id: Uuid\n    at: {ty}\n}}\n"
            )),
            NONE,
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

#[test]
fn a_geographic_index_is_on_one_geometry_field() {
    // Area 20, "Indexes" (chunk 5.5u4): declared with `kind:`.
    let entity = |indexes: &str| {
        checked(&format!(
            "entity Depot {{\n    id: Uuid\n    name: Text\n    at: Point\n    area: Polygon\n\n    indexes: {{\n{indexes}    }}\n}}\n"
        ))
    };
    assert_eq!(
        entity(
            "        by_at: {\n            fields: [at]\n            kind: IndexKind.geographic\n        }\n"
        ),
        NONE
    );
    assert_eq!(
        entity(
            "        by_at: {\n            fields: [at]\n            kind: IndexKind.geographic\n        }\n        by_at_plain: [at]\n"
        ),
        NONE,
        "a plain index on the same field is another index"
    );
    assert_eq!(
        entity("        by_at: {\n            fields: [at]\n        }\n"),
        NONE,
        "a block with no kind is a plain index"
    );
    assert_eq!(
        entity(
            "        by_name: {\n            fields: [name]\n            kind: IndexKind.geographic\n        }\n"
        ),
        ["type.setting_value"],
        "not a geometry"
    );
    assert_eq!(
        entity(
            "        by_both: {\n            fields: [at, area]\n            kind: IndexKind.geographic\n        }\n"
        ),
        ["type.setting_value"],
        "two fields"
    );
    assert_eq!(
        entity(
            "        by_at: {\n            fields: [at]\n            kind: IndexKind.spatial\n        }\n"
        ),
        ["type.setting_value"],
        "no such kind"
    );
    assert_eq!(
        entity("        by_at: {\n            fields: [at]\n            method: gist\n        }\n"),
        ["type.unknown_setting"],
        "no such setting"
    );
    assert_eq!(
        entity("        by_at: {\n            kind: IndexKind.geographic\n        }\n"),
        ["data.field_list_required"],
        "no fields"
    );
    assert_eq!(
        entity(
            "        by_at: {\n            fields: [at]\n            kind: IndexKind.geographic\n        }\n        again: {\n            fields: [at]\n            kind: IndexKind.geographic\n        }\n"
        ),
        ["data.duplicate_rule"],
        "the same index twice"
    );
}

// --- query conditions (area 20, "Queries"; chunk 5.5u5) ----------------------

/// A bakery with a spot and a patch, and a query of its cards with
/// `lines` (its `where:` lines and the rest) and the parameters every
/// condition below uses.
fn query(lines: &str) -> Vec<String> {
    query_returning("List<BakeryCard>", lines)
}

fn query_returning(result: &str, lines: &str) -> Vec<String> {
    checked(&format!(
        "type BakeryName = Text {{\n    max_length: 80\n}}\n\ntype SearchReach = Length {{\n    min: 0metre\n    max: 50km\n}}\n\ntype Gap = Length {{}}\n\nentity Bakery {{\n    id: Uuid\n    name: BakeryName\n    spot: Point\n    patch: Polygon?\n    reach: Circle\n}}\n\ntype BakeryCard = Output {{\n    pick: [Bakery.id, Bakery.name]\n}}\n\n/// Bakeries found by where they are.\nquery found(doorstep: Point, max_trip: SearchReach, area: Polygon, areas: MultiPolygon, view: BoundingBox, circle: Circle, line: LineString, shape: Geometry, plain: Gap, name: BakeryName, page: PageSize, after: PageCursor?) -> {result} {{\n    from: Bakery\n{lines}}}\n"
    ))
}

const BY_NAME: &str = "    order: [Bakery.name asc]\n    limit: 20\n";

#[test]
fn each_geo_condition_is_a_query_condition() {
    for condition in [
        "geo.within(Bakery.spot, doorstep, distance: max_trip)",
        "geo.within(Bakery.spot, doorstep, distance: 5km)",
        "geo.within(Bakery.spot, area)",
        "geo.within(Bakery.spot, areas)",
        "geo.within(Bakery.spot, view)",
        "geo.within(Bakery.spot, circle)",
        "geo.within(Bakery.patch, area)",
        "geo.contains(Bakery.patch, doorstep)",
        "geo.contains(Bakery.patch, line)",
        "geo.contains(Bakery.patch, shape)",
        "geo.intersects(Bakery.patch, area)",
        "geo.intersects(Bakery.spot, shape)",
    ] {
        assert_eq!(
            query(&format!("    where: {condition}\n{BY_NAME}")),
            NONE,
            "{condition}"
        );
    }
    assert_eq!(
        query(&format!(
            "    where: Bakery.name == name and geo.within(Bakery.spot, view)\n{BY_NAME}"
        )),
        NONE,
        "beside another condition, joined by `and`"
    );
    assert_eq!(
        query_returning(
            "Page<BakeryCard>",
            "    where: geo.within(Bakery.spot, view)\n    order: [Bakery.name asc]\n    page: {\n        size: page\n        after: after\n    }\n"
        ),
        NONE,
        "a page"
    );
}

#[test]
fn a_geo_condition_is_refused_where_no_index_serves_it() {
    let refused = |condition: &str| query(&format!("    where: {condition}\n{BY_NAME}"));
    assert_eq!(
        refused("geo.within(doorstep, Bakery.spot, distance: max_trip)"),
        ["query.condition_unsupported"],
        "the field second"
    );
    assert_eq!(
        refused("geo.within(Bakery.patch, view)"),
        ["query.condition_unsupported"],
        "a box on a field that isn't a point (question 11)"
    );
    assert_eq!(
        refused("not geo.within(Bakery.spot, view)"),
        ["query.condition_unsupported"],
        "under `not`"
    );
    assert_eq!(
        refused("Bakery.name == name or geo.within(Bakery.spot, view)"),
        ["query.condition_unsupported"],
        "under `or`"
    );
    assert_eq!(
        refused("geo.within(Bakery.reach, area)"),
        ["query.condition_unsupported"],
        "a circle's field is kept in named columns"
    );
    assert_eq!(
        refused("geo.area(Bakery.patch) <= plain"),
        ["query.condition_unsupported"],
        "any other function of a field"
    );
    assert_eq!(
        query(&format!(
            "    where_present: geo.within(Bakery.spot, view)\n{BY_NAME}"
        )),
        ["query.condition_unsupported"],
        "in `where_present:`"
    );
}

#[test]
fn a_geo_condition_s_values_are_typed_as_in_code() {
    let refused = |condition: &str| query(&format!("    where: {condition}\n{BY_NAME}"));
    assert_eq!(
        refused("geo.within(Bakery.patch, doorstep, distance: max_trip)"),
        ["type.mismatch"],
        "a distance from a polygon (question 10)"
    );
    assert_eq!(
        refused("geo.within(Bakery.spot, area, distance: max_trip)"),
        ["type.mismatch"],
        "a distance to a polygon"
    );
    assert_eq!(
        refused("geo.within(Bakery.patch, circle)"),
        ["type.mismatch"],
        "a circle holds points only"
    );
    assert_eq!(
        refused("geo.within(Bakery.spot, doorstep)"),
        ["type.mismatch"],
        "inside a point"
    );
    assert_eq!(
        refused("geo.contains(Bakery.patch, name)"),
        ["type.mismatch"],
        "text isn't a geometry"
    );
    assert_eq!(
        refused("geo.intersects(Bakery.name, area)"),
        ["type.mismatch"],
        "nor is a text field"
    );
    assert_eq!(
        refused("geo.within(Bakery.spot, doorstep, distance: plain)"),
        ["type.argument_negative"],
        "a length that may be negative"
    );
    assert_eq!(
        refused("geo.within(Bakery.spot, doorstep, distance: -5km)"),
        ["type.argument_negative"],
        "a negative length"
    );
    assert_eq!(
        refused("geo.within(Bakery.spot, doorstep, distance: name)"),
        ["type.mismatch"],
        "a distance is a length"
    );
}

#[test]
fn nearest_first_inside_a_radius() {
    let near = "    where: geo.within(Bakery.spot, doorstep, distance: max_trip)\n";
    assert_eq!(
        query(&format!(
            "{near}    order: [geo.distance(Bakery.spot, doorstep) asc]\n    limit: 20\n"
        )),
        NONE
    );
    assert_eq!(
        query(&format!(
            "{near}    order: [geo.distance(Bakery.spot, doorstep) desc]\n    limit: 20\n"
        )),
        ["query.order_direction"],
        "farthest first"
    );
    assert_eq!(
        query("    order: [geo.distance(Bakery.spot, doorstep) asc]\n    limit: 20\n"),
        ["query.order_field"],
        "no radius (question 4)"
    );
    assert_eq!(
        query(
            "    where: geo.within(Bakery.spot, circle)\n    order: [geo.distance(Bakery.spot, doorstep) asc]\n    limit: 20\n"
        ),
        ["query.order_field"],
        "a radius from another point"
    );
    assert_eq!(
        query(&format!(
            "{near}    order: [geo.distance(Bakery.spot, doorstep) asc, Bakery.name asc]\n    limit: 20\n"
        )),
        ["query.order_field"],
        "the only order"
    );
    assert_eq!(
        query_returning(
            "Page<BakeryCard>",
            &format!(
                "{near}    order: [geo.distance(Bakery.spot, doorstep) asc]\n    page: {{\n        size: page\n        after: after\n    }}\n"
            )
        ),
        ["query.order_field"],
        "a page by distance"
    );
    assert_eq!(
        query(&format!(
            "{near}    order: [geo.area(Bakery.patch) asc]\n    limit: 20\n"
        )),
        ["query.order_field"],
        "any other function of a field"
    );
}

#[test]
fn a_geo_condition_derives_its_gist_index() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            "entity Bakery {\n    id: Uuid\n    spot: Point\n    patch: Polygon\n}\n\n/// Near a door.\nquery near(doorstep: Point) -> List<Bakery> {\n    from: Bakery\n    where: geo.within(Bakery.spot, doorstep, distance: 5km)\n    order: [geo.distance(Bakery.spot, doorstep) asc]\n    limit: 20\n}\n\n/// In view.\nquery seen(view: BoundingBox) -> List<Bakery> {\n    from: Bakery\n    where: geo.within(Bakery.spot, view)\n    order: [Bakery.id asc]\n    limit: 20\n}\n\n/// Holding a door.\nquery holding(doorstep: Point) -> List<Bakery> {\n    from: Bakery\n    where: geo.contains(Bakery.patch, doorstep)\n    order: [Bakery.id asc]\n    limit: 20\n}\n",
        )],
    );
    let schema = jadpo_check::data::schema(&db, project);
    let table = &schema.stores["main"].tables["bakery"];
    let gist: Vec<(String, Vec<String>, Option<String>)> = table
        .indexes
        .iter()
        .filter_map(|(name, index)| {
            let method = index.method.as_ref()?;
            Some((name.clone(), index.columns.clone(), method.cast.clone()))
        })
        .collect();
    assert_eq!(
        gist,
        [
            (
                "bakery_patch_idx".to_string(),
                vec!["patch".to_string()],
                None
            ),
            (
                "bakery_spot_box_idx".to_string(),
                vec!["spot".to_string()],
                Some("geometry".to_string())
            ),
            (
                "bakery_spot_idx".to_string(),
                vec!["spot".to_string()],
                None
            ),
        ]
    );
}

#[test]
fn a_geo_condition_isn_t_counted_by_the_database_yet() {
    // The code decides a `geo.` condition's records, and an aggregate's
    // totals are the database's (chunk 5.5u5).
    assert_eq!(
        checked(
            "entity Bakery {\n    id: Uuid\n    spot: Point\n}\n\ntype BakeryCount = Output {\n    bakeries: Int\n}\n\n/// How many bakeries a map shows.\nquery counted(view: BoundingBox) -> BakeryCount {\n    from: Bakery\n    where: geo.within(Bakery.spot, view)\n    aggregate: {\n        bakeries: count()\n    }\n}\n"
        ),
        ["toolchain.not_built_yet"]
    );
    assert_eq!(
        checked(
            "entity Bakery {\n    id: Uuid\n    spot: Point\n    area: Polygon\n}\n\nentity Stall {\n    id: Uuid\n    bakery_id: Bakery.id {\n        on_delete: OnDelete.cascade\n    }\n    spot: Point\n}\n\ntype StallView = Output {\n    pick: [Stall.id]\n}\n\ntype BakeryStalls = Output {\n    pick: [Bakery.id]\n    stalls: List<StallView>\n}\n\n/// A bakery's stalls in a view.\nquery stalls_in(bakery_id: Bakery.id, view: BoundingBox) -> BakeryStalls? {\n    from: Bakery\n    where: Bakery.id == bakery_id\n    include: {\n        stalls: {\n            via: Stall.bakery_id\n            where: geo.within(Stall.spot, view)\n            order: [Stall.id asc]\n            limit: 10\n        }\n    }\n}\n"
        ),
        ["toolchain.not_built_yet"],
        "nor are an include's records rechecked yet"
    );
}
