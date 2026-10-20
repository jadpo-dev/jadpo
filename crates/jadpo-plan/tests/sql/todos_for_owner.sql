-- query todos_for_owner: page, store main
-- page: size argument 1.page_size, after argument 1.after, cursor column 3 as Timestamp, column 0 as Uuid
SELECT "id", "title", "status", "created_at"
FROM "todo"
WHERE "owner_id" = $1
  AND ($2 IS NULL OR "status" = $2)
  AND ($3 IS NULL OR "due_at" <= $3)
  AND ($5 IS NULL OR ("created_at", "id") < ($5, $6))
  AND "todo"."owner_id" = $4
ORDER BY "created_at" DESC, "id" DESC
LIMIT $7
-- $1: argument 0 as Uuid
-- $2: argument 1.status as Text
-- $3: argument 1.due_before as Timestamp
-- $4: caller User as Uuid
-- $5: cursor 0 as Timestamp
-- $6: cursor 1 as Uuid
-- $7: page size + 1 as Int8
-- columns: uuid, text, enum TodoStatus, instant
-- record TodoView: id = column 0, title = column 1, status = column 2
-- row versions of todo, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "todo"
--   WHERE "id" = ANY($1)
