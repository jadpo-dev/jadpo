-- query todos_by_ids: list, store main
SELECT "id", "title", "status"
FROM "todo"
WHERE "id" = ANY($1)
  AND ("due_at" IS NULL OR "due_at" > $2)
  AND "status" IN ($3, $4)
  AND "score" >= $5
  AND ("project_id" IS NOT NULL AND ("due_at" >= $2 OR "due_at" IS NULL))
  AND "todo"."owner_id" = $6
ORDER BY "created_at" ASC, "id" ASC
LIMIT 100
-- $1: argument 0 as Array(Uuid)
-- $2: clock.now as Timestamp
-- $3: literal Text("open") as Text
-- $4: literal Text("waiting") as Text
-- $5: literal Int8(10) as Int8
-- $6: caller User as Uuid
-- columns: uuid, text, enum TodoStatus
-- record TodoView: id = column 0, title = column 1, status = column 2
-- row versions of todo, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "todo"
--   WHERE "id" = ANY($1)
