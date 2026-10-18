-- query bakeries_in_area: list, store main
SELECT "id", "name", "spot"
FROM "bakery"
WHERE ST_DWithin("spot", $1, 0.001)
ORDER BY "name" COLLATE "C" ASC, "id" ASC
LIMIT $2
-- $1: argument 0 as Geography
-- $2: rows to fetch as Int8
-- columns: uuid, text, geography("point")
-- record BakeryCard: id = column 0, name = column 1, spot = column 2
-- recheck: Within column 2 with argument 0
-- recheck keeps: 50
