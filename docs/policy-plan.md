# POLICY-001 decision plan

**Status:** approved contract; POLICY-P0–P4 and the executable P5 core implemented, protected approval and full golden exit pending

**Prepared:** 2026-09-27

**Approved:** 2026-09-27

**Accepted-contract digest:** `sha256:66e7a8f586b62ed92c3a7220f524e25aee5808ca60b8504fb3c2d225ef2d68bd` (section 2)

**Scope:** provider-independent authorisation for user and service principals;
application-, resource-, tenant-, and record-scoped roles; membership and direct
relationship role bindings; entity-, operation-, and field-level permissions;
automatic query and mutation scoping; input, persistence, projection, and output
validation; concealment; audit; approval; diagnostics; and deterministic policy
evidence

**Out of scope:** authentication providers and credential validation, business
and lifecycle rules, database administration, general-purpose rule engines,
arbitrary policy code, role hierarchy, implicit administrator bypass, raw SQL,
and a second capability/permission vocabulary

This plan replaces the earlier candidate actor/guard, manual `require policy`,
and separate human policy-file sketches. It keeps policy human-owned through
semantic approval rather than by duplicating application structure in a distant
file.

## 1. Existing constraints

The policy design must preserve these accepted repository commitments:

- authentication produces exactly one provider-independent user or service
  principal, or no principal for an explicit `auth: none` route;
- credentials contain identity and authentication facts, never roles,
  permissions, tenant authority, or capabilities;
- every entity has one authoritative dossier and owns its direct mutations;
- named queries own reads, while routes and workflows cannot contain raw query
  expressions or bypass entity operations;
- the compiler knows every query source, selected field, mutation target,
  route, action, job, service, projection, transaction, and external effect;
- field references carry intrinsic value validation but do not themselves
  prove existence or authorisation;
- boundary inputs and outputs use exact closed declared types;
- a database read reconstructs and validates nominal application values before
  they become trusted;
- partial reads produce complete projection values, not partial entities;
- lifecycle, business invariants, generated-field ownership, and policy remain
  distinct semantic facts even when lowered into one database statement;
- policy and invariant reads guarding a mutation share that mutation's checked
  transaction and concurrency plan;
- authentication is required by default and `auth: none` is the sole initial
  public-route opt-out;
- a policy weakening requires a canonical before/after semantic diff and an
  externally protected human approval attestation; and
- generated tests and audits are evidence, not substitutes for policy intent,
  compiler enforcement, or independent human review.

## 2. Approved v0.1 decisions

