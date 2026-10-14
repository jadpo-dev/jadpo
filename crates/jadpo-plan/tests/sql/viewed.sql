-- action viewed: update by id, store main
UPDATE "project"
SET "views" = ("views" + $1)
WHERE "id" = $2
RETURNING "id", "owner_id", "name", "views"
-- $1: input 1 as Int8
-- $2: input 0 as Uuid
-- returns: uuid, uuid, text, int
-- no row: the entity's not-found failure
