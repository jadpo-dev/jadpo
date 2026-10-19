-- query spine_titles: list, store main
SELECT "id", "title"
FROM "book"
WHERE char_length("title")::bigint <= $1
ORDER BY "title" COLLATE "C" ASC, "id" ASC
LIMIT 50
-- $1: literal Int8(24) as Int8
-- columns: uuid, text
-- record BookCard: id = column 0, title = column 1
