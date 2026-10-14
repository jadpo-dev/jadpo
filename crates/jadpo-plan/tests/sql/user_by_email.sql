-- query user_by_email: optional, store main
SELECT "id", "name"
FROM "user"
WHERE "email" = $1
-- $1: argument 0 as Text
-- columns: uuid, text
-- record UserView: id = column 0, name = column 1
