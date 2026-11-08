# `data.cleared_required_reference`

A reference that can't be `none` is set to be cleared when the record it refers to is deleted.

## Why Jadpo refuses it

`OnDelete.cleared` says that when the record a reference points to is
deleted, the reference becomes `none`. A field typed `Project.id`, with no
`?`, can't hold `none`, so the delete would have nothing it could write.
The field's type and its `on_delete:` say opposite things.

## What it looks like

main.jadpo:

```jadpo check project=e-cleared-required expect=data.cleared_required_reference
entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    project_id: Project.id {
        on_delete: OnDelete.cleared
    }
}
```

```text output
error: `Todo.project_id` can't be cleared
 --> main.jadpo:8:20
  |
8 |         on_delete: OnDelete.cleared
  |                    ^^^^^^^^^^^^^^^^ the field can't be `none`
  reason: `OnDelete.cleared` sets the reference to `none` when the `Project` is deleted, which only an optional reference (`Project.id?`) can hold.
  next: Make it `Project.id?`, or use `OnDelete.restricted` or `OnDelete.cascade` (agent)
  rule: data.cleared_required_reference
  help: https://jadpo.dev/docs/errors/data.cleared_required_reference

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If a todo can exist without a project**, make the reference optional:

```jadpo fragment
    project_id: Project.id? {
        on_delete: OnDelete.cleared
    }
```

```jadpo check project=e-cleared-required-fix-1 hidden
entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    project_id: Project.id? {
        on_delete: OnDelete.cleared
    }
}
```

Deleting a project now leaves its todos in place, outside any project.

**If a todo can't outlive its project**, delete it with the project:

```jadpo fragment
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
```

```jadpo check project=e-cleared-required-fix-2 hidden
entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    project_id: Project.id {
        on_delete: OnDelete.cascade
    }
}
```

Deleting a project now deletes its todos in the same transaction.

**If a project with todos must not be deleted**, restrict the delete:

```jadpo fragment
    project_id: Project.id {
        on_delete: OnDelete.restricted
    }
```

```jadpo check project=e-cleared-required-fix-3 hidden
entity Project {
    id: Uuid
}

entity Todo {
    id: Uuid
    project_id: Project.id {
        on_delete: OnDelete.restricted
    }
}
```

Deleting a project that still has todos now fails with a conflict.

## Related

- [References and `on_delete`](../language/05-data.md#references-and-on_delete)
