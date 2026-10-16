-- Pin.get: one, store main
SELECT "id", "card_id"
FROM "pin"
WHERE "id" = $1
  AND FALSE
-- $1: argument 0 as Uuid
-- columns: uuid, uuid
-- record Pin: id = column 0, card_id = column 1
