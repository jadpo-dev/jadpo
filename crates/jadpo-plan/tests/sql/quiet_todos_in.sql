-- query quiet_todos_in: list, store main
SELECT "id", "title", "status"
FROM "todo"
WHERE EXISTS (SELECT 1 FROM "project" WHERE "project"."id" = "todo"."project_id" AND "name" = $1 AND ("project"."owner_id" = $2 OR $3))
  AND NOT EXISTS (SELECT 1 FROM "comment" WHERE "comment"."todo_id" = "todo"."id")
  AND "todo"."owner_id" = $2
ORDER BY "created_at" ASC, "id" ASC
LIMIT 50
-- $1: argument 0 as Text
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, text, enum TodoStatus
-- record TodoView: id = column 0, title = column 1, status = column 2
-- row versions of todo, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "todo"
--   WHERE "id" = ANY($1)
