-- action drop_profile: delete, store main
DELETE FROM "profile"
WHERE "id" = $1
  AND FALSE
-- $1: input 0.id as Uuid
-- loaded before a call: profile row input 0.id held to its version first
--   SELECT "xmin"::text
--   FROM "profile"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
