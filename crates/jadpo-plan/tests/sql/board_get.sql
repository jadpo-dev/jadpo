-- Board.get: one, store main
SELECT "id", "name", "deleted_at"
FROM "board"
WHERE "id" = $1
  AND "deleted_at" IS NULL
  AND FALSE
-- $1: argument 0 as Uuid
-- columns: uuid, text, instant
-- record Board: id = column 0, name = column 1, deleted_at = column 2
