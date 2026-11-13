-- query patches_tallied: list, store main
-- first, the records, each id in column 0:
SELECT "id", "outline"
FROM "delivery_patch"
WHERE ST_DWithin("outline", $1, 0.001)
-- $1: argument 0 as Geography
-- columns: uuid, geography("polygon")
-- record DeliveryPatch
-- recheck: geo.contains(column 1, argument 0)
-- then, the totals of those kept:
SELECT "bakery_id", "patches"
FROM (
  SELECT
    "bakery_id" AS "bakery_id",
    count(*) AS "patches"
  FROM "delivery_patch"
  WHERE ST_DWithin("outline", $1, 0.001)
    AND "id" = ANY($2)
  GROUP BY "bakery_id"
) AS "grouped"
ORDER BY "patches" DESC, "bakery_id" ASC
LIMIT 10
-- $1: argument 0 as Geography
-- $2: the ids the code kept as Array(Uuid)
-- columns: uuid, int
-- record PatchTally: bakery_id = column 0, patches = column 1
