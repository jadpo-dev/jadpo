-- 20261003T1000_initial: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "book" (
    "id" uuid NOT NULL,
    "isbn" text NOT NULL,
    "title" text NOT NULL,
    "shelf_code" text NOT NULL,
    CONSTRAINT "book_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_deliveries" (
    "id" uuid NOT NULL,
    "outbox_id" uuid NOT NULL,
    "subscriber" text NOT NULL,
    "status" text NOT NULL,
    "attempts" integer NOT NULL,
    "next_attempt_at" timestamptz(3),
    "last_failure" jsonb,
    "last_tried_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_deliveries_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_idempotency_keys" (
    "scope" text NOT NULL,
    "key" text NOT NULL,
    "request_digest" text NOT NULL,
    "status" smallint,
    "response" jsonb,
    "stored_at" timestamptz(3) NOT NULL,
    "kept_until" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_idempotency_keys_pkey" PRIMARY KEY ("scope", "key")
);
CREATE TABLE "jadpo_outbox" (
    "id" uuid NOT NULL,
    "event" text NOT NULL,
    "payload" jsonb NOT NULL,
    "emitted_at" timestamptz(3) NOT NULL,
    "traceparent" text,
    CONSTRAINT "jadpo_outbox_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "loan" (
    "id" uuid NOT NULL,
    "book_id" uuid NOT NULL,
    "member_id" uuid NOT NULL,
    "vouched_by" uuid,
    "due_on" date NOT NULL,
    "changed_at" timestamptz(3) NOT NULL,
    CONSTRAINT "loan_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "member" (
    "id" uuid NOT NULL,
    "email" text NOT NULL,
    "name" text NOT NULL,
    "joined_at" timestamptz(3) NOT NULL,
    CONSTRAINT "member_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_subscriber_key" UNIQUE ("outbox_id", "subscriber");
ALTER TABLE "loan" ADD CONSTRAINT "loan_one_loan_per_due_date_key" UNIQUE ("book_id", "due_on");
ALTER TABLE "member" ADD CONSTRAINT "member_email_key" UNIQUE ("email");
ALTER TABLE "jadpo_deliveries" ADD CONSTRAINT "jadpo_deliveries_outbox_id_fkey" FOREIGN KEY ("outbox_id") REFERENCES "jadpo_outbox" ("id") ON DELETE CASCADE;
ALTER TABLE "loan" ADD CONSTRAINT "loan_book_id_fkey" FOREIGN KEY ("book_id") REFERENCES "book" ("id") ON DELETE RESTRICT;
ALTER TABLE "loan" ADD CONSTRAINT "loan_member_id_fkey" FOREIGN KEY ("member_id") REFERENCES "member" ("id") ON DELETE CASCADE;
ALTER TABLE "loan" ADD CONSTRAINT "loan_vouched_by_fkey" FOREIGN KEY ("vouched_by") REFERENCES "member" ("id") ON DELETE SET NULL;
CREATE INDEX "jadpo_deliveries_status_next_attempt_at_idx" ON "jadpo_deliveries" ("status", "next_attempt_at");
CREATE INDEX "jadpo_idempotency_keys_kept_until_idx" ON "jadpo_idempotency_keys" ("kept_until");
CREATE INDEX "jadpo_outbox_emitted_at_idx" ON "jadpo_outbox" ("emitted_at");
CREATE INDEX "loan_by_due_on_idx" ON "loan" ("due_on");
CREATE INDEX "loan_member_id_idx" ON "loan" ("member_id");
CREATE INDEX "loan_vouched_by_idx" ON "loan" ("vouched_by");
