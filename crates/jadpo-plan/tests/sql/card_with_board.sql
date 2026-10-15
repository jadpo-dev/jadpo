-- query card_with_board: optional, store main
SELECT "id", "stage", "board_id"
FROM "card"
WHERE "id" = $1
  AND ("published_at" IS NOT NULL AND "published_at" <= $2)
-- $1: argument 0 as Uuid
-- $2: clock.now as Timestamp
-- columns: uuid, enum Stage, uuid
-- record CardWithBoard: id = column 0, stage = column 1, board = include 0

-- include 0: one, matching column 2
  SELECT "id", "name"
  FROM "board"
  WHERE "id" = ANY($1)
  -- $1: keys as Array(Uuid)
  -- columns: uuid, text
  -- record BoardView: id = column 0, name = column 1
