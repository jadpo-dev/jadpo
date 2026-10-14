-- User.create: create, store main
INSERT INTO "user" ("id", "email", "name")
VALUES ($1, $2, $3)
-- $1: a new identity as Uuid
-- $2: field given 0 as Text
-- $3: field given 1 as Text
