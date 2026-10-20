-- action rename_board: update, store main
UPDATE "board"
SET "name" = $1
WHERE "id" = $2
  AND "deleted_at" IS NULL
  AND FALSE
RETURNING "id", "name", "deleted_at"
-- $1: input 1 as Text
-- $2: input 0.id as Uuid
-- returns: uuid, text, instant
-- loaded before a call: board row input 0.id held to its version first
--   SELECT "xmin"::text
--   FROM "board"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
