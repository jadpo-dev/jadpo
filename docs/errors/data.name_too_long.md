# `data.name_too_long`

An entity's or a field's name is too long to be the name of a table or a column.

## Why Jadpo refuses it

A table or a column takes its name from the entity or field that declares
it, and PostgreSQL keeps names of at most 63 bytes. It would cut a longer
one without saying so, and two long names could then become the same name.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.name_too_long

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

An entity whose table's name would be too long is refused the same way:

```jadpo check project=e-name-too-long-table expect=data.name_too_long
entity TheRecordOfEachNoteThatWasShownToItsOwnerInTheWeeklyDigestEmail {
    id: Uuid
}
```

```text output
error: `TheRecordOfEachNoteThatWasShownToItsOwnerInTheWeeklyDigestEmail` is too long a name for a table
 --> main.jadpo:1:8
  |
1 | entity TheRecordOfEachNoteThatWasShownToItsOwnerInTheWeeklyDigestEmail {
  |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 78 characters
  reason: Its table would be called `the_record_of_each_note_that_was_shown_to_its_owner_in_the_weekly_digest_email`, and PostgreSQL keeps names of at most 63 bytes, so a longer one would be cut and could clash with another.
  next: Choose a shorter name (agent)
  rule: data.name_too_long
  help: https://jadpo.dev/docs/errors/data.name_too_long

check failed: 1 error, 1 file
```

## How to fix it

**If the name is longer than it needs to be**, choose a shorter one:

```jadpo fragment
    last_shown_at: Instant?
```

```jadpo check project=e-name-too-long-fix hidden
entity Note {
    id: Uuid
    last_shown_at: Instant?
}
```

The field's column now keeps the whole of its name.

## Related

- [Storage types](../language/05-data.md#storage-types)
- [Fields and their settings](../language/05-data.md#fields-and-their-settings)
