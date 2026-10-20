-- query quiet_subtitles: list, store main
SELECT "id", "title"
FROM "book"
WHERE left("title", LEAST(3, 2147483647)::integer) = $1
  AND ("subtitle" IS NOT NULL AND ((right("subtitle", char_length('!')) = ('!') COLLATE "C")) IS FALSE)
ORDER BY "id" ASC
LIMIT 20
-- $1: argument 0 as Text
-- columns: uuid, text
-- record BookCard: id = column 0, title = column 1
-- row versions of book, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "book"
--   WHERE "id" = ANY($1)
