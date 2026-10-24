CREATE TABLE "todo_request_receipts" (
	"caller" text NOT NULL,
	"key" text NOT NULL,
	"fingerprint" text NOT NULL,
	"token" uuid NOT NULL,
	"lease_until" timestamp with time zone NOT NULL,
	"expires_at" timestamp with time zone NOT NULL,
	"response_status" integer,
	"response_body" text,
	CONSTRAINT "todo_request_receipts_caller_key_pk" PRIMARY KEY("caller","key")
);
