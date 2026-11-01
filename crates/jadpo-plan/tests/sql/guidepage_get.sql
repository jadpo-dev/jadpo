-- GuidePage.get: one, store main
SELECT "id", "title", "body", "shown_publicly"
FROM "guide_page"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, text, bool
-- record GuidePage: id = column 0, title = column 1, body = column 2, shown_publicly = column 3
-- row versions of guide_page, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "guide_page"
--   WHERE "id" = ANY($1)
