//! A query giving a map layer (area 6, chunk 5.5u8), in the checker: a
//! query returning `FeatureCollection<G, P>` names its features' geometry
//! field in `geometry:`, a stored `G` every record has, and fills `P` from
//! its records' fields of the same names; it is a bounded list, which may
//! use area 20's `geo.` conditions and order, and is held to field policy
//! as an `Output` is.

use crate::support;

use jadpo_db::Layout;

const ENTITY: &str = "type PinName = Text {
    max_length: 80
}

type WalkReach = Length {
    min: 0metre
    max: 10km
}

type DropPointView = Output {
    pick: [DropPoint.id, DropPoint.name]
}

entity DropPoint {
    id: Uuid
    name: PinName
    spot: Point
    porch: Point?
    note: PinName {
        policy: FieldAccess.server_only
    }

    policy: {
        read: [Access.public]
    }
}

type PinProperties = Object {
    id: DropPoint.id
    name: PinName
}

";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{ENTITY}{source}"))],
    )
}

fn problems(source: &str) -> Vec<String> {
    support::problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{ENTITY}{source}"))],
    )
}

/// The page's layer query, with `body` in place of its sections.
fn layer(result: &str, body: &str) -> String {
    format!("query drop_point_layer() -> {result} {{\n    from: DropPoint\n{body}}}\n")
}

const SECTIONS: &str =
    "    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n    limit: 500\n";

const LAYER: &str = "FeatureCollection<Point, PinProperties>";

const NONE: &[&str] = &[];

#[test]
fn a_query_gives_a_layer_of_its_records() {
    assert_eq!(rules(&layer(LAYER, SECTIONS)), NONE);
    // With area 20's conditions and its order by distance.
    assert_eq!(
        rules(
            "query near_layer(doorstep: Point, reach: WalkReach) -> FeatureCollection<Point, PinProperties> {
    from: DropPoint
    geometry: DropPoint.spot
    where: geo.within(DropPoint.spot, doorstep, distance: reach)
    order: [geo.distance(DropPoint.spot, doorstep) asc]
    limit: 10
}
"
        ),
        NONE
    );
}

#[test]
fn a_layer_names_a_stored_geometry_of_its_type_every_record_has() {
    let with = |geometry: &str| {
        rules(&layer(
            LAYER,
            &format!("    geometry: {geometry}\n    order: [DropPoint.name asc]\n    limit: 500\n"),
        ))
    };
    assert_eq!(
        with("DropPoint.name"),
        ["query.geometry_field"],
        "not a point"
    );
    assert_eq!(
        with("DropPoint.porch"),
        ["query.geometry_field"],
        "may be none"
    );
    assert_eq!(
        with("spot"),
        ["query.geometry_field"],
        "not named with its entity"
    );
    assert_eq!(with("DropPoint.spt"), ["query.unknown_field"]);
    assert_eq!(
        rules(&layer(
            "FeatureCollection<Polygon, PinProperties>",
            SECTIONS
        )),
        ["query.geometry_field"],
        "a point isn't the layer's polygon"
    );
    let found = problems(&layer(
        LAYER,
        "    geometry: DropPoint.name\n    order: [DropPoint.name asc]\n    limit: 500\n",
    ));
    assert!(
        found[0]
            .contains("`DropPoint.name` isn't a `Point`, so it can't be each feature's geometry"),
        "{found:?}"
    );
    let found = problems(&layer(
        LAYER,
        "    geometry: DropPoint.porch\n    order: [DropPoint.name asc]\n    limit: 500\n",
    ));
    assert!(
        found[0].contains("`DropPoint.porch` may be `none`, and every feature has a geometry"),
        "{found:?}"
    );
}