| ID | Decision |
| --- | --- |
| POLICY-D01 | Authentication and authorisation remain separate. Authentication yields a typed user or service identity; policy decides what that identity may do. |
| POLICY-D02 | Roles are closed nominal enum values and are always referenced with their declaring type, such as `CompanyRole.editor` or `ApplicationRole.support`. There are no unqualified role names. |
| POLICY-D03 | A role fact consists semantically of principal identity, qualified role, and scope. Scope may be the application, one tenant/resource, or one entity record. Application authors do not construct role facts directly. |
| POLICY-D04 | Role facts come only from compiler-recognised authoritative bindings. Credentials, request fields, token claims, headers, route parameters, and arbitrary action expressions cannot grant roles. |
| POLICY-D05 | A membership declaration identifies exactly one scope field, member field, and role-value field. A current validated membership row produces a scoped role fact for that member. |
| POLICY-D06 | A principal-reference field may establish one fixed qualified role, for example `Company.owner_id { role: CompanyRole.owner }`. The binding establishes a role fact; it grants no operation by itself. |
| POLICY-D07 | A protected entity using a scoped role must have one unambiguous relationship to that role's scope. The compiler infers the common single-reference case; multiple possible scope paths require one explicit `scope:` selection and ambiguity is a compile error. |
| POLICY-D08 | Role namespaces describe the scope in which authority is meaningful rather than the kind of principal holding it. Prefer `CompanyRole.owner`, `ProjectRole.maintainer`, and `ApplicationRole.support`; user versus service remains an orthogonal principal distinction. |
| POLICY-D09 | Entity policy is a role-first matrix mapping qualified roles to the standard semantic effects `create`, `read`, `update`, and `delete`. It is colocated in the authoritative entity dossier. |
| POLICY-D10 | The compiler derives an operation's entity effects from the checked query and mutation graph. Authors do not attach routine access declarations to every query/action and do not write `require policy` calls. |
| POLICY-D11 | Every named query and entity action automatically inherits the policy for every entity effect it reaches. Ten update actions therefore share one entity `update` rule without repetition or drift. |
| POLICY-D12 | Policy is default-deny. An absent entity effect, absent non-entity `invoke` permission, unresolved role binding, ambiguous scope, unsupported principal kind, or unproved composition blocks compilation or execution; omission never grants access. |
| POLICY-D13 | Multiple role entries for one target effect are alternatives: any matching effective role may authorise that effect. Independent requirements across entities, effects, fields, or nested calls compose conjunctively and must all pass. |
| POLICY-D14 | Rare named-operation exceptions live inside the entity policy, not beside action bodies. They replace the inherited permission only for the named effect of that named operation and are always security-review subjects. |
| POLICY-D15 | Ordinary fields inherit entity permissions. An optional field policy is an additional narrowing constraint for exceptional fields; it cannot widen the containing entity or operation permission. |
| POLICY-D16 | Policy never silently removes input or output fields. A forbidden supplied input field rejects the whole operation atomically. An output projection that is not readable by every role admitted to that output path fails compilation or requires a narrower named operation/projection. |
| POLICY-D17 | Input validation and authorisation are separate gates. Closed decoding proves shape, presence, nullability, nominal type, enum membership, and intrinsic constraints; it does not prove record existence, role, scope, lifecycle, or permission. |
| POLICY-D18 | Patch presence remains visible after validation. Policy checks only fields actually supplied, but one unauthorised supplied field rejects the entire mutation; no field is ignored and no partial update commits. |
| POLICY-D19 | Identity, compiler-generated, lifecycle-owned, immutable, secret, and role-binding fields retain their own write-ownership rules. Entity `create` or `update` permission does not make those fields caller-writable. |
| POLICY-D20 | Creation policy is evaluated against the complete proposed new record before insertion. A resource role established by a new relationship field therefore matches only when the proposed relationship actually identifies the principal or a valid scoped membership. |
| POLICY-D21 | Read, update, and delete policy are evaluated against authoritative existing state. Policy predicates are lowered into the database read or mutation whenever possible rather than checking a materialised row afterwards. |
| POLICY-D22 | A field that establishes role or tenant scope is protected from ordinary update by default. Ownership transfer or tenant movement requires a named reviewed operation with explicit source- and destination-scope obligations. |
| POLICY-D23 | Named entity queries automatically receive row-scope predicates. Collection queries exclude unauthorised rows; optional/required lookup paths conceal row existence by default; and a query cannot weaken policy by supplying its own tenant filter. |
| POLICY-D24 | Includes, traversals, cross-entity projections, and workflows apply policy independently to every protected source entity and selected field. Permission on a parent never implies permission on a child, and permission on one mutation scope never implies another. |
| POLICY-D25 | Business predicates, lifecycle visibility, and policy predicates remain separate semantic graph nodes with separate audit provenance. The compiler may combine them into one efficient SQL predicate without changing their ownership. |
| POLICY-D26 | `deleted_at == none`, expiry, workflow state, and permitted domain transitions are lifecycle or business rules, not policy. A role or membership status may itself have lifecycle, and only current authoritative bindings produce effective roles. |
| POLICY-D27 | Routes retain the accepted authentication contract: authenticated by default, explicit `auth: none` for public access, and an explicit fresh-authority requirement for sensitive boundaries. Entity roles are not repeated on routes. |
| POLICY-D28 | A route calling an entity query/action inherits its policy transitively. A public route can reach only paths explicitly admitting `Access.public`; it cannot reach a principal-dependent role path. A route may still constrain a company or parent identifier for URL/business meaning, but that predicate is never treated as authorisation. |
| POLICY-D29 | A non-entity operation with independently meaningful authority declares a local role-to-`invoke` policy. This is used for administrative health, exports, application commands, and similar operations that have no entity effect from which access can be derived. |
| POLICY-D30 | Jobs and internal callers use authenticated service principals and qualified service or resource roles. Being generated, scheduled, or internal does not bypass entity, field, service, or external-effect policy. |
| POLICY-D31 | Authorisation facts default to authoritative freshness. A cache or derived store may participate only when its declared propagation and revocation bounds satisfy the policy; otherwise the compiler revalidates against authority before releasing or changing data. |
| POLICY-D32 | A missing credential produces the authentication failure before protected business work. A valid principal lacking a non-resource permission produces `NotPermitted`. A row-scoped lookup or mutation that matches no authorised row produces the same concealed result as absence by default. No authored `conceal_as` setting exists in v0.1. |
| POLICY-D33 | Database constraints own storage integrity, not application authorisation. Jadpo generates policy-scoped parameterised statements and keeps one semantic policy authority. Adapter-specific row-level security may later be generated as defence in depth but cannot be a divergent second policy source. |
| POLICY-D34 | Every selected database value is decoded and validated into its declared Jadpo type before projection or output. Corrupt, stale-schema, invalid-enum, or otherwise contradictory database data produces a contained persistence fault rather than a trusted value. |
| POLICY-D35 | Output policy is proved before serialization, and the runtime then validates and closed-serializes the exact declared output type. Undeclared fields, incomplete values, invalid values, secrets, and policy-forbidden fields fail closed; output shape never varies silently by role. |
| POLICY-D36 | The compiler tracks input provenance through action construction and mutation. Renaming an input field, using a base type instead of an entity field type, or copying through a local value cannot bypass target-field policy or write ownership. |
| POLICY-D37 | Raw SQL, target-language calls, arbitrary callbacks, unrecognised stores, and direct workflow/route mutations cannot bypass policy. Unsupported escape paths remain compile errors rather than unscoped operations. |
| POLICY-D38 | Policy source is semantically colocated with the protected entity, exceptional field, or non-entity operation. Human ownership is enforced by the approval protocol over canonical policy and semantic-graph digests, not by maintaining a duplicate distant policy file. |
| POLICY-D39 | The approval subject includes role declarations, membership and direct-role bindings, scope paths, entity matrices, operation exceptions, field restrictions, input-to-write flows, projections/outputs, routes/jobs/services, lifecycle interaction, generated predicates, and direct/transitive effects. |
| POLICY-D40 | A change that makes a new actor, row, field, operation, route, service, or effect reachable is a policy weakening even when no `policy` block changed. Adding an output field, accepting an input field, changing an operation effect, or adding a scope path can therefore require approval through the semantic-graph diff. |
| POLICY-D41 | Policy tooling renders both the authored role-first view (“what can this role do?”) and a derived effect-first matrix (“who can read/update/delete?”). The authored source has one canonical spelling; alternate rendered views are not additional policy authorities. |
| POLICY-D42 | There is no capability layer, permission-string layer, role hierarchy, implicit inheritance, implicit administrator override, deny-precedence language, actor/guard alias layer, or method-local policy DSL in v0.1. New abstraction layers require demonstrated application pressure. |
| POLICY-D43 | The compiler supplies exactly two qualified non-role access subjects: `Access.public` means no principal is required and `Access.authenticated` means any valid user or service principal. Public exposure still requires the route's explicit `auth: none`; restricting by principal kind or domain standing uses an authoritative application/resource role rather than another built-in subject. |

## 3. Conceptual model

The author-facing model has four concepts plus two explicit built-in access
subjects for public and role-free authenticated behaviour.

### 3.1 Principal

Authentication produces either `Principal.user`, `Principal.service`, or no
principal for an explicit public route. The principal contains stable identity
and allowed authentication facts. It does not contain role or permission claims
copied from credentials.

Policy uses the principal only as the subject whose authoritative role facts
must be resolved. Raw cookies, keys, JWT claims, provider identities, and
authentication SDK objects never enter policy.

### 3.2 Qualified role

