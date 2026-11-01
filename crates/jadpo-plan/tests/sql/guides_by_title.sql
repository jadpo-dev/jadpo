-- query guides_by_title: list, store main
SELECT "id", "title"
FROM "guide_page"
WHERE "jadpo_search" @@ websearch_to_tsquery('english'::regconfig, $1)
ORDER BY "title" COLLATE "C" ASC, "id" ASC
LIMIT 50
-- $1: search phrase of argument 0 as Text
-- columns: uuid, text
-- record GuideCard: id = column 0, title = column 1
-- row versions of guide_page, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "guide_page"
--   WHERE "id" = ANY($1)
