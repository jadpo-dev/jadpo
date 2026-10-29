CREATE TABLE "jadpo_deliveries" (
	"id" uuid PRIMARY KEY NOT NULL,
	"subscriber" text NOT NULL,
	"payload" jsonb NOT NULL,
	"status" text NOT NULL,
	"lease_token" uuid,
	"lease_until" timestamp with time zone,
	"attempts" integer DEFAULT 0 NOT NULL,
	"first_attempt_at" timestamp with time zone,
	"next_attempt_at" timestamp with time zone NOT NULL,
	"provider_key" text NOT NULL,
	"last_failure" jsonb,
	"created_at" timestamp with time zone NOT NULL,
	CONSTRAINT "jadpo_deliveries_provider_key_unique" UNIQUE("provider_key")
);
--> statement-breakpoint
CREATE TABLE "todo_schedule_claims" (
	"name" text PRIMARY KEY NOT NULL,
	"bucket" bigint NOT NULL
);
--> statement-breakpoint
CREATE TABLE "jadpo_schedule_runs" (
	"id" uuid PRIMARY KEY NOT NULL,
	"schedule" text NOT NULL,
	"started_at" timestamp with time zone NOT NULL,
	"outcome" text NOT NULL,
	"items" integer NOT NULL
);
--> statement-breakpoint
CREATE TABLE "jadpo_service_keys" (
	"id" uuid PRIMARY KEY NOT NULL,
	"principal_id" uuid NOT NULL,
	"verifier" text NOT NULL,
	"started_at" timestamp with time zone NOT NULL,
	"ends_at" timestamp with time zone NOT NULL,
	CONSTRAINT "jadpo_service_keys_verifier_unique" UNIQUE("verifier")
);
--> statement-breakpoint
CREATE TABLE "service" (
	"id" uuid PRIMARY KEY NOT NULL,
	"owner_id" uuid NOT NULL,
	"name" text NOT NULL,
	"status" text NOT NULL,
	CONSTRAINT "service_name_unique" UNIQUE("name")
);
--> statement-breakpoint
CREATE TABLE "jadpo_sessions" (
	"id" uuid PRIMARY KEY NOT NULL,
	"principal_id" uuid NOT NULL,
	"verifier" text NOT NULL,
	"csrf_verifier" text,
	"started_at" timestamp with time zone NOT NULL,
	"ends_at" timestamp with time zone NOT NULL,
	CONSTRAINT "jadpo_sessions_verifier_unique" UNIQUE("verifier")
);
--> statement-breakpoint
CREATE TABLE "todo" (
	"id" uuid PRIMARY KEY NOT NULL,
	"owner_id" uuid NOT NULL,
	"title" text NOT NULL,
	"status" text NOT NULL,
	"due_at" timestamp with time zone,
	"reminder_sent_at" timestamp with time zone,
	"reminder_queued_at" timestamp with time zone,
	"reminder_revision" integer NOT NULL,
	"deleted_at" timestamp with time zone,
	"created_at" timestamp with time zone NOT NULL,
	"updated_at" timestamp with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "user" (
	"id" uuid PRIMARY KEY NOT NULL,
	"authentication_subject" text NOT NULL,
	"status" text NOT NULL,
	"email" text NOT NULL,
	CONSTRAINT "user_authentication_subject_unique" UNIQUE("authentication_subject")
);
--> statement-breakpoint
ALTER TABLE "jadpo_service_keys" ADD CONSTRAINT "jadpo_service_keys_principal_id_service_id_fk" FOREIGN KEY ("principal_id") REFERENCES "public"."service"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "service" ADD CONSTRAINT "service_owner_id_user_id_fk" FOREIGN KEY ("owner_id") REFERENCES "public"."user"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "jadpo_sessions" ADD CONSTRAINT "jadpo_sessions_principal_id_user_id_fk" FOREIGN KEY ("principal_id") REFERENCES "public"."user"("id") ON DELETE no action ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "todo" ADD CONSTRAINT "todo_owner_id_user_id_fk" FOREIGN KEY ("owner_id") REFERENCES "public"."user"("id") ON DELETE restrict ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "todo_owner_page" ON "todo" USING btree ("owner_id","created_at","id");