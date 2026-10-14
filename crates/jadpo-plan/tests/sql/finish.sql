-- action finish: update, store main
UPDATE "todo"
SET "status" = $1,
    "updated_at" = $2
WHERE "id" = $3
RETURNING "id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at", "updated_at"
-- $1: input 1 as Text
-- $2: clock.now as Timestamp
-- $3: input 0.id as Uuid
-- returns: uuid, uuid, uuid, text, enum TodoStatus, int, instant, instant, instant
