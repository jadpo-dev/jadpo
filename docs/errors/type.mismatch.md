# `type.mismatch`

A value of one type is used where a different type is required.

## Why Jadpo refuses it

Every named type says what its values mean and which rules they follow.
A value can stand in only for its own type, or for a type it builds on.
Going the other way, such as from a plain `Int` to a `Stock`, always
goes through a constructor, which checks the target's rules, so a value
never takes on a type whose rules nobody checked.

## What it looks like

main.jadpo:

```jadpo check project=e-mismatch expect=type.mismatch
type Stock = Int {
    min: 0
}

function add(a: Stock, b: Stock) -> Stock {
    return a + b
}
```

```text output
error: `Int` can't be used where `Stock` is required
 --> main.jadpo:6:12
  |
6 |     return a + b
  |            ^^^^^ this is `Int`
  reason: Arithmetic works on the representation, so this is a plain `Int`; only `Stock(...)` makes it a `Stock`, checking its rules.
  next: Convert it with `Stock(a + b)` (agent)
  fix:
    6 - return a + b
    6 + return Stock(a + b)
  rule: type.mismatch
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file

next: jadpo fix  fixes this error for you, then checks again
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Where an object is required, the error shows its shape and the value
written out. A text goes in the one field that can't be `none`, and every
other field is given as `none`, as an edit your coding agent can apply; a
standard type's page is one command away:

```jadpo check project=e-mismatch-object expect=type.mismatch
function author() -> PersonName {
    return "Frank Herbert"
}
```

```text output
error: `"Frank Herbert"` can't be used where `PersonName` is required
 --> main.jadpo:2:12
  |
2 |     return "Frank Herbert"
  |            ^^^^^^^^^^^^^^^ the wrong kind of value
  reason: This is text, and `PersonName` is an object: `PersonName { given: GivenName, family: FamilyName? }`.
  next: Write `PersonName { given: "Frank Herbert", family: none }`, and see `jadpo help PersonName` for the type (agent)
  fix:
    2 - return "Frank Herbert"
    2 + return PersonName { given: "Frank Herbert", family: none }
  rule: type.mismatch
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file

next: jadpo fix  fixes this error for you, then checks again
```

A second error with the same reason keeps its place and its own edit, and
its `reason:` says `` As the first `type.mismatch` above ``. `jadpo check
--full` writes every reason out, and `--json` gives the place of the first in
`sameAs`.

## How to fix it

**If the value should become the required type**, convert it with that
type's constructor:

```jadpo fragment
    return Stock(a + b)
```

```jadpo check project=e-mismatch-fix-1 hidden
type Stock = Int {
    min: 0
}

function add(a: Stock, b: Stock) -> Stock {
    return Stock(a + b)
}
```

The sum is checked against `Stock`'s rules. Here that check can't fail:
`a` and `b` are never below zero and `Stock` has no maximum, so their
declared ranges prove the sum is a valid `Stock`. When nothing proves it,
the construction starts with `attempt` and says what happens when it
fails.

**If the value holds what is required**, read that field. When an object
has exactly one field of the required type, such as a query's
`UserWithTodos` given where a list goes, the error names it:

```text
  reason: `UserWithTodos` is an object, and of its fields only `todos` is a `List<T>`.
  next: Read its `todos` field: `mine.todos` (agent)
```

Reading the field passes the list the object holds, and the object's
other fields stay where they are.

**If each item of a list converts to the required item**, map the list.
A list never widens, so a list of one type goes where a list of another
is required only through `list.map`. When each item has one field of the
required type, has every field the required object is built from, or
becomes the required type through a constructor that can't fail, the
error writes the `list.map` out, as an edit:

```jadpo check project=e-mismatch-list expect=type.mismatch
type BookTitle = Text {
    max_length: 200
}

type PageCount = Int {
    min: 1
}

type Book = Object {
    title: BookTitle
    pages: PageCount
}

function titles(books: List<Book>) -> List<BookTitle> {
    return books
}
```

```text output
error: `List<Book>` can't be used where `List<BookTitle>` is required
 --> main.jadpo:15:12
   |
15 |     return books
   |            ^^^^^ this is `List<Book>`
  reason: Collections keep their element type, so a `List<Book>` is never a `List<BookTitle>`; each `Book`'s `title` field is a `BookTitle`.
  next: Convert each item with `list.map(books, book -> book.title)` (agent)
  fix:
    15 - return books
    15 + return list.map(books, book -> book.title)
  rule: type.mismatch
  help: https://jadpo.dev/docs/errors/type.mismatch

check failed: 1 error, 1 file

next: jadpo fix  fixes this error for you, then checks again
```

```jadpo check project=e-mismatch-list-fix hidden
type BookTitle = Text {
    max_length: 200
}

type PageCount = Int {
    min: 1
}

type Book = Object {
    title: BookTitle
    pages: PageCount
}

function titles(books: List<Book>) -> List<BookTitle> {
    return list.map(books, book -> book.title)
}
```

An inline function can't use `attempt`, so a construction that may fail
gets no `list.map`: build each item in a `for` loop instead.

## Related

- [Creating values](../language/02-values-and-types.md#creating-values)
- [Construction failures by rule](../language/02-values-and-types.md#construction-failures-by-rule)
- [When one type can stand in for another](../language/02-values-and-types.md#when-one-type-can-stand-in-for-another)
- [Named types](../language/02-values-and-types.md#named-types)
