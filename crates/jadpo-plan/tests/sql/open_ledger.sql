-- action open_ledger: create, store main
INSERT INTO "ledger" ("id", "owner_id", "title", "balance", "import_reference")
SELECT $1, $2, $3, $4, $5
WHERE $2 = $6 AND ($5 IS NULL OR FALSE)
RETURNING "id", "owner_id", "title", "balance", "import_reference"
-- $1: a new identity as Uuid
-- $2: input 0 as Uuid
-- $3: input 1 as Text
-- $4: input 2 as Int8
-- $5: input 3 as Text
-- $6: caller User as Uuid
-- returns: uuid, uuid, text, int, text
-- refused by ledger_owner_id_fkey: fault: the `User` this create refers to was deleted after it was loaded
