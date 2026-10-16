# 12 · A recipe book for many bakeries

One application that many bakeries share: each keeps its recipes and its
staff in it, and none may ever see another's. It teaches policy beyond a
user field: roles held in a membership, tenancy, actions that need other
people than their entities' grants, fields narrower than their record,
and the permissions report that shows all of it in one place.

Each bakery is a tenant. Its staff hold a role in it (owner, baker or
counter), and every grant on its records applies only to people who hold
a role there. The code never checks a role: each entity's `policy:` says
who may do what, the compiler puts it in every statement, and a route
test signs in as each person to show what they get.

## What it teaches

1. **Tenancy.** `tenant: Bakery` in root `policy.jadpo` makes every entity belong to a bakery, or say `tenant: Tenancy.shared`, as bakers and the shared ingredient list do. See `policy.jadpo`, `domains/kitchen/entities/baker.jadpo` and [area 9, Tenancy](../../docs/language/09-policy.md#tenancy).
2. **A membership and its roles.** `StaffMember` says where a role is held, who holds it and which field holds it; the role enum sits beside it. See `domains/kitchen/entities/staff_member.jadpo`, `domains/kitchen/types/staff_role.jadpo` and [area 9, A membership role](../../docs/language/09-policy.md#a-membership-role).
3. **Roles and a user field together.** A recipe may be changed by its bakery's owners and by its author; for any other baker it isn't there, and changing it is `RecipeNotFound`. See `domains/kitchen/entities/recipe.jadpo`, `domains/kitchen/actions/rename_recipe.jadpo` and [area 9, When policy says no](../../docs/language/09-policy.md#when-policy-says-no).
4. **Queries that name no caller.** `my_bakeries` reads every bakery and `team` every place in one, and policy leaves each caller what they may see: an owner the whole team, anyone else their own place. See `domains/kitchen/queries/my_bakeries.jadpo` and `domains/kitchen/queries/team.jadpo`.
5. **Listed actions.** Opening a bakery is for anyone signed in, though nobody holds a role in it yet; handing a recipe on is for its author only, since an ordinary update can't change `author_id`. See `policy.jadpo`, `domains/kitchen/actions/open_bakery.jadpo`, `domains/kitchen/actions/hand_over_recipe.jadpo` and [area 9, Actions](../../docs/language/09-policy.md#actions).
6. **Field restrictions.** `batch_cost` narrows reading and changing to owners, so the recipe card leaves it out and an author's change to it is refused whole, with 403; `old_reference` is `FieldAccess.server_only`, written only by an import no request runs. See `domains/kitchen/entities/recipe.jadpo`, `domains/kitchen/types/recipe_card.jadpo`, `domains/kitchen/actions/cost_recipe.jadpo`, `domains/kitchen/actions/import_recipe.jadpo` and [area 9, Field restrictions](../../docs/language/09-policy.md#field-restrictions).
7. **A role across the whole application.** The recipe book's editors hold `PlatformRole.editor` with `scope: MembershipScope.application`, and only they change the ingredient list every bakery reads. See `domains/kitchen/entities/platform_staff.jadpo`, `domains/kitchen/entities/ingredient.jadpo` and [area 9, A membership role](../../docs/language/09-policy.md#a-membership-role).
8. **Placeholders in a prefix.** `prefix: /bakeries/{bakery_id: Bakery.id}/recipes` gives every route in the group the bakery, and its file is named after the fixed segments. See `domains/kitchen/routes/bakeries_recipes.jadpo` and [area 7, Route groups](../../docs/language/07-http.md#route-groups).
9. **Proving policy in route tests.** A fixture sets up a bakery with an owner, a baker and someone on the counter; each test signs in as them and as outsiders, and checks a 403, a hidden record, an empty list and a role ended by its owner. See `domains/kitchen/fixtures/northwind.jadpo`, `domains/kitchen/routes/recipes.test.jadpo` and [area 13, Acting as a user](../../docs/language/13-testing.md#acting-as-a-user).
10. **The permissions report.** `jadpo check` writes `build/report/permissions.md` and `.json`: who may do what to every entity and restricted field, each tenancy and membership, and the actions listed separately. See [area 9, The permissions report](../../docs/language/09-policy.md#the-permissions-report).

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/12-recipe-book
target/release/jadpo test examples/12-recipe-book
target/release/jadpo fmt --check examples/12-recipe-book
cd examples/12-recipe-book && ../../target/release/jadpo sql bakery_recipes
```

- `check` ends with `checked: <n> files`, and writes the permissions
  report to `build/report/permissions.md`: a table per entity, a row for
  each field that narrows its record, and the two listed actions.
- `test` ends with `tested: 12 passed, 1 action test, 11 route tests`.
- `sql bakery_recipes` prints the query's statement with the policy it
  carries: the roles that may read a recipe, and the tenancy, as
  conditions on the caller's places in the recipe's bakery.

## Try next

- In `recipe_card.jadpo`, add `Recipe.batch_cost` to `pick:`. `jadpo check` reports `policy.output_field_unproved` at `bakery_recipes`: bakers and counter staff may call it, and only owners may read the cost. It also reports `type.missing_field` where the actions build a card without the cost, and `api.record_out_of_date`, since the card is part of what clients are sent.
  <!-- try {"file": "domains/kitchen/types/recipe_card.jadpo", "replace": "pick: [Recipe.id, Recipe.name, Recipe.author_id]", "with": "pick: [Recipe.id, Recipe.name, Recipe.author_id, Recipe.batch_cost]", "check": ["api.record_out_of_date", "policy.output_field_unproved", "type.missing_field"]} -->
- In `recipe.jadpo`, let `Access.authenticated` read `batch_cost`. `jadpo check` reports `policy.field_widens_entity`: a field only narrows its record's grants, and the record lets only its bakery's staff read it.
  <!-- try {"file": "domains/kitchen/entities/recipe.jadpo", "replace": "            read: [StaffRole.owner]\n            update: [StaffRole.owner]", "with": "            read: [Access.authenticated]\n            update: [StaffRole.owner]", "check": ["policy.field_widens_entity"]} -->
- In `policy.jadpo`, delete the `kitchen.hand_over_recipe` line, and `Recipe` from the `use` line. `jadpo check` reports `type.field_update_forbidden`: `author_id` decides who may change a recipe, so changing it is a listed action.
  <!-- try {"file": "policy.jadpo", "replace": "        kitchen.hand_over_recipe: [Recipe.author_id]\n", "with": "", "also": [{"replace": "use kitchen/entities { Bakery, Recipe }", "with": "use kitchen/entities { Bakery }"}], "check": ["type.field_update_forbidden"]} -->
- In `baker.jadpo`, delete `tenant: Tenancy.shared`. `jadpo check` reports `policy.scope_missing`: a baker has no reference to a bakery, so it must say it belongs to every tenant.
  <!-- try {"file": "domains/kitchen/entities/baker.jadpo", "replace": "        tenant: Tenancy.shared\n", "with": "", "check": ["policy.scope_missing"]} -->
- In `staff_member.jadpo`, change `read: [StaffRole.owner, baker_id]` to `read: [StaffRole.owner]`. The team's route test fails with `test.assertion_failed`: a baker no longer sees even their own place.
  <!-- try {"file": "domains/kitchen/entities/staff_member.jadpo", "replace": "read: [StaffRole.owner, baker_id]", "with": "read: [StaffRole.owner]", "test": ["test.assertion_failed"], "failed": 1} -->
- That is the last example. The [language pages](../../docs/language/README.md) hold the full specification.
