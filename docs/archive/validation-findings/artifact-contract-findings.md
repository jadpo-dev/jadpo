# Independent artifact/source/runtime agreement

## Expected contracts (before implementation inspection for repair)

Authority: `docs/type-system.md` §§2.2–2.5,3.6,4.1,6.1–6.2;
`docs/runtime-target-v0.1.md` “At the HTTP boundary”; and
`docs/compiler-runtime.md` §§4,10. Generated schemas must preserve the accepted
value constraints and exact public shapes; one authored rule must not diverge
between boundary validation and OpenAPI.

- Field-local constraints intersect with parent and ancestor constraints,
  including references through fields and named refinements. A field cannot
  relax constraints inherited from its parent.
- Nullable values remain distinct from omitted input properties; a nullable
  wrapper must not erase constraints on present values. Containers preserve
  element contracts and their own non-nullable shape.
- Tagged enum payload fields obey their complete typed contracts. Entity
  reference wire values obey the entity identity contract, not the full entity
  object shape.
- Failure responses expose only typed public fields, with the authored code;
  internal context must be absent from both response and public failure schema.
- Rebuilding unchanged source produces identical contract artifacts; a second
  build must not depend on residual generated files or random metadata.

Tests compare concrete accepted/rejected examples to both the emitted schema
and actual generated handler behaviour. The bounded schema evaluator used by
these tests is not a general OpenAPI certification tool and must reject unknown
assertion keywords rather than silently treating them as valid.

## Results

Added `tests/runtime/validation-artifact-contracts.test.ts`: **11 passing tests**
with 646 assertions, comparing independently stated boundary cases against the
actual generated handler and the generated request/response schemas. Existing
four artifact-nullability tests pass. Existing nine Rust artifact unit tests
pass, retaining the check that compiler-synthesized inline names stay private.

```sh
bun --no-install --no-env-file test tests/runtime/validation-artifact-contracts.test.ts
bun --no-install --no-env-file test tests/runtime/validation-artifact-nullability.test.ts
cargo test --manifest-path jadpo/Cargo.toml -p jadpo-core --lib artifacts::tests
```

Initial independent run: only deterministic rebuilding passed; local/refined,
nullable, collection and tagged-payload field schemas accepted values rejected
by the generated runtime. Entity references lost their UUID wire format. The
failure-envelope assertion was aligned with the accepted dedicated `details`
shape in `docs/failure-model.md` §6.2, exposing that OpenAPI itself incorrectly
placed untyped fields directly in the envelope. An additional exact-failure-set
case reproduced one declared failure disappearing when two failures shared HTTP
422. Every reproduced discrepancy remains an enabled regression.

## Repairs

- Authored field contracts now have resolvable component schemas. Property
  schemas apply local constraints, and field references retain the whole parent
  constraint chain through `$ref` and `allOf`; recursive named structures do
  not require unbounded expansion.
- Named refinements, direct and inherited nullability, collection elements,
  numeric bounds and enum payload constraints remain intact. Closed inline
  objects stay inline without exposing `__jadpo_` implementation names.
- Entity references use the declared identity field's wire schema, including
  UUID validation, rather than becoming unconstrained strings.
- Failure schemas mirror the canonical envelope and its typed, closed
  `error.details`. Unrelated internal failure fields are omitted from public
  schema components; public references to an internal field's value contract
  remain resolvable without adding that internal payload to any response.
- Failures sharing an HTTP status are represented by one response with `oneOf`
  alternatives. The prior duplicate JSON response keys silently discarded all
  but one schema.
- Rebuilding unchanged source preserves seven contract artifacts byte for byte,
  including normalized semantic metadata. This proves same-source/same-project
  repeatability, not every toolchain or platform combination.

The independent evaluator recognizes only the assertion keywords used by these
cases and throws for unsupported keywords. It is explicitly not a full schema
library. Cases use ASCII text and supported UUID-v4 examples; Unicode length,
format vocabularies, arbitrary regex semantics, Map/Set wire encodings and all
possible reference cycles require their own evidence. No feature or golden
obligation is marked complete by this bounded schema/runtime agreement suite.


## Explicit Set/Map wire-contract gap

A separate reproducible probe shows that current OpenAPI advertises a JSON
array for `Set<Key>` and a JSON object for `Map<Key, Key>`, while both advertised
shapes independently receive HTTP 400 from generated handlers. The target
expects host-language `Set`/`Map` instances. The accepted prelude and grammar
state that these containers exist but do not settle their HTTP wire encodings,
map-key encoding, duplicate handling or null rules. Those semantics were not
invented by this review, and parity for these boundaries remains an open gate.

Source, build/run commands, responses and schema fragments are retained in
`build/validation/set-map-wire-contract/`. This gap is separate from the eleven
passing artifact-conformance cases; it is not counted as a passing collection
boundary test.
