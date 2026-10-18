-- DeliveryPatch.get: one, store main
SELECT "id", "bakery_id", "outline"
FROM "delivery_patch"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, geography("polygon")
-- record DeliveryPatch: id = column 0, bakery_id = column 1, outline = column 2
