-- Banner.get: one, store main
SELECT "id", "board_id", (FALSE) IS TRUE AS "jadpo_permitted"
FROM "banner"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, bool
-- the caller may read a row: column 2
-- record Banner: id = column 0, board_id = column 1
