-- action viewed: update by id, store main
UPDATE "project"
SET "views" = ("views" + $1)
WHERE "id" = $2
  AND ($4::text IS NULL OR "xmin"::text = $4)
  AND "project"."owner_id" = $3
RETURNING "id", "owner_id", "name", "views"
-- $1: input 1 as Int8
-- $2: input 0 as Uuid
-- $3: caller User as Uuid
-- $4: literal Null(Text) as Text
-- returns: uuid, uuid, text, int
-- no row: the entity's not-found failure
-- loaded before a call: project row input 0, expected version in $4; no row probes version first
--   SELECT "xmin"::text
--   FROM "project"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
