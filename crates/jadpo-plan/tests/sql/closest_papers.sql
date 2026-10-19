-- query closest_papers: list, store main
SET LOCAL hnsw.ef_search = 40;
SET LOCAL hnsw.iterative_scan = strict_order;
SET LOCAL hnsw.max_scan_tuples = 2147483647;
SELECT *
FROM (
  SELECT "id", "meaning", "full_meaning", "meaning" <=> $1 AS "jadpo_distance"
  FROM "paper"
  WHERE "meaning" IS NOT NULL
  ORDER BY "meaning"::halfvec(3072) <=> $1::vector(3072)::halfvec(3072)
  LIMIT 10
) AS "nearest"
ORDER BY "jadpo_distance", "id"
-- $1: argument 0 as Vector
-- columns: uuid, vector, vector, distance
-- record Nearest: item = (Paper: id = column 0, meaning = column 1, full_meaning = column 2), distance = column 3
