# Artifact nullability validation

Contract: type-system §3.6 states that field references inherit nullability;
compiler-runtime §6 requires generated contracts to follow the same declared
rules. Optional shape remains independent of whether a present field permits
`none`.

`tests/runtime/validation-artifact-nullability.test.ts` builds temporary source
with the current compiler and parses the emitted JSON artifacts. Initially one
case passed and three failed: chained nullability was missing from OpenAPI,
nullable list elements were described as non-nullable, and validator plans
reported inherited nullable fields as non-nullable.

The artifact emitter now consults resolved semantic nullability. Named scalar
schemas preserve their parent schema and compose local constraints, so a named
refinement of a nullable field retains absence and its parent representation.
The suite also checks required-field shape, named nullable components and the
distinction between nullable collection elements and a nullable collection.

All four artifact tests and the existing 57 core library tests pass after the
fix. This is artifact evidence, not database or HTTP runtime evidence. General
field-local constraint projection, schema-validator conformance, public failure
field schemas and the complete OpenAPI surface remain work for wave three.
