-- Project.create: create, store main
INSERT INTO "project" ("id", "owner_id", "name")
VALUES ($1, $2, $3)
-- $1: a new identity as Uuid
-- $2: field given 0 as Uuid
-- $3: field given 1 as Text
