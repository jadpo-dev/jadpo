-- Patron.get: one, store main
SELECT "id", "name", ($2) IS TRUE AS "jadpo_permitted"
FROM "patron"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- $2: caller Signed as Bool
-- columns: uuid, text, bool
-- the caller may read a row: column 2
-- record Patron: id = column 0, name = column 1
-- row versions of patron, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "patron"
--   WHERE "id" = ANY($1)
