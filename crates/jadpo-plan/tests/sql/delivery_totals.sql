-- query delivery_totals: one, store main
SELECT "deliveries", "latest", "first_courier"
FROM (
  SELECT
    count(*) AS "deliveries",
    max("sent_on") AS "latest",
    min("courier" COLLATE "C") AS "first_courier"
  FROM "delivery"
) AS "grouped"
-- columns: int, date, text
-- record DeliveryTotals: deliveries = column 0, latest = column 1, first_courier = column 2
