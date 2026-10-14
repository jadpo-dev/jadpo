-- query todos_by_ids: list, store main
SELECT "id", "title", "status"
FROM "todo"
WHERE "id" = ANY($1)
  AND ("due_at" IS NULL OR "due_at" > $2)
  AND "status" IN ('open', 'waiting')
  AND "score" >= 10
  AND ("project_id" IS NOT NULL AND ("due_at" >= $2 OR "due_at" IS NULL))
ORDER BY "created_at" ASC, "id" ASC
LIMIT 100
-- $1: argument 0 as Array(Uuid)
-- $2: clock.now as Timestamp
-- columns: uuid, text, enum TodoStatus
-- record TodoView: id = column 0, title = column 1, status = column 2
