-- action archive_card: update, store main
UPDATE "card"
SET "stage" = $1,
    "archived_at" = $2,
    "changed_at" = $2
WHERE "id" = $3
  AND ("published_at" IS NOT NULL AND "published_at" <= $2)
  AND "stage" = $4
  AND ($5::text IS NULL OR "xmin"::text = $5)
  AND FALSE
RETURNING "id", "board_id", "stage", "published_at", "archived_at", "changed_at"
-- $1: literal Text("archived") as Text
-- $2: clock.now as Timestamp
-- $3: input 0.id as Uuid
-- $4: literal Text("live") as Text
-- $5: literal Null(Text) as Text
-- returns: uuid, uuid, enum Stage, instant, instant, instant
-- refused by when: Card.archive: conflict arm 0
-- no row: asks, refused by when: Card.archive when the record is there
--   SELECT 1
--   FROM "card"
--   WHERE "id" = $1
--     AND ("published_at" IS NOT NULL AND "published_at" <= $2)
--     AND FALSE
--   $1: Field { input: 0, field: "id" } as Uuid
--   $2: Now as Timestamp
-- loaded before a call: card row input 0.id, expected version in $5; no row probes version first
--   SELECT "xmin"::text
--   FROM "card"
--   WHERE "id" = $1
--   FOR NO KEY UPDATE