A role is a closed enum value:

```text
enum CompanyRole {
    owner
    author
    editor
    viewer
    administrator
}

enum ApplicationRole {
    support
    operator
}
```

Source always uses qualified variants:

```text
CompanyRole.editor
ApplicationRole.support
```

The role namespace names the scope whose authority it describes. It does not
name the principal variant that happens to hold it. A user can therefore be
`CompanyRole.owner` in one company and `CompanyRole.viewer` in another. A
service may hold a declared company role if the membership contract accepts a
service member.

Internally, the compiler reasons over a fact equivalent to:

```text
RoleFact {
    principal
    role
    scope
}
```

That internal representation is not an authored value and cannot be forged or
returned.

The permission matrix may also use two compiler-owned qualified subjects:

```text
Access.public
Access.authenticated
```

`Access.public` requires no principal but does not by itself make a route
public; the route must still state `auth: none`. `Access.authenticated` matches
any successfully authenticated user or service. If only users, only services,
or only current domain members should qualify, the application declares an
authoritative role instead of relying on principal kind or credential claims.

### 3.3 Role binding

A role binding tells the compiler where a role fact comes from. v0.1 has two
forms.

A direct relationship binds one fixed role:

```text
entity Company {
    id: CompanyId

    owner_id: User.id {
        role: CompanyRole.owner
        immutable: true
    }
}
```

For Company `A`, the user referenced by `owner_id` has
`CompanyRole.owner` scoped to Company `A`. The annotation alone grants no read,
update, or delete operation.

A membership entity binds a stored role value:

```text
entity CompanyMembership {
    id: CompanyMembershipId
    company_id: Company.id
    user_id: User.id
    role: CompanyRole

    membership {
        scope: company_id
        member: user_id
        role: role
    }
}
```

The declaration means that a current validated membership row produces its
qualified `CompanyRole` for the referenced user and company. The membership
entity may also contain invitation state, expiry, timestamps, or other domain
data, but lifecycle determines whether the binding is current. Policy does not
repeat `status == active` in every consuming entity.

Application-wide assignments use the same semantic form with
`scope: application`. Service membership uses a `Service.id` member. More
complex many-to-many authority remains an explicit membership entity rather
than a hidden array claim.

### 3.4 Permission matrix

An entity policy maps qualified roles or the two built-in access subjects to
semantic effects:

```text
policy {
    CompanyRole.owner: [create, read, update, delete]
    CompanyRole.author: [create, read]
    CompanyRole.editor: [read, update]
    CompanyRole.viewer: [read]
    CompanyRole.administrator: [create, read, update, delete]
}
```

This is the only ordinary mapping from subjects to entity permissions. Defining
a role or binding does not grant access. A role-gated operation is permitted
only when the principal has an effective role in the target scope and the
matrix grants that role the derived effect. `Access.public` and
`Access.authenticated` have the fixed meanings above and cannot be rebound.

The source is role-first because it directly answers “what can this role do?”.
The compiler and audit also render the transposed effect-first matrix for
questions such as “who can delete this entity?”.

## 4. Complete multi-company example

The following source is the canonical pressure example for the initial syntax.
Exact parser punctuation remains fixture-first, but its semantic facts are
fixed by this contract.

```text
enum CompanyRole {
    owner
    author
    editor
    viewer
    administrator
}

entity Company {
    id: CompanyId {
        generated: identity
    }

    owner_id: User.id {
        role: CompanyRole.owner
        immutable: true
    }

    name: CompanyName

    policy {
        CompanyRole.owner: [read, update, delete]
        CompanyRole.editor: [read]
        CompanyRole.viewer: [read]
        CompanyRole.administrator: [read, update, delete]
    }
}

entity CompanyMembership {
    id: CompanyMembershipId {
        generated: identity
    }

    company_id: Company.id
    user_id: User.id
    role: CompanyRole
    status: MembershipStatus

    membership {
        scope: company_id
        member: user_id
        role: role
    }

    lifecycle {
        // Only current memberships establish roles.
    }

    policy {
        CompanyRole.owner: [create, read, update, delete]
        CompanyRole.administrator: [create, read, update, delete]
    }
}

entity Article {
    id: ArticleId {
        generated: identity
    }

    company_id: Company.id {
        immutable: true
    }

    created_by: User.id {
        immutable: true
    }

    title: ArticleTitle
    content: ArticleContent

    billing_reference: Text? {
        policy {
            CompanyRole.owner: [read, update]
            CompanyRole.administrator: [read, update]
        }
    }

    created_at: Instant {
        on: create
    }

    updated_at: Instant {
        on: create_or_change
    }

    deleted_at: Instant? {
        lifecycle: soft_delete
    }

    policy {
        CompanyRole.owner: [create, read, update, delete]
        CompanyRole.author: [create, read]
        CompanyRole.editor: [read, update]
        CompanyRole.viewer: [read]
        CompanyRole.administrator: [create, read, update, delete]
    }

    lifecycle {
        // Visibility and state transition rules, not role permissions.
    }

    query by_id(id: Article.id) -> ArticleView? {
        where: id == id
        project: ArticleView
    }

    action rename(self: ref, input: RenameArticle)
        fails ArticleNotFound
        -> ArticleView
    {
        return attempt update required Article {
            where: id == self.id
            patch: input
            missing: ArticleNotFound
        }
    }
}
```

No query or action repeats Company roles. The compiler sees that `by_id` reads
Article and `rename` updates Article, identifies `company_id` as the sole
Company scope, resolves direct owner and membership role facts, applies field
restrictions, and lowers the complete authorised plan.

The `created_by` field is audit data only. It becomes a role binding only if it
is explicitly annotated. Creation does not silently mean continuing ownership.

## 5. Scope resolution

### 5.1 Common inference

When an entity policy uses `CompanyRole` and the entity has exactly one path to
`Company`, the compiler selects that path:

```text
entity Article {
    company_id: Company.id

    policy {
        CompanyRole.editor: [read, update]
    }
}
```

The selected scope is `Article.company_id`. The author does not repeat it in
queries and actions.

### 5.2 Ambiguity

