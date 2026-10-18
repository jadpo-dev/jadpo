-- action add_card: create, store main
INSERT INTO "card" ("id", "board_id", "stage", "published_at", "archived_at", "changed_at")
SELECT $1, $2, $3, $4, NULL, $5
WHERE FALSE
RETURNING "id", "board_id", "stage", "published_at", "archived_at", "changed_at"
-- $1: a new identity as Uuid
-- $2: input 0 as Uuid
-- $3: literal Text("draft") as Text
-- $4: input 1 as Timestamp
-- $5: clock.now as Timestamp
-- returns: uuid, uuid, enum Stage, instant, instant, instant
-- refused by card_board_id_fkey: fault: the `Board` this create refers to was deleted after it was loaded
