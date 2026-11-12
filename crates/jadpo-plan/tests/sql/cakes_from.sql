-- query cakes_from: page, store main
-- page: size 10, after argument 3, cursor column 1 as Int8, column 0 as Uuid
SELECT "id", "price_amount", "price_currency", "month_year", "month_month"
FROM "cake"
WHERE ("month_year", "month_month") >= ($1, $2)
  AND EXISTS (SELECT 1 FROM unnest($3, $4) AS "held"("p0", "p1") WHERE "month_year" = "held"."p0" AND "month_month" = "held"."p1")
  AND "tags" <> $5
  AND ($6 IS NULL OR ("price_amount", "id") < ($6, $7))
  AND FALSE
ORDER BY "price_amount" DESC, "id" DESC
LIMIT 11
-- $1: Part(0) of argument 0 as Int8
-- $2: Part(1) of argument 0 as Int8
-- $3: Part(0) of argument 1 as Array(Int8)
-- $4: Part(1) of argument 1 as Array(Int8)
-- $5: Json(0) of argument 2 as Json
-- $6: cursor 0 as Int8
-- $7: cursor 1 as Uuid
-- columns: uuid, compound { ty: "money", may_be_none: false }, part, compound { ty: "yearmonth", may_be_none: false }, part
-- record CakeView: id = column 0, price = column 1, month = column 3
-- row versions of cake, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "cake"
--   WHERE "id" = ANY($1)
