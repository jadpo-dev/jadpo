-- purge Board: after 2592000000ms, store main
DELETE FROM "board"
WHERE "id" IN (
  SELECT "id"
  FROM "board"
  WHERE "deleted_at" IS NOT NULL
    AND "deleted_at" < $1
    AND NOT EXISTS (
      SELECT 1
      FROM "banner" AS "held1"
      WHERE "held1"."board_id" = "board"."id"
    )
    AND NOT EXISTS (
      SELECT 1
      FROM "card" AS "held1"
      JOIN "pin" AS "held2" ON "held2"."card_id" = "held1"."id"
      WHERE "held1"."board_id" = "board"."id"
    )
  ORDER BY "deleted_at" ASC, "id" ASC
  LIMIT 500
  FOR UPDATE SKIP LOCKED
)
RETURNING "id"
-- $1: Cutoff as Timestamp