An entity with several Company references must select one:

```text
entity InvoiceTransfer {
    source_company_id: Company.id
    destination_company_id: Company.id

    policy {
        scope: source_company_id
        CompanyRole.administrator: [read, update]
    }
}
```

If both scopes matter, a named operation exception must state the separate
source and destination obligations. The compiler never guesses from field
names, declaration order, route paths, or a predicate written by the action.

### 5.3 Direct record roles

A directly owned record can bind a record-scoped role:

```text
enum NoteRole {
    owner
}

entity PrivateNote {
    owner_id: User.id {
        role: NoteRole.owner
        immutable: true
    }

    policy {
        NoteRole.owner: [create, read, update, delete]
    }
}
```

The role namespace is resource-oriented. `NoteRole.owner` communicates that
the role is meaningful for a PrivateNote, while `CompanyRole.owner` is
meaningful for a Company and company-scoped resources.

### 5.4 Role freshness

Direct relationship roles come from the authoritative row participating in the
operation. Membership roles come from the authoritative membership source.
Inactive, expired, removed, or not-yet-accepted membership state is excluded by
the membership lifecycle contract.

A derived membership index may accelerate evaluation only when its declared
delivery and revocation bound satisfies policy. Otherwise it supplies candidate
identities that are revalidated against authority. A request credential never
acts as the role source.

## 6. Effect derivation and operation inheritance

The compiler classifies checked operation graphs:

| Graph effect | Required entity permission |
| --- | --- |
| Insert a new entity row | `create` against the proposed row |
| Select or disclose existing entity state | `read` against existing rows |
| Change persisted semantic state | `update` against existing rows |
| Hard delete or invoke the entity's declared delete lifecycle | `delete` against existing rows |

Implementation-only reads required to locate, lock, validate, or mutate a row
do not create a separate disclosure permission. Returning selected entity
fields does require `read` permission and field readability.

An operation reaching several effects must satisfy every applicable
requirement. A workflow updating Article and CompanyMembership needs both
entity policies. A nested entity action retains the principal context and
applies its own policy; its caller cannot lend or manufacture authority.

Pure functions do not acquire database access or policy authority. Policy is
not inferred from function names such as `admin_update`, route paths, or HTTP
methods.

### 6.1 Named-operation exception

A rare operation may use a different role matrix:

```text
policy {
    CompanyRole.owner: [read, update]
    CompanyRole.editor: [read, update]

    operations {
        restore {
            CompanyRole.administrator: [update]
        }
    }
}
```

The exception is declared once, centrally, and names a checked entity
operation. The compiler rejects an unknown operation, an effect the operation
does not perform, or an exception that leaves another reached effect
unresolved. Ordinary actions never carry policy declarations merely to restate
the default.

## 7. Field policy and write ownership

### 7.1 Inheritance and narrowing

A normal field inherits the entity role/effect matrix. Only exceptional fields
declare policy:

```text
billing_reference: Text? {
    policy {
        CompanyRole.owner: [read, update]
        CompanyRole.administrator: [read, update]
    }
}
```

An editor may update an Article generally but may not read or update
`billing_reference`. Effective permission is:

```text
entity permission
AND named-operation permission, when present
AND field permission, when present
AND field write-ownership rules
```

A field policy cannot make `CompanyRole.viewer` an updater when the entity
policy grants that role only `read`. Broadening belongs at the entity or named
operation layer and receives the corresponding review.

### 7.2 Supplied input fields

For an omission-aware patch, policy is evaluated for each supplied target
field. An editor submitting only `title` may succeed. The same editor
submitting `title` and `billing_reference` receives one policy rejection and no
field changes commit.

The decoder does not delete the forbidden field. The mutation planner does not
ignore it. This preserves exact input validation, deterministic failure, and
atomicity.

### 7.3 Compiler-owned and protected fields

Policy permission does not override field ownership:

- an identity field is assigned by the identity authority;
- `on: create` and `on: create_or_change` timestamps are assigned by generated
  application persistence using the stable operation instant;
- lifecycle fields are changed only through declared lifecycle transitions;
- immutable fields cannot be updated;
- secrets cannot flow to ordinary outputs;
- direct role-binding and scope fields cannot be changed by ordinary update;
  and
- a membership role field cannot be changed through an unreviewed general
  patch merely because the caller may update unrelated membership data.

The compiler tracks the target field and value provenance through locals,
function calls, projections, input aliases, and base-type widening. Replacing
`Article.company_id` with a compatible-looking `Company.id` input does not
bypass the target field's policy or ownership.

### 7.4 Ownership and tenant transfer

Changing a direct role-binding or scope field changes future authority. It is
therefore denied in ordinary updates. A transfer operation must be named and
must declare enough policy to prove:

- authority over the source resource;
- authority to establish the destination relationship;
- the destination entity or tenant exists and is current;
- fields and dependent entities have deliberate migration/lifecycle meaning;
- no unauthorised intermediate state is observable; and
- approval covers the resulting access expansion or removal.

The initial implementation may reject cross-tenant transfer entirely until a
golden application requires it.

## 8. Validation and trust-boundary composition

Policy is one gate in a complete trust chain. No gate substitutes for another.

```text
authentication
    -> closed input decoding
    -> intrinsic value validation
    -> effect and target-field analysis
    -> authoritative role resolution
    -> entity/operation/field policy
    -> lifecycle and business invariants
    -> policy-scoped database operation
    -> database constraint enforcement
    -> database result decoding and validation
    -> authorised projection construction
    -> exact output validation and serialization
```

### 8.1 Authentication

For a protected route, authentication runs before request input decoding or
business database access, as required by AUTH-001. It produces one principal.
Missing, invalid, ambiguous, disabled, or stale credentials follow the
authentication contract and never become policy role facts.

An `auth: none` route has no principal. It can call an entity operation only
when the effective path admits `Access.public`; it cannot call an operation
whose policy requires `Access.authenticated` or a user/service role. This is
checked through the call/effect graph rather than discovered from a runtime
failure.

### 8.2 Input decoding and intrinsic validation

Generated closed decoders enforce:

