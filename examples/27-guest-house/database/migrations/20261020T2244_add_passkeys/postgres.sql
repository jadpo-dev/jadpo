-- 20261020T2244_add_passkeys: written by `jadpo migrate new` from the snapshots; never edit it by hand.

-- store: main
CREATE TABLE "jadpo_passkey_challenges" (
    "id" uuid NOT NULL,
    "challenge" text NOT NULL,
    "purpose" text NOT NULL,
    "binding" text NOT NULL,
    "principal_id" uuid,
    "user_handle" bytea,
    "state" text NOT NULL,
    "expires_at" timestamptz(3) NOT NULL,
    CONSTRAINT "jadpo_passkey_challenges_pkey" PRIMARY KEY ("id")
);
CREATE TABLE "jadpo_passkeys" (
    "id" uuid NOT NULL,
    "principal_id" uuid NOT NULL,
    "credential_id" bytea NOT NULL,
    "user_handle" bytea NOT NULL,
    "credential" jsonb NOT NULL,
    "registered_at" timestamptz(3) NOT NULL,
    "encoding" smallint,
    CONSTRAINT "jadpo_passkeys_pkey" PRIMARY KEY ("id")
);
ALTER TABLE "jadpo_passkey_challenges" ADD CONSTRAINT "jadpo_passkey_challenges_challenge_key" UNIQUE ("challenge");
ALTER TABLE "jadpo_passkey_challenges" ADD CONSTRAINT "jadpo_passkey_challenges_purpose_binding_key" UNIQUE ("purpose", "binding");
ALTER TABLE "jadpo_passkeys" ADD CONSTRAINT "jadpo_passkeys_credential_id_key" UNIQUE ("credential_id");
CREATE INDEX "jadpo_passkey_challenges_expires_at_idx" ON "jadpo_passkey_challenges" ("expires_at");
CREATE INDEX "jadpo_passkeys_principal_id_idx" ON "jadpo_passkeys" ("principal_id");
