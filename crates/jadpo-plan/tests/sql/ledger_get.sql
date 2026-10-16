-- Ledger.get: one, store main
SELECT "id", "owner_id", "title", "balance", "import_reference", ("ledger"."owner_id" = $2 OR $3) IS TRUE AS "jadpo_permitted"
FROM "ledger"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- $2: caller User as Uuid
-- $3: caller Signed as Bool
-- columns: uuid, uuid, text, int, text, bool
-- the caller may read a row: column 5
-- record Ledger: id = column 0, owner_id = column 1, title = column 2, balance = column 3, import_reference = column 4
