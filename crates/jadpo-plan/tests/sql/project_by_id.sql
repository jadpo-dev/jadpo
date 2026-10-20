-- query project_by_id: one, store main
SELECT "id", "owner_id", "name", "views", ("project"."owner_id" = $2 OR $3) IS TRUE AS "jadpo_permitted"
FROM "project"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, uuid, text, int, bool
-- the caller may read a row: column 4
-- record Project: id = column 0, owner_id = column 1, name = column 2, views = column 3
-- row versions of project, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "project"
--   WHERE "id" = ANY($1)
