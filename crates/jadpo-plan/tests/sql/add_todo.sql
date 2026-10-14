-- action add_todo: create, store main
INSERT INTO "todo" ("id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at", "updated_at")
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
RETURNING "id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at", "updated_at"
-- $1: a new identity as Uuid
-- $2: input 0 as Uuid
-- $3: input 1 as Uuid
-- $4: input 2 as Text
-- $5: input 3 as Text
-- $6: input 4 as Int8
-- $7: input 5 as Timestamp
-- $8: clock.now as Timestamp
-- returns: uuid, uuid, uuid, text, enum TodoStatus, int, instant, instant, instant
-- refused by todo_owner_id_fkey: not found, failure 4
-- refused by todo_project_id_fkey: fault: the `Project` this create refers to was deleted after it was loaded
