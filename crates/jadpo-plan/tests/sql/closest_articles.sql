-- query closest_articles: list, store main
SET LOCAL hnsw.ef_search = 50;
SET LOCAL hnsw.iterative_scan = strict_order;
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
-- $3: literal Numeric("0.6") as Numeric
-- columns: text, uuid, distance
-- record Nearest: item = (ArticleExcerpt: title = column 0), distance = column 2
