# Policy: simplification proposals

**Status:** research for [area 9](../09-policy.md), question 1, 2026-10-12.
This decides nothing.

## What is complicated today

Policy is the area with the most ideas a reader has to hold at once. Today's
model (the approved POLICY-001 plan, [policy-plan.md](../../policy-plan.md))
has:

1. role enums, always qualified (`CompanyRole.editor`);
2. two built-in subjects, `Access.public` and `Access.authenticated`;
3. owner fields (`role:` in a field's block, plus a mandatory `immutable: true`);
4. membership entities (`membership { scope member role }`);
5. scope, inferred from the one reference to the role's entity, or chosen
   with `scope:`;
6. the entity grant matrix (`Role: [create, read, update, delete]`);
7. field restrictions, with an empty `policy {}` meaning "no client";
8. action grants in two places: `policy { Role: [invoke] }` in an action's
   body, and `operations { }` exceptions in the entity's policy.

None is wrong, and the compiler already proves every query and write against
them. What makes it feel heavy is the ritual (`immutable: true`, the empty
block), the two homes for action grants, the separate `invoke` effect, and
that tenancy is never stated: it emerges from which roles each entity
happens to name. The design-only `policy.jadpo` sentence language in the
golden todo application adds a second, conflicting picture.

The question is whether a different model would be simpler, or whether this
one only needs tidying.

## How others keep authorisation understandable

- **Pundit** (Rails): one plain class per model with yes/no methods
  (`update?`), and a `Scope` class whose `resolve` filters lists. Checks are
  called by hand; `verify_authorized` only warns in development that one was
  forgotten, and its README says it is not a failsafe.
  <https://github.com/varvet/pundit>
- **CanCanCan** (Rails): one `Ability` class, `can :update, Article,
  company_id: user.company_id`; hash conditions double as list filters
  (`accessible_by`). <https://github.com/CanCanCommunity/cancancan>
- **Django**: model-level permissions only (`add`, `change`, `delete`,
  `view`); per-record rules need add-ons (django-guardian, django-rules).
  <https://docs.djangoproject.com/en/stable/topics/auth/default/#permissions-and-authorization>
- **PostgreSQL row-level security / Supabase**: a SQL condition per table and
  operation, applied as an extra `WHERE`; `USING` limits existing rows and
  `WITH CHECK` limits new ones. Default deny, but only once enabled per table,
  and Supabase advises wrapping `auth.uid()` and indexing every column a
  policy filters on.
  <https://www.postgresql.org/docs/current/ddl-rowsecurity.html>,
  <https://supabase.com/docs/guides/database/postgres/row-level-security>
- **Ash** (Elixir): `policies` inside the resource; conditions such as
  `action_type(:read)` with `authorize_if` checks; default deny; read checks
  become query filters, so a forbidden record reads as not found and lists
  shrink. Field policies are separate. <https://ash.hexdocs.pm/policies.html>
- **Cerbos**: one YAML policy file per resource kind outside the code, rules
  of actions, roles, derived roles and conditions; adapters turn a query plan
  into ORM filters. <https://docs.cerbos.dev/cerbos/latest/policies/resource_policies.html>
- **Oso Polar**: resource blocks declaring roles, permissions and relations,
  with shorthand such as `"reader" if "member" on "organization"`.
  <https://www.osohq.com/docs/reference/polar/resource-blocks>
- **OpenFGA / Zanzibar**: types with relations, stored relationship tuples
  (`user:anne editor document:x`), relations defined from other relations;
  `check` for one object, `list objects` for many.
  <https://openfga.dev/docs/concepts>,
  <https://research.google/pubs/zanzibar-googles-consistent-global-authorization-system/>

Three lessons repeat. Frameworks that filter lists from the same rule that
checks one record (Ash, row-level security, CanCanCan hashes) avoid the
classic leak where the list forgot the check. Rules kept beside the model
(Ash, Pundit) are found; rules kept far away (Cerbos) are reviewed as a set.
And free-form conditions are the source of most mistakes: Supabase's own
guidance is mostly about keeping conditions simple and indexed.

## Option A: today's role model, tidied (recommended)

Keep the five concepts, remove the ritual and the second homes:

```jadpo fragment
// domains/accounts/entities/article.jadpo
entity Article {
    id: Uuid
    company_id: Company.id {
        on_delete: cascade
    }
    title: ArticleTitle
    billing_reference: Text? {
        policy: {
            CompanyRole.owner: [read, update]
        }
    }

    policy: {
        CompanyRole.owner: [create, read, update, delete]
        CompanyRole.editor: [read, update]
        CompanyRole.viewer: [read]
    }
}

// policy.jadpo
policy {
    tenant: Company
    actions: {
        monitoring.administrative_health: [ApplicationRole.operator]
    }
}
```

Changes: policy always on; `tenant:` stated once; role fields immutable
without saying so; `policy: server_only` instead of an empty block; every
action grant in root `actions:`, so `invoke` and `operations { }` go; colons
on every section.

- **For:** already built and proved against SQLite and the multi-company
  pressure application; every rule is a list a reviewer can scan; lists and
  lookups are filtered from the same grant; tenancy becomes impossible to
  forget.
- **Against:** no role hierarchy, so an owner is listed on every line an
  editor is; rules that depend on a record's state ("only drafts can be
  edited") are lifecycle or business rules, not policy, which a newcomer may
  look for in the wrong place.

## Option B: rules written as conditions

Each entity states, per effect, a condition over the record and the caller,
as row-level security, Pundit scopes and the golden design `policy.jadpo` do:

```jadpo fragment
entity Article {
    ...
    policy: {
        read: is_member(Article.company_id)
        update: has_role(Article.company_id, [CompanyRole.owner, CompanyRole.editor])
        delete: has_role(Article.company_id, [CompanyRole.owner])
    }
}
```

- **For:** familiar to anyone who has written SQL policies; states like
  "published articles are public" fit in one line.
- **Against:** every rule can be different, so a reviewer must read each
  condition; the compiler can prove and index only a subset (the proof kernel
  marks the rest "cannot prove"); conditions on record state blur policy with
  business rules, which the plan separates on purpose (POLICY-D25, D26); it
  reintroduces what POLICY-001 replaced.

## Option C: relationships

Declare how records relate and define permissions from relationships, as
OpenFGA and Oso do:

```jadpo fragment
entity Article {
    ...
    relations: {
        company: company_id
    }
    policy: {
        read: [CompanyRole.viewer on company]
        update: [CompanyRole.editor on company]
        delete: [CompanyRole.owner on company]
    }
    roles: {
        CompanyRole.editor: [CompanyRole.owner]     // an owner is also an editor
    }
}
```

- **For:** role hierarchy and inheritance from parents (folders, projects
  inside organisations) are first-class, and large shared-document products
  scale on it.
- **Against:** two more concepts (relations and implied roles); "who can
  delete this?" needs the graph followed rather than a list read; filtering a
  list needs a "list objects" search or a separate index, which Zanzibar-style
  systems maintain apart from the database; most business applications have
  one tenant level and two or three roles.

## Where policy lives (a separate axis)

Any option can be written beside each entity or in one central file.
Area 1 settled protection: policy changes need approval wherever they sit, so
placement is only about reading. Beside the entity (Ash, Pundit) keeps a
field and its rule together and avoids two files drifting; one file (Cerbos)
lets a reviewer see every grant at once. The compiler already renders both
the role-first and effect-first views from source (POLICY-D41), so the
central view exists without a second source. Option A therefore keeps entity
grants in entity files and puts only application-wide rules (tenancy,
action grants) in root `policy.jadpo`.

## Recommendation

Option A. It removes the ritual and the second home for action grants, adds the
one missing safety net (tenancy), and keeps everything the compiler can
already prove. Revisit role hierarchy (Option C's `roles:`) only if the order
and payment application shows matrices repeating the same role lists; revisit
conditions (Option B) only for public-by-state cases, and then as a lifecycle
`visible:` rule rather than policy.