- the declared object/list shape;
- required versus omitted fields;
- `T?` values versus omission;
- unknown-field rejection;
- nominal types and field refinements;
- constrained values;
- valid qualified enum wire values; and
- bounded collection and request limits.

Successful decoding produces a trusted value. It does not prove that an entity
exists, that a reference resolves, that a membership is current, or that the
principal may cause a write.

Using an entity field type in an input reuses value semantics only. For example,
`company_id: Article.company_id` validates the nominal identifier shape but
does not grant Company membership or Article access.

### 8.3 Effect and field analysis

The compiler resolves every query and mutation target before runtime. It knows
the entity effects, possible patch fields, fixed derived writes, returned
fields, and reachable nested operations. An unanalysable target or escape hatch
is rejected.

At runtime, omission-aware inputs retain supplied flags. Policy checks the
actual supplied set before the mutation. Fixed mutation fields are known
statically.

### 8.4 Authoritative role resolution

The runtime resolves only role sources needed by the operation. Direct
bindings are evaluated from the authoritative target/scope row. Memberships
use generated bounded predicates or joins over their authority. Role state is
not trusted from input, local variables, or credential claims.

### 8.5 Lifecycle and business validation

Lifecycle decides whether the current state may participate and whether a
transition is permitted. Business invariants validate domain meaning.
Examples include active membership, article publication state, expiry, due
dates, status transitions, and soft-delete visibility.

These conditions may remove a row from a generated statement or reject a
typed domain failure, but audit labels them separately from policy.

### 8.6 Database constraints

The generated schema continues to enforce nullability, references, uniqueness,
checks, enum representations, identity, and declared delete actions. Policy
does not replace these constraints, and a constraint does not prove policy.

A database violation is normalized into a declared conflict or a contained
persistence fault according to the persistence/failure contracts. Driver text
never becomes public policy output.

### 8.7 Database result validation

Every selected column is decoded to its declared Jadpo value. Invalid enum
representations, malformed semantic values, unexpected nulls, precision
violations, stale columns, or inconsistent result shapes produce a contained
fault. The runtime never treats a driver string, JavaScript object, or database
native value as an already trusted application value.

### 8.8 Projection and output validation

The compiler proves the selected projection is readable under the operation's
effective role paths. The runtime constructs the exact declared projection and
validates the exact output type before closed serialization.

A richer entity cannot leak undeclared fields. A role-restricted field cannot
be conditionally omitted to make a broad output appear safe. Separate
audiences use separate named projections/queries or an explicitly tagged
output design.

## 9. Database lowering

### 9.1 Read and list

An authored query names only domain selection:

```text
query by_id(id: Article.id) -> ArticleView? {
    where: id == id
    project: ArticleView
}
```

The compiler adds the role scope. A collection query receives the same
predicate and therefore never materialises inaccessible rows for later
filtering.

### 9.2 Update

An authored action may contain:

```text
return attempt update required Article {
    where: id == article_id
    patch: input
    missing: ArticleNotFound
}
```

The generated PostgreSQL is semantically equivalent to:

```sql
UPDATE article
SET
    title = $new_title,
    updated_at = $operation_time
WHERE article.id = $article_id
  AND (
      EXISTS (
          SELECT 1
          FROM company
          WHERE company.id = article.company_id
            AND company.owner_id = $principal_user_id
      )
      OR EXISTS (
          SELECT 1
          FROM company_membership
          WHERE company_membership.company_id = article.company_id
            AND company_membership.user_id = $principal_user_id
            AND company_membership.role IN ('editor', 'administrator')
            /* membership lifecycle predicate */
      )
  )
  /* article lifecycle predicate */
RETURNING ...;
```

The exact plan may use joins, subqueries, prevalidated role sets, prepared
parameters, or another equivalent adapter strategy. It must preserve scope,
freshness, transaction, concealment, cardinality, and field obligations.

The security check uses the Article's stored `company_id`; a caller-supplied
company identifier cannot redirect it. A nested route may additionally require
`article.company_id == path.company_id` to honour URL meaning, but that is a
request predicate, not the membership proof.

### 9.3 Creation

Creation constructs and validates a complete proposed row before insertion.
Policy evaluates scoped roles against that proposed state. For a direct
record-owner policy, a new `owner_id` that does not match the principal cannot
satisfy the owner role. For a company policy, the proposed `company_id` must
identify a company in which the principal has an allowed current role.

Field ownership then proves which values may be caller-controlled, derived from
the principal, compiler-generated, lifecycle-owned, or supplied by a reviewed
service/job path. The policy check and insert occur in one safe transaction or
equivalent statement plan.

### 9.4 Delete

Hard delete and the entity's declared delete lifecycle are authorised through
`delete`. A soft-delete implementation may physically be an update, but its
semantic effect remains delete. Ordinary `update` permission cannot be used to
set `deleted_at` directly.

### 9.5 Concurrency

Role, lifecycle, and invariant reads guarding a write use the same transaction,
locking, revision, or conditional-write plan as the mutation. Membership or
ownership cannot be checked in one stale context and used after concurrent
revocation without satisfying the declared freshness/concurrency contract.

### 9.6 Database policy features

PostgreSQL row-level security may later be emitted as an adapter-specific
redundant enforcement layer. It must be mechanically generated from the same
semantic graph, digest-linked, tested for equivalence, and incapable of
granting more access than Jadpo policy. v0.1 correctness does not depend on it.

## 10. Outputs, projections, and includes

### 10.1 Stable output shape

An output is complete and role-independent. If `ArticleView` contains only
ordinary Article fields, every role admitted to the query must be able to read
them. If `ArticleBillingView` contains `billing_reference`, its query must be
restricted to the roles permitted by that field.

The following is invalid when owners may call `by_id` but cannot read a
selected support-only field:

```text
query by_id(...) -> ArticleInternalView {
    project: ArticleInternalView
}
```

The repair is to remove the field or introduce a narrower named query and
operation policy. The runtime never returns `{ billing_reference: omitted }`
for one role and a full object for another under the same output type.

### 10.2 Cross-entity projections

