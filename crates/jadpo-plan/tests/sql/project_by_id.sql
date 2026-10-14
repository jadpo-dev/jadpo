-- query project_by_id: one, store main
SELECT "id", "owner_id", "name"
FROM "project"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, text
-- record Project: id = column 0, owner_id = column 1, name = column 2
