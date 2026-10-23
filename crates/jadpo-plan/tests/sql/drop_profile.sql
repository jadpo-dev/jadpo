-- action drop_profile: delete, store main
DELETE FROM "profile"
WHERE "id" = $1
  AND ($2::text IS NULL OR "xmin"::text = $2)
  AND FALSE
-- $1: input 0.id as Uuid
-- $2: literal Null(Text) as Text
-- loaded before a call: profile row input 0.id, expected version in $2; no row probes version first
--   SELECT "xmin"::text
--   FROM "profile"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
