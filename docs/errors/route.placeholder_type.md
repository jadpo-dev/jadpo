# `route.placeholder_type`

A placeholder is typed with a named scalar or an `X.id`, never optional:

```jadpo check project=e-placeholder-type expect=route.placeholder_type
type Memo = Text {
    max_length: 200
}

type MemoId = Uuid {}

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

action tag_memo(memo_id: MemoId, input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}

routes Memos {
    prefix: /memos

    POST /{memo_id: MemoId?} {
        input: AddMemo
        output: MemoView
        run: tag_memo(memo_id, input)
    }
}
```

```text output
error: `memo_id` can't be typed `MemoId?`
 --> main.jadpo:30:21
   |
30 |     POST /{memo_id: MemoId?} {
   |                     ^^^^^^^ not a named scalar
  reason: A placeholder's type is a named scalar or an `X.id`: a placeholder is always there, so it is never optional.
  next: Type it with a named type built on a scalar, or an `X.id` (agent)
  rule: route.placeholder_type
  help: docs/language/07-http.html#route-placeholder-type

check failed: 1 error, 1 file
```

A placeholder typed with a field its type doesn't declare is
`type.unknown_field` at its type, as any field type is:

```jadpo check project=e-placeholder-unknown-field expect=type.unknown_field
type NoteText = Text {
    max_length: 200
}

type NoteView = Output {
    text: NoteText
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

    GET /{note_id: Note.ident} {
        authentication: RouteAuthentication.none
        output: NoteView
        run: find_note(note_id)
    }
}
```

```text output
error: `Note` has no field `ident`
 --> main.jadpo:27:20
   |
27 |     GET /{note_id: Note.ident} {
   |                    ^^^^^^^^^^ not declared
  reason: A field type is written `Type.field`, naming a field the type declares.
  next: Name a field of `Note` (agent)
  rule: type.unknown_field
  help: docs/language/02-values-and-types.html#type-unknown-field

check failed: 1 error, 1 file
```
