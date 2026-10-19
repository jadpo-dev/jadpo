# 21 · A farm's veg box rounds

A farm delivers veg boxes to drop points around Keswick, where customers
collect them: the drop points and the vans' rounds are stored with their
places, customers find the drop points they could walk to and see them
on a map, the planner finds the rounds a road closure affects, and the
farm reports each month's deliveries. It teaches stored places and
searching by them, and adding up what is stored.

A geometry field is one PostGIS `geography` column, so the database
measures in metres on the globe, as the `geo.` operations do. A query
searches by a `geo.` condition through a GiST index, then Jadpo's own
code decides each record it finds, so a query keeps exactly what the
same test in code would keep. An aggregate query adds up records in the
database instead of returning them, and gives what code adding up the
same records would give.

## What it teaches

1. **Places in a store.** A `Point` field and a `Polygon` field, each a checked `geography` column, and an index of kind `IndexKind.geographic` for lookups from outside the application. The first migration creates the PostGIS extension. See `domains/vans/entities/drop_point.jadpo`, `domains/vans/entities/van_round.jadpo`, `database/migrations/20261102T0900_initial/` and [area 20, Storage](../../docs/language/20-geospatial.md#storage).
2. **Nearest first inside a radius.** `where: geo.within(field, point, distance:)` with `order: [geo.distance(field, point) asc]`. See `domains/vans/queries/drop_points_near.jadpo` and [area 20, Queries](../../docs/language/20-geospatial.md#queries).
3. **Inside a box or a circle.** `geo.within` of a `BoundingBox` or a `Circle` parameter. See `domains/vans/queries/drop_points_in_view.jadpo` and `domains/vans/queries/drop_points_around.jadpo`.
4. **Areas that hold or touch.** `geo.contains` and `geo.intersects` on a stored polygon, the edge counting as inside. See `domains/vans/queries/rounds_serving.jadpo` and `domains/vans/queries/rounds_crossed.jadpo`.
5. **The indexes the queries need.** A GiST index for `outline` and a box index for `spot`, derived by the compiler, and none for `spot` itself, which the declared index already holds. See `database/migrations/20261102T0900_initial/postgres.sql` and [area 20, Indexes](../../docs/language/20-geospatial.md#indexes).
6. **A layer from a route.** An action builds a `FeatureCollection` from what a query found, and a `POST` route sends it bare, as plain GeoJSON. See `domains/vans/actions/drop_point_map.jadpo`, `domains/vans/routes/drop_points.jadpo` and [area 20, Interop](../../docs/language/20-geospatial.md#interop).
7. **Counts, sums and time buckets.** `group:` by `temporal.year` and `temporal.month` of a date, and `count`, `sum`, `min`, `max` and `average`, a `Money` sum among them. See `domains/vans/queries/monthly_deliveries.jadpo`, `domains/vans/types/month_of_deliveries.jadpo` and [area 6, Counts, sums and groups](../../docs/language/06-reads-and-writes.md#counts-sums-and-groups).
8. **A computed backfill.** A required field added to deliveries already stored, each row's value worked out from its own boxes by the library function the action uses. See `database/migrations/20261116T0900_pack_in_crates/`, `domains/vans/libraries/crates.jadpo` and [area 5, Computed backfills](../../docs/language/05-data.md#computed-backfills).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project; `jadpo sql` works on the project it runs in:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/21-veg-box-rounds
target/release/jadpo test examples/21-veg-box-rounds
target/release/jadpo fmt --check examples/21-veg-box-rounds
cd examples/21-veg-box-rounds && ../../target/release/jadpo sql drop_points_near
```

- `check` ends with `checked: <n> files` and no errors.
- `test` starts a disposable PostgreSQL from the machine's own tools,
  which need PostGIS (Postgres.app has it), and ends with
  `tested: 9 passed, 8 action tests, 1 route test`.
- `sql` prints the statement the query sends: PostGIS finds the drop
  points within a millimetre more than the walk, through the index, and
  Jadpo's code then decides each one and sorts them by `geo.distance`.

## Try next

- In `drop_points_near.jadpo`, delete the `where:` line. `jadpo check` reports `query.order_field`: nearest first needs the radius the index searches.
  <!-- try {"file": "domains/vans/queries/drop_points_near.jadpo", "replace": "    where: geo.within(DropPoint.spot, doorstep, distance: max_walk)\n", "with": "", "check": ["query.order_field"]} -->
- In `monthly_deliveries.jadpo`, average the boxes to `scale: 0`. The month test fails with `test.assertion_failed`: November's 9.7 boxes a delivery is now 10.
  <!-- try {"file": "domains/vans/queries/monthly_deliveries.jadpo", "replace": "scale: 1,", "with": "scale: 0,", "test": ["test.assertion_failed"], "failed": 1} -->
- Then read [22-plant-helpline](../22-plant-helpline/README.md), which stores what text means and searches by it.
