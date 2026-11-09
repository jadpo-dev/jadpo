# `translation.review_invalid`

An application's `review:` setting for translations holds something other than a `TranslationReview`.

## Why Jadpo refuses it

`review:` says whether wording outside the default locale waits for a person
to accept it, so a value Jadpo can't read would leave that open. It is
`TranslationReview.none`, the default, or `TranslationReview.required`.

## What it looks like

`TranslationReview` has no value `always`.

`app.jadpo`:

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
  help: https://jadpo.dev/docs/errors/translation.review_invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If you want a person to accept each translation**, use
`TranslationReview.required`. In `app.jadpo`:

```jadpo check project=e-tr-review-invalid-fix file=app.jadpo
application {
    translations: {
        review: TranslationReview.required
    }
}
```

New and changed wording outside the default locale now waits for
`jadpo translations accept`.

## Related

- [Review](../language/10-configuration-and-time.md#review)
