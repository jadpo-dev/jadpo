-- query published_in: list, store main
SELECT "id", "title"
FROM "book"
WHERE (CASE WHEN extract(year FROM "published_on") < 0 THEN extract(year FROM "published_on") + 1 ELSE extract(year FROM "published_on") END)::bigint = $1
ORDER BY "title" COLLATE "C" ASC, "id" ASC
LIMIT 100
-- $1: argument 0 as Int8
-- columns: uuid, text
-- record BookCard: id = column 0, title = column 1
