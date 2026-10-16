-- action leave: delete by id, store main
DELETE FROM "user"
WHERE "id" = $1
  AND FALSE
-- $1: input 0 as Uuid
-- refused by comment_todo_id_fkey: conflict arm 0
-- no row: the entity's not-found failure
