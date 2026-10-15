-- action close_board: delete by id, store main
UPDATE "board"
SET "deleted_at" = $1
WHERE "id" = $2
  AND "deleted_at" IS NULL
-- $1: clock.now as Timestamp
-- $2: input 0 as Uuid
-- no row: the entity's not-found failure
