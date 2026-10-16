-- Project.get: one, store main
SELECT "id", "owner_id", "name", "views"
FROM "project"
WHERE "id" = $1
  AND ("project"."owner_id" = $2 OR $3)
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, uuid, text, int
-- record Project: id = column 0, owner_id = column 1, name = column 2, views = column 3
