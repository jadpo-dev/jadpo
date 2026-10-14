-- query todos_by_title: page, store main
-- page: size 20, after argument 1, cursor column 1 as Text, column 3 as Int8, column 0 as Uuid
SELECT "id", "title", "status", "score"
FROM "todo"
WHERE "owner_id" = $1
  AND ($2 IS NULL OR ("title" COLLATE "C" > $2 COLLATE "C" OR ("title" COLLATE "C" = $2 COLLATE "C" AND "score" < $3) OR ("title" COLLATE "C" = $2 COLLATE "C" AND "score" = $3 AND "id" < $4)))
ORDER BY "title" COLLATE "C" ASC, "score" DESC, "id" DESC
LIMIT 21
-- $1: argument 0 as Uuid
-- $2: cursor 0 as Text
-- $3: cursor 1 as Int8
-- $4: cursor 2 as Uuid
-- columns: uuid, text, enum TodoStatus, int
-- record TodoView: id = column 0, title = column 1, status = column 2
