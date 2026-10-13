# Jadpo formatter rule matrix

**Status:** TOOL-005 in progress  
**Scope:** canonical whitespace and layout for the accepted syntax in
[`grammar-v0.1.md`](grammar-v0.1.md)

Formatting preserves each non-whitespace token and its multiplicity. Route
items may move into their canonical order because the accepted route grammar
defines their textual order as non-semantic. String-literal bytes and comment
text are preserved; comment order within each attached group is preserved,
with route-item groups following the canonical item order.
The compiler parser remains the syntax authority; editor clients do not define
separate layout rules.

## Canonical output rules

The owner selected syntax-derived canonical layout on 2026-10-01; see the
[decision](decision-register.md#keyword-names-and-canonical-formatting--2026-10-01).
FMT-005 below is the accepted target, not a claim of complete implementation.
The superseded rule preserved existing nonblank source line breaks and collapsed
existing blank-line runs to one. That describes the current implementation and
historical snapshots; RM-203 must replace it with reviewed AST-guided layout.

| Rule | Canonical form |
|---|---|
| FMT-001 Indentation | Four spaces per open `{`, `(`, or `[` nesting level. A leading closing delimiter reduces indentation before that line. Tabs are not emitted. Callable `fails` and `->` continuation lines use one additional level. |
| FMT-002 Horizontal separators | One space between adjacent tokens unless a punctuation rule below joins or separates them. Spacing is normalized outside string literals and comments. |
| FMT-003 Punctuation | No space before `:`, `,`, `.`, `?`, `)`, `]`, or `}`; one space after `:` and `,`; no space after `.`, `(`, or `[`. Calls use `name(...)`. A non-empty inline brace body has one space inside each brace. |
| FMT-004 Operators and type arguments | Binary, comparison, logical, assignment, and arrow operators have one space on both sides. Unary minus attaches to its operand. Generic type arguments have no space inside `<...>`. Duration units attach to their number (`5m`). |
| FMT-005 Canonical layout | Owner-selected target, 2026-10-01: derive line and blank-line placement from syntax; equivalent parsed code has identical output regardless of authored non-comment line breaks. Emit LF, no leading/trailing blank lines and one final LF for non-empty source. Preserve syntax-tree attachment, tokens, comment text/attachment and literal bytes. A variation moving a line-sensitive `optional` modifier to a different field is not semantically equivalent. AST-guided reconstruction remains RM-203 implementation work. |
| FMT-006 Comments and strings | Leading indentation is normalized. String-literal bytes and line-comment text, including comment contents, are preserved. Comment order within a route-item comment group is preserved. A leading full-line route comment follows the next route item; a trailing same-line comment stays with the preceding item. Code before a trailing line comment is separated from it by one space. Delimiters inside strings/comments do not affect indentation. |
| FMT-007 Route items | Emit each item on its own line in the canonical order `auth`, `path`, `query`, `headers`, `input`, `output`, `success`, then one behaviour item (`run` or `action`). Leading full-line comments travel with the following item; trailing same-line comments stay with the preceding item. |
| FMT-008 Lexically invalid editor text | Preserve unrecognized source characters and the contents of a lexically invalid line. Continue normalizing surrounding indentation and line endings without dropping text that has no lexer token. |
| FMT-009 Context-sensitive route paths | Keep route path spelling compact, including `/` separators and `{placeholder}` segments. A physical line break between the route method and path is valid whitespace and must not cause the formatter to insert spaces inside the path token. |

## Grammar coverage matrix

The table maps every named production and alternative in the
implemented `ebnf`/module grammar in `grammar-v0.1.md` to canonical rules and
executable source evidence. Explicitly unimplemented `ebnf-candidate` proposals
are not coverage claims and need their own fixtures when adopted.
The formatter unit matrix exercises each surface family, and the unit suite
compares exact output for every `.jadpo` file under `tests/compile/pass` against
a checked-in snapshot. Both paths perturb horizontal whitespace, tabs and
indentation, line endings, and excess blank lines. Route item order is checked
against its canonical semantic order.

| Grammar productions and alternatives | Formatting rules | Representative exact-output cases |
|---|---|---|
| `letter`, `digit`, `identifier`, `integer_literal`, `decimal_literal`, `string_literal`, `boolean_literal`, `none_literal`, escapes, `line_comment`, `whitespace` | FMT-002, FMT-006, FMT-008 | [`01_valid_email_literal`](../tests/formatter/golden/01_valid_email_literal.jadpo.formatted), [`57_operators_and_precedence`](../tests/formatter/golden/57_operators_and_precedence.jadpo.formatted), all compile-fail fixtures; exact comment/string cases in the formatter unit tests |
| `source_file`, `declaration`, `module_declaration`/`module-declaration`, `import_declaration`/`import-declaration`, `public_declaration`/`public-declaration`; ambient and modular files; all declaration alternatives including `locales` | FMT-001, FMT-002, FMT-005 | [`customer.jadpo` from the project-role module](../tests/formatter/golden/customer.jadpo.formatted), [`108_unified_types_and_persistence`](../tests/formatter/golden/108_unified_types_and_persistence.jadpo.formatted); module/import, locale, and declaration-family formatter unit cases |
| `application_declaration`, `application_authentication`, `revocation_declaration`, `revocation_mode`, `duration_literal`, `principal_declaration`, `principal_variant` (`user`/`service`) | FMT-001–FMT-006 | [`127_application_principal_contract`](../tests/formatter/golden/127_application_principal_contract.jadpo.formatted), [`138_route_principal_variant`](../tests/formatter/golden/138_route_principal_variant.jadpo.formatted); bounded/immediate and user/service forms in fixtures and the formatter unit matrix |
| `authentication_strategy`, `authentication_item`, `transport` (cookie/bearer), `validators`, `validator`, `validator_setting`, `claims`, `authentication_mapping`, `resolution` (user/service) | FMT-001–FMT-006 | [`128_authentication_strategy_topology`](../tests/formatter/golden/128_authentication_strategy_topology.jadpo.formatted); signed, opaque, API-key, JWT and resolution forms in the pass corpus and formatter unit matrix |
| `credential_binding` | FMT-001–FMT-006 | [`explicit credentials`](../tests/formatter/cases/uncovered-grammar-alternatives.jadpo.formatted); renamed identity, principal, verifier, active, expiry and revocation roles, whitespace perturbation and idempotence |
| `entity_declaration`, `entity_field`, `entity_field_options` (including generated identity), `entity_dossier_item`, `persistence_capability`, `representation_declaration`, `entity_operation`, `query_declaration`, `freshness` | FMT-001–FMT-006 | [`120_entity_dossier_queries`](../tests/formatter/golden/120_entity_dossier_queries.jadpo.formatted), [`132_scoped_role_policy`](../tests/formatter/golden/132_scoped_role_policy.jadpo.formatted), [`133_policy_effect_inheritance`](../tests/formatter/golden/133_policy_effect_inheritance.jadpo.formatted), [`135_policy_relationship_composition`](../tests/formatter/golden/135_policy_relationship_composition.jadpo.formatted), [`137_generated_identity`](../tests/formatter/golden/137_generated_identity.jadpo.formatted) |
| Previously unrepresented alternatives: `reference_delete_action: restrict`, `inverse_cardinality: many` in both `entity` and compatibility `persist` forms, `representation_declaration: projection` without `strategy`, compatibility `type Name = Enum { ... }`, `freshness: read_your_writes`/`bounded_staleness`/`eventual`, `order_direction: desc` with many-query ordering and pagination, `if_statement: else`, negative numeric literals, escaped string content, `pattern` and `format` constraints, nullable input `default none`, integer/decimal/boolean `config_default`, and optional authentication `previous_secret`/`origin` validator settings | FMT-001–FMT-006 | Exact source and snapshot in [`uncovered-grammar-alternatives.jadpo`](../tests/formatter/cases/uncovered-grammar-alternatives.jadpo) and [`its expected output`](../tests/formatter/cases/uncovered-grammar-alternatives.jadpo.formatted); exercised by `formats_grammar_alternatives_missing_from_compile_pass_fixtures` |
| `test_declaration`, `fixture_declaration`, `fixture_setting`, `fixture_config_value`, `assert_statement`, `advance_statement` | FMT-001–FMT-006 | [`58_authored_tests`](../tests/formatter/golden/58_authored_tests.jadpo.formatted), [`130_test_fixture_clock`](../tests/formatter/golden/130_test_fixture_clock.jadpo.formatted), [`131_test_fixture_configuration`](../tests/formatter/golden/131_test_fixture_configuration.jadpo.formatted) |
| `type_expression`, `primary_type`, `named_type`, `field_type`, `generic_type`, `inline_object_type`; nullable suffix; `type_declaration` (including compatibility `type Name = Enum { ... }`), `scalar_type_body`, `object_type_body`, `enum_type_body`, `enum_variant`, `constraint_block`, each `constraint`, `number_literal` | FMT-001–FMT-004 | [`11_optional_and_nullable`](../tests/formatter/golden/11_optional_and_nullable.jadpo.formatted), [`22_nested_structured_field_selection`](../tests/formatter/golden/22_nested_structured_field_selection.jadpo.formatted), [`53_plain_enum_exhaustive_match`](../tests/formatter/golden/53_plain_enum_exhaustive_match.jadpo.formatted), [`56_tagged_sum_match`](../tests/formatter/golden/56_tagged_sum_match.jadpo.formatted), [`57_operators_and_precedence`](../tests/formatter/golden/57_operators_and_precedence.jadpo.formatted); the compatibility enum form has an exact snapshot in the focused case below |
| `object_body`, `field_declaration` (required/optional/nullable/constrained); `persistence_declaration`, `persistence_item`, `reference_item`, `reference_delete_action` (`restrict`/`cascade`/`set_null`), `inverse_declaration`, `inverse_cardinality` (`many`/`optional`) | FMT-001–FMT-006 | [`108_unified_types_and_persistence`](../tests/formatter/golden/108_unified_types_and_persistence.jadpo.formatted), [`120_entity_dossier_queries`](../tests/formatter/golden/120_entity_dossier_queries.jadpo.formatted), [`50_named_owning_reference`](../tests/formatter/golden/50_named_owning_reference.jadpo.formatted); legacy `persist` syntax in the formatter unit matrix |
| `config_declaration`, `config_field`, `config_binding`, `config_secret`, `config_default` (string/integer/decimal/boolean/duration); `failure_declaration`, `public_schema`, `internal_schema` | FMT-001–FMT-006 | [`125_typed_configuration`](../tests/formatter/golden/125_typed_configuration.jadpo.formatted), [`09_internal_context_not_public`](../tests/formatter/golden/09_internal_context_not_public.jadpo.formatted), [`61_distinct_failure_context`](../tests/formatter/golden/61_distinct_failure_context.jadpo.formatted); configuration and failure family unit cases |
| `function_declaration`, `action_declaration`, `parameter_list`, `parameter`, `fails_clause`, `block`, `statement`, `binding_statement`, `assignment_statement`, `return_statement`, `reject_statement`, `rejection_body`, `if_statement` | FMT-001–FMT-006 | [`51_mutable_local_reassignment`](../tests/formatter/golden/51_mutable_local_reassignment.jadpo.formatted), [`110_callable_execution_matrix`](../tests/formatter/golden/110_callable_execution_matrix.jadpo.formatted), [`111_transitive_internal_suspension`](../tests/formatter/golden/111_transitive_internal_suspension.jadpo.formatted), [`121_atomic_entity_actions`](../tests/formatter/golden/121_atomic_entity_actions.jadpo.formatted); callable/statement unit cases |
| `expression`, `equality_expression`, `prefix_expression`, `primary_expression`, `literal`, `named_expression`, `qualified_name`, `invocation_suffix`, `construction_suffix`, `argument`, `field_initialiser`; grouping, unary and binary precedence; `outcome_match_expression`, `outcome_match_arm`, `outcome_success_arm`, `outcome_failure_arm`, `outcome_arm_body` (`expression`/`reject`/`propagate`) | FMT-002–FMT-006 | [`22_nested_structured_field_selection`](../tests/formatter/golden/22_nested_structured_field_selection.jadpo.formatted), [`54_scalar_match`](../tests/formatter/golden/54_scalar_match.jadpo.formatted), [`56_tagged_sum_match`](../tests/formatter/golden/56_tagged_sum_match.jadpo.formatted), [`57_operators_and_precedence`](../tests/formatter/golden/57_operators_and_precedence.jadpo.formatted), [`113_outcome_match_recovery_mapping_propagation`](../tests/formatter/golden/113_outcome_match_recovery_mapping_propagation.jadpo.formatted) |
| `create_expression`, `query_expression`, `include_clause`, `inverse_include_clause`, `parent_include_clause`, `relationship_path`, `parent_include_cardinality`, `pagination_clause`, `query_cardinality`, `order_direction`, `failure_binding` | FMT-001–FMT-006 | [`24_create_entity`](../tests/formatter/golden/24_create_entity.jadpo.formatted), [`26_query_optional_entity`](../tests/formatter/golden/26_query_optional_entity.jadpo.formatted), [`29_query_required_entity`](../tests/formatter/golden/29_query_required_entity.jadpo.formatted), [`120_entity_dossier_queries`](../tests/formatter/golden/120_entity_dossier_queries.jadpo.formatted), [`46_optional_parent_include`](../tests/formatter/golden/46_optional_parent_include.jadpo.formatted), [`48_optional_inverse_include`](../tests/formatter/golden/48_optional_inverse_include.jadpo.formatted), [`49_bounded_nested_include`](../tests/formatter/golden/49_bounded_nested_include.jadpo.formatted) |
| `update_expression`, `patch_set_body`, `delete_expression`; `set`/`patch`, conditional derived writes, empty/missing/conflict bindings | FMT-001–FMT-006 | [`32_update_required_entity`](../tests/formatter/golden/32_update_required_entity.jadpo.formatted), [`34_update_multiple_fields`](../tests/formatter/golden/34_update_multiple_fields.jadpo.formatted), [`40_omission_aware_patch`](../tests/formatter/golden/40_omission_aware_patch.jadpo.formatted), [`44_patch_derived_change`](../tests/formatter/golden/44_patch_derived_change.jadpo.formatted), [`33_delete_required_entity`](../tests/formatter/golden/33_delete_required_entity.jadpo.formatted), [`38_compound_constraint_mapping`](../tests/formatter/golden/38_compound_constraint_mapping.jadpo.formatted) |
| `route_declaration`, `http_method` (GET/POST/PUT/PATCH/DELETE), `route_path`, `route_item`, `route_path_schema`, `route_header_schema`, `inline_action`; default/auth-none, typed query and explicitly mapped headers, both success modes, named run/action and non-semantic item order | FMT-001–FMT-009 | [`59_p106_failure_route`](../tests/formatter/golden/59_p106_failure_route.jadpo.formatted), [`60_p106_inline_action`](../tests/formatter/golden/60_p106_inline_action.jadpo.formatted), [`64_multi_path_inline_route`](../tests/formatter/golden/64_multi_path_inline_route.jadpo.formatted), [`65_authenticated_route_default`](../tests/formatter/golden/65_authenticated_route_default.jadpo.formatted), [`67_colon_separated_route_blocks`](../tests/formatter/golden/67_colon_separated_route_blocks.jadpo.formatted), [`68_route_success_modes`](../tests/formatter/golden/68_route_success_modes.jadpo.formatted), [`138_route_principal_variant`](../tests/formatter/golden/138_route_principal_variant.jadpo.formatted), [`165_route_query_headers`](../tests/formatter/golden/165_route_query_headers.jadpo.formatted); all five methods in the formatter unit matrix |
| `job_declaration`, `job_item`; fixed interval, singleton, named clock-snapshot run and next-schedule wake-up | FMT-001–FMT-006 | [`173_scheduled_job`](../tests/formatter/golden/173_scheduled_job.jadpo.formatted); frontend-only, target execution remains fail-closed |

The representative files are not an allowlist: the integration test walks the
whole compile-pass corpus and automatically includes new syntax fixtures.
When the grammar adds a production or alternative, add its canonical rule and
executable coverage here and in `jadpo/crates/core/src/formatter.rs`. If a
syntax alternative has no positive compile fixture, add a focused case before
claiming formatter coverage.

## Executable checks

- `jadpo/crates/core/src/formatter.rs` contains eleven unit tests, including
  exact-output cases for indentation, blank lines, comments, signatures,
  punctuation, operators, literal preservation, and the surface-family matrix,
  plus regressions for line-break removal, route-path token preservation, and
  canonical route ordering with attached comments. A focused exact-output case
  covers the alternatives listed in the matrix row above. A corpus-driven golden test
  checks exact formatted text for all 60 compile-pass fixtures. For each
  fixture it also tests extra legal horizontal spacing, tabbing, incorrect
  indentation, CRLF, and excess blank lines; it reparses every variation and
  checks token/comment/string preservation and exact canonical output.
- `jadpo/crates/core/tests/formatter_grammar_conformance.rs` runs every
  compile-pass source through the formatter, verifies token preservation and
  parseability, checks idempotence, and perturbs tabs, spaces, indentation,
  excess blank lines, removed non-comment line breaks, and LF/CRLF whitespace.
  It also runs all compile-fail sources and checks token preservation, parser
  diagnostic-code stability, and idempotence.
- Each grammar production and alternative in the matrix links to its exact
  fixture snapshot or focused formatter case. The compile corpus automatically
  adds exact snapshots for new positive fixtures. The
  `formatter_rule_matrix_covers_each_named_grammar_production` test extracts the
  grammar's named productions and checks that every one appears in a matrix row
  with a valid exact-output snapshot link. Cases that compare output after
  removing nonblank line breaks now follow the accepted FMT-005 direction;
  their exact-output implementation remains RM-203.
- `jadpo/crates/core/tests/validation_tooling_contract.rs` checks literal and
  comment preservation across line endings.

The production-to-rule map is complete. The formatter currently preserves
nonblank source line breaks. The roadmap's byte-for-byte acceptance contract
also requires stable output when those breaks are removed; AST-guided line
reconstruction and exact-output assertions remain unimplemented. This is
tracked as [`TOOL-005`](language-issues.md#active-issues).
