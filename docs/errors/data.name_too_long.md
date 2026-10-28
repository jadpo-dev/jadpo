# `data.name_too_long`

```jadpo check project=e-name-too-long expect=data.name_too_long
entity Note {
    id: Uuid
    the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest: Instant?
}
```

```text output
error: `Note.the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest` is too long a name for a column
 --> main.jadpo:3:5
  |
3 |     the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest: Instant?
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 67 characters
  reason: Its column would be called `the_date_this_note_was_last_shown_to_its_owner_in_the_weekly_digest`, and PostgreSQL keeps names of at most 63 bytes, so a longer one would be cut and could clash with another.
  next: Choose a shorter name (agent)
  rule: data.name_too_long
  help: docs/errors/data.name_too_long.md

check failed: 1 error, 1 file
```
