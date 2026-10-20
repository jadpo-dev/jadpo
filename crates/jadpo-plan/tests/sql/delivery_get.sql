-- Delivery.get: one, store main
SELECT "id", "courier", "state", "sent_on", "stops", "weight", "fee_amount", "fee_currency", "rate"
FROM "delivery"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, enum DeliveryState, date, int, quantity(mass), compound { ty: "money", may_be_none: false }, part, decimal
-- record Delivery: id = column 0, courier = column 1, state = column 2, sent_on = column 3, stops = column 4, weight = column 5, fee = column 6, rate = column 8
-- row versions of delivery, by the id in column 0:
--   SELECT "id", "xmin"::text
--   FROM "delivery"
--   WHERE "id" = ANY($1)
