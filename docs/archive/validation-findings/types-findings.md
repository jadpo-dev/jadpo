# Independent types/value validation

## Expected contract (recorded before implementation inspection)

Authority: `docs/type-system.md` §§2.2–2.5, 3.1–3.7, 4.3, 6.1–6.3;
`docs/decision-register.md` “Bindings and data” and “Types and boundaries”;
`docs/grammar-v0.1.md` §§5–7; `docs/entity-query-model.md` §5.

- Named scalar and record declarations are nominal even when representations or
  fields match. Primitive-to-semantic and semantic-to-primitive substitution fail.
- Declared field values may widen along explicit declared parent chains; sibling
  fields and implicit base-to-field narrowing fail. Nested selection uses the
  declared nested record's field identity, not a path-dependent identity.
- Literal constructors validate inclusive intrinsic bounds and inherited
  constraints. Explicit field construction supplies the required narrowing.
- `T?` admits `none` and present compatible values; absent values cannot be
  consumed as non-nullable values. Redundant nullable field references fail.
- Shape omission does not propagate through a field type reference. Concrete
  record construction is complete and closed.
- Entity references preserve nominal identity and never implicitly hydrate an
  entity. Storage uniqueness/existence are not proved by value validation.

Collection variance and still-provisional syntax are not asserted as settled
contracts. Runtime decoding, persisted integrity, and existence/authorization
need separate runtime/database evidence; compiler checks cannot establish them.

## Results

Added `jadpo/crates/core/tests/validation_types_contract.rs`: 36 independent
integration tests exercising `analyze_sources`, including boundary tables.
The integration repair adds five more inherited-nullability cases: **41 total**.
Cargo's workspace test command discovers this file without manifest changes.

Focused command:

```sh
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --test validation_types_contract
```

Initial final authoring run: **31 passed, 5 failed, 0 ignored**. Five failures
exposed two contract violations; assertions remained enabled, not relabelled or
silently skipped. After the parent fixed dotted-constructor receiver probing and
this package repaired inherited nullability, the focused suite passed **41/41**.
The semantic unit suite passed **34/34**, and the diagnostic suite **26/26**.
`git diff --check` passed for this package. The diagnostic helper requires one local diagnostic,
expected code, exact offending source slice, and nonempty explanation/repair.
The sibling diagnostic also checks expected/received semantic metadata and that
its repair does not silently apply a cast.

| Contract | New evidence |
|---|---|
| Type-system §§2.2,2.4,3 | Equal scalar constraints and record shapes remain nominal; named `=` does not create a transparent alias; primitive substitution rejected in both directions |
| §§2.3,2.4.1,3.1–3.4 | Transitive field widening, exact field parents, sibling rejection, no implicit narrowing, nested selection using declared record field identity |
| §§2.5,3.6 | List/Set/Map preserve element/key identity; nullable widening preserves sibling distinctions |
| §§2.4,3.6,6.2 | Omission differs from `none`; nullable fields still required; optionality does not propagate through field references; exhaustive nullable handling |
| §§4.3,6.1 | Inclusive integer/text boundaries, singleton integer domain, wrong representation, field and parent constraints, inherited constraints through another record |
| §2.2 | Closed enum cannot be constructed from its wire string; qualified variant is accepted |
| Entity/query §5 | Snapshot-to-reference projection; no implicit reference hydration; cross-entity references rejected |
| §8 | Sibling diagnostic names both contracts, supplies context and keeps semantic choice explicit |

### Finding 1: dotted constructors spuriously resolve a value receiver

Accepted contract: type-system §4.3 explicitly accepts `Customer.email(value)`;
construction validates a field rather than asking for a local `Customer` value.

Minimal reproduction:

```jadpo
type Label = Text { min_length: 2 max_length: 8 }
type Customer = Object { label: Label }
function make_label() -> Customer.label {
    return Customer.label("valid")
}
```

Expected: accepted. Actual: `TYPE_UNKNOWN_VALUE` on `Customer`.

After reproducing from the contract, implementation inspection located
`infer_invocation` in `jadpo/crates/semantic/src/typecheck.rs`. It probes a
receiver for dotted callees absent from the callable catalogue before checking
whether the authored callee names a graph type. The later constructor check
succeeds, but the speculative receiver diagnostic remains. Invalid field
literals receive both the legitimate `TYPE_INVALID_LITERAL` and the unrelated
`TYPE_UNKNOWN_VALUE`. This affects reserved and ordinary field names alike.
Four tests catch the positive, inherited, and negative/cascade forms.

### Finding 2: redundant nullable field references silently compile

Accepted contract: type-system §3.6 and grammar §5 explicitly require a
redundant-nullability diagnostic rather than nested absence.

```jadpo
type Contact = Object { email: Email? }
type Request = Object { email: Contact.email? }
```

Expected: semantic rejection of the redundant nullable suffix. Actual before repair: no
diagnostic. The repair adds `TYPE_REDUNDANT_NULLABILITY`, naming the referenced
contract and explaining that removing the extra suffix preserves absence. Tests
pin its code, full reference span, summary, repair and repaired program behaviour.

The semantic graph now computes inherited nullability to a finite fixed point,
independent of declaration order; the type checker uses that information for
record and enum fields, configuration, callable signatures, local annotations,
route types and nested collection arguments. Tests additionally establish that
nullable references cannot be passed as present values or selected through
without handling `none`, while exhaustive `some` handling still succeeds.

## Proof scope and remaining gaps

These tests establish compiler acceptance/rejection and local diagnostics;
they do not prove emitted JavaScript behaviour, runtime boundary validation,
persistence constraints, existence, authorization, HTTP serialization or
backend parity. Parent review additionally reproduced a separate emitted-target
failure for dotted field constructors (`Customer is not defined`) and is handling
that runtime defect independently; compiler acceptance alone did not close it.
Runtime schema/SQL/relationship consumers that inspect only an authored `?`
still need auditing against inherited nullability; this package verifies the
semantic graph and type checker, not all artifact consumers.
No source/compiler mutation campaign was run in this bounded
package. In a separate review, plausible mutations to challenge include erasing
field parents, permitting sibling substitution, changing inclusive comparisons,
ignoring ancestor constraints, propagating optional shape flags through field
references, or inserting implicit entity loads.

Collection covariance is still provisional, so tests require preservation of
nominal arguments but do not settle variance. Unicode text-length units,
normalization and Unicode pattern policy need a precise cross-target contract
before boundary tests can claim conformance. Generic user declarations and
stable identity through renames require their own design/migration evidence.
Dynamic constructor failures and tagged-payload control flow are separate
failure/effect and runtime work packages, not silently covered by these tests.

