# `search.settings_invalid`

An entity's `search:` holds something it doesn't take, or leaves out what it needs.

## Why Jadpo refuses it

`search:` says which of an entity's fields people search its records by
and what language their words are in, and the compiler builds the
column, the index and the migration from exactly that. A field that
isn't text has no words to search, and a search with no language can't
tell a word's stem or its stop words, so Jadpo won't guess either. Here
`priority` is a number:

## What it looks like

main.jadpo:

```jadpo check project=e-search-settings-invalid expect=search.settings_invalid
type TodoTitle = Text {
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 5
}

entity Todo {
    id: Uuid
    title: TodoTitle
    priority: Priority

    search: {
        fields: [title, priority]
        language: SearchLanguage.english
    }
}
```

```text output
error: `Todo.priority` isn't text, so it can't be searched by words
 --> main.jadpo:16:25
   |
16 |         fields: [title, priority]
   |                         ^^^^^^^^ `Priority`
  reason: A search reads the words of text fields: a type built on `Text`, kept in one text column. `Todo.priority` is `Priority`.
  next: Remove `priority` from `fields:` (agent)
  rule: search.settings_invalid
  help: https://jadpo.dev/docs/errors/search.settings_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A search that doesn't say its language is refused the same way, since
there is no default:

```jadpo check project=e-search-settings-invalid-language expect=search.settings_invalid
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle

    search: {
        fields: [title]
    }
}
```

```text output
error: `Todo`'s `search:` doesn't say what language its words are in
 --> main.jadpo:9:5
  |
9 |     search: {
  |     ^^^^^^ no `language:`
  reason: Words are found by their stem in a language ("running" finds "runs" in English), and stop words such as "the" are left out, so the search needs the language its fields are written in; `SearchLanguage.simple` stems nothing and drops nothing.
  next: Add `language: SearchLanguage.english`, or the language its fields are in (agent)
  rule: search.settings_invalid
  help: https://jadpo.dev/docs/errors/search.settings_invalid

check failed: 1 error, 1 file
```

So is a field that isn't the entity's, a field listed twice, a setting
`search:` doesn't take, a language that isn't one of `SearchLanguage`'s,
and a field stored as `jadpo_search`, the column the words are kept in.

## How to fix it

**If the number isn't something people type into a search box**, list
only the text fields:

```jadpo fragment
entity Todo {
    ...
    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}
```

```jadpo check project=e-search-settings-invalid-fix hidden
type TodoTitle = Text {
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 5
}

entity Todo {
    id: Uuid
    title: TodoTitle
    priority: Priority

    search: {
        fields: [title]
        language: SearchLanguage.english
    }
}
```

A query can still narrow a search by the number, with a condition of its
own beside `matches`: `where: Todo matches phrase and Todo.priority == 1`.

**If the fields are in another language**, name it, or
`SearchLanguage.simple` to search each word exactly as written:

```jadpo fragment
entity Todo {
    ...
    search: {
        fields: [title]
        language: SearchLanguage.simple
    }
}
```

```jadpo check project=e-search-settings-invalid-language-fix hidden
type TodoTitle = Text {
    max_length: 200
}

entity Todo {
    id: Uuid
    title: TodoTitle

    search: {
        fields: [title]
        language: SearchLanguage.simple
    }
}
```

A search for "running" then finds only "running", not "runs", and finds
"the" too.

## Related

- [Search](../language/16-built-in-services.md#search)
- [How it is kept](../language/16-built-in-services.md#how-it-is-kept)
- [`search.undeclared`](../errors/search.undeclared.md)
