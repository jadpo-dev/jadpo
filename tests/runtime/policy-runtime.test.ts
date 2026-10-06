import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Database } from "bun:sqlite";
import { SQL } from "bun";
import { createHash } from "node:crypto";

const postgresUrl = Bun.env.JADPO_POLICY_RUNTIME_DATABASE_URL;
if (postgresUrl) {
  const url = new URL(postgresUrl);
  if (!["postgres:", "postgresql:"].includes(url.protocol) || !["127.0.0.1", "localhost"].includes(url.hostname) || !/^\/jadpo_auth_test_[a-z0-9_]+$/u.test(url.pathname)) throw Error("Requires a disposable local policy database");
}

const root = mkdtempSync(join(tmpdir(), "jadpo-policy-runtime-"));
const databasePath = join(root, "policy.sqlite");
const old = Object.fromEntries(["DATABASE_URL", "SQLITE_PATH"].map(name => [name, Bun.env[name]]));
if (postgresUrl) {
  Bun.env.DATABASE_URL = postgresUrl;
  delete Bun.env.SQLITE_PATH;
} else {
  delete Bun.env.DATABASE_URL;
  Bun.env.SQLITE_PATH = databasePath;
}
// Imported generated policy calls use fixture-owned principals below. These
// cases do not assert credential authentication or production main startup.
const observations: Record<string, unknown>[] = [];

const { persistence, PolicyFault } = await import(
  "../../examples/policy-runtime/build/target/persistence.ts"
);
const { handleRequest } = await import("../../examples/policy-runtime/build/target/app.ts");

const ids = {
  owner: "00000000-0000-0000-0000-000000000001",
  editor: "00000000-0000-0000-0000-000000000002",
  viewer: "00000000-0000-0000-0000-000000000003",
  outsider: "00000000-0000-0000-0000-000000000004",
  service: "00000000-0000-4000-8000-000000000005",
  company: "00000000-0000-0000-0000-000000000010",
  otherCompany: "00000000-0000-0000-0000-000000000011",
  article: "00000000-0000-0000-0000-000000000020",
  visibleSharedArticle: "00000000-0000-0000-0000-000000000030",
};

function principal(userId: string) {
  return {
    kind: "user" as const,
    subject: `subject:${userId}`,
    values: { user_id: userId },
  };
}

function servicePrincipal(serviceId: string) {
  return {
    kind: "service" as const,
    subject: `service:${serviceId}`,
    values: { service_id: serviceId },
  };
}

const postgres = postgresUrl ? new SQL({ url: postgresUrl, prepare: false }) : null;
const sqlite = postgres ? null : new Database(databasePath, { strict: true });
const db = {
  async run(statement: string, ...values: any[]) {
    if (postgres) return postgres.unsafe(statement.replace(/\?(\d+)/gu, (_match, index) => "$" + index), values);
    return sqlite!.prepare(statement).run(...values);
  },
  async get(statement: string, ...values: any[]): Promise<Record<string, any> | null> {
    if (postgres) return (await postgres.unsafe(statement.replace(/\?(\d+)/gu, (_match, index) => "$" + index), values))[0] ?? null;
    return sqlite!.prepare(statement).get(...values) as Record<string, any> | null;
  },
};
beforeAll(async () => {
  for (const [id, name, email] of [[ids.owner, "Owner", "owner@example.test"], [ids.editor, "Editor", "editor@example.test"], [ids.viewer, "Viewer", "viewer@example.test"], [ids.outsider, "Outsider", "outsider@example.test"]]) {
    await db.run('INSERT INTO "user" ("id", "display_name", "private_email") VALUES (?1, ?2, ?3)', id, name, email);
  }
  await db.run('INSERT INTO "service" ("id") VALUES (?1)', ids.service);
  for (const [id, owner, name] of [[ids.company, ids.owner, "Policy Co"], [ids.otherCompany, ids.outsider, "Other Co"]]) {
    await db.run('INSERT INTO "company" ("id", "owner_id", "name") VALUES (?1, ?2, ?3)', id, owner, name);
  }
  for (const [id, user, role] of [["00000000-0000-0000-0000-000000000101", ids.editor, "editor"], ["00000000-0000-0000-0000-000000000102", ids.viewer, "viewer"]]) {
    await db.run('INSERT INTO "company_membership" ("id", "company_id", "user_id", "role") VALUES (?1, ?2, ?3, ?4)', id, ids.company, user, role);
  }
  for (const [id, user, role] of [["00000000-0000-0000-0000-000000000103", ids.editor, "operator"], ["00000000-0000-0000-0000-000000000105", ids.owner, "administrator"]]) {
    await db.run('INSERT INTO "application_membership" ("id", "user_id", "role") VALUES (?1, ?2, ?3)', id, user, role);
  }
  await db.run('INSERT INTO "application_service_membership" ("id", "service_id", "role") VALUES (?1, ?2, ?3)', "00000000-0000-0000-0000-000000000104", ids.service, "operator");
  for (const [id, company, title, billing] of [[ids.article, ids.company, "Initial", "invoice-secret"], ["00000000-0000-0000-0000-000000000021", ids.otherCompany, "Shared", null], ["00000000-0000-0000-0000-000000000022", ids.otherCompany, "Shared", null], [ids.visibleSharedArticle, ids.company, "Shared", null]]) {
    await db.run('INSERT INTO "article" ("id", "company_id", "title", "billing_reference") VALUES (?1, ?2, ?3, ?4)', id, company, title, billing);
  }
});