#[test]
fn a_layer_is_a_bounded_list_of_its_own_records() {
    assert_eq!(
        rules(&layer(
            LAYER,
            "    order: [DropPoint.name asc]\n    limit: 500\n"
        )),
        ["query.section_required"],
        "no geometry"
    );
    assert_eq!(
        rules(&layer(
            LAYER,
            "    geometry: DropPoint.spot\n    limit: 500\n"
        )),
        ["query.section_required"],
        "no order"
    );
    assert_eq!(
        rules(&layer(
            LAYER,
            "    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n"
        )),
        ["query.section_required"],
        "no limit"
    );
    assert_eq!(
        rules(
            "type ListPins = Input {\n    after: PageCursor {\n        optional: true\n    }\n}\n\nquery drop_point_layer(filter: ListPins) -> FeatureCollection<Point, PinProperties> {\n    from: DropPoint\n    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n    limit: 500\n    page: {\n        size: 20\n        after: filter.after\n    }\n}\n"
        ),
        ["query.section_unexpected"],
        "no pages"
    );
    // Neither grouped nor including other records.
    for section in [
        "    group: {\n        name: DropPoint.name\n    }\n",
        "    include: {\n        name: DropPoint.name\n    }\n",
    ] {
        assert_eq!(
            rules(&layer(LAYER, &format!("{SECTIONS}{section}"))),
            ["query.section_unexpected"],
            "{section}"
        );
    }
    // Only a layer has a geometry.
    assert_eq!(
        rules(
            "query drop_points() -> List<DropPointView> {\n    from: DropPoint\n    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n    limit: 500\n}\n"
        ),
        ["query.section_unexpected"]
    );
}

#[test]
fn a_layer_s_properties_are_its_records_fields() {
    let found = problems(
        "type PinCard = Object {\n    name: PinName\n    title: PinName\n}\n\nquery drop_point_layer() -> FeatureCollection<Point, PinCard> {\n    from: DropPoint\n    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n    limit: 500\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("query.result_field: `PinCard.title` isn't a field of `DropPoint`"),
        "{found:?}"
    );
    assert!(!found[0].contains("include"), "{found:?}");
}

#[test]
fn a_layer_carries_only_what_every_caller_may_read() {
    let found = problems(
        "type PinNote = Object {\n    name: PinName\n    note: PinName\n}\n\nquery drop_point_layer() -> FeatureCollection<Point, PinNote> {\n    from: DropPoint\n    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n    limit: 500\n}\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains(
            "policy.output_field_unproved: `drop_point_layer` sends `DropPoint.note`, which not every caller may read"
        ),
        "{found:?}"
    );
    // Its geometry too, and the fields its conditions and order test.
    let hidden = "type PinName = Text {\n    max_length: 80\n}\n\nentity Hideout {\n    id: Uuid\n    name: PinName\n    spot: Point {\n        policy: FieldAccess.server_only\n    }\n    rank: PinName {\n        policy: FieldAccess.server_only\n    }\n\n    policy: {\n        read: [Access.public]\n    }\n}\n\ntype HideoutPin = Object {\n    name: PinName\n}\n\n";
    let hidden_rules = |query: &str| {
        support::rules(
            Layout::Loose,
            &[("main.jadpo", &format!("{hidden}{query}"))],
        )
    };
    assert_eq!(
        hidden_rules(
            "query hideouts() -> FeatureCollection<Point, HideoutPin> {\n    from: Hideout\n    geometry: Hideout.spot\n    order: [Hideout.name asc]\n    limit: 50\n}\n"
        ),
        ["policy.output_field_unproved"],
        "a restricted geometry"
    );
    let open_spot = hidden.replace(
        "    spot: Point {\n        policy: FieldAccess.server_only\n    }\n",
        "    spot: Point\n",
    );
    assert_eq!(
        support::rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                &format!(
                    "{open_spot}query hideouts() -> FeatureCollection<Point, HideoutPin> {{\n    from: Hideout\n    geometry: Hideout.spot\n    order: [Hideout.rank asc]\n    limit: 50\n}}\n"
                )
            )]
        ),
        ["policy.output_field_unproved"],
        "ordered by a restricted field"
    );
}
