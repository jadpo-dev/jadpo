-- action close_board: delete by id, store main
UPDATE "board"
SET "deleted_at" = $1
WHERE "id" = $2
  AND "deleted_at" IS NULL
  AND ($3::text IS NULL OR "xmin"::text = $3)
  AND FALSE
-- $1: clock.now as Timestamp
-- $2: input 0 as Uuid
-- $3: literal Null(Text) as Text
-- no row: the entity's not-found failure
-- loaded before a call: board row input 0, expected version in $3; no row probes version first
--   SELECT "xmin"::text
--   FROM "board"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
