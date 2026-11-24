CREATE TABLE "customer" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"name" text NOT NULL,
	"email" text NOT NULL,
	"phone" text,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL,
	CONSTRAINT "customer_email_key" UNIQUE("email","workspace_id")
);
--> statement-breakpoint
CREATE TABLE "macro" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"title" text NOT NULL,
	"body" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "membership" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"user_id" uuid NOT NULL,
	"role" text NOT NULL,
	"display_name" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL,
	CONSTRAINT "membership_one_per_person_key" UNIQUE("workspace_id","user_id")
);
--> statement-breakpoint
CREATE TABLE "note" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"ticket_id" uuid NOT NULL,
	"author_id" uuid,
	"body" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "reply" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"ticket_id" uuid NOT NULL,
	"author_id" uuid,
	"body" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "jadpo_sessions" (
	"id" uuid PRIMARY KEY NOT NULL,
	"principal_id" uuid NOT NULL,
	"verifier" text NOT NULL,
	"csrf_verifier" text,
	"started_at" timestamp (3) with time zone NOT NULL,
	"ends_at" timestamp (3) with time zone NOT NULL,
	"sign_in_method" text,
	"strength" text,
	CONSTRAINT "jadpo_sessions_verifier_key" UNIQUE("verifier")
);
--> statement-breakpoint
CREATE TABLE "tag" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"name" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	CONSTRAINT "tag_name_key" UNIQUE("name","workspace_id")
);
--> statement-breakpoint
CREATE TABLE "ticket_tag" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"ticket_id" uuid NOT NULL,
	"tag_id" uuid NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	CONSTRAINT "ticket_tag_once_per_ticket_key" UNIQUE("ticket_id","tag_id","workspace_id")
);
--> statement-breakpoint
CREATE TABLE "ticket" (
	"id" uuid PRIMARY KEY NOT NULL,
	"workspace_id" uuid NOT NULL,
	"customer_id" uuid,
	"assignee_id" uuid,
	"subject" text NOT NULL,
	"status" text NOT NULL,
	"priority" text NOT NULL,
	"respond_by" timestamp (3) with time zone NOT NULL,
	"first_response_at" timestamp (3) with time zone,
	"breached_at" timestamp (3) with time zone,
	"solved_at" timestamp (3) with time zone,
	"deleted_at" timestamp (3) with time zone,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL
);
--> statement-breakpoint
CREATE TABLE "user" (
	"id" uuid PRIMARY KEY NOT NULL,
	"authentication_subject" text NOT NULL,
	"status" text NOT NULL,
	"email" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	CONSTRAINT "user_authentication_subject_key" UNIQUE("authentication_subject")
);
--> statement-breakpoint
CREATE TABLE "workspace" (
	"id" uuid PRIMARY KEY NOT NULL,
	"name" text NOT NULL,
	"created_at" timestamp (3) with time zone NOT NULL,
	"updated_at" timestamp (3) with time zone NOT NULL
);
--> statement-breakpoint
ALTER TABLE "customer" ADD CONSTRAINT "customer_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "macro" ADD CONSTRAINT "macro_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "membership" ADD CONSTRAINT "membership_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "membership" ADD CONSTRAINT "membership_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "public"."user"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "note" ADD CONSTRAINT "note_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "note" ADD CONSTRAINT "note_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "public"."ticket"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "note" ADD CONSTRAINT "note_author_id_fkey" FOREIGN KEY ("author_id") REFERENCES "public"."user"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "reply" ADD CONSTRAINT "reply_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "reply" ADD CONSTRAINT "reply_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "public"."ticket"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "reply" ADD CONSTRAINT "reply_author_id_fkey" FOREIGN KEY ("author_id") REFERENCES "public"."user"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "tag" ADD CONSTRAINT "tag_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_ticket_id_fkey" FOREIGN KEY ("ticket_id") REFERENCES "public"."ticket"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ticket_tag" ADD CONSTRAINT "ticket_tag_tag_id_fkey" FOREIGN KEY ("tag_id") REFERENCES "public"."tag"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ticket" ADD CONSTRAINT "ticket_workspace_id_fkey" FOREIGN KEY ("workspace_id") REFERENCES "public"."workspace"("id") ON DELETE cascade ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ticket" ADD CONSTRAINT "ticket_customer_id_fkey" FOREIGN KEY ("customer_id") REFERENCES "public"."customer"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
ALTER TABLE "ticket" ADD CONSTRAINT "ticket_assignee_id_fkey" FOREIGN KEY ("assignee_id") REFERENCES "public"."membership"("id") ON DELETE set null ON UPDATE no action;--> statement-breakpoint
CREATE INDEX "customer_workspace_id_created_at_id_idx" ON "customer" USING btree ("workspace_id","created_at","id");--> statement-breakpoint
CREATE INDEX "macro_workspace_id_title_id_sorted_idx" ON "macro" USING btree ("workspace_id","title" COLLATE "C","id");--> statement-breakpoint
CREATE INDEX "membership_user_id_idx" ON "membership" USING btree ("user_id");--> statement-breakpoint
CREATE INDEX "membership_workspace_id_created_at_id_idx" ON "membership" USING btree ("workspace_id","created_at","id");--> statement-breakpoint
CREATE INDEX "membership_workspace_id_role_id_idx" ON "membership" USING btree ("workspace_id","role","id");--> statement-breakpoint
CREATE INDEX "note_author_id_idx" ON "note" USING btree ("author_id");--> statement-breakpoint
CREATE INDEX "note_workspace_id_idx" ON "note" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "note_ticket_id_created_at_id_idx" ON "note" USING btree ("ticket_id","created_at","id");--> statement-breakpoint
CREATE INDEX "reply_author_id_idx" ON "reply" USING btree ("author_id");--> statement-breakpoint
CREATE INDEX "reply_workspace_id_idx" ON "reply" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "reply_ticket_id_created_at_id_idx" ON "reply" USING btree ("ticket_id","created_at","id");--> statement-breakpoint
CREATE INDEX "jadpo_sessions_ends_at_idx" ON "jadpo_sessions" USING btree ("ends_at");--> statement-breakpoint
CREATE INDEX "jadpo_sessions_principal_id_idx" ON "jadpo_sessions" USING btree ("principal_id");--> statement-breakpoint
CREATE INDEX "tag_workspace_id_name_id_sorted_idx" ON "tag" USING btree ("workspace_id","name" COLLATE "C","id");--> statement-breakpoint
CREATE INDEX "ticket_tag_tag_id_idx" ON "ticket_tag" USING btree ("tag_id");--> statement-breakpoint
CREATE INDEX "ticket_tag_workspace_id_idx" ON "ticket_tag" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "ticket_tag_ticket_id_created_at_id_idx" ON "ticket_tag" USING btree ("ticket_id","created_at","id");--> statement-breakpoint
CREATE INDEX "ticket_assignee_id_idx" ON "ticket" USING btree ("assignee_id");--> statement-breakpoint
CREATE INDEX "ticket_customer_id_idx" ON "ticket" USING btree ("customer_id");--> statement-breakpoint
CREATE INDEX "ticket_workspace_id_idx" ON "ticket" USING btree ("workspace_id");--> statement-breakpoint
CREATE INDEX "ticket_customer_id_created_at_id_idx" ON "ticket" USING btree ("customer_id","created_at","id") WHERE "deleted_at" IS NULL;--> statement-breakpoint
CREATE INDEX "ticket_status_respond_by_id_idx" ON "ticket" USING btree ("status","respond_by","id") WHERE "deleted_at" IS NULL;--> statement-breakpoint
CREATE INDEX "ticket_workspace_id_created_at_id_idx" ON "ticket" USING btree ("workspace_id","created_at","id") WHERE "deleted_at" IS NULL;--> statement-breakpoint
CREATE INDEX "workspace_created_at_id_idx" ON "workspace" USING btree ("created_at","id");