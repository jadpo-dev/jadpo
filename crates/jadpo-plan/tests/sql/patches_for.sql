-- query patches_for: list, store main
SELECT "id", "bakery_id", "outline"
FROM "delivery_patch"
WHERE ST_DWithin("outline", $1, 0.001)
ORDER BY "id" ASC
LIMIT $2
-- $1: argument 0 as Geography
-- $2: rows to fetch as Int8
-- columns: uuid, uuid, geography("polygon")
-- record DeliveryPatch: id = column 0, bakery_id = column 1, outline = column 2
-- recheck: geo.contains(column 2, argument 0)
-- recheck keeps: 10
