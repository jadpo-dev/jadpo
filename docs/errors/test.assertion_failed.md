# `test.assertion_failed`

An `assert` whose condition is false, or an `assert … fails` whose call
returned or failed differently. Records are compared field by field:

```jadpo check project=e-assertion-failed file=todos.jadpo hidden
type TodoTitle = Text {
    min_length: 1
    max_length: 200
}

type Priority = Int {
    min: 1
    max: 9
}

type TodoDraft = Object {
    title: TodoTitle
    priority: Priority
}

function draft(title: TodoTitle) -> TodoDraft {
    return TodoDraft {
        title: title
        priority: 3
    }
}
```

```jadpo check project=e-assertion-failed file=todos.test.jadpo expect=test.assertion_failed
test "a new draft has normal priority" {
    var fresh: TodoDraft = draft("Buy milk")
    assert fresh == TodoDraft {
        title: "Buy milk"
        priority: 2
    }
}
```

```text output
FAIL  a new draft has normal priority
 --> todos.test.jadpo:3

  assert fresh == TodoDraft { ...
         |        |
         |        TodoDraft { title: "Buy milk", priority: 2 }
         TodoDraft { title: "Buy milk", priority: 3 }

  expected: TodoDraft { title: "Buy milk", priority: 2 }
  actual: TodoDraft { title: "Buy milk", priority: 3 }
  diff: ~ priority: expected 2, actual 3

  rule: test.assertion_failed
  rerun: jadpo test todos.test.jadpo:3 --seed 1

test failed: 1 failed, 1 pure test
```
