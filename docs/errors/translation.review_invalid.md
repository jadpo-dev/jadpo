# `translation.review_invalid`

`review:` says whether wording outside the default locale waits for a person to accept it: `TranslationReview.none`, the default, or `TranslationReview.required`.

```jadpo check project=e-tr-review-invalid file=app.jadpo expect=translation.review_invalid
application {
    translations: {
        review: TranslationReview.always
    }
}
```

```text output
error: `review:` isn't a `TranslationReview`
 --> app.jadpo:3:17
  |
3 |         review: TranslationReview.always
  |                 ^^^^^^^^^^^^^^^^^^^^^^^^ not `TranslationReview.none` or `.required`
  reason: `review:` says whether wording outside the default locale waits for a person to accept it: `TranslationReview.none`, the default, or `TranslationReview.required`.
  next: Write `review: TranslationReview.required`, or leave `review:` out (agent)
  rule: translation.review_invalid
  help: docs/errors/translation.review_invalid.md

check failed: 1 error, 1 file
```
