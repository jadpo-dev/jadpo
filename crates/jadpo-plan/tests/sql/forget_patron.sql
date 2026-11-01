-- action forget_patron: delete by id, store main
WITH RECURSIVE "erased" ("entity", "id") AS (
  SELECT 0, "patron"."id"
  FROM "patron"
  WHERE "id" = $1
    AND $3
  UNION
  SELECT "edge"."entity", "edge"."id"
  FROM "erased"
  JOIN (
    SELECT 0 AS "parent", "edge1"."patron_id" AS "parent_id", 1 AS "entity", "edge1"."id" AS "id" FROM "loan" AS "edge1"
    UNION ALL
    SELECT 1 AS "parent", "edge2"."renewal_of" AS "parent_id", 1 AS "entity", "edge2"."id" AS "id" FROM "loan" AS "edge2"
    UNION ALL
    SELECT 1 AS "parent", "edge3"."loan_id" AS "parent_id", 2 AS "entity", "edge3"."id" AS "id" FROM "loan_note" AS "edge3"
  ) AS "edge" ON "edge"."parent" = "erased"."entity" AND "edge"."parent_id" = "erased"."id"
), "kept1" AS (
  UPDATE "fine"
  SET "patron_id" = CASE WHEN "patron_id" IN (SELECT "id" FROM "erased" WHERE "entity" = 0) THEN NULL ELSE "patron_id" END,
    "loan_id" = CASE WHEN "loan_id" IN (SELECT "id" FROM "erased" WHERE "entity" = 1) THEN NULL ELSE "loan_id" END
  WHERE ("patron_id" IN (SELECT "id" FROM "erased" WHERE "entity" = 0) OR "loan_id" IN (SELECT "id" FROM "erased" WHERE "entity" = 1))
), "kept2" AS (
  UPDATE "review"
  SET "patron_id" = NULL,
    "signed_as" = NULL,
    "changed_at" = $2
  WHERE "patron_id" IN (SELECT "id" FROM "erased" WHERE "entity" = 0)
), "deleted1" AS (
  DELETE FROM "loan"
  WHERE "id" IN (SELECT "id" FROM "erased" WHERE "entity" = 1)
), "deleted2" AS (
  DELETE FROM "loan_note"
  WHERE "id" IN (SELECT "id" FROM "erased" WHERE "entity" = 2)
), "deleted0" AS (
  DELETE FROM "patron"
  WHERE "id" IN (SELECT "id" FROM "erased" WHERE "entity" = 0)
  RETURNING "id"
)
SELECT "id" FROM "deleted0"
-- $1: input 0 as Uuid
-- $2: clock.now as Timestamp
-- $3: caller Signed as Bool
-- no row: the entity's not-found failure
