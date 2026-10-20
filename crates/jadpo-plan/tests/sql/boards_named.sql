-- query boards_named: list, store main
SELECT "id", "name"
FROM "board"
WHERE "name" = $1
  AND "deleted_at" IS NULL
  AND FALSE
ORDER BY "id" ASC
LIMIT 10
-- $1: argument 0 as Text
-- columns: uuid, text
-- record BoardView: id = column 0, name = column 1
-- row versions of board, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "board"
--   WHERE "id" = ANY($1)
