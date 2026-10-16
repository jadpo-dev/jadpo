-- Banner.get: one, store main
SELECT "id", "board_id"
FROM "banner"
WHERE "id" = $1
  AND FALSE
-- $1: argument 0 as Uuid
-- columns: uuid, uuid
-- record Banner: id = column 0, board_id = column 1
