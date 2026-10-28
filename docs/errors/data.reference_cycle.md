# `data.reference_cycle`

Neither record could be created first, whatever happens on delete:

```jadpo check project=e-reference-cycle expect=data.reference_cycle
entity Team {
    id: Uuid
    lead_id: Member.id {
        on_delete: OnDelete.restricted
    }
}

entity Member {
    id: Uuid
    team_id: Team.id {
        on_delete: OnDelete.restricted
    }
}
```

```text output
error: Required references form a cycle through `Member`
 --> main.jadpo:10:5
   |
 3 |     lead_id: Member.id {
   |     ------- and this one
10 |     team_id: Team.id {
   |     ^^^^^^^ required reference
  reason: `Member.team_id` and `Team.lead_id` are both required, so no record could be created first: each needs one that already exists.
  next: Make one of these references optional (`X.id?`) (human)
  rule: data.reference_cycle
  help: docs/language/05-data.html#data-reference-cycle

check failed: 1 error, 1 file
```
