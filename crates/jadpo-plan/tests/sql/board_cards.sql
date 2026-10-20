-- query board_cards: optional, store main
SELECT "id", "name"
FROM "board"
WHERE "id" = $1
  AND "deleted_at" IS NULL
  AND FALSE
-- $1: argument 0 as Uuid
-- columns: uuid, text
-- record BoardWithCards: id = column 0, name = column 1, cards = include 0
-- row versions of board, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "board"
--   WHERE "id" = ANY($1)

-- include 0: list, matching column 0
  SELECT "key"."id", "row"."id", "row"."stage", "row"."published_at"
  FROM unnest($1) AS "key"("id")
  CROSS JOIN LATERAL (
    SELECT "board_id", "id", "stage", "published_at"
    FROM "card"
    WHERE "board_id" = "key"."id"
      AND ("published_at" IS NOT NULL AND "published_at" <= $2)
      AND FALSE
    ORDER BY "published_at" DESC, "id" DESC
    LIMIT 20
  ) AS "row"
  ORDER BY "key"."id", "row"."published_at" DESC, "row"."id" DESC
  -- $1: keys as Array(Uuid)
  -- $2: clock.now as Timestamp
  -- columns: uuid, uuid, enum Stage, instant
  -- record CardView: id = column 1, stage = column 2
  -- row versions of card, by the id in column 1:
  --   SELECT "id", "xmin"::text
  --   FROM "card"
  --   WHERE "id" = ANY($1)
