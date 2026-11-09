# `translation.option_unknown`

An application's `translations:` setting has an option it doesn't take.

## Why Jadpo refuses it

`translations:` in `application { }` takes `review:` and nothing else. Jadpo
reports anything else, because an option it ignored would look like it had
taken effect while your translations went unreviewed.

## What it looks like

`review` is written `reviews`.

`app.jadpo`:

```jadpo check project=e-tr-option-unknown file=app.jadpo expect=translation.option_unknown
application {
    translations: {
        reviews: TranslationReview.required
    }
}
```

```text output
error: `reviews` isn't an option of `translations`
 --> app.jadpo:3:9
  |
3 |         reviews: TranslationReview.required
  |         ^^^^^^^ unknown option
  reason: `translations:` in `application` takes `review:` and nothing else. Did you mean `review`?
  next: Use `review` instead of `reviews` (agent)
  rule: translation.option_unknown
  help: https://jadpo.dev/docs/errors/translation.option_unknown

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the option is misspelt**, use `review:`. In `app.jadpo`:

```jadpo check project=e-tr-option-unknown-fix file=app.jadpo
application {
    translations: {
        review: TranslationReview.required
    }
}
```

New and changed wording outside the default locale now waits for someone
to accept it.

## Related

- [Review](../language/10-configuration-and-time.md#review)
