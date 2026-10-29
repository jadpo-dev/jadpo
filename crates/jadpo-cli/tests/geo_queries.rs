//! Area 20's query conditions (chunk 5.5u5), against live PostgreSQL with
//! PostGIS: each `geo.` condition and the order by `geo.distance`, run by
//! the interpreter through an action, gives exactly the records Jadpo's own
//! `geo.` operations give on the same values, through the GiST index its
//! migration builds.
//!
//! - The page's examples: bakeries nearest a door inside a radius, in a
//!   map's view and inside a patch or a circle, and the patches holding a
//!   customer or crossing a van's round, each through its index (`EXPLAIN`)
//!   and each the code's answer over two thousand bakeries and three
//!   hundred patches.
//! - Chunk 5.5u3's cases (`crates/jadpo-value/examples/geodesics/
//!   shapes.tsv`), each as a query against every case's shape at once,
//!   including the four where PostGIS's `ST_Covers` and the page part: the
//!   query gives the page's answer.
//! - The recheck asking again: a list whose first rows the code turns down,
//!   and ties at the last place kept.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools, which must have PostGIS (CI's image
//! does, `tools/fixtures/postgres/Dockerfile`; so does Postgres.app);
//! `JADPO_SKIP_POSTGRES=1` skips the live part.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Outcome, SqlValue};
use jadpo_std::read::Source;
use jadpo_std::standard::geo::{BOUNDING_BOX, CIRCLE};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::{Decimal, Quantity, Value, geo};

use crate::support;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// Each query: its name, parameters, result card, entity, `where:` line,
/// order and limit. An action `find_<name>` returns it.
const QUERIES: &[(&str, &str, &str, &str, &str, &str, u32)] = &[
    (
        "bakeries_near",
        "doorstep: Point, max_trip: SearchReach",
        "BakeryCard",
        "Bakery",
        "geo.within(Bakery.spot, doorstep, distance: max_trip)",
        "geo.distance(Bakery.spot, doorstep) asc",
        20,
    ),
    (
        "nearest_bakery",
        "doorstep: Point, max_trip: SearchReach",
        "BakeryCard",
        "Bakery",
        "geo.within(Bakery.spot, doorstep, distance: max_trip)",
        "geo.distance(Bakery.spot, doorstep) asc",
        1,
    ),
    (
        "bakeries_in_view",
        "view: BoundingBox",
        "BakeryCard",
        "Bakery",
        "geo.within(Bakery.spot, view)",
        "Bakery.name asc",
        200,
    ),
    (
        "bakeries_in_area",
        "area: Polygon",
        "BakeryCard",
        "Bakery",
        "geo.within(Bakery.spot, area)",
        "Bakery.name asc",
        200,
    ),
    (
        "bakeries_in_circle",
        "circle: Circle",
        "BakeryCard",
        "Bakery",
        "geo.within(Bakery.spot, circle)",
        "Bakery.name asc",
        200,
    ),
    (
        "patches_for",
        "customer: Point",
        "PatchCard",
        "DeliveryPatch",
        "geo.contains(DeliveryPatch.outline, customer)",
        "DeliveryPatch.name asc",
        10,
    ),
    (
        "first_patch_holding",
        "shape: Polygon",
        "PatchCard",
        "DeliveryPatch",
        "geo.contains(DeliveryPatch.outline, shape)",
        "DeliveryPatch.name asc",
        1,
    ),
    (
        "patches_crossing",
        "van_round: LineString",
        "PatchCard",
        "DeliveryPatch",
        "geo.intersects(DeliveryPatch.outline, van_round)",
        "DeliveryPatch.name asc",
        100,
    ),
    (
        "samples_containing",
        "shape: Geometry",
        "SampleCard",
        "Sample",
        "geo.contains(Sample.shape, shape)",
        "Sample.label asc",
        500,
    ),
    (
        "samples_intersecting",
        "shape: Geometry",
        "SampleCard",
        "Sample",
        "geo.intersects(Sample.shape, shape)",
        "Sample.label asc",
        500,
    ),
    (
        "samples_within_polygon",
        "area: Polygon",
        "SampleCard",
        "Sample",
        "geo.within(Sample.shape, area)",
        "Sample.label asc",
        500,
    ),
    (
        "samples_within_polygons",
        "areas: MultiPolygon",
        "SampleCard",
        "Sample",
        "geo.within(Sample.shape, areas)",
        "Sample.label asc",
        500,
    ),
    (
        "samples_in_view",
        "view: BoundingBox",
        "SampleCard",
        "Sample",
        "geo.within(Sample.spot, view)",
        "Sample.label asc",
        500,
    ),
    (
        "samples_in_circle",
        "circle: Circle",
        "SampleCard",
        "Sample",
        "geo.within(Sample.spot, circle)",
        "Sample.label asc",
        500,
    ),
    (
        "samples_near",
        "at: Point, reach: SearchReach",
        "SampleCard",
        "Sample",
        "geo.within(Sample.spot, at, distance: reach)",
        "Sample.label asc",
        500,
    ),
];

