-- Todo.get: one, store main
SELECT "id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at", "updated_at"
FROM "todo"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, uuid, text, enum TodoStatus, int, instant, instant, instant
-- record Todo: id = column 0, owner_id = column 1, project_id = column 2, title = column 3, status = column 4, score = column 5, due_at = column 6, created_at = column 7, updated_at = column 8