A projection combining Company, Article, and Membership data needs read access
to every selected row and field. Company permission does not imply Article
permission. Article permission does not imply permission to expose member
details. The compiler composes all required predicates and rejects a query plan
that cannot preserve them through joins, batching, pagination, or a derived
representation.

### 10.3 Includes

For an allowed parent with inaccessible children, a collection include contains
only authorised children. A required child include whose child is not visible
follows the declared required/absence failure contract without disclosing that
the child exists. Generated bounded multi-query plans carry parent identities
and policy scope without cross-parent or cross-tenant mixing.

### 10.4 Derived stores

A cache, search index, or graph projection may answer a query only when it can
preserve selected fields, tenant scope, role revocation, lifecycle, and required
freshness. Otherwise it supplies candidates for authoritative revalidation or
is not a valid plan.

## 11. Routes, application operations, jobs, and services

### 11.1 Routes

Routes own transport authentication requirements, input/output bindings, and
failure mapping. They do not restate entity roles:

```text
route PATCH "/companies/{company_id}/articles/{article_id}" {
    path: {
        company_id: Company.id
        article_id: Article.id
    }
    input: RenameArticle
    output: ArticleView
    run: Article.rename(article_id, input)
}
```

Authentication is required because `auth: none` is absent. The Article action
brings Article policy. The path company may be checked for route consistency,
while membership authority is still derived from the stored Article scope.

### 11.2 Public routes

A public liveness route is explicit:

```text
route GET "/health/live" {
    auth: none
    output: PublicHealth
    action: {
        return PublicHealth(...)
    }
}
```

The call graph must contain no principal-dependent policy, secret, protected
entity, or unsafe dependency disclosure. Readiness remains a deployment-plane
capability under CONFIG-001.

A deliberately public entity query requires both boundaries to agree:

```text
entity PublishedArticle {
    policy {
        Access.public: [read]
    }
}

route GET "/articles" {
    auth: none
    output: PublishedArticlePage
    run: PublishedArticle.list()
}
```

Removing either the route exception or the entity grant removes public
reachability. `Access.authenticated` is available for an entity or non-entity
operation intended for every valid principal without a stored role.

### 11.3 Non-entity operations

An application command without a deriving entity effect declares `invoke`:

```text
action administrative_health() -> AdministrativeHealth {
    policy {
        ApplicationRole.operator: [invoke]
    }

    // Checked implementation.
}
```

Reusable non-entity policy remains local to the operation. Route code does not
duplicate it. A one-off public inline action is authorised by the explicit
public route boundary and still cannot reach protected effects.

### 11.4 Jobs and services

A job activation has a named service principal or another compiler-owned
internal authority mapped into declared qualified roles. Calling an entity
action from a job applies normal policy. Generated scheduling is not authority.

External service permission, egress, credentials, retry, and idempotency remain
owned by SERVICE-001/ASYNC-001. Policy can restrict which operation/job may
reach a service, but it does not replace the service contract.

## 12. Failure and concealment semantics

The failure order is:

1. authentication failures for missing/invalid/ambiguous credentials;
2. input validation failures for malformed or invalid request data;
3. non-resource `NotPermitted` where denial does not reveal a protected row;
4. concealed absence for row-scoped read/update/delete paths;
5. lifecycle or domain failures after the caller is permitted to observe them;
6. normalised conflict/operational failures; and
7. contained faults for impossible states or invalid trusted-boundary data.

List queries exclude unauthorised rows. Optional-one queries return `none` for
absent or unauthorised rows. Required-one queries and mutations use their
declared `NotFound`-derived failure for either condition unless a separately
reviewed product contract proves disclosure safe. v0.1 has no per-operation
`conceal_as` syntax.

Internal telemetry may distinguish absence, role mismatch, lifecycle exclusion,
and constraint failure without sending those facts to the caller. Logs use
stable entity, operation, policy, and role identifiers without secret values or
raw credentials.

## 13. Human ownership, approval, and audit

### 13.1 One source, protected semantics

Policy is written once beside the entity/field/operation it protects. The
earlier separate `policy.jadpo` shape becomes migration evidence rather than a
second source of truth. Colocation avoids reconciliation drift; it does not let
an implementation agent authorise its own weakening.

### 13.2 Weakening classification

The compiler treats at least these changes as possible weakenings:

- adding an effect to a role;
- adding a role or role source;
- widening a membership lifecycle;
- changing or adding a direct role-binding field;
- selecting a new or broader scope path;
- adding or widening an operation exception;
- removing or widening a field restriction;
- making a protected field caller-writable;
- adding an input field that can influence a write;
- adding an output/projection field;
- changing an operation so it reaches another entity/effect;
- adding a public route or making protected behaviour reachable from one;
- weakening concealment, freshness, transaction, or revocation behaviour; and
- introducing an escape hatch or unproved derived-store policy path.

Removing access may still require lifecycle/product review because it can break
administration, recovery, or contractual behaviour, but it is not mislabeled as
an access expansion.

### 13.3 Canonical approval subject

Approval follows `approval-protocol.md`. The subject binds before/after policy
digests and before/after semantic graph digests, including transitive routes,
jobs, fields, projections, statements, services, failures, and tests. A normal
repository file, commit message, generated test, or agent assertion cannot
satisfy reviewer authority.

### 13.4 Audit output

For every externally reachable operation, generated audit states:

- principal kinds and authentication requirement;
- derived entity effects;
- accepted roles and their authoritative sources;
- selected scope path;
- field restrictions and supplied-field behaviour;
- authored request predicates;
- injected policy predicates;
- lifecycle and business predicates;
- generated/write-owned fields;
- database/transaction plan;
- selected/returned fields and output validator;
- reachable failures and public disclosure;
- direct/transitive callers and downstream effects; and
- evidence classification (`proved`, `runtime validated`, `generated-test
  supported`, `operationally enforced`, `assumed`, `unsupported`, or
  `cannot prove`).

The audit may render role-first and effect-first views, but neither is editable
authority.

## 14. Required diagnostics

The initial compiler needs stable diagnostics for at least:

