-- query closest_articles: list, store main
SET LOCAL hnsw.ef_search = 50;
SET LOCAL hnsw.iterative_scan = strict_order;
SET LOCAL hnsw.max_scan_tuples = 2147483647;
SELECT *
FROM (
  SELECT "title", "id", "body_meaning" <-> $1 AS "jadpo_distance"
  FROM "help_article"
  WHERE "shown_publicly" = $2
    AND "body_meaning" <-> $1 <= $3
    AND "body_meaning" IS NOT NULL
  ORDER BY "body_meaning" <-> $1
  LIMIT 50
) AS "nearest"
ORDER BY "jadpo_distance", "id"
-- $1: argument 0 as Vector
-- $2: literal Bool(true) as Bool
-- $3: literal Float8(4603579539098121011) as Float8
-- columns: text, uuid, distance
-- record Nearest: item = (ArticleExcerpt: title = column 0), distance = column 2
-- row versions of help_article, by the id in column 1:
--   SELECT "id", "xmin"::text
--   FROM "help_article"
--   WHERE "id" = ANY($1)
