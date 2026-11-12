# `semantic.duplicate_declaration`

A name, or a test's sentence, is already taken where it is declared.

## Why Jadpo refuses it

A name has to mean one thing, so that reading it, or calling it, can only
lead to one declaration. Jadpo won't pick a new name for you, because it
doesn't know what the two things are for; the error says what is taken,
so whoever renames one can follow the names around it.

If two declarations are written the same way, ignoring layout, comments
and `public`, they are a copy: the error says to remove one and use the
other, or to move it to `shared/` if two domains need it. If they differ,
they are two things with one name, and one has to be renamed.

## What it looks like

```jadpo check project=e-duplicate file=a.jadpo hidden
type Count = Int {
    min: 0
}
```

b.jadpo:


```jadpo check project=e-duplicate file=b.jadpo expect=semantic.duplicate_declaration
type Count = Int {
    min: 1
}
```

```text output
error: `Count` is declared twice
 --> b.jadpo:1:6
  |
1 | type Count = Int {
  |      ^^^^^ declared again here
  reason: `Count` is already a type in `a.jadpo`, declared differently, so these are two things with one name.
  next: Rename one of them, here and where it's used (agent)
  in use:
    Count  a.jadpo
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A private function or type needs a name that is unique only within its
domain. Here `shipping`'s own `Address` is no clash, while a second one in
`billing` is. The `in use:` list shows the names that contain the same
words, so an agent renaming one sees what is taken. It shows at most
twelve names, the twelfth line counting the rest:

```jadpo check project=e-duplicate-domain file=app.jadpo hidden
application {}
```

```jadpo check project=e-duplicate-domain file=domains/billing/actions/send_invoice.jadpo hidden
type Address = Object {
    street: Text
}

action send_invoice(address: Address) -> Unit {}
```

```jadpo check project=e-duplicate-domain file=domains/shipping/actions/ship_parcel.jadpo hidden
type Address = Object {
    postcode: Text
}

action ship_parcel(address: Address) -> Unit {}
```

```jadpo check project=e-duplicate-domain file=domains/shipping/actions/print_label.jadpo hidden
type ShippingAddress = Object {
    line_one: Text
}

action print_label(address: ShippingAddress) -> Unit {}
```

domains/billing/actions/send_reminder.jadpo:


```jadpo check project=e-duplicate-domain file=domains/billing/actions/send_reminder.jadpo expect=semantic.duplicate_declaration
type Address = Object {
    street: Text
    town: Text
}

action send_reminder(address: Address) -> Unit {}
```

```text output
error: `Address` is declared twice in `billing`
 --> domains/billing/actions/send_reminder.jadpo:1:6
  |
1 | type Address = Object {
  |      ^^^^^^^ declared again here
  reason: `Address` is already a type in `domains/billing/actions/send_invoice.jadpo`, declared differently, so these are two things with one name.
  next: Rename one of them, here and where it's used (agent)
  in use:
    Address          domains/billing/actions/send_invoice.jadpo
    Address          domains/shipping/actions/ship_parcel.jadpo
    ShippingAddress  domains/shipping/actions/print_label.jadpo
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 5 files
```

A variable can't reuse a name already in scope, parameters included:

```jadpo check project=e-duplicate-variable expect=semantic.duplicate_declaration
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    var count: Count = count
    return count
}
```

```text output
error: `count` is already declared
 --> main.jadpo:6:9
  |
5 | function doubled(count: Count) -> Count {
  |                  ----- first declared here
6 |     var count: Count = count
  |         ^^^^^ declared again
  reason: A name in scope can't be declared again, parameters included, so one name means one value wherever it is read.
  next: Give this variable a name of its own (agent)
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 1 file
```

A name can't be the same as a built-in type:

```jadpo check project=e-duplicate-builtin expect=semantic.duplicate_declaration
type Int = Text {
    max_length: 36
}
```

```text output
error: `Int` is a built-in type
 --> main.jadpo:1:6
  |
1 | type Int = Text {
  |      ^^^ already declared by Jadpo
  reason: Built-in types exist without being declared, and their names can't be reused.
  next: Choose another name for this type (agent)
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 1 file
```

A test's sentence is its name, unique across the project:

```jadpo check project=e-duplicate-test expect=semantic.duplicate_declaration
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    return Count(count * 2)
}

test "doubling keeps a count" {
    assert doubled(2) == 4
}

test "doubling keeps a count" {
    assert doubled(3) == 6
}
```

```text output
error: The test "doubling keeps a count" is declared twice
 --> main.jadpo:13:6
   |
13 | test "doubling keeps a count" {
   |      ^^^^^^^^^^^^^^^^^^^^^^^^ declared again here
  reason: A test's sentence is its name, unique across the project, so a report or a rerun names exactly one test; it is already a test in `main.jadpo`.
  next: Reword this test's sentence to say what it checks that the other doesn't, or remove it if it's a copy (agent)
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 1 file
```

A name something was born with is still in use after a rename, since
stored records and the review know that thing by it
([Stable identity](../language/18-records-and-manifest.md#stable-identity)).
Here `jadpo rename greet welcome` renamed an action:

```jadpo check project=e-duplicate-born file=app.jadpo hidden
application {}
```

```jadpo check project=e-duplicate-born file=database/identities.jadpo_record hidden
// Generated by Jadpo from renames and moves; update it with `jadpo rename`, `jadpo domain` or `jadpo fmt --layout`, never by hand.
record identities 1

action greet -> welcome
```

```jadpo check project=e-duplicate-born file=domains/hello/actions/welcome.jadpo hidden
/// Says hello.
action welcome() -> Unit {}
```

domains/hello/actions/greet.jadpo:


```jadpo check project=e-duplicate-born file=domains/hello/actions/greet.jadpo expect=semantic.duplicate_declaration
/// Says hello again.
action greet() -> Unit {}
```

```text output
error: `greet` is the name `welcome` was born with
 --> domains/hello/actions/greet.jadpo:2:8
  |
2 | action greet() -> Unit {}
  |        ^^^^^ the name `welcome` was born with
  reason: Stored records and the review know `welcome` by the name it was born with, so a new action can't take `greet` while `welcome` keeps that identity.
  next: Choose another name for this action (agent)
  rule: semantic.duplicate_declaration
  help: https://jadpo.dev/docs/errors/semantic.duplicate_declaration

check failed: 1 error, 3 files
```

## How to fix it

**If the two are the same thing**, remove one and use the other:

a.jadpo:

```jadpo check project=e-duplicate-fix-1 file=a.jadpo
type Count = Int {
    min: 0
}
```

`Count` is now declared once, and everything that used either copy uses
it.

**If they are two different things**, rename one, here and where it is
used:

b.jadpo:

```jadpo check project=e-duplicate-fix-2 file=b.jadpo
type VisitCount = Int {
    min: 1
}
```

```jadpo check project=e-duplicate-fix-2 file=a.jadpo hidden
type Count = Int {
    min: 0
}
```

Each name now means one thing.

## Related

- [Names are unique](../language/01-project-layout-and-modules.md#names-are-unique)
- [When two names clash](../language/01-project-layout-and-modules.md#when-two-names-clash)
