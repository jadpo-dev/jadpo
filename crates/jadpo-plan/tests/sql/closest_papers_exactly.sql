-- query closest_papers_exactly: list, store main
SELECT "id", "meaning", "full_meaning", "full_meaning" <#> $1 AS "jadpo_distance"
FROM "paper"
WHERE "full_meaning" IS NOT NULL
ORDER BY "jadpo_distance", "id"
LIMIT 10
-- $1: argument 0 as Vector
-- columns: uuid, vector, vector, distance
-- record Nearest: item = (Paper: id = column 0, meaning = column 1, full_meaning = column 2), distance = column 3
