# `publish.changelog_missing`

The new version's section of a package's `CHANGELOG.md` doesn't say why it is released.

## Why Jadpo refuses it

`jadpo package check` writes what changed into the new version's section
of `CHANGELOG.md`, from the package's surface: `- first release`, or a
line for each type, failure or function added, changed or removed. Why
the version is released only its author knows, so that is left to you,
beneath the list. Reviewers can't judge a release whose purpose they
can't tell, so the bar fails until its section says why.

## What it looks like

A package started with `jadpo package new`, whose changelog has lost the
sentence beneath its list:

```sh run project=e-changelog-missing expect=publish.changelog_missing
jadpo package new Postcodes --library > /dev/null
cd postcodes
cat > CHANGELOG.md <<'DOC'
# Changelog

## 1.0.0

- first release
DOC
jadpo package check
```

```text output
error: CHANGELOG.md doesn't say why 1.0.0 is released
 --> CHANGELOG.md:3:1
  |
3 | ## 1.0.0
  | ^^^^^^^^ no why beneath the list
  reason: `jadpo package check` writes what changed from the surface; whoever releases it writes why, beneath it. A release reviewers can't tell the purpose of is one they can't judge.
  next: Write why 1.0.0 is released under `## 1.0.0` in CHANGELOG.md, after the list (agent)
  rule: publish.changelog_missing
  help: https://jadpo.dev/docs/errors/publish.changelog_missing

Facts
  kind: library, pure
  storable: EnteredText, TidyText
  failures: starter.tidied: NothingEntered

Publishing bar
  compiles      passed                        no errors
  described     passed                        every public declaration
  examples      passed                        1 example, every failure shown
  tests         passed                        1 test
  recipes       passed                        1 recipe compiles in sample/
  version       passed                        1.0.0, the first release
  changelog     failed                        1.0.0 doesn't say why
  publisher     left out                      publishing needs it
  licence       passed                        proprietary, registry refuses it

package check failed: 1 failed, 7 passed
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

For a later version, `jadpo package check` writes the new section above
the released ones, such as `## 1.1.0` and ``- added type `OutwardCode` ``,
and fails the same way until something is written beneath its list. A
section already released passes.

## How to fix it

**If the version is ready to release**, write why beneath the list, in a
sentence or two of your own:

```markdown
## 1.0.0

- first release

Reads and tidies UK postcodes, for apps that take them in forms.
```

Run `jadpo package check` again, and the `changelog` row passes with
`1.0.0 says why`. Until the version is tagged, the check rewrites its
heading and list as the package changes, and keeps what you wrote.

## Related

- [The changelog](../language/11-integrations.md#the-changelog)
- [What publishing requires](../language/11-integrations.md#what-publishing-requires)
- [`jadpo package check`](../language/14-tooling.md#jadpo-package-check)
