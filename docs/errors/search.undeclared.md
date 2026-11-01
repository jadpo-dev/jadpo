# `search.undeclared`

A query searches an entity by words that has no `search:` section.

## Why Jadpo refuses it

A query finds records by words only in the fields its entity's
`search:` lists: that section is where the compiler gets the column of
words it keeps up to date, its index and its language. Without one there
is nothing to search, and Jadpo won't pick the fields for you, since
which fields people search by, and which come first, is the entity's
decision. Here `Todo` has no `search:`:

## What it looks like

main.jadpo:

```jadpo check project=e-search-undeclared expect=search.undeclared
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle
}

type TodoView = Output {
    pick: [Todo.id, Todo.title]
}

query search_todos(phrase: SearchText) -> List<TodoView> {
    from: Todo
    where: Todo matches phrase
    limit: 20
}
```

```text output
error: `Todo` can't be searched by words
 --> main.jadpo:16:12
   |
16 |     where: Todo matches phrase
   |            ^^^^^^^^^^^^^^^^^^^ no `search:`
  reason: A query finds records by words only in the fields its entity's `search:` lists, which the compiler indexes; `Todo` has no `search:` section.
  next: Add `search: { fields: [...] language: SearchLanguage.english }` to `Todo`, listing the text fields people search by (agent)
  rule: search.undeclared
  help: https://jadpo.dev/docs/errors/search.undeclared

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If people should search todos by their titles**, give the entity a
`search:`, listing the text fields they search by, the one counting most
first, and their language:

```jadpo fragment
entity Todo {
    id: Uuid
    title: TodoTitle

    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}
```

```jadpo check project=e-search-undeclared-fix hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle

    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}

type TodoView = Output {
    pick: [Todo.id, Todo.title]
}

query search_todos(phrase: SearchText) -> List<TodoView> {
    from: Todo
    where: Todo matches phrase
    limit: 20
}
```

The next `jadpo migrate new` adds the column of words and its index, and
the query finds the best matches first.

## Related

- [Search](../language/16-built-in-services.md#search)
- [`search.settings_invalid`](../errors/search.settings_invalid.md)
