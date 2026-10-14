-- Profile.create: create, store main
INSERT INTO "profile" ("id", "user_id", "bio")
VALUES ($1, $2, $3)
-- $1: a new identity as Uuid
-- $2: field given 0 as Uuid
-- $3: field given 1 as Text