| Diagnostic | Meaning |
| --- | --- |
| `policy.effect_ungranted` | An externally reachable entity effect has no permitted role. |
| `policy.role_unbound` | A policy references a role with no authoritative binding for the selected scope. |
| `policy.scope_missing` | A scoped role is used by an entity with no path to its scope. |
| `policy.scope_ambiguous` | More than one valid scope path exists and none was selected. |
| `policy.binding_invalid` | A direct or membership binding has incompatible principal, role, or scope fields. |
| `policy.binding_not_authoritative` | A proposed role source is request-, credential-, cache-, or projection-owned without sufficient authority proof. |
| `policy.operation_unknown` | A named operation exception references no operation in the entity. |
| `policy.operation_effect_mismatch` | An exception names an effect the operation does not perform or omits another reached effect. |
| `policy.field_widens_entity` | A field policy attempts to grant an effect absent from the entity/operation policy. |
| `policy.field_input_forbidden` | A possible input-to-write flow reaches a field the role may not write. |
| `policy.field_output_forbidden` | A projection/output path includes a field not readable by every admitted role path. |
| `policy.role_field_update_forbidden` | An ordinary update attempts to change ownership, membership role, or scope. |
| `policy.public_principal_required` | An `auth: none` path reaches behaviour requiring a principal role. |
| `policy.public_grant_missing` | An `auth: none` path reaches a protected entity operation without `Access.public`. |
| `policy.cross_entity_unproved` | A traversal, include, workflow, or projection lacks policy proof for one source entity. |
| `policy.freshness_insufficient` | A derived role/policy representation cannot satisfy the required revocation/freshness bound. |
| `policy.escape_unscoped` | Raw/foreign/unsupported access would bypass the policy graph. |

Diagnostics identify the concrete entity, operation, role, scope path, field,
and source span. They explain the missing proof in application language and do
not recommend broadening policy as an automatic fix.

## 15. Deterministic evidence

### 15.1 Compiler fixtures

Positive fixtures cover:

- direct Company owner role binding;
- scoped Company membership roles;
- one user holding different roles in different companies;
- owner and editor independently updating the same company-scoped entity;
- central permission inheritance across several update actions;
- create policy over a proposed row;
- input patches with ordinary allowed fields;
- narrowed field access;
- stable audience-specific projections;
- parent/child policy composition;
- authenticated user and service principals;
- explicit `Access.public` and `Access.authenticated` paths;
- a reviewed operation exception; and
- a protected non-entity `invoke` operation.

Negative fixtures cover every diagnostic in section 14 plus naked role names,
credential-derived roles, request-derived roles, client-written generated
fields, owner/scope changes through ordinary patches, dynamic output stripping,
manual policy bypass, and public reachability of a principal-dependent path.

### 15.2 Runtime/database cases

Generated integration cases prove:

1. a Company owner can perform each granted effect;
2. an editor in the same Company can update but cannot delete;
3. an editor in another Company cannot read or update the row;
4. a viewer cannot update;
5. revoking or expiring membership removes access according to the declared
   freshness contract;
6. a required lookup conceals absent versus unauthorised rows;
7. a list excludes rows from other companies;
8. a caller cannot redirect policy with an input `company_id`;
9. a forbidden patch field rejects the entire update and changes nothing;
10. generated/lifecycle/role-binding fields cannot be client-written;
11. invalid database values fail during result validation;
12. a restricted output field cannot leak through a broader projection;
13. includes do not leak inaccessible children;
14. concurrent membership revocation and update follow the checked
    transaction/freshness plan;
15. a service principal has only its declared roles;
16. an explicit public health route cannot reach protected entity data; and
17. a public entity route requires both `auth: none` and `Access.public`, while
    either declaration alone is insufficient.

SQLite may exercise portable semantics. PostgreSQL evidence must additionally
verify generated SQL predicates, transactions/locks or conditional writes,
constraint normalization, query plans/index coverage, and any adapter-specific
defence-in-depth feature.

### 15.3 Approval adversarial cases

The approval protocol must catch policy-equivalent source changes whose graph
widens through a new field, projection, input, call edge, route, operation
effect, role binding, scope path, service, or derived representation. Replaying
an approval for a different graph or compiler/policy-schema version fails.

## 16. Implementation sequence

### POLICY-P0 — Freeze and migrate the contract

- digest-pin section 2;
- update the decision register, semantic model, decision sprint, roadmap,
  policy/proof kernel, validation rules, threat model, and golden-todo pressure
  contract;
- mark the separate candidate `policy.jadpo` syntax as superseded migration
  evidence; and
- freeze canonical positive/negative source fixtures before parser work.

Exit: documentation, fixtures, and issue status agree on one policy model.

### POLICY-P1 — Role and binding graph

- parse and resolve qualified role enums;
- implement direct role-binding field metadata;
- implement membership scope/member/role declarations;
- validate principal, role, scope, authority, lifecycle, and cardinality;
- infer unambiguous scope paths and diagnose ambiguity; and
- emit stable semantic nodes and source spans.

Exit: positive and negative binding/scope fixtures pass without database
lowering.

### POLICY-P2 — Permission and effect graph

- parse entity role-to-effect matrices, field narrowing, operation exceptions,
  and non-entity `invoke` policy;
- derive create/read/update/delete effects from named query/action graphs;
- compose nested/cross-entity obligations;
- reject missing, widened, public-principal, and escape paths; and
- expose IDE hover and derived role/effect matrices.

Exit: every reachable operation has a conservative policy judgement and exact
unresolved diagnostics.

### POLICY-P3 — Authoritative query and mutation lowering

- inject policy scope into reads, lists, includes, updates, deletes, and create
  preconditions;
- preserve lifecycle/business predicate provenance;
- keep policy/invariant reads in the checked transaction/concurrency plan;
- implement concealment and cardinality behavior;
- generate required indexes/advice from stable query shapes; and
- emit query-plan and policy-predicate audit metadata.

Exit: SQLite and PostgreSQL runtime matrices pass same-company, cross-company,
revocation, list, lookup, update, delete, and create cases.

### POLICY-P4 — Input, field, database-result, and output integration

