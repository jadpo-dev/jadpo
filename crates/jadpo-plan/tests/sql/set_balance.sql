-- action set_balance: update by id, store main
UPDATE "ledger"
SET "balance" = $2
WHERE "id" = $3
  AND ($5::text IS NULL OR "xmin"::text = $5)
  AND ("ledger"."owner_id" = $1 OR $4)
  AND "ledger"."owner_id" = $1
RETURNING "id", "owner_id", "title", "balance", "import_reference"
-- $1: caller User as Uuid
-- $2: input 1 as Int8
-- $3: input 0 as Uuid
-- $4: caller Signed as Bool
-- $5: literal Null(Text) as Text
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
-- loaded before a call: ledger row input 0, expected version in $5; no row probes version first
--   SELECT "xmin"::text
--   FROM "ledger"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
