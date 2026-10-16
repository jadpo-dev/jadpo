//! A declaration's name is not a value (area 3, `type.not_a_value`; chunk
//! 5.2c). A type, failure, entity, action, query, function, enum or other
//! declaration named where a value is read, in a variable's initial value,
//! an argument, a returned value, an object's field, a list's item, an
//! operand, a condition, a `match` subject, a `for` list, a function's
//! body, a `set:`, a route's `run:`, a query's condition or a test's
//! expectation, is one error, whatever the type of value expected there.
//! What is a value (a variant, `clock.now`, a setting, a local) stays one.

use crate::support;

use jadpo_db::Layout;
use support::rules;

/// A principal, types of every kind, an entity with a query and an action,
/// and a library function; the declarations a name can be mistaken for.
const PRELUDE: &str = r#"principal {
    user: {
        subject: Person.handle
    }
}

authentication {
    api_token: {}
}

type Handle = Text {
    max_length: 40
}

type NoteText = Text {
    max_length: 200
}

type Level = Int {
    min: 0
}

type Flag = Bool {}

type Delta = Int {}

type Placed = Object {
    level: Level
}

type Status = Enum {
    open
    done
}

type Refused = Failure {
    kind: FailureKind.rejected
}

type NoteView = Output {
    text: NoteText
}

type Pick = Input {
    note_id: Note.id
}

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        read: [id]
    }
}

entity Note {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    text: NoteText
    level: Level

    policy: {
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
    }
}

entity Memo {
    id: Uuid
    text: NoteText
}

function helper(level: Level) -> Level {
    return level
}

query note_view(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}

"#;

/// Where a value is read: the code around a name, and a value that fits
/// there, to show the code is otherwise clean.
struct Site {
    name: &'static str,
    code: &'static str,
    good: &'static str,
}

const SITES: &[Site] = &[
    Site {
        name: "a variable's value",
        code: "function f() -> Level {\n    var g: Level = {X}\n    return g\n}\n",
        good: "1",
    },
    Site {
        name: "a returned value",
        code: "function f() -> Level {\n    return {X}\n}\n",
        good: "1",
    },
    Site {
        name: "an argument",
        code: "function f() -> Level {\n    return helper({X})\n}\n",
        good: "1",
    },
    Site {
        name: "a constructor's value",
        code: "function f() -> Level {\n    return Level({X})\n}\n",
        good: "1",
    },
    Site {
        name: "an object's field",
        code: "function f() -> Placed {\n    return Placed {\n        level: {X}\n    }\n}\n",
        good: "1",
    },
    Site {
        name: "a list's item",
        code: "function f() -> List<Level> {\n    return [{X}]\n}\n",
        good: "1",
    },
    Site {
        name: "a list's second item",
        code: "function f() -> List<Level> {\n    return [1, {X}]\n}\n",
        good: "1",
    },
    Site {
        name: "the left of an operator",
        code: "function f() -> Level {\n    return Level({X} + 1)\n}\n",
        good: "1",
    },
    Site {
        name: "the right of an operator",
        code: "function f() -> Level {\n    return Level(1 + {X})\n}\n",
        good: "1",
    },
    Site {
        name: "the left of a comparison",
        code: "function f() -> Flag {\n    return Flag({X} == 1)\n}\n",
        good: "1",
    },
    Site {
        name: "the right of a comparison",
        code: "function f() -> Flag {\n    return Flag(1 == {X})\n}\n",
        good: "1",
    },
    Site {
        name: "a negated value",
        code: "function f() -> Delta {\n    return Delta(-{X})\n}\n",
        good: "1",
    },
    Site {
        name: "the operand of not",
        code: "function f() -> Flag {\n    return Flag(not {X})\n}\n",
        good: "true",
    },
    Site {
        name: "the operand of and",
        code: "function f() -> Flag {\n    return Flag(true and {X})\n}\n",
        good: "true",
    },
    Site {
        name: "an if condition",
        code: "function f() -> Level {\n    if ({X}) {\n        return 1\n    }\n    return 2\n}\n",
        good: "true",
    },
    Site {
        name: "a match subject",
        code: "function f() -> Level {\n    return match {X} {\n        Status.open -> 1\n        Status.done -> 2\n    }\n}\n",
        good: "Status.open",
    },
    Site {
        name: "the list a for loop walks",
        code: "function f() -> Level {\n    for item in {X} {\n        return item\n    }\n    return 0\n}\n",
        good: "[Level(1)]",
    },
    Site {
        name: "an inline function's body",
        code: "function f(levels: List<Level>) -> List<Level> {\n    return collection.map(levels, level -> helper({X}))\n}\n",
        good: "1",
    },
    Site {
        name: "a collection operation's list",
        code: "function f() -> List<Level> {\n    return collection.map({X}, level -> level)\n}\n",
        good: "[Level(1)]",
    },
    Site {
        name: "a field read",
        code: "function f() -> Level {\n    return {X}.level\n}\n",
        good: "Placed {\n        level: 1\n    }",
    },
    Site {
        name: "a set's value",
        code: "action f(note_id: Note.id) fails NoteNotFound -> NoteView {\n    var note: Note = attempt Note.update(note_id) {\n        set: {\n            text: {X}\n        }\n    }\n    return NoteView {\n        text: note.text\n    }\n}\n",
        good: "\"x\"",
    },
    Site {
        name: "a created record's field",
        code: "action f() -> NoteView {\n    var memo: Memo = Memo.create {\n        set: {\n            text: {X}\n        }\n    }\n    return NoteView {\n        text: memo.text\n    }\n}\n",
        good: "\"x\"",
    },
    Site {
        name: "a failure's field",
        code: "type TooMany = Failure {\n    kind: FailureKind.invalid_value\n    internal: {\n        limit: Level\n    }\n}\n\nfunction f() fails TooMany -> Level {\n    reject TooMany {\n        limit: {X}\n    }\n}\n",
        good: "1",
    },
    Site {
        name: "a route's run argument",
        code: "routes Notes {\n    prefix: /notes\n\n    GET /{note_id: Note.id} {\n        output: NoteView\n        run: note_view({X})\n    }\n}\n",
        good: "note_id",
    },
    Site {
        name: "a query's condition",
        code: "query by_level() -> List<NoteView> {\n    from: Note\n    where: Note.level == {X}\n    order: [Note.id asc]\n    limit: 10\n}\n",
        good: "1",
    },
    Site {
        name: "an input field's default",
        code: "type Paging = Input {\n    limit: Level {\n        default: {X}\n    }\n}\n",
        good: "5",
    },
    Site {
        name: "a test's expectation",
        code: "test \"a value\" {\n    assert {X} == 1\n}\n",
        good: "1",
    },
    Site {
        name: "a test's variable",
        code: "test \"a value\" {\n    var g: Level = {X}\n    assert g == 1\n}\n",
        good: "1",
    },
    Site {
        name: "a test's input",
        code: "test \"a value\" {\n    assert helper({X}) == 1\n}\n",
        good: "1",
    },
];