- retain patch supplied flags through policy checks;
- reject whole mutations on forbidden supplied fields;
- enforce generated/lifecycle/identity/immutable/role-field ownership;
- track target-field provenance through inputs and locals;
- validate every database result before trust;
- prove projection/field readability; and
- validate and closed-serialize exact outputs without dynamic stripping.

Exit: every validation stage in section 8 has positive and adversarial evidence.

### POLICY-P5 — Routes, jobs, services, audit, and approval

- compose route authentication and fresh-authority requirements with policy;
- implement non-entity `invoke` and service-principal paths;
- generate complete role/scope/effect/field audit views;
- classify semantic weakenings beyond direct policy-source edits;
- generate canonical approval subjects and verify protected attestations; and
- integrate diagnostics and review surfaces with existing tooling contracts.

Exit: a release-equivalent build cannot accept an unapproved weakening and all
public/protected reachability cases fail closed.

### POLICY-P6 — Golden todo and multi-company pressure application

- migrate golden todo ownership CRUD, user disablement, reminder job, soft
  delete, field ownership, and public health to the accepted model;
- add the Company/CompanyMembership/Article pressure fixture from this plan;
- regenerate acceptance, adversarial, audit, and target-baseline artifacts;
- run fresh-agent implementation/repair evidence without teaching the hidden
  checks; and
- retain external P10R review and first-user sessions as separate gates.

Exit: both a direct-owner application and a scoped multi-company application
compile, run, and produce understandable policy/audit evidence without manual
tenant predicates or duplicated action permissions.

Implementation record: qualified role bindings, scoped memberships, effect
derivation, operation exceptions, narrowing field policy, relationship
composition, automatic query/mutation predicates, create preconditions,
concealment, field-write checks, exact output/projection proofs, derived-field
read checks, route/non-entity invoke policy, user/service principals, and
value-free policy audit output are implemented. The multi-company SQLite
pressure application proves same- and cross-company access, pagination before
disclosure, create/update/delete behavior, field narrowing, public invoke,
application roles, and immediate membership revocation. Protected approval
verification cannot exit without the external protected CI/review attestation
provider required by section 13; live PostgreSQL evidence needs a supplied
database; and the complete golden todo still depends on AUTH-P4+, SERVICE-001,
and ASYNC-001. None of those gates is silently counted as complete.

## 17. Explicitly rejected designs

The following are not alternative v0.1 spellings:

- entity `actors` or `guards` that merely rename role enum values;
- naked `owner`/`editor` strings instead of qualified variants;
- manual `require policy` calls inside actions;
- role lists repeated on every CRUD action;
- a generic entity-wide role that silently grants all future operations;
- policy predicates copied into every query/action;
- route-level role lists duplicating entity policy;
- permissions or roles embedded in credentials;
- treating `created_by` as ownership without an explicit role binding;
- treating `deleted_at`, expiry, or domain state as policy;
- field lists duplicated in every entity policy;
- dynamic field stripping or role-dependent output shapes;
- partial application of an otherwise forbidden patch;
- implicit role hierarchy or administrator bypass;
- explicit deny rules and precedence in the initial language;
- arbitrary policy functions or host-language callbacks;
- trusting client-supplied company/tenant IDs as membership proof;
- fetch-then-authorise mutation plans vulnerable to races;
- a separate hand-maintained database policy; and
- editable generated audit as policy authority.

## 18. Stop conditions

Implementation stops and returns to owner decision if:

- the parser cannot express the common Company membership case with one
  membership declaration and one entity permission matrix;
- safe scope inference requires field-name conventions or hidden heuristics;
- the compiler cannot preserve policy through a supported query/include plan;
- field-level input policy would require silent stripping or partial mutation;
- output compatibility would require role-dependent omission under one type;
- role revocation cannot satisfy the declared authoritative/freshness contract;
- an adapter requires a second independently authored permission source;
- policy cannot be kept in the same transaction/concurrency plan as a guarded
  write;
- a policy weakening can evade semantic-graph approval by changing only input,
  output, projection, route, call, or role-binding structure;
- diagnostics require users to understand compiler-internal proof terms to fix
  ordinary role/scope mistakes; or
- the multi-company pressure application still requires hand-authored tenant
  checks in routine queries/actions.

These are design failures, not invitations to add unchecked escape hatches.

## 19. Retention maintenance addendum — POLICY-D30-M1

**Owner direction:** 2026-10-02, compiler-owned narrow retention maintenance,
as recorded in the [batch decision](archive/work-plans/golden-delivery-planning.md).
**Status:** contract addendum frozen on 2026-10-02 after the
independent RM-205 correction review
accepted the candidate. Its reviewed semantics and decision-row digest are unchanged;
compiler/runtime conformance remains RM-206.
It is outside the historically approved D01–D43 baseline in section 2, whose
exact UTF-8 bytes from its heading up to (excluding) the section-3 heading still
hash to the accepted `sha256:66e7a8f586b62ed92c3a7220f524e25aee5808ca60b8504fb3c2d225ef2d68bd`.
The owner direction does not retroactively amend that approval.

| ID | Decision |
| --- | --- |
| POLICY-D30-M1 | Owner-selected retention maintenance is a separate compiler maintenance plane, with only a clause-bound physical purge of already-soft-deleted, retention-expired rows in deterministic batches of at most 500. Authored jobs/internal application callers remain under POLICY-D30; no source caller can obtain the maintenance capability or general deletion authority. The exact authority, guarded transaction and audit contract is in [DATA-007](lifecycle-plan.md#retention-maintenance-authority); independent contract review gates its freeze. |

**Addendum decision-row digest:** `sha256:1fd3b1ad5f6714e35e989cd7f8268d6cd7986662c0475478655bf1e249532cf9`.
This hashes the single UTF-8 POLICY-D30-M1 table row including its trailing LF;
it does not claim approval of the whole document. The linked DATA-007 contract
and assurance catalog are separately byte-pinned in the
independent review.
The final review disposition is `approved_for_contract_freeze`; it establishes
contract acceptance, not future lowering or deployment approval.
