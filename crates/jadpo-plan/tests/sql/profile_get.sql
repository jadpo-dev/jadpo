-- Profile.get: one, store main
SELECT "id", "user_id", "bio", ("profile"."user_id" = $2) IS TRUE AS "jadpo_permitted"
FROM "profile"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- columns: uuid, uuid, text, bool
-- the caller may read a row: column 3
-- record Profile: id = column 0, user_id = column 1, bio = column 2
