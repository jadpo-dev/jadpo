# `syntax.expected_name`

A name is missing, or a reserved word stands where a name is needed.

## Why Jadpo refuses it

A name is how everything else refers to a declaration, a member or a domain,
so each of them has to have one. A reserved word can't be that name, because
the same word has to mean the same thing wherever it is written: `type` on
its own is always the keyword.

## What it looks like

main.jadpo:

```jadpo check project=e-expected-name expect=syntax.expected_name
type Pair = Object {
    : Int
}
```

```text output
error: This member has no name
 --> main.jadpo:2:5
  |
2 |     : Int
  |     ^ expected a name before `:`
  reason: Every member inside braces is `name: value`.
  next: Add the member's name before `:` (agent)
  rule: syntax.expected_name
  help: https://jadpo.dev/docs/errors/syntax.expected_name

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A block whose names are wrong reports the first: a later line is most often
the same mistake again, and is reported once the first is fixed.

A domain is named by its folder, so a folder named with a reserved word is
refused there, once. Where the domain is used, `library.borrow(...)`, the
word is read as its name and nothing more is reported:

```jadpo check project=e-expected-name-domain file=app.jadpo hidden
application {}
```

domains/desk/actions/lend_book.jadpo:

```jadpo check project=e-expected-name-domain file=domains/desk/actions/lend_book.jadpo
use library

action lend_book() -> Unit {
    library.borrow()
}
```

domains/library/actions/borrow.jadpo:

```jadpo check project=e-expected-name-domain file=domains/library/actions/borrow.jadpo expect=syntax.expected_name
public action borrow() -> Unit {}
```

```text output
error: `library` is a reserved word and can't name a domain
 --> domains/library/actions/borrow.jadpo
  reason: A domain's folder name is its name, written before each call into it, and a reserved word can't be a name: `library.` would read as the word, not the domain.
  next: Rename the folder `domains/library/` after the business area it covers, and write the new name wherever `library.` names the domain (agent)
  rule: syntax.expected_name
  help: https://jadpo.dev/docs/errors/syntax.expected_name

check failed: 1 error, 3 files
```

Here your coding agent can rename it, because this project has no commit yet
and so no one depends on the name. Once the project has a commit, the next
step ends `(human)`, and a person decides.

A field may be named with a reserved word, but only what holds it can read
it, as in `booking.type`. On its own the word is never a name, and a rule
across fields names its fields alone, so it can't read this one:

main.jadpo:

```jadpo check project=e-expected-name-value expect=syntax.expected_name
type Kind = Enum {
    a
    b
}

type Booking = Object {
    type: Kind
    other: Kind
    rule same: type == other
}
```

```text output
error: `type` is a reserved word
 --> main.jadpo:9:16
  |
9 |     rule same: type == other
  |                ^^^^ can't be read on its own
  reason: A field may be named with a reserved word, and is then read through what holds it, `thing.type`. On its own the word is never a name, so a rule's condition, which names fields alone, can't read one.
  next: Read it as `thing.type`; in a rule's condition, rename the field (agent)
  rule: syntax.expected_name
  help: https://jadpo.dev/docs/errors/syntax.expected_name

check failed: 1 error, 1 file
```

## How to fix it

**If the member has no name**, add one before the colon:

```jadpo check project=e-expected-name-fix-1
type Pair = Object {
    first: Int
}
```

The member is now `first`, and it can be read as `pair.first`.

**If a folder's name is a reserved word**, rename the folder after the
business area it covers, and write the new name wherever the old one names
the domain:

```jadpo fragment
use lending

action lend_book() -> Unit {
    lending.borrow()
}
```

```jadpo check project=e-expected-name-fix-2 file=domains/desk/actions/lend_book.jadpo hidden
use lending

action lend_book() -> Unit {
    lending.borrow()
}
```

```jadpo check project=e-expected-name-fix-2 file=app.jadpo hidden
application {}
```

```jadpo check project=e-expected-name-fix-2 file=domains/lending/actions/borrow.jadpo hidden
public action borrow() -> Unit {}
```

The domain is now `domains/lending/`, and other domains call it as
`lending`.

**If a rule reads a field named with a reserved word**, rename the field:

```jadpo fragment
type Booking = Object {
    kind: Kind
    other: Kind
    rule same: kind == other
}
```

```jadpo check project=e-expected-name-fix-3 hidden
type Kind = Enum {
    a
    b
}

type Booking = Object {
    kind: Kind
    other: Kind
    rule same: kind == other
}
```

The field is now `kind`, so the rule can read it.

## Related

- [Naming](../language/01-project-layout-and-modules.md#naming)
- [Reserved words](../language/01-project-layout-and-modules.md#reserved-words)
- [Domains](../language/01-project-layout-and-modules.md#domains)
