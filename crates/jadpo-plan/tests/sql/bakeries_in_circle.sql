-- query bakeries_in_circle: list, store main
SELECT "id", "name", "spot"
FROM "bakery"
WHERE ST_DWithin("spot", $1, $2 + 0.001)
ORDER BY "name" COLLATE "C" ASC, "id" ASC
LIMIT $3
-- $1: argument 0.centre as Geography
-- $2: argument 0.radius as Numeric
-- $3: rows to fetch as Int8
-- columns: uuid, text, geography("point")
-- record BakeryCard: id = column 0, name = column 1, spot = column 2
-- recheck: Within column 2 with argument 0
-- recheck keeps: 50
