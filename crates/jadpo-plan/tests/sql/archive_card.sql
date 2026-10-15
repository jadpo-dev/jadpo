-- action archive_card: update, store main
UPDATE "card"
SET "stage" = 'archived',
    "archived_at" = $1,
    "changed_at" = $1
WHERE "id" = $2
  AND ("published_at" IS NOT NULL AND "published_at" <= $1)
  AND "stage" = 'live'
RETURNING "id", "board_id", "stage", "published_at", "archived_at", "changed_at"
-- $1: clock.now as Timestamp
-- $2: input 0.id as Uuid
-- returns: uuid, uuid, enum Stage, instant, instant, instant
-- refused by when: Card.archive: conflict arm 0
-- no row: asks, refused by when: Card.archive when the record is there
--   SELECT 1
--   FROM "card"
--   WHERE "id" = $1
--     AND ("published_at" IS NOT NULL AND "published_at" <= $2)
--   $1: Field { input: 0, field: "id" } as Uuid
--   $2: Now as Timestamp
