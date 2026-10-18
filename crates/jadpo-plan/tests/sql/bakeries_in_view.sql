-- query bakeries_in_view: list, store main
SELECT "id", "name", "spot"
FROM "bakery"
WHERE ("spot"::geometry && ST_MakeEnvelope($3, $1, $4, $2, 4326) OR "spot"::geometry && ST_MakeEnvelope($5, $1, $6, $2, 4326))
ORDER BY "name" COLLATE "C" ASC, "id" ASC
LIMIT $7
-- $1: argument 0.south as Numeric
-- $2: argument 0.north as Numeric
-- $3: FirstWest of argument 0 as Numeric
-- $4: FirstEast of argument 0 as Numeric
-- $5: SecondWest of argument 0 as Numeric
-- $6: SecondEast of argument 0 as Numeric
-- $7: rows to fetch as Int8
-- columns: uuid, text, geography("point")
-- record BakeryCard: id = column 0, name = column 1, spot = column 2
-- recheck: Within column 2 with argument 0
-- recheck keeps: 200
