# `translation.option_unknown`

`translations:` in `application { }` takes `review:` and nothing else.

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
  help: docs/errors/translation.option_unknown.md

check failed: 1 error, 1 file
```
