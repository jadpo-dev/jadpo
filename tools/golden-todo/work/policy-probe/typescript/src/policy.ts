import { AbilityBuilder, createMongoAbility, subject } from "@casl/ability";
import type { MongoAbility } from "@casl/ability";
import { and, eq, isNull } from "drizzle-orm";
import { todos } from "./db/schema.ts";
import type { Principal, TodoChanges } from "./schemas.ts";
import { AppError } from "./errors.ts";

type Actions = "read" | "create" | "update" | "delete";
type Subjects =
  | "Todo"
  | "User"
  | ReturnType<typeof subject<"Todo", { owner_id: string }>>
  | ReturnType<typeof subject<"User", { id: string }>>;
export function abilityFor(
  principal: Principal,
): MongoAbility<[Actions, Subjects]> {
  const { can, build } = new AbilityBuilder<MongoAbility<[Actions, Subjects]>>(
    createMongoAbility,
  );
  if (principal.kind === "user") {
    can(["read", "create", "update", "delete"], "Todo", {
      owner_id: principal.id,
    });
    can(["read", "delete"], "User", { id: principal.id });
  }
  return build();
}
export function userId(principal: Principal): string {
  if (principal.kind !== "user") throw new AppError("not_permitted");
  return principal.id;
}
// Every business Todo access receives this SQL predicate, before ordering/limits.
export function todoScope(principal: Principal) {
  const owner = userId(principal);
  if (!abilityFor(principal).can("read", subject("Todo", { owner_id: owner })))
    throw new AppError("not_permitted");
  return and(eq(todos.owner_id, owner), isNull(todos.deleted_at));
}
export function self(
  principal: Principal,
  id: string,
  action: "read" | "delete",
): void {
  if (!abilityFor(principal).can(action, subject("User", { id })))
    throw new AppError("user_not_found");
}
// No general entity update API is exported. Both compile-time excess-property checks
// and strict runtime parsing reject owner/generated/private lifecycle mutation.
export type AllowedTodoUpdate = TodoChanges;