/// Declarations of every kind, to name at each site. Some are in the
/// prelude, some come with the site.
const NAMES: &[(&str, &str)] = &[
    ("a type", "Level"),
    ("an object", "Placed"),
    ("an input", "Pick"),
    ("an output", "NoteView"),
    ("an enum", "Status"),
    ("a failure", "Refused"),
    ("an entity", "Note"),
    ("a function", "helper"),
    ("a query", "note_view"),
    ("a standard type", "Text"),
    ("a standard enum", "FailureKind"),
    ("a standard namespace", "collection"),
    ("a generated namespace", "config"),
    ("the clock", "clock"),
];

fn check(code: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{PRELUDE}{code}"))],
    )
}

fn at(site: &Site, value: &str) -> String {
    site.code.replace("{X}", value)
}

#[test]
fn every_site_is_clean_with_a_value() {
    let mut wrong = Vec::new();
    for site in SITES {
        let found = check(&at(site, site.good));
        if !found.is_empty() {
            wrong.push(format!("{}: {found:?}", site.name));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_declaration_is_not_a_value_wherever_a_value_is_read() {
    let mut missed = Vec::new();
    for site in SITES {
        for (what, name) in NAMES {
            let found = check(&at(site, name));
            // A member read of these has its own, more exact error.
            let own = match (site.name, *name) {
                ("a field read", "Status" | "FailureKind") => "type.unknown_enum_variant",
                ("a field read", "Note") => "data.stored_value_outside_update",
                ("a field read", "config") => "configuration.field_unknown",
                ("a field read", "clock") => "toolchain.not_built_yet",
                // A default is a literal, and a lower-case name never is.
                (
                    "an input field's default",
                    "helper" | "note_view" | "collection" | "config" | "clock",
                ) => "type.setting_value",
                _ => "type.not_a_value",
            };
            if found != [own] {
                missed.push(format!("{what} `{name}` as {}: {found:?}", site.name));
            }
        }
    }
    assert!(
        missed.is_empty(),
        "{} missed:\n{}",
        missed.len(),
        missed.join("\n")
    );
}

#[test]
fn what_is_a_value_stays_one() {
    for (code, why) in [
        (
            "function f() -> Status {\n    return Status.open\n}\n",
            "an enum's variant",
        ),
        (
            "function f(status: Status) -> Level {\n    return match status {\n        Status.open -> 1\n        Status.done -> 2\n    }\n}\n",
            "a variant as a pattern",
        ),
        (
            "function f(level: Level) -> Level {\n    return level\n}\n",
            "a parameter",
        ),
        (
            "action f() -> Level {\n    var now: Instant = clock.now\n    return 1\n}\n",
            "the time",
        ),
        (
            "function f() -> Zone {\n    return Zone.utc\n}\n",
            "a built-in enum's variant",
        ),
        (
            "function f() -> Level {\n    return collection.count(list: [1, 2])\n}\n",
            "a call through a namespace",
        ),
    ] {
        let found = check(code);
        assert!(
            !found.contains(&"type.not_a_value".to_string()),
            "{why}: {found:?}"
        );
    }
}
