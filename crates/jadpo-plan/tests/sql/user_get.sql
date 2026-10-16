-- User.get: one, store main
SELECT "id", "email", "name", ("user"."id" = $2 OR $3) IS TRUE AS "jadpo_permitted"
FROM "user"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, text, text, bool
-- the caller may read a row: column 3
-- record User: id = column 0, email = column 1, name = column 2
