-- User.get: one, store main
SELECT "id", "email", "name"
FROM "user"
WHERE "id" = $1
  AND ("user"."id" = $2 OR $3)
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, text, text
-- record User: id = column 0, email = column 1, name = column 2
