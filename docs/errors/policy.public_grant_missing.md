# `policy.public_grant_missing`

A route that anyone may call, with no sign-in, reads or changes a record that only signed-in people are allowed to see.

## Why Jadpo refuses it

Two things must agree before someone who isn't signed in can reach a
record. The route must skip sign-in, with
`authentication: RouteAuthentication.none`, and the entity's grant must
name `Access.public`. Here the route lets anyone call it, but `Article`'s
`read:` names only `Access.authenticated`, so every call would be
refused with 403.

Jadpo won't choose the fix for you. One fix makes every article readable
by the whole world, and the other makes people sign in to read one.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/policy.public_grant_missing

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides who can read an article.

The same error is raised for an action the route runs whose line under
`actions:` doesn't name `Access.public`. On a tenant's records the only
fix it offers is sign-in, since every grant on them, `Access.public`
included, applies only within the tenants where the caller has a
membership.

## How to fix it

The question is whether people must sign in to read an article.

**If anyone at all may read an article**, add `Access.public` to
`Article`'s `read:`:

```jadpo fragment
    policy: {
        read: [Access.public]
    }
```

```jadpo check project=e-public-grant-missing-public hidden
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
        read: [Access.public]
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

`GET /articles/{article_id}` now answers anyone, signed in or not, with
the article's title. Widening a grant needs a person's acceptance once the
app is on its default branch
([Protecting policy](../language/09-policy.md#protecting-policy)).

**If only signed-in people may read one**, remove
`authentication: RouteAuthentication.none` from the route. The app must
sign people in, so this example adds a principal and
`authentication { }`:

```jadpo fragment
    GET /{article_id: Article.id} {
        output: ArticleView
        run: find_article(article_id)
    }
```

```jadpo check project=e-public-grant-missing-signed-in hidden
type Handle = Text {
    max_length: 40
}

entity User {
    id: Uuid
    handle: Handle {
        unique: true
    }
}

principal {
    user: {
        subject: User.handle
    }
}

authentication {
    api_token: {}
}

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
        output: ArticleView
        run: find_article(article_id)
    }
}
```

The route now needs a signed-in caller. A request without a valid token
gets 401, and anyone signed in may read an article, as `Article`'s
`read:` says.

## Related

- [Anyone signed in, or anyone](../language/09-policy.md#anyone-signed-in-or-anyone)
- [Open to anyone: `RouteAuthentication.none`](../language/08-identity.md#open-to-anyone-routeauthenticationnone)
- [Routes: signed in by default](../language/08-identity.md#routes-signed-in-by-default)
- [Grants](../language/09-policy.md#grants)
- [Protecting policy](../language/09-policy.md#protecting-policy)
