-- 20261103T0900_add_cards_and_reads: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
DROP INDEX "loan_member_id_idx";
CREATE TABLE "card" (
    "id" uuid NOT NULL,
    "member_id" uuid NOT NULL,
    "number" text NOT NULL,
    CONSTRAINT "card_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "card" ADD CONSTRAINT "card_member_id_key" UNIQUE ("member_id");
ALTER TABLE "card" ADD CONSTRAINT "card_member_id_fkey" FOREIGN KEY ("member_id") REFERENCES "member" ("id") ON DELETE CASCADE;
CREATE INDEX "book_title_id_idx" ON "book" ("title", "id");
CREATE INDEX "loan_book_id_due_on_id_idx" ON "loan" ("book_id", "due_on", "id");
CREATE INDEX "loan_member_id_due_on_id_idx" ON "loan" ("member_id", "due_on", "id");
