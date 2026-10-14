-- action drop_profile: delete, store main
DELETE FROM "profile"
WHERE "id" = $1
-- $1: input 0.id as Uuid
