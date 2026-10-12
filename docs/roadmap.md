# Jadpo roadmap

**Updated:** 2026-10-11. This page is the only list of outstanding work. Keep it
to one page; record decisions in the [decision register](decision-register.md)
and specifications, not here.

## Now — in order

1. **Clean-up (done 2026-10-11).** Removed per-case evidence packets, review
   records, timing logs and committed build output; archived the Codex planning
   documents; parked stranded branches as `archive/*` tags (see the
   [archive index](archive/README.md)).
2. **Language review.** Walk every language area with Richard, one at a time,
   in [docs/language](language/README.md). Each page shows what the compiler
   accepts today with compiled examples, every error the area can produce,
   consistency with the rest of the language, and open questions; Richard
   comments on the rendered page. Outcome per area: keep, change or drop.
   Area 15 (Personal data: GDPR support out of the box) was added on
   2026-10-12. Area 1 now covers project layout as well as modules, absorbing the old
   LAYOUT-001 work. Replaces the old RM-222, RM-224 and RM-225 and absorbs the
   open E02 language/tooling gaps.
3. **Rework the areas the review flags.** Keep `python3 tools/verify.py` and
   the passing golden cases green throughout. Rebuild only what the review says
   is wrong.
   Area 1 (decided 2026-10-12) needs: the domain layout and its layout
   checks; domain ownership and `public` (including public actions sharing
   the caller's transaction) replacing `module`/`import`; owner-qualified
   calls and named libraries; route groups with checked prefixes; the
   `database/` folder, line-per-entry registry and timestamped migrations;
   single-file checking; reserved-word policy; `///` comments; formatter
   blank lines and section order; the library catalogue, `jadpo impact`,
   similar-helper warning and domain commands; then the scaffold, compile
   tests, examples and golden app moved to the new layout.
   Area 2 (decided 2026-10-12) needs: the `type Name = Base { ... }` form
   with `Object`, `Enum`, `Input` and `Output` bases and removal of the old
   keywords and anonymous objects; derived types; exact 64-bit `Int` and
   scaled `Decimal` with JSON and storage mapping; UUID versions; literal
   typing and mandatory variable types; field settings blocks and named
   options; bracket lists; fallible runtime construction; object
   construction running field rules; input-only `optional`; list literals,
   `for` and first collection functions; `Set`/`Map` JSON; full-match linear
   patterns and constraint validation; signature-rule scope; structural `==`.
   Area 3 (decided 2026-10-12) needs: bracketed conditions and `else if`;
   unused-variable, shadowing and missing-return errors; standalone
   statements; operator type rules; `/` semantics with non-zero divisor types;
   one `match` with `->`, bracket data binding and no closed-type wildcard;
   `attempt` placement; `for` loops and inline collection functions;
   recursion limits; removal of callable settings; the colon and block rule
   across the grammar.
   Area 4 (decided 2026-10-12) needs: `client`/`internal` blocks;
   `{ field, rule }` input details and complete OpenAPI error responses; the
   single mapping form with kept causes; overflow and recursion-limit
   defects; concealment as `not_found`; failure logging with hashed text and
   request ids; enum kinds; codes defaulting to failure names with
   uniqueness checks; `failures/` folders and `type X = Failure` declarations.
4. **Finish the golden todo** on the reviewed language. Done means
   `python3 tools/verify.py --require-golden` passes 88/88 (44 cases × SQLite
   and PostgreSQL) on one commit, plus one independent review of the combined
   change. No per-case evidence packets.

## Golden todo — where it stopped

The successor app is `examples/golden-todo-migration` (compiles cleanly); the
frozen contract is `examples/golden-todo/acceptance.json`. The formal runner
records 35 SQLite passes and 53 not run. The same 35 cases pass on PostgreSQL in
the supporting checkpoint, but the runner will not count them (see below).

Still missing: JOB-001–004, CONFIG-001–003, BOUNDARY-001 and REL-003.

## Known special cases and gaps (input to the review)

- **Services:** the compiler accepts exactly one service declaration — the
  golden reminder mail service — matched line by line
  (`checked_service_effect` in `jadpo/crates/semantic/src/lib.rs`).
- **Jobs:** only jobs with a checked `delivery:` block lower; any other job
  fails with `JADPO_TARGET_JOB_NOT_IMPLEMENTED`. The golden app's reminder and
  purge jobs live only in test fixtures, not in its source.
- **Readiness:** `/health/ready` is served on the application router; there is
  no deployment-plane listener, which CONFIG-003 requires.
- **Configuration URLs:** `Url` accepts only http(s), so `DatabaseUrl = Url`
  rejects `postgres://`; the generated app reads the raw `DATABASE_URL`
  instead. This is why PostgreSQL is not counted.
- **Approval:** nothing emits the `requires_human_decision` disposition REL-003
  expects.
- **Stranded code:** about 20k lines of unmerged compiler/runtime work (error
  metadata, Instant storage, startup ownership, protected approval attempts)
  sits on `archive/golden-loop-next` and four related tags.

## Later — parked

- Cloudflare D1 as a store engine, which first needs a Workers runtime.
- Acting as an OAuth provider for other applications (a requirement;
  designed in language area 8, built after the golden todo).
- PostgreSQL row-level security as an opt-in store setting, generated from
  policy (decided in language area 6, 2026-10-12).

Scope for each is in the
[archived roadmap](archive/implementation-roadmap-2026-10-11.md); revisit after
the golden todo.

- Durable-job operator surfaces and enforced component messaging (E03).
- Transaction retry and deployment readiness qualification (E04).
- Engineering validation: mutation, hostile and cross-feature cases (E05).
- Enforceable policy approval and the review UI (E06).
- Independent assurance and first-user studies (E07).
- Order/payment application compared against TypeScript (E08).
- Alternative runtimes: WASM and native Rust (E09).
- Public documentation site (E10), developer console and MCP (E11), frontend
  contracts (E12).

## How we work

- Branch from `main` for each change; commit and push often. Never put a
  worktree under `/private/tmp`.
- `tools/verify.py` output is the evidence. Do not add evidence packets,
  per-step reviews, timing logs or model-routing records.
- One independent review per milestone, not per test case.
- Update this page when an item finishes or the order changes, and nothing else.
