-- Card.get: one, store main
SELECT "id", "board_id", "stage", "published_at", "archived_at", "changed_at"
FROM "card"
WHERE "id" = $1
  AND ("published_at" IS NOT NULL AND "published_at" <= $2)
-- $1: argument 0 as Uuid
-- $2: clock.now as Timestamp
-- columns: uuid, uuid, enum Stage, instant, instant, instant
-- record Card: id = column 0, board_id = column 1, stage = column 2, published_at = column 3, archived_at = column 4, changed_at = column 5
