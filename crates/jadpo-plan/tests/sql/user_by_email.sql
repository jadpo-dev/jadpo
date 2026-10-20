-- query user_by_email: optional, store main
SELECT "id", "name"
FROM "user"
WHERE "email" = $1
  AND ("user"."id" = $2 OR $3)
-- $1: argument 0 as Text
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, text
-- record UserView: id = column 0, name = column 1
-- row versions of user, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "user"
--   WHERE "id" = ANY($1)
