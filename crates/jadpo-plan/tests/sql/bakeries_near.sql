-- query bakeries_near: list, store main
SELECT "id", "name", "spot", ST_Distance("spot", $1) AS "jadpo_measured"
FROM "bakery"
WHERE ST_DWithin("spot", $1, $2 + 0.001)
ORDER BY "jadpo_measured", "id"
LIMIT $3
-- $1: argument 0 as Geography
-- $2: argument 1 as Numeric
-- $3: rows to fetch as Int8
-- columns: uuid, text, geography("point"), measured
-- record BakeryCard: id = column 0, name = column 1, spot = column 2
-- recheck: geo.within(column 2, argument 0, argument 1)
-- recheck order: geo.distance(column 2, argument 0), the database's in column 3, ties by column 0
-- recheck keeps: 20
