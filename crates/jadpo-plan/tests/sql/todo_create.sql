-- Todo.create: create, store main
INSERT INTO "todo" ("id", "owner_id", "project_id", "title", "status", "score", "due_at", "created_at")
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
-- $1: a new identity as Uuid
-- $2: field given 0 as Uuid
-- $3: field given 1 as Uuid
-- $4: field given 2 as Text
-- $5: field given 3 as Text
-- $6: field given 4 as Int8
-- $7: field given 5 as Timestamp
-- $8: clock.now as Timestamp
