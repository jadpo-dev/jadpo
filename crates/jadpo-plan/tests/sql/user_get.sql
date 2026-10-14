-- User.get: one, store main
SELECT "id", "email", "name"
FROM "user"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, text
-- record User: id = column 0, email = column 1, name = column 2
