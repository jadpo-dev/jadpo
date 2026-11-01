-- query search_guides: page, store main
-- page: size 20, after argument 1, cursor column 2 as Float8, column 0 as Uuid
SELECT "id", "title", ts_rank("jadpo_search", websearch_to_tsquery('english'::regconfig, $1))::float8 AS "jadpo_rank"
FROM "guide_page"
WHERE "jadpo_search" @@ websearch_to_tsquery('english'::regconfig, $1)
  AND "shown_publicly" = $2
  AND ($3 IS NULL OR ts_rank("jadpo_search", websearch_to_tsquery('english'::regconfig, $1))::float8 < $3 OR (ts_rank("jadpo_search", websearch_to_tsquery('english'::regconfig, $1))::float8 = $3 AND "id" > $4))
ORDER BY "jadpo_rank" DESC, "id" ASC
LIMIT 21
-- $1: search phrase of argument 0 as Text
-- $2: literal Bool(true) as Bool
-- $3: cursor 0 as Float8
-- $4: cursor 1 as Uuid
-- columns: uuid, text, rank
-- record GuideCard: id = column 0, title = column 1
-- row versions of guide_page, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "guide_page"
--   WHERE "id" = ANY($1)
