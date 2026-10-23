-- action edit_ledger: update by id, store main
UPDATE "ledger"
SET "title" = CASE WHEN $1 THEN $2 ELSE "title" END,
    "balance" = CASE WHEN $3 THEN $4 ELSE "balance" END
WHERE "id" = $6
  AND ($8::text IS NULL OR "xmin"::text = $8)
  AND ("ledger"."owner_id" = $5 OR $7)
  AND (NOT $3 OR "ledger"."owner_id" = $5)
RETURNING "id", "owner_id", "title", "balance", "import_reference"
-- $1: whether input 1 supplied title as Bool
-- $2: input 1.title as Text
-- $3: whether input 1 supplied balance as Bool
-- $4: input 1.balance as Int8
-- $5: caller User as Uuid
-- $6: input 0 as Uuid
-- $7: caller Signed as Bool
-- $8: literal Null(Text) as Text
-- returns: uuid, uuid, text, int, text
-- no row: asks, refused by a field's own policy when the record's grants let the caller in
--   SELECT 1
--   FROM "ledger"
--   WHERE "id" = $1
--     AND ("ledger"."owner_id" = $2 OR $3)
--   $1: Input(0) as Uuid
--   $2: Caller(User) as Uuid
--   $3: Caller(Signed) as Bool
-- no row: the entity's not-found failure
-- loaded before a call: ledger row input 0, expected version in $8; no row probes version first
--   SELECT "xmin"::text
--   FROM "ledger"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
