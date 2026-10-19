-- Bakery.get: one, store main
SELECT "id", "name", "spot"
FROM "bakery"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, geography("point")
-- record Bakery: id = column 0, name = column 1, spot = column 2
