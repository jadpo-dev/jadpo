-- query projects_with_open_todos: page, store main
-- page: size 20, after argument 0, cursor column 1 as Text, column 0 as Uuid
SELECT "id", "name"
FROM "project"
WHERE ($4 IS NULL OR ("name" COLLATE "C", "id") > ($4 COLLATE "C", $5))
  AND EXISTS (SELECT 1 FROM "todo" WHERE "todo"."project_id" = "project"."id" AND "status" = $1 AND "todo"."owner_id" = $2)
  AND ("project"."owner_id" = $2 OR $3)
ORDER BY "name" COLLATE "C" ASC, "id" ASC
LIMIT 21
-- $1: literal Text("open") as Text
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- $4: cursor 0 as Text
-- $5: cursor 1 as Uuid
-- columns: uuid, text
-- record ProjectCard: id = column 0, name = column 1
-- row versions of project, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "project"
--   WHERE "id" = ANY($1)
