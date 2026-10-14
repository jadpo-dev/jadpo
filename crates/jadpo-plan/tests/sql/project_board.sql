-- query project_board: one, store main
SELECT "id", "name", "owner_id"
FROM "project"
WHERE "id" = $1
-- $1: argument 0 as Uuid
-- columns: uuid, text, uuid
-- record ProjectBoardView: id = column 0, name = column 1, owner = include 0, todos = include 1

-- include 0: one, matching column 2
  SELECT "id", "name"
  FROM "user"
  WHERE "id" = ANY($1)
  -- $1: keys as Array(Uuid)
  -- columns: uuid, text
  -- record UserCard: id = column 0, name = column 1, profile = include 0

  -- include 0: optional, matching column 0
    SELECT "user_id", "bio"
    FROM "profile"
    WHERE "user_id" = ANY($1)
    -- $1: keys as Array(Uuid)
    -- columns: uuid, text
    -- record ProfileView: bio = column 1

-- include 1: list, matching column 0
  SELECT "key"."id", "row"."id", "row"."title", "row"."owner_id", "row"."created_at"
  FROM unnest($1) AS "key"("id")
  CROSS JOIN LATERAL (
    SELECT "project_id", "id", "title", "owner_id", "created_at"
    FROM "todo"
    WHERE "project_id" = "key"."id"
      AND "status" <> 'done'
    ORDER BY "created_at" ASC, "id" ASC
    LIMIT 50
  ) AS "row"
  ORDER BY "key"."id", "row"."created_at" ASC, "row"."id" ASC
  -- $1: keys as Array(Uuid)
  -- columns: uuid, uuid, text, uuid, instant
  -- record TodoWithOwner: id = column 1, title = column 2, owner = include 0

  -- include 0: one, matching column 3
    SELECT "id", "name"
    FROM "user"
    WHERE "id" = ANY($1)
    -- $1: keys as Array(Uuid)
    -- columns: uuid, text
    -- record UserView: id = column 0, name = column 1
