-- 20261110T0901_add_lifecycles: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
DROP INDEX "loan_book_id_due_on_id_idx";
ALTER TABLE "loan" ADD COLUMN "returned_at" timestamptz(3);
ALTER TABLE "loan" ADD COLUMN "status" text;
UPDATE "loan" SET "status" = 'out' WHERE "status" IS NULL;
ALTER TABLE "loan" ALTER COLUMN "status" SET NOT NULL;
ALTER TABLE "member" ADD COLUMN "left_at" timestamptz(3);
CREATE INDEX "loan_book_id_status_due_on_id_idx" ON "loan" ("book_id", "status", "due_on", "id");
CREATE INDEX "member_name_joined_at_id_idx" ON "member" ("name", "joined_at", "id") WHERE "left_at" IS NULL;
