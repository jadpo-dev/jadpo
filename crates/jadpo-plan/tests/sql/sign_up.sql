-- action sign_up: create, store main
INSERT INTO "user" ("id", "email", "name")
VALUES ($1, $2, $3)
RETURNING "id", "email", "name"
-- $1: a new identity as Uuid
-- $2: input 0 as Text
-- $3: input 1 as Text
-- returns: uuid, text, text
-- refused by user_email_key: conflict arm 0
