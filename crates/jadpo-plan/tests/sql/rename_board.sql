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
