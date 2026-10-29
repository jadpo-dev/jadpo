CREATE TABLE "todo_provider_breakers" (
	"name" text PRIMARY KEY NOT NULL,
	"failures" integer DEFAULT 0 NOT NULL,
	"open_until" timestamp with time zone,
	"backoff_ms" integer DEFAULT 60000 NOT NULL,
	"probe_token" uuid,
	"probe_until" timestamp with time zone
);
