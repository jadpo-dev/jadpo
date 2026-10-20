-- query spine_titles: list, store main
SELECT "id", "title"
FROM "book"
WHERE char_length("title")::bigint <= $1
ORDER BY "title" COLLATE "C" ASC, "id" ASC
LIMIT 50
-- $1: literal Int8(24) as Int8
-- columns: uuid, text
-- record BookCard: id = column 0, title = column 1
-- row versions of book, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "book"
--   WHERE "id" = ANY($1)
