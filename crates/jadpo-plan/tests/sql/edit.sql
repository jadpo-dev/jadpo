-- action edit: update, store main
UPDATE "todo"
SET "title" = CASE WHEN $1 THEN $2 ELSE "title" END,
    "due_at" = CASE WHEN $3 THEN $4 ELSE "due_at" END,
    "project_id" = CASE WHEN $5 THEN $6 ELSE "project_id" END,
    "status" = CASE WHEN $1 THEN $7 ELSE "status" END,
    "updated_at" = CASE WHEN $9 THEN $8 ELSE "updated_at" END
WHERE "id" = $10
  AND ($12::text IS NULL OR "xmin"::text = $12)
  AND "todo"."owner_id" = $11
RETURNING "id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at", "updated_at"
-- $1: whether input 1 supplied title as Bool
-- $2: input 1.title as Text
-- $3: whether input 1 supplied due_at as Bool
-- $4: input 1.due_at as Timestamp
-- $5: whether input 1 supplied project_id as Bool
-- $6: input 1.project_id as Uuid
-- $7: input 2 as Text
-- $8: clock.now as Timestamp
-- $9: whether input 1 supplied any of title, due_at, project_id as Bool
-- $10: input 0.id as Uuid
-- $11: caller User as Uuid
-- $12: literal Null(Text) as Text
-- returns: uuid, uuid, uuid, text, enum TodoStatus, int, instant, instant, instant
-- refused by todo_project_id_fkey: not found, failure 0
-- loaded before a call: todo row input 0.id, expected version in $12; no row probes version first
--   SELECT "xmin"::text
--   FROM "todo"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
