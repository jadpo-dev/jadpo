-- query patches_crossing: page, store main
-- page: size 25, after argument 1, cursor column 0 as Uuid
SELECT "id", "bakery_id", "outline"
FROM "delivery_patch"
WHERE ST_DWithin("outline", $1, 0.001)
  AND ($2 IS NULL OR "id" > $2)
ORDER BY "id" ASC
LIMIT $3
-- $1: argument 0 as Geography
-- $2: cursor 0 as Uuid
-- $3: rows to fetch as Int8
-- columns: uuid, uuid, geography("polygon")
-- record DeliveryPatch: id = column 0, bakery_id = column 1, outline = column 2
-- recheck: Intersects column 2 with argument 0
