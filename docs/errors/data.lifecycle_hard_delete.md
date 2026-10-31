# `data.lifecycle_hard_delete`

Code would remove a record that has a lifecycle for good, which only the lifecycle's own purge may do.

## Why Jadpo refuses it

A lifecycle keeps a record through its stages, and only its own purge
removes a record for good, once it has been hidden long enough. Code
never removes such a record. `Article.delete` runs the lifecycle's
`delete` transition, which changes the record and keeps it, and
`Article`'s lifecycle has none.

Jadpo won't add the transition for you, because it decides what deleting
an article means: which field records it, and whether the article is
ever removed for good.

## What it looks like

main.jadpo:

```jadpo check project=e-lifecycle-hard-delete expect=data.lifecycle_hard_delete
type ArticleTitle = Text {
    max_length: 120
}

entity Article {
    id: Uuid
    title: ArticleTitle
    published_at: Instant?

    lifecycle: {
        visible: published_at != none and published_at <= clock.now
    }
}

action remove_article(article: Article) -> Unit {
    Article.delete(article)
}
```

```text output
error: `Article.delete` would remove a record `Article`'s lifecycle keeps
 --> main.jadpo:16:5
   |
16 |     Article.delete(article)
   |     ^^^^^^^^^^^^^^ a lifecycle keeps it
  reason: Code never removes a record that has a lifecycle: `delete` runs its `delete` transition, and `Article`'s lifecycle has none. Only purge, the compiler's own, removes such records, once they have been hidden long enough.
  next: Add a `delete` transition to `Article`'s lifecycle, saying what deleting one changes (human)
  rule: data.lifecycle_hard_delete
  help: https://jadpo.dev/docs/errors/data.lifecycle_hard_delete

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides what deleting an article does, and whether it is ever removed
for good.

Deleting a record of another entity is refused in the same way when the
delete would cascade to a record with a lifecycle. Here deleting an
author would delete their articles:

main.jadpo:

```jadpo check project=e-lifecycle-hard-delete-cascade expect=data.lifecycle_hard_delete
type ArticleTitle = Text {
    max_length: 120
}

entity Author {
    id: Uuid
}

entity Article {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.cascade
    }
    title: ArticleTitle
    published_at: Instant?

    lifecycle: {
        visible: published_at != none and published_at <= clock.now
    }
}

action remove_author(author: Author) -> Unit {
    Author.delete(author)
}
```

```text output
error: `Author.delete` would remove `Article` records their lifecycle keeps
 --> main.jadpo:23:5
   |
23 |     Author.delete(author)
   |     ^^^^^^^^^^^^^ a lifecycle keeps it
  reason: Removing a `Author` removes the `Article` records whose `author_id` points at it (`on_delete: OnDelete.cascade`), and `Article` has a lifecycle; code never removes a record that has one.
  next: Give `Author` a lifecycle with a `delete` transition, or change `Article.author_id`'s `on_delete:` (human)
  rule: data.lifecycle_hard_delete
  help: https://jadpo.dev/docs/errors/data.lifecycle_hard_delete

check failed: 1 error, 1 file
```

## How to fix it

**If a deleted article should be hidden and kept**, give the lifecycle a
`delete` transition that records when it happened, and leave out
records it has set from `visible:`:

```jadpo fragment
    removed_at: Instant?

    lifecycle: {
        visible: removed_at == none and published_at != none and published_at <= clock.now
        transitions: {
            delete: {
                set: {
                    removed_at: clock.now
                }
            }
        }
    }
```

```jadpo check project=e-lifecycle-hard-delete-fix-1 hidden
type ArticleTitle = Text {
    max_length: 120
}

entity Article {
    id: Uuid
    title: ArticleTitle
    published_at: Instant?
    removed_at: Instant?

    lifecycle: {
        visible: removed_at == none and published_at != none and published_at <= clock.now
        transitions: {
            delete: {
                set: {
                    removed_at: clock.now
                }
            }
        }
    }
}

action remove_article(article: Article) -> Unit {
    Article.delete(article)
}
```

`Article.delete` now sets `removed_at`. The article drops out of every
read but stays in the database, where it can be looked into or restored.

**If a deleted article should later be removed for good**, add a
`purge:` to the same lifecycle as well:

```jadpo fragment
        purge: {
            after: 30d
            since: removed_at
        }
```

```jadpo check project=e-lifecycle-hard-delete-fix-2 hidden
type ArticleTitle = Text {
    max_length: 120
}

entity Article {
    id: Uuid
    title: ArticleTitle
    published_at: Instant?
    removed_at: Instant?

    lifecycle: {
        visible: removed_at == none and published_at != none and published_at <= clock.now
        transitions: {
            delete: {
                set: {
                    removed_at: clock.now
                }
            }
        }
        purge: {
            after: 30d
            since: removed_at
        }
    }
}

action remove_article(article: Article) -> Unit {
    Article.delete(article)
}
```

Deleting hides the article at once. Thirty days after `removed_at`, the
built-in hourly purge removes it for good, along with any other hidden
article whose `removed_at` is older.

**If deleting an author should be refused while they have articles**,
make the reference `restricted`, and map the refusal in the delete's
`conflict:`:

```jadpo fragment
    author_id: Author.id {
        on_delete: OnDelete.restricted
    }
```

```jadpo fragment
type AuthorHasArticles = Failure {
    kind: FailureKind.conflict
}

action remove_author(author: Author) fails AuthorHasArticles -> Unit {
    attempt Author.delete(author) {
        conflict: {
            Article.author_id: AuthorHasArticles
        }
    }
}
```

```jadpo check project=e-lifecycle-hard-delete-fix-3 hidden
type ArticleTitle = Text {
    max_length: 120
}

entity Author {
    id: Uuid
}

entity Article {
    id: Uuid
    author_id: Author.id {
        on_delete: OnDelete.restricted
    }
    title: ArticleTitle
    published_at: Instant?

    lifecycle: {
        visible: published_at != none and published_at <= clock.now
    }
}

type AuthorHasArticles = Failure {
    kind: FailureKind.conflict
}

action remove_author(author: Author) fails AuthorHasArticles -> Unit {
    attempt Author.delete(author) {
        conflict: {
            Article.author_id: AuthorHasArticles
        }
    }
}
```

Deleting an author fails with `AuthorHasArticles` while any article
refers to them, and their articles are never removed by it. An author can
instead be given a lifecycle of their own with a `delete` transition, so
deleting one hides them and keeps their articles.

## Related

- [Lifecycle](../language/05-data.md#lifecycle)
- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
- [Transitions](../language/06-reads-and-writes.md#transitions)
- [The purge schedule](../language/12-jobs-and-events.md#the-purge-schedule)
