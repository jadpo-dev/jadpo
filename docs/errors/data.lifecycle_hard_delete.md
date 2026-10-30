# `data.lifecycle_hard_delete`

Code never removes a record that has a lifecycle: `delete` runs its `delete` transition, and an entity with none can't be deleted:

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
