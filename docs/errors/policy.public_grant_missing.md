# `policy.public_grant_missing`

A route anyone may call reaches an entity that does not grant `Access.public`:

```jadpo check project=e-public-grant-missing expect=policy.public_grant_missing
type ArticleTitle = Text {
    max_length: 120
}

type ArticleView = Output {
    title: ArticleTitle
}

entity Article {
    id: Uuid
    title: ArticleTitle

    policy: {
        read: [Access.authenticated]
    }
}

query find_article(article_id: Article.id) fails ArticleNotFound -> ArticleView {
    from: Article
    where: Article.id == article_id
    missing: ArticleNotFound
}

routes Articles {
    prefix: /articles

    GET /{article_id: Article.id} {
        authentication: RouteAuthentication.none
        output: ArticleView
        run: find_article(article_id)
    }
}
```

```text output
error: `GET /articles/{article_id}` lets anyone call it, but not everyone may read `Article`
 --> main.jadpo:27:5
   |
27 |     GET /{article_id: Article.id} {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ anyone may call it
  reason: A route with `authentication: RouteAuthentication.none` has no caller, so everything it reaches needs `Access.public`; `Article`'s `read:` doesn't name `Access.public`. Either alone is not enough.
  next: Add `Access.public` to `Article`'s `read:`, or remove `authentication: RouteAuthentication.none` from the route (human)
  rule: policy.public_grant_missing
  help: docs/errors/policy.public_grant_missing.md

check failed: 1 error, 1 file
```
