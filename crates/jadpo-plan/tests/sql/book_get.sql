-- Book.get: one, store main
SELECT "id", "title", "subtitle", "published_on"
FROM "book"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, text, date
-- record Book: id = column 0, title = column 1, subtitle = column 2, published_on = column 3
