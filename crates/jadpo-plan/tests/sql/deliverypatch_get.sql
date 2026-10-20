-- DeliveryPatch.get: one, store main
SELECT "id", "bakery_id", "outline"
FROM "delivery_patch"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, geography("polygon")
-- record DeliveryPatch: id = column 0, bakery_id = column 1, outline = column 2
-- row versions of delivery_patch, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "delivery_patch"
--   WHERE "id" = ANY($1)
