-- Comment.get: one, store main
SELECT "id", "todo_id", "body"
FROM "comment"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, uuid, text
-- record Comment: id = column 0, todo_id = column 1, body = column 2
-- row versions of comment, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "comment"
--   WHERE "id" = ANY($1)
