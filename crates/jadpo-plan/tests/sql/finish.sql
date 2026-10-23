-- action finish: update, store main
UPDATE "todo"
SET "status" = $1,
    "updated_at" = $2
WHERE "id" = $3
  AND ($5::text IS NULL OR "xmin"::text = $5)
  AND "todo"."owner_id" = $4
RETURNING "id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at", "updated_at"
-- $1: input 1 as Text
-- $2: clock.now as Timestamp
-- $3: input 0.id as Uuid
-- $4: caller User as Uuid
-- $5: literal Null(Text) as Text
-- returns: uuid, uuid, uuid, text, enum TodoStatus, int, instant, instant, instant
-- loaded before a call: todo row input 0.id, expected version in $5; no row probes version first
--   SELECT "xmin"::text
--   FROM "todo"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
