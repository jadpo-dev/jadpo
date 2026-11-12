# `type.hole`

A `_` stands where a value goes, for one the code doesn't give yet.

## Why Jadpo refuses it

`_` written where a value goes asks the checker what goes there, and the
answer is this error: the type the place requires, written as the code
would write it, and up to five values in scope that fit, closest first.
It is always an error, so a project holding one never checks, runs its
tests or builds, and it can't be declined. A placeholder that compiles,
such as `""` or `0`, could ship unnoticed; a hole can't.

## What it looks like

main.jadpo:

```jadpo check project=e-hole expect=type.hole
type BookTitle = Text {
    max_length: 200
}

type Book = Object {
    title: BookTitle
    author: PersonName
}

function shelve(title: BookTitle, author: PersonName) -> Book {
    return Book {
        title: title
        author: _
    }
}
```

```text output
error: `_` stands for a `PersonName`
 --> main.jadpo:13:17
   |
13 |         author: _
   |                 ^ a `PersonName` goes here
  reason: `PersonName` is an object: `PersonName { given: GivenName, family: FamilyName? }`. In scope, `author` fits.
  next: Write `author` (agent)
  rule: type.hole
  help: https://jadpo.dev/docs/errors/type.hole

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Where nothing in scope fits an object, the next step writes the object
out with a `_` for each field that can't be `none`, so the next check
says what goes in each:

```jadpo check project=e-hole-object expect=type.hole
type BookTitle = Text {
    max_length: 200
}

type Book = Object {
    title: BookTitle
    author: PersonName
}

function shelve(title: BookTitle) -> Book {
    return Book {
        title: title
        author: _
    }
}
```

```text output
error: `_` stands for a `PersonName`
 --> main.jadpo:13:17
   |
13 |         author: _
   |                 ^ a `PersonName` goes here
  reason: `PersonName` is an object: `PersonName { given: GivenName, family: FamilyName? }`. Nothing in scope fits.
  next: Write `PersonName { given: _, family: none }`, and see `jadpo help PersonName` for the type (agent)
  rule: type.hole
  help: https://jadpo.dev/docs/errors/type.hole

check failed: 1 error, 1 file
```

Where nothing around the `_` says its type, as for an operator's operand,
the error says so. `jadpo check --json` gives the
type as `expected` and each fit as `fits`, with its text, its type and the
edit that writes it, and hovering over the `_` in the editor shows the
same.

## How to fix it

**If one of the values that fit is the one meant**, write it in place of
the `_`:

```jadpo fragment
        author: author
```

```jadpo check project=e-hole-fix-1 hidden
type BookTitle = Text {
    max_length: 200
}

type Book = Object {
    title: BookTitle
    author: PersonName
}

function shelve(title: BookTitle, author: PersonName) -> Book {
    return Book {
        title: title
        author: author
    }
}
```

The value goes in as it is, so the project checks.

## Related

- [Typed holes](../language/02-values-and-types.md#typed-holes)
- [Creating values](../language/02-values-and-types.md#creating-values)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)
