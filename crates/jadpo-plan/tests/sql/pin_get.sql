-- Pin.get: one, store main
SELECT "id", "card_id", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "pin"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, bool
-- the caller may read a row: column 2
-- record Pin: id = column 0, card_id = column 1
