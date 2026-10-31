# `data.reference_cycle`

Two or more entities refer to each other through required fields, so none of their records could ever be created first.

## Why Jadpo refuses it

A required reference must point at a record that already exists. Here a
`Team` needs its lead, a `Member`, and a `Member` needs its `Team`. The
first team can't be created without a member, and the first member can't
be created without a team, whatever `on_delete:` says.

Jadpo won't choose which reference becomes optional, because that decides
which record can exist on its own for a while.

## What it looks like

main.jadpo:

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
  help: https://jadpo.dev/docs/errors/data.reference_cycle

check failed: 1 error, 1 file
```

`(human)` at the end of the `next:` line tells your coding agent to stop
and ask you which fix you want, because the choice is yours to make: it
decides whether a team can be without a lead, or a member without a team.

## How to fix it

The question is which record may exist without the other.

**If a team may be without a lead**, make `lead_id` optional. An optional
reference can also be cleared when its member is deleted:

```jadpo fragment
    lead_id: Member.id? {
        on_delete: OnDelete.cleared
    }
```

```jadpo check project=e-reference-cycle-fix-1 hidden
entity Team {
    id: Uuid
    lead_id: Member.id? {
        on_delete: OnDelete.cleared
    }
}

entity Member {
    id: Uuid
    team_id: Team.id {
        on_delete: OnDelete.restricted
    }
}
```

A team is created with no lead, then its members, and then its lead is
set. Deleting the lead leaves the team without one.

**If a member may be without a team**, make `team_id` optional instead:

```jadpo fragment
    team_id: Team.id? {
        on_delete: OnDelete.cleared
    }
```

```jadpo check project=e-reference-cycle-fix-2 hidden
entity Team {
    id: Uuid
    lead_id: Member.id {
        on_delete: OnDelete.restricted
    }
}

entity Member {
    id: Uuid
    team_id: Team.id? {
        on_delete: OnDelete.cleared
    }
}
```

A member is created with no team, then the team with that member as its
lead, and then the member joins it. Deleting a team leaves its members
without one.

## Related

- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
