-- query bakeries_counted: one, store main
-- first, the records, each id in column 0:
SELECT "id", "spot"
FROM "bakery"
WHERE ("spot"::geometry && ST_MakeEnvelope($3, $1, $4, $2, 4326) OR "spot"::geometry && ST_MakeEnvelope($5, $1, $6, $2, 4326))
-- $1: argument 0.south as Numeric
-- $2: argument 0.north as Numeric
-- $3: FirstWest of argument 0 as Numeric
-- $4: FirstEast of argument 0 as Numeric
-- $5: SecondWest of argument 0 as Numeric
-- $6: SecondEast of argument 0 as Numeric
-- columns: uuid, geography("point")
-- record Bakery
-- recheck: geo.within(column 1, argument 0)
-- then, the totals of those kept:
SELECT "bakeries"
FROM (
  SELECT
    count(*) AS "bakeries"
  FROM "bakery"
  WHERE ("spot"::geometry && ST_MakeEnvelope($3, $1, $4, $2, 4326) OR "spot"::geometry && ST_MakeEnvelope($5, $1, $6, $2, 4326))
    AND "id" = ANY($7)
) AS "grouped"
-- $1: argument 0.south as Numeric
-- $2: argument 0.north as Numeric
-- $3: FirstWest of argument 0 as Numeric
-- $4: FirstEast of argument 0 as Numeric
-- $5: SecondWest of argument 0 as Numeric
-- $6: SecondEast of argument 0 as Numeric
-- $7: the ids the code kept as Array(Uuid)
-- columns: int
-- record BakeryCount: bakeries = column 0
