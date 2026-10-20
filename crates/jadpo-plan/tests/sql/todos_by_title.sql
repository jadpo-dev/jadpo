-- query todos_by_title: page, store main
-- page: size 20, after argument 1, cursor column 1 as Text, column 3 as Int8, column 0 as Uuid
SELECT "id", "title", "status", "score"
FROM "todo"
WHERE "owner_id" = $1
  AND ($3 IS NULL OR ("title" COLLATE "C" > $3 COLLATE "C" OR ("title" COLLATE "C" = $3 COLLATE "C" AND "score" < $4) OR ("title" COLLATE "C" = $3 COLLATE "C" AND "score" = $4 AND "id" < $5)))
  AND "todo"."owner_id" = $2
ORDER BY "title" COLLATE "C" ASC, "score" DESC, "id" DESC
LIMIT 21
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- $3: cursor 0 as Text
-- $4: cursor 1 as Int8
-- $5: cursor 2 as Uuid
-- columns: uuid, text, enum TodoStatus, int
-- record TodoView: id = column 0, title = column 1, status = column 2
-- row versions of todo, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "todo"
--   WHERE "id" = ANY($1)
