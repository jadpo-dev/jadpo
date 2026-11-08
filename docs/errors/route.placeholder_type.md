# `route.placeholder_type`

A placeholder in a path isn't a whole segment typed with a named scalar or an `X.id`.

## Why Jadpo refuses it

A placeholder is decoded from the URL once and checked. It needs a type that
says what the value means, so it is a named type over a scalar, or an
entity's id (`X.id`), written where the placeholder appears. It fills a whole
segment between `/`s and is always there, so it is never optional, and
Jadpo can't tell what a bare or partial segment should be.

## What it looks like

main.jadpo:

```jadpo check project=e-placeholder-type expect=route.placeholder_type
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId?} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: `memo_id` can't be typed `MemoId?`
 --> main.jadpo:32:21
   |
32 |     POST /{memo_id: MemoId?} {
   |                     ^^^^^^^ not a named scalar
  reason: A placeholder's type is a named scalar or an `X.id`: a placeholder is always there, so it is never optional.
  next: Type it with a named type built on a scalar, or an `X.id` (agent)
  rule: route.placeholder_type
  help: https://jadpo.dev/docs/errors/route.placeholder_type

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A built-in type says nothing about what the value means:

```jadpo check project=e-placeholder-builtin expect=route.placeholder_type
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: Uuid} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: `memo_id` can't be typed `Uuid`
 --> main.jadpo:32:21
   |
32 |     POST /{memo_id: Uuid} {
   |                     ^^^^ not a named scalar
  reason: A placeholder's type is a named scalar or an `X.id`: a built-in type says nothing about what the value means.
  next: Type it with a named type built on a scalar, or an `X.id` (agent)
  rule: route.placeholder_type
  help: https://jadpo.dev/docs/errors/route.placeholder_type

check failed: 1 error, 1 file
```

A placeholder is typed where it appears:

```jadpo check project=e-placeholder-untyped expect=route.placeholder_type
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: `memo_id` has no type
 --> main.jadpo:32:11
   |
32 |     POST /{memo_id} {
   |           ^^^^^^^^^ untyped
  reason: A placeholder is typed where it appears, so its segment is decoded and checked once: `{<name>: <Type>}`.
  next: Write `{memo_id: <Type>}` (agent)
  rule: route.placeholder_type
  help: https://jadpo.dev/docs/errors/route.placeholder_type

check failed: 1 error, 1 file
```

And it fills a whole segment, with no fixed text beside it:

```jadpo check project=e-placeholder-shared expect=route.placeholder_type
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /memo-{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: `memo_id` shares its segment with fixed text
 --> main.jadpo:32:16
   |
32 |     POST /memo-{memo_id: MemoId} {
   |                ^^^^^^^^^^^^^^^^^ a whole segment
  reason: A placeholder fills a whole segment between `/`s, so the value read from the URL is exactly the segment.
  next: Put the placeholder in a segment of its own (agent)
  rule: route.placeholder_type
  help: https://jadpo.dev/docs/errors/route.placeholder_type

check failed: 1 error, 1 file
```

## How to fix it

**If the type is optional or built in**, use a named type on its own:

```jadpo fragment
POST /{memo_id: MemoId} {
    ...
}
```

```jadpo check project=e-placeholder-type-fixed hidden
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    id: MemoId
    text: Memo
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        id: memo_id
        text: input.text
    }
}

policy {
    actions: {
        tag_memo: [Access.public]
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId} {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

A request with a value that isn't a `MemoId` now gets a 422, like any bad
input.

**If the value is a record's id**, type it `X.id`:

```jadpo fragment
GET /{note_id: Note.id} {
    ...
}
```

```jadpo check project=e-placeholder-type-entity hidden
type NoteText = Text {
    max_length: 200
}

type NoteView = Output {
    text: NoteText
}

type NoteNotFound = Failure {
    kind: FailureKind.not_found
    message: "No note has this id."
}

entity Note {
    id: Uuid
    text: NoteText

    policy: {
        read: [Access.public]
    }
}

query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

routes Notes {
    prefix: /notes

    GET /{note_id: Note.id} {
        authentication: RouteAuthentication.none
        output: NoteView
        run: find_note(note_id)
    }
}
```

The segment is now checked as the id of a `Note`.

## Related

- [Paths and placeholders](../language/07-http.md#paths-and-placeholders)
- [Named types](../language/02-values-and-types.md#named-types)
- [`type.unknown_field`](type.unknown_field.md), raised for a placeholder typed with a field its type doesn't declare
