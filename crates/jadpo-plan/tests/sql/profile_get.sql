-- Profile.get: one, store main
SELECT "id", "user_id", "bio"
FROM "profile"
WHERE "id" = $1
  AND "profile"."user_id" = $2
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- columns: uuid, uuid, text
-- record Profile: id = column 0, user_id = column 1, bio = column 2