afterAll(async () => {
  try { await postgres?.close(); sqlite?.close(); }
  finally {
    for (const [name, value] of Object.entries(old)) { if (value === undefined) delete Bun.env[name]; else Bun.env[name] = value; }
    rmSync(root, { recursive: true, force: true });
  }
  expect(observations).toHaveLength(9);
  const generatedPins = Object.fromEntries(["app.ts", "persistence.ts", "startup-owner.ts"].map(name => [name, createHash("sha256").update(readFileSync(new URL("../../examples/policy-runtime/build/target/" + name, import.meta.url))).digest("hex")]));
  console.log(JSON.stringify({kind:"native_generated_policy_parity",backend:postgresUrl?"postgres":"sqlite",evidenceSurface:"imported_generated_policy_calls_and_direct_public_handler",authentication:"not_exercised",mainEntry:"not_exercised",observations,generatedPins}));
});

describe("generated scoped policy enforcement", () => {
  test("uses separate stable user projections for regular and administrator audiences", async () => {
    const regular = persistence.withPolicy(principal(ids.viewer), "User.summary");
    const regularRow = await regular.query_optional_User_by_id(ids.owner);
    expect(regularRow).not.toBeNull();
    expect({
      id: regularRow!.id,
      display_name: regularRow!.display_name,
    }).toEqual({ id: ids.owner, display_name: "Owner" });

    const forbiddenAdministrativeView = persistence.withPolicy(
      principal(ids.viewer),
      "User.administrative_details",
    );
    expect(
      await forbiddenAdministrativeView.query_optional_User_by_id(ids.owner),
    ).toBeNull();

    const administrator = persistence.withPolicy(
      principal(ids.owner),
      "User.administrative_details",
    );
    const administrativeRow = await administrator.query_optional_User_by_id(ids.owner);
    expect(administrativeRow).not.toBeNull();
    expect({
      id: administrativeRow!.id,
      display_name: administrativeRow!.display_name,
      private_email: administrativeRow!.private_email,
    }).toEqual({
      id: ids.owner,
      display_name: "Owner",
      private_email: "owner@example.test",
    });
    observations.push({case:"projections",regularSelectedFields:{id:regularRow!.id,display_name:regularRow!.display_name},administratorSelectedFields:{id:administrativeRow!.id,display_name:administrativeRow!.display_name},regularAdministrativeViewDenied:true,administrativeFieldAuthorized:administrativeRow!.private_email === "owner@example.test"});
  });

  test("conceals rows outside the principal's authoritative role scope", async () => {
    const outsider = persistence.withPolicy(principal(ids.outsider), "Article.find");
    expect(await outsider.query_optional_Article_by_id(ids.article)).toBeNull();

    const viewer = persistence.withPolicy(principal(ids.viewer), "Article.find");
    expect(await viewer.query_optional_Article_by_id(ids.article)).toMatchObject({
      id: ids.article,
      title: "Initial",
    });
    observations.push({case:"company_scope",outsiderConcealed:true,viewerVisible:true});
  });

  test("uses entity permissions and narrowing field permissions conjunctively", async () => {
    const editor = persistence.withPolicy(principal(ids.editor), "Article.edit");
    expect(
      await editor.update_required_Article_by_id_set_title(ids.article, "Edited"),
    ).toMatchObject({ title: "Edited" });
    expect(
      await editor.update_required_Article_by_id_set_billing_reference(
        ids.article,
        "editor-must-not-write",
      ),
    ).toBeNull();

    const afterDeniedField = await db.get('SELECT billing_reference FROM "article" WHERE id = ?1', ids.article);
    expect(afterDeniedField?.billing_reference).toBe("invoice-secret");
    const owner = persistence.withPolicy(principal(ids.owner), "Article.edit");
    expect(
      await owner.update_required_Article_by_id_set_billing_reference(
        ids.article,
        "owner-write",
      ),
    ).toMatchObject({ billing_reference: "owner-write" });
    expect((await db.get('SELECT billing_reference FROM "article" WHERE id = ?1', ids.article))?.billing_reference).toBe("owner-write");
    observations.push({case:"field_mutation",editorTitleAllowed:true,deniedFieldStoredUnchanged:afterDeniedField?.billing_reference === "invoice-secret",ownerFieldChanged:true});
  });

  test("injects row policy before ordering and pagination", async () => {
    const viewer = persistence.withPolicy(principal(ids.viewer), "Article.list");
    const rows = (await viewer.query_many_Article_by_title_order_by_id_asc_paginated(
      "Shared",
      1,
      0,
    )) as Array<{ id: string }>;
    expect(rows.map((row) => row.id)).toEqual([ids.visibleSharedArticle]);
    observations.push({case:"policy_before_page",rowIds:rows.map(row=>row.id),limit:1,offset:0});
  });

  test("rejects a create before insertion when the role lacks create", async () => {
    const editor = persistence.withPolicy(principal(ids.editor), "Article.create");
    await expect(
      editor.create_Article({
        id: "00000000-0000-0000-0000-000000000040",
        company_id: ids.company,
        title: "Denied",
        billing_reference: null,
      }),
    ).rejects.toBeInstanceOf(PolicyFault);
    const deniedStored = await db.get('SELECT id FROM "article" WHERE id = ?1', "00000000-0000-0000-0000-000000000040");
    expect(deniedStored).toBeNull();
    observations.push({case:"denied_create",rowAbsent:deniedStored === null});
  });

  test("authorizes create from proposed scope and conceals denied deletes", async () => {
    const createdId = "00000000-0000-0000-0000-000000000041";
    const owner = persistence.withPolicy(principal(ids.owner), "Article.create");
    expect(
      await owner.create_Article({
        id: createdId,
        company_id: ids.company,
        title: "Owner-created",
        billing_reference: null,
      }),
    ).toMatchObject({ id: createdId, company_id: ids.company });

    const viewer = persistence.withPolicy(principal(ids.viewer), "Article.delete");
    expect(await viewer.delete_required_Article_by_id(createdId)).toBeNull();
    expect(await db.get('SELECT id FROM "article" WHERE id = ?1', createdId)).toMatchObject({id:createdId});

    const deletingOwner = persistence.withPolicy(principal(ids.owner), "Article.delete");
    expect(await deletingOwner.delete_required_Article_by_id(createdId)).toMatchObject({
      id: createdId,
    });
    expect(await db.get('SELECT id FROM "article" WHERE id = ?1', createdId)).toBeNull();
    observations.push({case:"create_delete_scope",ownerCreated:true,deniedDeleteRowRetained:true,ownerDeleteRowAbsent:true});
  });

  test("resolves application roles for non-entity invoke policies", async () => {
    const operator = persistence.withPolicy(principal(ids.editor), "administrative_health");
    expect(await operator.allowsPolicy("administrative_health", "invoke")).toBe(true);

    const viewer = persistence.withPolicy(principal(ids.viewer), "administrative_health");
    expect(await viewer.allowsPolicy("administrative_health", "invoke")).toBe(false);

    const service = persistence.withPolicy(
      servicePrincipal(ids.service),
      "administrative_health",
    );
    expect(await service.allowsPolicy("administrative_health", "invoke")).toBe(true);
    observations.push({case:"application_and_service_roles",operatorAllowed:true,viewerDenied:true,serviceOperatorAllowed:true});
  });

  test("enforces a public callable invoke policy through the route", async () => {
    const response = await handleRequest(new Request("http://jadpo.test/public-health"));
    expect(response.status).toBe(200);
    const body = await response.json();
    expect(body).toEqual({ ok: true });
    observations.push({case:"public_handler_invoke",status:response.status,body});
  });

  test("observes membership revocation on the next authoritative operation", async () => {
    await db.run('DELETE FROM "company_membership" WHERE "user_id" = ?1', ids.editor);
    expect(await db.get('SELECT id FROM "company_membership" WHERE "user_id" = ?1', ids.editor)).toBeNull();

    const editor = persistence.withPolicy(principal(ids.editor), "Article.find_after_revocation");
    expect(await editor.query_optional_Article_by_id(ids.article)).toBeNull();
    observations.push({case:"membership_revocation",storedMembershipAbsent:true,nextOperationConcealed:true});
  });
});
