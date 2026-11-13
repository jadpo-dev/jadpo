-- query bakery_reaching: optional, store main
SELECT "id", "name"
FROM "bakery"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text
-- record BakeryReach: id = column 0, name = column 1, patches = include 0
-- row versions of bakery, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "bakery"
--   WHERE "id" = ANY($1)

-- include 0: list, matching column 0
  SELECT "key"."id", "row"."id", "row"."outline"
  FROM unnest($1) AS "key"("id")
  CROSS JOIN LATERAL (
    SELECT "bakery_id", "id", "outline"
    FROM "delivery_patch"
    WHERE "bakery_id" = "key"."id"
      AND ST_DWithin("outline", $2, 0.001)
    ORDER BY "id" ASC
    LIMIT $3
  ) AS "row"
  ORDER BY "key"."id", "row"."id" ASC
  -- $1: keys as Array(Uuid)
  -- $2: argument 1 as Geography
  -- $3: rows to fetch as Int8
  -- columns: uuid, uuid, geography("polygon")
  -- record PatchCard: id = column 1
  -- row versions of delivery_patch, by the id in column 1:
  --   SELECT "id", "xmin"::text
  --   FROM "delivery_patch"
  --   WHERE "id" = ANY($1)
  -- recheck: geo.contains(column 2, argument 1)
  -- recheck keeps: 10
