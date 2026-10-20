-- action leave: delete by id, store main
DELETE FROM "user"
WHERE "id" = $1
  AND FALSE
-- $1: input 0 as Uuid
-- refused by comment_todo_id_fkey: conflict arm 0
-- no row: the entity's not-found failure
-- loaded before a call: user row input 0 held to its version first
--   SELECT "xmin"::text
--   FROM "user"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