/// A project of bakeries, their delivery patches and a table of samples,
/// with each query above and an action returning it, migrated.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("geo-queries-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    let domain = |path: &str, text: &str| write(&root, &format!("domains/baking/{path}"), text);
    for (file, text) in [
        (
            "bakery_name",
            "/// What a bakery is called.\ntype BakeryName = Text {\n    max_length: 80\n}\n",
        ),
        (
            "patch_name",
            "/// What a delivery patch is called.\ntype PatchName = Text {\n    max_length: 80\n}\n",
        ),
        (
            "sample_label",
            "/// Which case a sample is.\ntype SampleLabel = Text {\n    max_length: 5000\n}\n",
        ),
        (
            "search_reach",
            "/// How far a search reaches.\ntype SearchReach = Length {\n    min: 0metre\n    max: 20000km\n}\n",
        ),
        (
            "bakery_card",
            "use baking/entities { Bakery }\n\n/// A bakery in a list.\ntype BakeryCard = Output {\n    pick: [Bakery.id, Bakery.name]\n}\n",
        ),
        (
            "patch_card",
            "use baking/entities { DeliveryPatch }\n\n/// A patch in a list.\ntype PatchCard = Output {\n    pick: [DeliveryPatch.id, DeliveryPatch.name]\n}\n",
        ),
        (
            "sample_card",
            "use baking/entities { Sample }\n\n/// A sample in a list.\ntype SampleCard = Output {\n    pick: [Sample.id, Sample.label]\n}\n",
        ),
    ] {
        domain(&format!("types/{file}.jadpo"), text);
    }
    domain(
        "entities/bakery.jadpo",
        "use baking/types { BakeryName }\n\n/// A shop that bakes and delivers.\nentity Bakery {\n    id: Uuid\n    name: BakeryName\n    spot: Point\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    domain(
        "entities/delivery_patch.jadpo",
        "use baking/types { PatchName }\n\n/// An area a bakery delivers to.\nentity DeliveryPatch {\n    id: Uuid\n    name: PatchName\n    outline: Polygon\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    domain(
        "entities/sample.jadpo",
        "use baking/types { SampleLabel }\n\n/// A shape or a point from the predicates' cases.\nentity Sample {\n    id: Uuid\n    label: SampleLabel\n    shape: Geometry?\n    spot: Point?\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    for (name, params, card, entity, condition, order, limit) in QUERIES {
        let args: Vec<&str> = params
            .split(", ")
            .map(|param| param.split(':').next().unwrap_or(param))
            .collect();
        let types = if params.contains("SearchReach") {
            format!("{card}, SearchReach")
        } else {
            card.to_string()
        };
        domain(
            &format!("queries/{name}.jadpo"),
            &format!(
                "use baking/entities {{ {entity} }}\nuse baking/types {{ {types} }}\n\n/// Records found by where they are.\nquery {name}({params}) -> List<{card}> {{\n    from: {entity}\n    where: {condition}\n    order: [{order}]\n    limit: {limit}\n}}\n"
            ),
        );
        domain(
            &format!("actions/find_{name}.jadpo"),
            &format!(
                "use baking/queries {{ {name} }}\nuse baking/types {{ {types} }}\n\n/// What the query finds.\naction find_{name}({params}) -> List<{card}> {{\n    return {name}({})\n}}\n",
                args.join(", ")
            ),
        );
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn each_geo_condition_derives_the_gist_index_it_searches() {
    let root = project("indexes");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("a migration");
    let sql = fs::read_to_string(folder.join("postgres.sql")).expect("postgres.sql");
    for index in [
        "CREATE INDEX \"bakery_spot_idx\" ON \"bakery\" USING gist (\"spot\");",
        "CREATE INDEX \"bakery_spot_box_idx\" ON \"bakery\" USING gist ((\"spot\"::geometry));",
        "CREATE INDEX \"delivery_patch_outline_idx\" ON \"delivery_patch\" USING gist (\"outline\");",
        "CREATE INDEX \"sample_shape_idx\" ON \"sample\" USING gist (\"shape\");",
        "CREATE INDEX \"sample_spot_idx\" ON \"sample\" USING gist (\"spot\");",
        "CREATE INDEX \"sample_spot_box_idx\" ON \"sample\" USING gist ((\"spot\"::geometry));",
    ] {
        assert!(sql.contains(index), "{index}\n{sql}");
    }
}

/// A migrated database for the project, its program, and a runtime to
/// call it with.
struct Live {
    url: String,
    program: Program,
    host: Harness,
    runtime: jadpo_host_native::Runtime,
}

impl Live {
    fn new(root: &Path, stem: &str) -> Live {
        let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
        let url = server.create_database(stem).expect("database");
        let applied = jadpo(root, &["migrate", "apply", "--url", &url]);
        assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
        let loaded = jadpo_db::load(root);
        let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
        let host = Harness::postgres(&url);
        Live {
            url,
            program,
            host,
            runtime: jadpo_host_native::Runtime::new().expect("runtime"),
        }
    }

    /// Runs `sql` with `params`, committed.
    fn rows(&self, sql: &str, params: &[SqlValue]) -> Vec<Vec<SqlValue>> {
        let store = jadpo_host_native::postgres(&self.url).expect("store");
        self.runtime.block_on(async {
            let mut transaction = store.begin().await.expect("begin");
            let rows = transaction
                .query(sql, params)
                .await
                .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
            transaction.commit().await.expect("commit");
            rows
        })
    }

    /// Inserts rows of `table`, each its values, in one transaction.
    fn insert(&self, sql: &str, rows: &[Vec<SqlValue>]) {
        let store = jadpo_host_native::postgres(&self.url).expect("store");
        self.runtime.block_on(async {
            let mut transaction = store.begin().await.expect("begin");
            for row in rows {
                transaction
                    .execute(sql, row)
                    .await
                    .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
            }
            transaction.commit().await.expect("commit");
        });
    }

    /// The names (or labels) of the records `find_<query>` returns, in
    /// order.
    fn call(&self, query: &str, args: Vec<Value>) -> Vec<String> {
        let action = format!("find_{query}");
        let id = self.program.plan().find(&action).expect("the action");
        let found = self.runtime.block_on(async {
            let env = Env {
                seed: Some(1),
                clock: Clock::TEST,
            };
            let mut ctx = Ctx::of(&self.program, &self.host, env);
            self.program.run(&mut ctx, id, args).await
        });
        let Outcome::Value(Value::List(found)) = found else {
            panic!("{action}: expected a list, got {found:?}");
        };
        found
            .iter()
            .map(|card| {
                let Value::Record(card) = card else {
                    panic!("not a record: {card:?}");
                };
                match card.get("name").or_else(|| card.get("label")) {
                    Some(Value::Text(name)) => name.to_string(),
                    other => panic!("no name: {other:?}"),
                }
            })
            .collect()
    }

    /// The plan PostgreSQL makes for the query's statement with `args`.
    fn explain(&self, query: &str, args: &[Value]) -> String {
        let plan = self
            .program
            .plan()
            .query_named(query)
            .expect("the query's plan");
        let params: Vec<SqlValue> = plan
            .read
            .select
            .params
            .iter()
            .map(|param| match &param.source {
                Source::Arg { index, path } => {
                    jadpo_runtime::to_sql(&at(&args[*index as usize], path), param.ty)
                        .expect("a parameter")
                }
                Source::Envelope { index, path, part } => {
                    let bounds = at(&args[*index as usize], path);
                    let side = |name: &str| match at(&bounds, &[name.to_string()]) {
                        Value::Decimal(d) => jadpo_std::Num {
                            mantissa: d.mantissa,
                            scale: d.scale,
                        },
                        other => panic!("a side {other:?}"),
                    };
                    SqlValue::Numeric(part.of(&side("west"), &side("east")).to_string())
                }
                // As many rows as the list keeps, as the runtime first
                // asks for.
                Source::Fetch => match plan.read.recheck.as_ref().and_then(|r| r.limit.as_ref()) {
                    Some(jadpo_std::read::Size::Fixed(n)) => SqlValue::Int8(*n as i64),
                    _ => SqlValue::Int8(20),
                },
                Source::Stored(value) => value.clone(),
                other => panic!("a parameter from {other:?}"),
            })
            .collect();
        let rows = self.rows(&format!("EXPLAIN {}", plan.read.select.sql), &params);
        rows.iter()
            .filter_map(|row| match row.first() {
                Some(SqlValue::Text(line)) => Some(line.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A field of a record value, by path.
fn at(value: &Value, path: &[String]) -> Value {
    let mut value = value.clone();
    for name in path {
        value = match &value {
            Value::Record(record) => record.get(name).expect("a field").clone(),
            other => panic!("not a record: {other:?}"),
        };
    }
    value
}

/// A geometry of type `ty` from its GeoJSON.
fn geometry(ty: &str, text: &str) -> Value {
    let json: serde_json::Value = serde_json::from_str(text).expect("JSON");
    geo::read(ty, &json).unwrap_or_else(|e| panic!("`{text}` isn't a {ty}: {e:?}"))
}

fn point(latitude: &str, longitude: &str) -> Value {
    geometry(
        "Point",
        &format!(r#"{{"type":"Point","coordinates":[{longitude},{latitude}]}}"#),
    )
}

fn decimal(text: &str) -> Value {
    Value::Decimal(Decimal::parse(text).expect("a decimal"))
}

/// Metres as a `Length`.
fn metres(amount: i128) -> Value {
    Value::Quantity(Quantity {
        kind: jadpo_std::QuantityKind::Length,
        amount: amount * 1_000_000,
    })
}

fn bounding_box(south: &str, west: &str, north: &str, east: &str) -> Value {
    Value::record(
        BOUNDING_BOX,
        vec![
            ("south".into(), decimal(south)),
            ("west".into(), decimal(west)),
            ("north".into(), decimal(north)),
            ("east".into(), decimal(east)),
        ],
    )
}

fn circle(centre: Value, radius: Value) -> Value {
    Value::record(
        CIRCLE,
        vec![("centre".into(), centre), ("radius".into(), radius)],
    )
}

/// Whether the code's `geo.` operation holds.
fn holds(gave: Result<Value, jadpo_value::Fault>) -> bool {
    match gave {
        Ok(Value::Bool(held)) => held,
        other => panic!("not a yes or no: {other:?}"),
    }
}

/// A UUID from a number, as version 4 lays one out.
fn uuid(n: u64) -> [u8; 16] {
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&n.to_be_bytes());
    id[8..].copy_from_slice(&n.wrapping_mul(2_654_435_761).to_be_bytes());
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    id
}

/// A geometry value's GeoJSON, which PostGIS reads.
fn geojson_of(value: &Value) -> SqlValue {
    let mut out = String::new();
    assert!(geo::write(value, &mut out), "writes {value:?}");
    SqlValue::Text(out)
}

/// The London patch the page's examples use.
const LONDON_PATCH: &str = r#"{"type":"Polygon","coordinates":[[[-0.2,51.49],[-0.05,51.49],[-0.05,51.54],[-0.2,51.54],[-0.2,51.49]]]}"#;

/// A square with a hole, and a square over the hole: the first of chunk
/// 5.5u3's four, which `ST_Covers` says it covers and the page doesn't.
const SQUARE_HOLE: &str = r#"{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]],[[2,2],[2,4],[4,4],[4,2],[2,2]]]}"#;
const SQUARE: &str = r#"{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]}"#;
const OVER_HOLE: &str = r#"{"type":"Polygon","coordinates":[[[1,1],[6,1],[6,6],[1,6],[1,1]]]}"#;

#[test]
fn the_page_s_queries_give_the_code_s_answer_through_their_indexes() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("page");
    let live = Live::new(&root, "geo_queries_page");
    // Two thousand bakeries on a grid over London, the page's three
    // places, and three at one spot far away, to tie.
    let mut bakeries: Vec<(u64, String, Value)> = Vec::new();
    for i in 0..2000u64 {
        let latitude = format!("{:.4}", 51.40 + (i / 50) as f64 * 0.0041);
        let longitude = format!("{:.4}", -0.35 + (i % 50) as f64 * 0.0097);
        bakeries.push((
            i + 1,
            format!("Bakery {i:04}"),
            point(&latitude, &longitude),
        ));
    }
    for (n, name, latitude, longitude) in [
        (3001, "Big Ben", "51.5007", "-0.1246"),
        (3002, "On the edge", "51.5", "-0.2"),
        (3003, "Eiffel Tower", "48.8566", "2.3522"),
        (3006, "Tie C", "10.0", "10.0"),
        (3004, "Tie A", "10.0", "10.0"),
        (3005, "Tie B", "10.0", "10.0"),
        (3007, "Date line east", "0.0", "180.0"),
        (3008, "Date line west", "0.5", "-180.0"),
        (3009, "Just east", "0.0", "179.5"),
        (3010, "Just west", "0.0", "-179.5"),
        (3011, "North Pole", "90.0", "37.0"),
    ] {
        bakeries.push((n, name.to_string(), point(latitude, longitude)));
    }
    live.insert(
        "INSERT INTO bakery (id, name, spot) VALUES ($1, $2, ST_GeomFromGeoJSON($3)::geography)",
        &bakeries
            .iter()
            .map(|(n, name, spot)| {
                vec![
                    SqlValue::Uuid(uuid(*n)),
                    SqlValue::Text(name.clone()),
                    geojson_of(spot),
                ]
            })
            .collect::<Vec<_>>(),
    );
    // Three hundred small patches over London, the page's patch, and six
    // far away: five squares with a hole before one without.
    let mut patches: Vec<(u64, String, Value)> = Vec::new();
    for i in 0..300u64 {
        let (south, west) = (
            51.40 + (i / 20) as f64 * 0.013,
            -0.35 + (i % 20) as f64 * 0.024,
        );
        let ring = format!(
            "[[{w},{s}],[{e},{s}],[{e},{n}],[{w},{n}],[{w},{s}]]",
            s = format!("{south:.4}"),
            n = format!("{:.4}", south + 0.01),
            w = format!("{west:.4}"),
            e = format!("{:.4}", west + 0.02)
        );
        patches.push((
            i + 1,
            format!("Patch {i:03}"),
            geometry(
                "Polygon",
                &format!(r#"{{"type":"Polygon","coordinates":[{ring}]}}"#),
            ),
        ));
    }
    patches.push((
        1001,
        "London".to_string(),
        geometry("Polygon", LONDON_PATCH),
    ));
    for n in 0..5u64 {
        patches.push((
            1002 + n,
            format!("Holed {n}"),
            geometry("Polygon", SQUARE_HOLE),
        ));
    }
    patches.push((1010, "Whole".to_string(), geometry("Polygon", SQUARE)));
    live.insert(
        "INSERT INTO delivery_patch (id, name, outline) VALUES ($1, $2, ST_GeomFromGeoJSON($3)::geography)",
        &patches
            .iter()
            .map(|(n, name, outline)| {
                vec![
                    SqlValue::Uuid(uuid(*n)),
                    SqlValue::Text(name.clone()),
                    geojson_of(outline),
                ]
            })
            .collect::<Vec<_>>(),
    );
    live.rows("ANALYZE bakery", &[]);
    live.rows("ANALYZE delivery_patch", &[]);

    let big_ben = point("51.5007", "-0.1246");
    let patch = geometry("Polygon", LONDON_PATCH);
    let view = geo::bounding_box(&patch).expect("the patch's view");
    let round = geometry(
        "LineString",
        r#"{"type":"LineString","coordinates":[[-0.1246,51.5007],[-0.0754,51.5081],[-0.03,51.53]]}"#,
    );
    let reach = circle(big_ben.clone(), metres(1500));

    // Nearest first: the code's distances, then `id`, the first twenty
    // within six kilometres.
    let mut near: Vec<(i128, [u8; 16], String)> = bakeries
        .iter()
        .filter_map(|(n, name, spot)| {
            let Ok(Value::Quantity(apart)) = geo::distance(spot, &big_ben) else {
                panic!("a distance");
            };
            (apart.amount <= 6_000_000_000).then(|| (apart.amount, uuid(*n), name.clone()))
        })
        .collect();
    near.sort();
    let wanted: Vec<String> = near
        .iter()
        .take(20)
        .map(|(_, _, name)| name.clone())
        .collect();
    assert_eq!(wanted.first().map(String::as_str), Some("Big Ben"));
    assert_eq!(
        live.call("bakeries_near", vec![big_ben.clone(), metres(6000)]),
        wanted,
        "nearest first, as `geo.distance` measures"
    );
    // By name, as `C` sorts it, each filtered as the code filters.
    let by_name = |test: &dyn Fn(&Value) -> bool, rows: &[(u64, String, Value)], most: usize| {
        let mut kept: Vec<String> = rows
            .iter()
            .filter(|(_, _, shape)| test(shape))
            .map(|(_, name, _)| name.clone())
            .collect();
        kept.sort();
        kept.truncate(most);
        kept
    };
    let in_view = by_name(
        &|spot| holds(geo::within(spot, &view, &Value::None)),
        &bakeries,
        200,
    );
    assert!(in_view.len() > 50 && in_view.contains(&"Big Ben".to_string()));
    assert!(in_view.contains(&"On the edge".to_string()));
    assert_eq!(live.call("bakeries_in_view", vec![view.clone()]), in_view);
    let in_area = by_name(
        &|spot| holds(geo::within(spot, &patch, &Value::None)),
        &bakeries,
        200,
    );
    assert!(
        in_area.contains(&"On the edge".to_string()),
        "edges included"
    );
    assert_eq!(live.call("bakeries_in_area", vec![patch.clone()]), in_area);
    let in_circle = by_name(
        &|spot| holds(geo::within(spot, &reach, &Value::None)),
        &bakeries,
        200,
    );
    assert!(in_circle.len() > 5);
    assert_eq!(
        live.call("bakeries_in_circle", vec![reach.clone()]),
        in_circle
    );
    let holding = by_name(
        &|outline| holds(geo::contains(outline, &big_ben)),
        &patches,
        10,
    );
    assert!(holding.contains(&"London".to_string()));
    assert_eq!(live.call("patches_for", vec![big_ben.clone()]), holding);
    let crossing = by_name(
        &|outline| holds(geo::intersects(outline, &round)),
        &patches,
        100,
    );
    assert!(crossing.len() > 3);
    assert_eq!(live.call("patches_crossing", vec![round.clone()]), crossing);

    // The recheck asks again: the five holed squares come first by name,
    // and the code turns each down.
    assert_eq!(
        live.call("first_patch_holding", vec![geometry("Polygon", OVER_HOLE)]),
        ["Whole"]
    );
    // Three at one spot: the first by `id`.
    assert_eq!(
        live.call(
            "nearest_bakery",
            vec![point("10.001", "10.0"), metres(1000)]
        ),
        ["Tie A"]
    );
    assert_eq!(
        live.call("bakeries_near", vec![point("10.001", "10.0"), metres(1000)]),
        ["Tie A", "Tie B", "Tie C"]
    );

    // Boxes across and onto the antimeridian, -180 being 180, and round
    // the pole, which only a box of every longitude holds.
    for view in [
        bounding_box("-1", "179", "1", "-179"),
        bounding_box("-1", "-180", "1", "-179"),
        bounding_box("-1", "179", "1", "180"),
        bounding_box("-1", "180", "1", "180"),
        bounding_box("-1", "-179", "1", "179"),
        bounding_box("80", "-180", "90", "180"),
        bounding_box("80", "0", "90", "100"),
    ] {
        let wanted = by_name(
            &|spot| holds(geo::within(spot, &view, &Value::None)),
            &bakeries,
            200,
        );
        assert_eq!(
            live.call("bakeries_in_view", vec![view.clone()]),
            wanted,
            "{view:?}"
        );
    }

    assert_eq!(
        live.call(
            "bakeries_in_view",
            vec![bounding_box("-1", "-180", "1", "-179")]
        ),
        ["Date line east", "Date line west", "Just west"],
        "a box from -180 holds a point at 180"
    );
    assert_eq!(
        live.call(
            "bakeries_in_view",
            vec![bounding_box("80", "-180", "90", "180")]
        ),
        ["North Pole"]
    );
    assert_eq!(
        live.call(
            "bakeries_in_view",
            vec![bounding_box("80", "0", "90", "100")]
        ),
        Vec::<String>::new(),
        "a pole is in only a box of every longitude"
    );

    // Each through its index.
    for (query, args, index) in [
        (
            "bakeries_near",
            vec![big_ben.clone(), metres(6000)],
            "bakery_spot_idx",
        ),
        (
            "bakeries_in_view",
            vec![view.clone()],
            "bakery_spot_box_idx",
        ),
        ("bakeries_in_area", vec![patch.clone()], "bakery_spot_idx"),
        ("bakeries_in_circle", vec![reach.clone()], "bakery_spot_idx"),
        (
            "patches_for",
            vec![big_ben.clone()],
            "delivery_patch_outline_idx",
        ),
        (
            "patches_crossing",
            vec![round.clone()],
            "delivery_patch_outline_idx",
        ),
    ] {
        let plan = live.explain(query, &args);
        assert!(
            plan.contains(&format!("Index Scan using {index}"))
                || plan.contains(&format!("Bitmap Index Scan on {index}")),
            "{query}:\n{plan}"
        );
    }
}

/// What the code says of a sample.
type Test = Box<dyn Fn(&Value) -> bool>;

/// Chunk 5.5u3's cases, beside this crate's sibling.
const CASES: &str = include_str!("../../jadpo-value/examples/geodesics/shapes.tsv");

#[test]
fn every_predicate_case_gives_the_code_s_answer_as_a_query() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("cases");
    let live = Live::new(&root, "geo_queries_cases");
    // Each case's shape (the field's side) is a sample, once each.
    struct Case<'a> {
        operation: &'a str,
        label: String,
        field: &'a str,
        value: &'a str,
        distance: &'a str,
        page: bool,
        /// Whether PostGIS's answer and the page's part here.
        parts: bool,
    }
    let mut cases = Vec::new();
    for line in CASES.lines().filter(|line| !line.starts_with('#')) {
        let parts: Vec<&str> = line.split('\t').collect();
        let [operation, label, a, b, distance, postgis, page] = parts[..] else {
            panic!("`{line}`");
        };
        let answer = page.split_once(':').map_or(postgis, |(wanted, _)| wanted);
        let (field, value) = match operation {
            "box" | "circle" => (b, a),
            _ => (a, b),
        };
        cases.push(Case {
            operation,
            label: label.to_string(),
            field,
            value,
            distance,
            page: answer == "t",
            parts: !page.is_empty(),
        });
    }
    let point_field =
        |case: &Case<'_>| matches!(case.operation, "box" | "circle" | "within_distance");
    let mut shapes: Vec<(String, Value)> = Vec::new();
    let mut spots: Vec<(String, Value)> = Vec::new();
    let mut rows = Vec::new();
    for case in &cases {
        if !matches!(
            case.operation,
            "contains" | "intersects" | "within" | "box" | "circle" | "within_distance"
        ) {
            continue;
        }
        let spot = point_field(case);
        if spot && !case.field.contains("\"Point\"") {
            continue;
        }
        let kept = if spot { &mut spots } else { &mut shapes };
        if kept.iter().any(|(text, _)| text == case.field) {
            continue;
        }
        let value = geometry(if spot { "Point" } else { "Geometry" }, case.field);
        kept.push((case.field.to_string(), value));
        let n = rows.len() as u64 + 1;
        let (shape, at) = if spot {
            (
                SqlValue::Null(jadpo_runtime::SqlType::Text),
                SqlValue::Text(case.field.to_string()),
            )
        } else {
            (
                SqlValue::Text(case.field.to_string()),
                SqlValue::Null(jadpo_runtime::SqlType::Text),
            )
        };
        rows.push(vec![
            SqlValue::Uuid(uuid(n)),
            SqlValue::Text(case.field.to_string()),
            shape,
            at,
        ]);
    }
    live.insert(
        "INSERT INTO sample (id, label, shape, spot) VALUES ($1, $2, ST_GeomFromGeoJSON($3)::geography, ST_GeomFromGeoJSON($4)::geography)",
        &rows,
    );
    let mut checked = 0;
    let mut parted = Vec::new();
    for case in &cases {
        let (query, args, test): (&str, Vec<Value>, Test) = match case.operation {
            "contains" => {
                let value = geometry("Geometry", case.value);
                let v = value.clone();
                (
                    "samples_containing",
                    vec![value],
                    Box::new(move |shape| holds(geo::contains(shape, &v))),
                )
            }
            "intersects" => {
                let value = geometry("Geometry", case.value);
                let v = value.clone();
                (
                    "samples_intersecting",
                    vec![value],
                    Box::new(move |shape| holds(geo::intersects(shape, &v))),
                )
            }
            "within" if case.value.contains("\"Polygon\"") => {
                let value = geometry("Polygon", case.value);
                let v = value.clone();
                (
                    "samples_within_polygon",
                    vec![value],
                    Box::new(move |shape| holds(geo::within(shape, &v, &Value::None))),
                )
            }
            "within" if case.value.contains("\"MultiPolygon\"") => {
                let value = geometry("MultiPolygon", case.value);
                let v = value.clone();
                (
                    "samples_within_polygons",
                    vec![value],
                    Box::new(move |shape| holds(geo::within(shape, &v, &Value::None))),
                )
            }
            "box" if case.field.contains("\"Point\"") => {
                let sides: Vec<&str> = case.value.trim_start_matches("box:").split(',').collect();
                let value = bounding_box(sides[0], sides[1], sides[2], sides[3]);
                let v = value.clone();
                (
                    "samples_in_view",
                    vec![value],
                    Box::new(move |spot| holds(geo::within(spot, &v, &Value::None))),
                )
            }
            "circle" => {
                let rest = case.value.trim_start_matches("circle:");
                let (centre, radius) = rest.rsplit_once(',').expect("a circle");
                let value = circle(
                    geometry("Point", centre),
                    metres(radius.parse().expect("metres")),
                );
                let v = value.clone();
                (
                    "samples_in_circle",
                    vec![value],
                    Box::new(move |spot| holds(geo::within(spot, &v, &Value::None))),
                )
            }
            "within_distance" => {
                let value = geometry("Point", case.value);
                let reach = metres(case.distance.parse().expect("metres"));
                let (v, r) = (value.clone(), reach.clone());
                (
                    "samples_near",
                    vec![value, reach],
                    Box::new(move |spot| holds(geo::within(spot, &v, &r))),
                )
            }
            _ => continue,
        };
        let pool = if point_field(case) { &spots } else { &shapes };
        let mut wanted: Vec<String> = pool
            .iter()
            .filter(|(_, shape)| test(shape))
            .map(|(text, _)| text.clone())
            .collect();
        wanted.sort();
        let found = live.call(query, args);
        let own = found.iter().any(|text| text == case.field);
        assert_eq!(
            own, case.page,
            "{} {}: the case's own shape, the page's answer",
            case.operation, case.label
        );
        assert_eq!(
            found, wanted,
            "{} {}: every sample, the code's answer",
            case.operation, case.label
        );
        if case.parts {
            parted.push(case.label.clone());
        }
        checked += 1;
    }
    assert!(checked > 100, "{checked} cases");
    let parted: BTreeSet<String> = parted.into_iter().collect();
    assert_eq!(
        parted,
        [
            "square square_hole",
            "square_hole around_hole",
            "square_hole over_hole",
            "two_patches one_in_each"
        ]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>(),
        "the four where `ST_Covers` and the page part, each the page's answer"
    );
}
