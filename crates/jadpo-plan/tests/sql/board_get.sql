-- Board.get: one, store main
SELECT "id", "name", "deleted_at", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "board"
WHERE "id" = $1
  AND "deleted_at" IS NULL
-- $1: argument 0 as Uuid
-- columns: uuid, text, instant, bool
-- the caller may read a row: column 3
-- record Board: id = column 0, name = column 1, deleted_at = column 2
