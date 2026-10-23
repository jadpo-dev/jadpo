-- action rename_board: update, store main
UPDATE "board"
SET "name" = $1
WHERE "id" = $2
  AND "deleted_at" IS NULL
  AND ($3::text IS NULL OR "xmin"::text = $3)
  AND FALSE
RETURNING "id", "name", "deleted_at"
-- $1: input 1 as Text
-- $2: input 0.id as Uuid
-- $3: literal Null(Text) as Text
-- returns: uuid, text, instant
-- loaded before a call: board row input 0.id, expected version in $3; no row probes version first
--   SELECT "xmin"::text
--   FROM "board"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
