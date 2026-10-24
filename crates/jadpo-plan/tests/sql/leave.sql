-- action leave: delete by id, store main
DELETE FROM "user"
WHERE "id" = $1
  AND ($2::text IS NULL OR "xmin"::text = $2)
  AND FALSE
-- $1: input 0 as Uuid
-- $2: literal Null(Text) as Text
-- refused by comment_todo_id_fkey: conflict arm 0
-- no row: the entity's not-found failure
-- loaded before a call: user row input 0, expected version in $2; no row probes version first
--   SELECT "xmin"::text
--   FROM "user"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
