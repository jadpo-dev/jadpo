//! Area 4, failures, and the failure side of area 3: the decided examples
//! check cleanly, each error is caught on its own mistake, one mistake
//! gives one error, each callable's failure set is a query of its own, and
//! a body edit reruns only the failure sets that depend on it.

use crate::support;

use jadpo_db::{Database, Layout};
use salsa::Setter;
use support::rules;

fn check(files: &[(&str, &str)]) -> Vec<String> {
    rules(Layout::Loose, files)
}

fn one(source: &str) -> Vec<String> {
    check(&[("main.jadpo", source)])
}

const CLEAN: &[String] = &[];

/// `lookup` fails with `LookupMissing` or `LookupRefused`, as on area 3's
/// page.
const LOOKUP: &str = "type LookupKey = Text {}\n\ntype LookupResult = Text {}\n\ntype LookupMissing = Failure {\n    kind: FailureKind.not_found\n}\n\ntype LookupRefused = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction lookup(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {\n    if (key == \"missing\") {\n        reject LookupMissing\n    }\n    if (key == \"refused\") {\n        reject LookupRefused\n    }\n    return \"found\"\n}\n";

fn with_lookup(source: &str) -> Vec<String> {
    check(&[("lookup.jadpo", LOOKUP), ("main.jadpo", source)])
}

const QUANTITY: &str = "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype QuantityTooLarge = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn with_quantity(source: &str) -> Vec<String> {
    one(&format!("{QUANTITY}{source}"))
}

#[test]
fn failures_are_declared_and_rejected_with_their_fields() {
    assert_eq!(
        one(
            "type Quantity = Int {\n    min: 1\n    max: 1000\n}\n\nentity Todo {\n    id: Uuid\n}\n\nentity Product {\n    id: Uuid\n}\n\n/// The todo does not exist.\ntype TodoNotFound = Failure {\n    kind: FailureKind.not_found\n    message: \"Todo not found.\"\n    internal: {\n        todo_id: Todo.id\n    }\n}\n\ntype QuantityUnavailable = Failure {\n    kind: FailureKind.conflict\n    message: \"The requested quantity is unavailable.\"\n    client: {\n        available: Quantity\n    }\n    internal: {\n        product_id: Product.id\n        requested: Quantity\n    }\n}\n\naction reserve(product_id: Product.id, requested: Quantity, available: Quantity) fails QuantityUnavailable -> Quantity {\n    if (requested > available) {\n        reject QuantityUnavailable {\n            available: available\n            product_id: product_id\n            requested: requested\n        }\n    }\n    return requested\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn fails_lists_exactly_what_can_leave() {
    assert_eq!(
        one(
            "type Title = Text {\n    max_length: 200\n}\n\ntype Count = Int {\n    min: 0\n}\n\ntype TitleBlank = Failure {\n    kind: FailureKind.invalid_value\n    message: \"Enter a title.\"\n}\n\ntype TitleTaken = Failure {\n    kind: FailureKind.conflict\n}\n\nfunction check_title(title: Title) fails TitleBlank -> Title {\n    if (title == \"\") {\n        reject TitleBlank\n    }\n    return title\n}\n\naction rename(title: Title, existing: Count) fails TitleBlank, TitleTaken -> Title {\n    var checked: Title = attempt check_title(title)\n    if (existing > 0) {\n        reject TitleTaken\n    }\n    return checked\n}\n"
        ),
        CLEAN
    );
    // A `Unit` callable may stand an `attempt` on its own, and a condition
    // may reject on every branch.
    assert_eq!(
        check(&[
            (
                "todos.jadpo",
                "type TodoStatus = Enum {\n    open\n    archived\n}\n\nentity Todo {\n    id: Uuid\n    status: TodoStatus\n}\n\ntype TodoNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\ntype Archived = Bool {}\n\nfunction already_archived(todo_id: Todo.id) -> Archived {\n    return false\n}\n\naction notify_owner(todo_id: Todo.id) -> Unit {}\n"
            ),
            (
                "main.jadpo",
                "action archive_todo(todo_id: Todo.id) fails TodoNotFound -> Unit {\n    attempt Todo.get(todo_id)\n    if (already_archived(todo_id)) {\n        return\n    }\n    notify_owner(todo_id)\n}\n"
            ),
        ]),
        CLEAN
    );
}

/// `reserve` passes `SoldOut` on, but its `fails` list forgot it: callers
/// that `attempt` it or match its outcome are right, so the one error is
/// the missing declaration, not a `next` step telling them to undo
/// correct code.
#[test]
fn an_undeclared_failure_is_one_error_not_one_per_caller() {
    let shelf = "type Stock = Int {\n    min: 0\n}\n\ntype SoldOut = Failure {\n    kind: FailureKind.conflict\n}\n\nfunction check_shelf(left: Stock) fails SoldOut -> Stock {\n    if (left == 0) {\n        reject SoldOut\n    }\n    return left\n}\n\naction reserve(left: Stock) -> Stock {\n    return attempt check_shelf(left)\n}\n\naction order(left: Stock) fails SoldOut -> Stock {\n    return attempt reserve(left)\n}\n\naction change(left: Stock) -> Stock {\n    var kept: Stock = match reserve(left) {\n        success(n) -> n\n        SoldOut -> left\n    }\n    return kept\n}\n\ntest \"reserving\" {\n    var n: Stock = attempt reserve(3)\n    assert n == 3\n}\n";
    assert_eq!(one(shelf), ["failure.undeclared_propagation"]);
    // A callable that really can't fail still has its callers told.
    assert_eq!(
        one(
            "type Stock = Int {\n    min: 0\n}\n\naction reserve(left: Stock) -> Stock {\n    return left\n}\n\naction order(left: Stock) -> Stock {\n    return attempt reserve(left)\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
}

#[test]
fn outcome_match_recovers_maps_and_passes_on() {
    assert_eq!(
        with_lookup(
            "type LookupRejected = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction lookup_or_default(key: LookupKey) fails LookupRejected -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"not found\"\n        LookupRefused -> reject LookupRejected\n    }\n}\n\naction relay(key: LookupKey) fails LookupMissing -> LookupResult {\n    match lookup(key) {\n        success(value) -> {\n            return value\n        }\n        LookupMissing -> propagate\n        LookupRefused -> {\n            return \"refused\"\n        }\n    }\n}\n\nfunction chained(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {\n    var first: LookupResult = attempt lookup(key)\n    return first\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn constructions_the_ranges_prove_safe_take_no_attempt() {
    assert_eq!(
        with_quantity(
            "function line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)\n    return total\n}\n\nfunction doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityTooLarge\n    }\n    return result\n}\n\nfunction halved(quantity: Quantity) -> Pence {\n    return Pence(quantity / 2)\n}\n\nfunction capped(quantity: Quantity) -> Quantity {\n    return Quantity(quantity)\n}\n"
        ),
        CLEAN
    );
    // `price - 1` can go below zero, `quantity * 2` above 1000.
    assert_eq!(
        with_quantity(
            "function cheaper(price: Pence) -> Pence {\n    return Pence(price - 1)\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // A shorter text type fits a longer one; the other way round can fail.
    assert_eq!(
        one(
            "type Short = Text {\n    max_length: 10\n}\n\ntype Long = Text {\n    max_length: 80\n}\n\nfunction widen(body: Short) -> Long {\n    return Long(body)\n}\n\nfunction narrow(body: Long) -> Short {\n    return Short(body)\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // Dividing by a type that excludes zero can still round down to zero.
    assert_eq!(
        one(
            "type People = Int {\n    min: 1\n}\n\nfunction per_person(total: People, people: People) -> People {\n    return People(total / people)\n}\n"
        ),
        ["failure.attempt_required"]
    );
}

#[test]
fn the_standard_divisions_are_proved_like_slash() {
    // `integer.divide` and `decimal.divide` of a value never below zero by
    // a divisor of at least 1 give a result from zero up to the dividend,
    // whichever way they round, as `/` does.
    let source = "type Buns = Int {\n    min: 0\n    max: 500\n}\n\ntype PerTray = Int {\n    min: 1\n    max: 48\n}\n\ntype Trays = Int {\n    min: 0\n    max: 500\n}\n\ntype ItemPrice = Decimal {\n    scale: 2\n    min: 0.00\n}\n\ntype People = Int {\n    min: 1\n}\n\nfunction trays(buns: Buns, per_tray: PerTray) -> Trays {\n    return Trays(integer.divide(buns, per_tray, rounding: Rounding.up))\n}\n\nfunction share(bill: ItemPrice, people: People) -> ItemPrice {\n    return ItemPrice(decimal.divide(dividend: bill, divisor: people, rounding: Rounding.half_up))\n}\n";
    assert_eq!(one(source), CLEAN);
    // A divisor that may be below 1 can make the result larger than the
    // dividend, so a bounded result still needs its check.
    assert_eq!(
        one(
            "type ItemPrice = Decimal {\n    scale: 2\n    min: 0.00\n    max: 10.00\n}\n\ntype Portion = Decimal {\n    scale: 2\n    min: 0.01\n    max: 1.00\n}\n\nfunction scaled(bill: ItemPrice, by: Portion) -> ItemPrice {\n    return ItemPrice(decimal.divide(bill, by, rounding: Rounding.half_up))\n}\n"
        ),
        ["failure.attempt_required"]
    );
}

/// A user whose display name allows 80 characters and initials 3, and a
/// badge whose name field adds its own limit of 12 (area 2, question 2).
const BADGES: &str = "entity User {\n    id: Uuid\n    display_name: Text {\n        max_length: 80\n    }\n    initials: Text {\n        max_length: 3\n    }\n}\n\ntype Nickname = Text {\n    max_length: 10\n}\n\ntype BadgeNameTooLong = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Badge = Object {\n    name: Text {\n        max_length: 12\n    }\n}\n\n";

fn with_badges(source: &str) -> Vec<String> {
    one(&format!("{BADGES}{source}"))
}

#[test]
fn building_an_object_runs_its_fields_own_rules() {
    // A value that widens to the field's declared type is accepted, but
    // an object literal never fails: a value that might break the field's
    // own `max_length` is built first, on its own line, as the field type.
    assert_eq!(
        with_badges(
            "action make_badge(user: User) fails BadgeNameTooLong -> Badge {\n    var name: Badge.name = attempt Badge.name(user.display_name) {\n        max_length: BadgeNameTooLong\n    }\n    return Badge {\n        name: name\n    }\n}\n"
        ),
        CLEAN
    );
    assert_eq!(
        with_badges(
            "action make_badge(user: User) -> Badge {\n    return Badge {\n        name: user.display_name\n    }\n}\n"
        ),
        ["type.field_unproved"]
    );
    // `attempt` doesn't make a literal fallible: the field is still built
    // first.
    assert_eq!(
        with_badges(
            "action make_badge(user: User) -> Badge {\n    var badge: Badge = attempt Badge {\n        name: user.display_name\n    }\n    return badge\n}\n"
        ),
        ["type.field_unproved"]
    );
    // Initials are at most 3 characters: the ranges prove the field's rule.
    assert_eq!(
        with_badges(
            "function initials_badge(user: User) -> Badge {\n    return Badge {\n        name: user.initials\n    }\n}\n"
        ),
        CLEAN
    );
    assert_eq!(
        with_badges(
            "function initials_badge(user: User) -> Badge {\n    var badge: Badge = attempt Badge {\n        name: user.initials\n    }\n    return badge\n}\n"
        ),
        ["failure.attempt_infallible"]
    );
    // A literal is checked while compiling.
    assert_eq!(
        with_badges(
            "function fixed() -> Badge {\n    return Badge {\n        name: \"Ada\"\n    }\n}\n"
        ),
        CLEAN
    );
    assert_eq!(
        with_badges(
            "function fixed() -> Badge {\n    return Badge {\n        name: \"A name far longer than twelve\"\n    }\n}\n"
        ),
        ["type.invalid_literal"]
    );
    // A named type built on `Text` doesn't widen to `Text`.
    assert_eq!(
        with_badges(
            "function nickname_badge(nickname: Nickname) -> Badge {\n    return Badge {\n        name: nickname\n    }\n}\n"
        ),
        ["type.mismatch"]
    );
    // A field without rules of its own: building never fails.
    assert_eq!(
        one(
            "type Label = Text {\n    max_length: 12\n}\n\ntype Tag = Object {\n    label: Label\n}\n\nfunction make_tag(label: Label) -> Tag {\n    return Tag {\n        label: label\n    }\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn entity_operations_fail_as_area_6_decides() {
    let todos = "entity Todo {\n    id: Uuid\n}\n\ntype TodoNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\n";
    // `Todo.get` fails with `TodoNotFound` when the project declares it.
    assert_eq!(
        one(&format!(
            "{todos}action load(todo_id: Todo.id) -> Todo {{\n    return Todo.get(todo_id)\n}}\n"
        )),
        ["failure.attempt_required"]
    );
    assert_eq!(
        one(&format!(
            "{todos}action load(todo_id: Todo.id) -> Todo {{\n    return attempt Todo.get(todo_id)\n}}\n"
        )),
        ["failure.undeclared_propagation"]
    );
    // Without one, the compiler provides it (area 6, "Write failures").
    assert_eq!(
        one(
            "entity Note {\n    id: Uuid\n}\n\naction load(note_id: Note.id) -> Note {\n    return attempt Note.get(note_id)\n}\n"
        ),
        ["failure.undeclared_propagation"]
    );
    assert!(
        one(
            "entity Note {\n    id: Uuid\n}\n\naction load(note_id: Note.id) fails NoteNotFound -> Note {\n    return attempt Note.get(note_id)\n}\n"
        )
        .is_empty()
    );
}

#[test]
fn writes_fail_with_what_they_can_break_or_miss() {
    let model = "type Name = Text {\n    max_length: 80\n}\n\nentity User {\n    id: Uuid\n    email: Email {\n        unique: true\n    }\n}\n\nentity Project {\n    id: Uuid\n    owner_id: User.id {\n        on_delete: OnDelete.cascade\n    }\n    name: Name\n}\n\nentity Todo {\n    id: Uuid\n    project_id: Project.id {\n        on_delete: OnDelete.restricted\n    }\n}\n\ntype TodosLeft = Failure {\n    kind: FailureKind.conflict\n}\n\n";
    // A reference read from a record loaded earlier names one that is
    // there, so the create can't fail; one given as a parameter may not.
    assert!(
        one(&format!(
            "{model}action start(owner: User, name: Project.name) -> Project {{\n    return Project.create {{\n        set: {{\n            owner_id: owner.id\n            name: name\n        }}\n    }}\n}}\n"
        ))
        .is_empty()
    );
    assert_eq!(
        one(&format!(
            "{model}action start(owner_id: User.id, name: Project.name) -> Project {{\n    return Project.create {{\n        set: {{\n            owner_id: owner_id\n            name: name\n        }}\n    }}\n}}\n"
        )),
        ["failure.attempt_required"]
    );
    assert!(
        one(&format!(
            "{model}action start(owner_id: User.id, name: Project.name) fails UserNotFound -> Project {{\n    return attempt Project.create {{\n        set: {{\n            owner_id: owner_id\n            name: name\n        }}\n    }}\n}}\n"
        ))
        .is_empty()
    );
    // Deleting a user cascades to its projects, which a todo's
    // `restricted` reference refuses: the delete maps it.
    assert_eq!(
        one(&format!(
            "{model}action leave(user: User) -> Unit {{\n    User.delete(user)\n}}\n"
        )),
        ["data.conflict_unmapped"]
    );
    assert!(
        one(&format!(
            "{model}action leave(user: User) fails TodosLeft -> Unit {{\n    attempt User.delete(user) {{\n        conflict: {{\n            Todo.project_id: TodosLeft\n        }}\n    }}\n}}\n"
        ))
        .is_empty()
    );
    // By `id`, no record is the not-found failure.
    assert!(
        one(&format!(
            "{model}action rename(project_id: Project.id, name: Project.name) fails ProjectNotFound -> Project {{\n    return attempt Project.update(project_id) {{\n        set: {{\n            name: name\n        }}\n    }}\n}}\n"
        ))
        .is_empty()
    );
    // An update of a loaded record that sets no unique field can't fail.
    assert_eq!(
        one(&format!(
            "{model}action rename(project: Project, name: Project.name) -> Project {{\n    return attempt Project.update(project) {{\n        set: {{\n            name: name\n        }}\n    }}\n}}\n"
        )),
        ["failure.attempt_infallible"]
    );
}

#[test]
fn each_mistake_gives_its_rule() {
    let lookup_cases: &[(&str, &str)] = &[
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"missing\"\n    }\n}\n",
            "failure.outcome_missing_arm",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"missing\"\n        LookupMissing -> \"again\"\n        LookupRefused -> \"refused\"\n    }\n}\n",
            "failure.outcome_duplicate_arm",
        ),
        (
            "type LookupTimedOut = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"missing\"\n        LookupRefused -> \"refused\"\n        LookupTimedOut -> \"timed out\"\n    }\n}\n",
            "failure.outcome_unknown_arm",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        _ -> \"fallback\"\n    }\n}\n",
            "failure.outcome_wildcard",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        LookupMissing -> \"missing\"\n        LookupRefused -> \"refused\"\n    }\n}\n",
            "failure.outcome_success_missing",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        success(other) -> other\n        LookupMissing -> \"missing\"\n        LookupRefused -> \"refused\"\n    }\n}\n",
            "failure.outcome_success_duplicate",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> propagate\n        LookupMissing -> \"missing\"\n        LookupRefused -> \"refused\"\n    }\n}\n",
            "failure.outcome_success_value_required",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"missing\"\n        LookupRefused -> \"refused\"\n        none -> \"nothing\"\n    }\n}\n",
            "failure.outcome_pattern_invalid",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    return lookup(key)\n}\n",
            "failure.attempt_required",
        ),
        (
            "function f(key: LookupKey) -> LookupResult {\n    var found: LookupResult = attempt lookup(key)\n    return found\n}\n",
            "failure.undeclared_propagation",
        ),
        (
            "function f(key: LookupKey) fails LookupMissing -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"missing\"\n        LookupRefused -> \"refused\"\n    }\n}\n",
            "failure.stale_declaration",
        ),
        (
            "function f(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {\n    return attempt lookup(key) {\n        missing: LookupMissing\n    }\n}\n",
            "failure.unknown_mapping",
        ),
        (
            "function f(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {\n    return attempt lookup(key) {\n        invalid: LookupMissing\n    }\n}\n",
            "failure.invalid_outside_construction",
        ),
    ];
    for (source, rule) in lookup_cases {
        assert_eq!(with_lookup(source), [*rule], "for:\n{source}");
    }
    let cases: &[(&str, &str)] = &[
        (
            "type CustomerNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\ntype CustomerMissing = Failure {\n    kind: FailureKind.not_found\n    code: \"customer_not_found\"\n}\n",
            "failure.duplicate_code",
        ),
        (
            "type Broken = Failure {\n    kind: FailureKind.not_found\n    code: \"internal_fault\"\n}\n",
            "failure.duplicate_code",
        ),
        (
            "type Reference = Text {}\n\ntype PaymentRefused = Failure {\n    kind: FailureKind.rejected\n    client: {\n        reference: Reference\n    }\n    internal: {\n        reference: Reference\n    }\n}\n",
            "failure.context_field_overlap",
        ),
        (
            "type Lookup = Text {}\n\ntype Missing = Failure {\n    kind: FailureKind.not_found\n    internal: {\n        lookup: Lookup\n    }\n}\n\naction find(lookup: Lookup) fails Missing -> Lookup {\n    reject Missing\n}\n",
            "failure.missing_context_field",
        ),
        (
            "type Lookup = Text {}\n\ntype Missing = Failure {\n    kind: FailureKind.not_found\n    internal: {\n        lookup: Lookup\n    }\n}\n\naction find(lookup: Lookup) fails Missing -> Lookup {\n    reject Missing {\n        lookup: lookup\n        search: lookup\n    }\n}\n",
            "failure.unknown_context_field",
        ),
        (
            "type Lookup = Text {}\n\ntype Missing = Failure {\n    kind: FailureKind.not_found\n    internal: {\n        lookup: Lookup\n    }\n}\n\naction find(lookup: Lookup) fails Missing -> Lookup {\n    reject Missing {\n        lookup: lookup\n        lookup: lookup\n    }\n}\n",
            "failure.duplicate_context_field",
        ),
        (
            "type Note = Text {}\n\ntype Count = Int {}\n\ntype Missing = Failure {\n    kind: FailureKind.not_found\n    internal: {\n        note: Note\n    }\n}\n\naction find(count: Count) fails Missing -> Note {\n    reject Missing {\n        note: count\n    }\n}\n",
            "type.mismatch",
        ),
        (
            "type Note = Text {}\n\ntype RegistrationClosed = Failure {\n    kind: FailureKind.conflict\n}\n\naction register(note: Note) -> Note {\n    reject RegistrationClosed\n}\n",
            "failure.undeclared_propagation",
        ),
        (
            "type Note = Text {}\n\ntype Refused = Failure {\n    kind: FailureKind.rejected\n}\n\naction complete(note: Note) fails Refused -> Note {\n    return note\n}\n",
            "failure.stale_declaration",
        ),
        (
            "type Note = Text {}\n\ntype Refused = Failure {\n    kind: FailureKind.rejected\n}\n\naction execute(note: Note) fails Refused, Refused -> Note {\n    reject Refused\n}\n",
            "failure.duplicate_declaration",
        ),
        (
            "type Missing = Failure {\n    message: \"Not found.\"\n}\n",
            "failure.kind_required",
        ),
        (
            "type Broken = Failure {\n    kind: FailureKind.internal_fault\n}\n",
            "failure.kind_runtime_only",
        ),
        (
            "type Missing = Failure {\n    kind: FailureKind.not_found\n    code: \"Not A Snake-Code\"\n}\n",
            "failure.code_format",
        ),
        (
            "type Missing = Failure {\n    kind: FailureKind.not_found\n    status: 404\n}\n",
            "type.unknown_setting",
        ),
        (
            "type Missing = Failure {\n    kind: NotFound\n}\n",
            "type.setting_value",
        ),
        (
            "type Missing = Failure {\n    kind: FailureKind.missing\n}\n",
            "type.unknown_enum_variant",
        ),
        (
            "type Missing = Failure {\n    kind: FailureKind.not_found\n    kind: FailureKind.conflict\n}\n",
            "type.constraint_repeated",
        ),
        (
            "type Note = Text {}\n\ntype Missing = Failure {\n    kind: FailureKind.not_found\n    client: {\n        note: Note {\n            optional: true\n        }\n    }\n}\n",
            "type.optional_outside_input",
        ),
        (
            "type Count = Int {}\n\nfunction plain(n: Count) -> Count {\n    return n\n}\n\nfunction wrapper(n: Count) -> Count {\n    return attempt plain(n)\n}\n",
            "failure.attempt_infallible",
        ),
        (
            "type Count = Int {}\n\nfunction plain(n: Count) -> Count {\n    return attempt Count(n + 1)\n}\n",
            "failure.attempt_infallible",
        ),
        (
            "type Note = Text {}\n\ntype Refused = Failure {\n    kind: FailureKind.rejected\n}\n\naction child(note: Note) fails Refused -> Note {\n    reject Refused\n}\n\naction parent(note: Note) fails Refused -> Note {\n    return child(note)\n}\n",
            "failure.attempt_required",
        ),
        (
            "type Count = Int {}\n\ntype TicketStatus = Enum {\n    open\n    closed\n}\n\nfunction priority(status: TicketStatus) -> Count {\n    return match status {\n        TicketStatus.open -> 1\n        TicketStatus.closed -> propagate\n    }\n}\n",
            "failure.propagate_placement",
        ),
        (
            "type LookupKey = Text {}\n\ntype LookupResult = Text {}\n\nfunction plain(key: LookupKey) -> LookupResult {\n    return \"ok\"\n}\n\nfunction needless(key: LookupKey) -> LookupResult {\n    return match plain(key) {\n        success(value) -> value\n    }\n}\n",
            "failure.outcome_infallible",
        ),
        (
            "type LookupKey = Text {}\n\nfunction not_a_call(key: LookupKey) -> LookupKey {\n    return match key {\n        success(value) -> value\n    }\n}\n",
            "failure.outcome_subject_required",
        ),
    ];
    for (source, rule) in cases {
        assert_eq!(one(source), [*rule], "for:\n{source}");
    }
    let quantity_cases: &[(&str, &str)] = &[
        (
            "type QuantityRefused = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction doubled(quantity: Quantity) fails QuantityRefused -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityRefused\n    }\n    return result\n}\n",
            "failure.mapping_kind",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        missing: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.unknown_mapping",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2)\n    return result\n}\n",
            "failure.rule_unmapped",
        ),
        (
            "function doubled(quantity: Quantity) -> Quantity {\n    return Quantity(quantity * 2)\n}\n",
            "failure.attempt_required",
        ),
        (
            "function doubled(quantity: Quantity) -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        invalid: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.undeclared_propagation",
        ),
    ];
    for (source, rule) in quantity_cases {
        assert_eq!(with_quantity(source), [*rule], "for:\n{source}");
    }
}

/// Types with more than one rule that can break, for mapping by rule.
const RULES: &str = "type Count = Int {}\n\ntype Amount = Decimal {\n    scale: 4\n}\n\ntype Small = Quantity {\n    max: 10\n}\n\ntype ItemPrice = Decimal {\n    scale: 2\n    min: 0.00\n}\n\ntype StockCode = Text {\n    max_length: 8\n    pattern: \"[A-Z]{3}-[0-9]{4}\"\n}\n\ntype Written = Text {\n    max_length: 8\n}\n\ntype QuantityTooSmall = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype SkuRejected = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn with_rules(source: &str) -> Vec<String> {
    with_quantity(&format!("{RULES}{source}"))
}

#[test]
fn construction_failures_are_mapped_by_rule() {
    // Each rule that can break has an arm, or `invalid:` covers it; a rule
    // the ranges prove can't break takes none.
    let clean = [
        // `quantity * 2` is at least 2, so only `max` can break.
        "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
        // `invalid:` alone covers every rule that can break.
        "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        invalid: QuantityTooLarge\n    }\n    return result\n}\n",
        // An arm for one rule, `invalid:` for the rest.
        "function counted(n: Count) fails QuantityTooLarge, QuantityTooSmall -> Quantity {\n    var result: Quantity = attempt Quantity(n) {\n        max: QuantityTooLarge\n        invalid: QuantityTooSmall\n    }\n    return result\n}\n",
        // An arm for each.
        "function counted(n: Count) fails QuantityTooLarge, QuantityTooSmall -> Quantity {\n    var result: Quantity = attempt Quantity(n) {\n        min: QuantityTooSmall\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
        // Rules come from the type and every type it builds on: `Small`'s
        // `max` and `Quantity`'s `min`.
        "function small(n: Count) fails QuantityTooLarge, QuantityTooSmall -> Small {\n    var result: Small = attempt Small(n) {\n        min: QuantityTooSmall\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
        // `quantity` is at least 1, so `Small` can break only its `max`.
        "function small(quantity: Quantity) fails QuantityTooLarge -> Small {\n    var result: Small = attempt Small(quantity) {\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
        // The compiler can't reason about a pattern, so it can always break;
        // the length is proved.
        "function parse(written: Written) fails SkuRejected -> StockCode {\n    var sku: StockCode = attempt StockCode(written) {\n        pattern: SkuRejected\n    }\n    return sku\n}\n",
        // `scale` rounds the value as it is kept, so it never breaks.
        "function price(amount: Amount) fails QuantityTooSmall -> ItemPrice {\n    var result: ItemPrice = attempt ItemPrice(amount) {\n        min: QuantityTooSmall\n    }\n    return result\n}\n",
    ];
    for source in clean {
        assert_eq!(with_rules(source), CLEAN, "for:\n{source}");
    }
    let cases: &[(&str, &str)] = &[
        (
            "function counted(n: Count) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(n) {\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.rule_unmapped",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {}\n    return result\n}\n",
            "failure.rule_unmapped",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge, QuantityTooSmall -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        min: QuantityTooSmall\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.rule_cannot_break",
        ),
        (
            "function price(amount: Amount) fails QuantityTooSmall, QuantityTooLarge -> ItemPrice {\n    var result: ItemPrice = attempt ItemPrice(amount) {\n        min: QuantityTooSmall\n        scale: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.rule_cannot_break",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge, QuantityTooSmall -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityTooLarge\n        invalid: QuantityTooSmall\n    }\n    return result\n}\n",
            "failure.invalid_covers_nothing",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        max: QuantityTooLarge\n        max: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.duplicate_mapping",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        pattern: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.unknown_mapping",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) {\n        maximum: QuantityTooLarge\n    }\n    return result\n}\n",
            "failure.unknown_mapping",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge\n    return result\n}\n",
            "syntax.mapping_block_required",
        ),
    ];
    for (source, rule) in cases {
        assert_eq!(with_rules(source), [*rule], "for:\n{source}");
    }
}

#[test]
fn invalid_covers_only_a_constructions_rules() {
    // An entity's operation or an integration call maps each of its own
    // conditions; `invalid:` is never their catch-all.
    assert_eq!(
        one(
            "entity Todo {\n    id: Uuid\n}\n\ntype TodoNotFound = Failure {\n    kind: FailureKind.not_found\n}\n\ntype TodoRefused = Failure {\n    kind: FailureKind.invalid_value\n}\n\naction load(todo_id: Todo.id) fails TodoNotFound, TodoRefused -> Todo {\n    return attempt Todo.get(todo_id) {\n        invalid: TodoRefused\n    }\n}\n"
        ),
        ["failure.invalid_outside_construction"]
    );
    assert_eq!(
        with_lookup(
            "type LookupInvalid = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction f(key: LookupKey) fails LookupMissing, LookupRefused, LookupInvalid -> LookupResult {\n    return attempt lookup(key) {\n        invalid: LookupInvalid\n    }\n}\n"
        ),
        ["failure.invalid_outside_construction"]
    );
}

#[test]
fn a_misnamed_failure_gives_one_error() {
    // Resolution reports the unknown name; nothing about the `fails` list
    // or the call follows from it.
    assert_eq!(
        with_lookup(
            "function f(key: LookupKey) fails LookupGone -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> reject LookupGone\n        LookupRefused -> \"refused\"\n    }\n}\n"
        ),
        ["semantic.unknown_name", "semantic.unknown_name"]
    );
}

#[test]
fn failure_sets_follow_rejects_attempts_and_arms() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[
            ("lookup.jadpo", LOOKUP),
            (
                "main.jadpo",
                "action relay(key: LookupKey) fails LookupMissing -> LookupResult {\n    match lookup(key) {\n        success(value) -> {\n            return value\n        }\n        LookupMissing -> propagate\n        LookupRefused -> {\n            return \"refused\"\n        }\n    }\n}\n",
            ),
        ],
    );
    assert!(jadpo_check::check(&db, project).is_empty());
    let [lookup, main] = project.files(&db).as_slice() else {
        panic!("two files")
    };
    let sets = jadpo_check::types::failure_sets(&db, project, *lookup);
    let set = &sets.callables["lookup"];
    assert_eq!(
        set.failures.iter().map(String::as_str).collect::<Vec<_>>(),
        ["LookupMissing", "LookupRefused"]
    );
    assert!(!set.open);
    let sets = jadpo_check::types::failure_sets(&db, project, *main);
    assert_eq!(
        sets.callables["relay"]
            .failures
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["LookupMissing"]
    );
}

fn executed(db: &Database, query: &str) -> usize {
    db.take_executions()
        .iter()
        .filter(|key| key.starts_with(&format!("{query}(")))
        .count()
}

#[test]
fn editing_a_body_reruns_only_its_own_failure_set() {
    let caller = "function lookup_twice(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {\n    var first: LookupResult = attempt lookup(key)\n    return first\n}\n";
    let mut db = Database::recording();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("lookup.jadpo", LOOKUP), ("main.jadpo", caller)],
    );
    let files = project.files(&db).clone();
    let check_sets = |db: &Database| {
        for &file in &files {
            jadpo_check::types::failure_sets(db, project, file);
        }
    };
    assert!(jadpo_check::check(&db, project).is_empty());
    check_sets(&db);
    let cold = db.take_executions();
    for query in [
        "callables",
        "failure_table",
        "typed_body",
        "failure_set",
        "failure_sets",
    ] {
        assert!(
            cold.iter().any(|key| key.starts_with(&format!("{query}("))),
            "{query} runs: {cold:?}"
        );
    }

    // A body edit in `lookup` keeps its `fails` list: the caller's checks
    // and failure set are reused.
    files[0].set_text(&mut db).to(LOOKUP.replace(
        "    return \"found\"\n",
        "    var found: LookupResult = \"found\"\n    return found\n",
    ));
    assert!(jadpo_check::check(&db, project).is_empty());
    check_sets(&db);
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(count("callables"), 0, "signatures are unchanged: {log:?}");
    assert_eq!(count("failure_table"), 0, "failures are unchanged: {log:?}");
    assert_eq!(count("typed_body"), 1, "only lookup's body: {log:?}");
    assert_eq!(
        count("failure_set"),
        1,
        "lookup's set is taken again: {log:?}"
    );
    assert_eq!(
        count("failure_sets"),
        0,
        "lookup's set came out equal, so nothing built from it reruns: {log:?}"
    );

    // An edit that changes what can leave `lookup` reruns its set alone:
    // the caller reads `lookup`'s `fails` list, which is unchanged.
    files[0].set_text(&mut db).to(LOOKUP.replace(
        "    if (key == \"refused\") {\n        reject LookupRefused\n    }\n",
        "",
    ));
    let problems: Vec<&str> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id)
        .collect();
    assert_eq!(problems, ["failure.stale_declaration"]);
    check_sets(&db);
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(count("typed_body"), 1, "only lookup's body: {log:?}");
    assert_eq!(count("failure_sets"), 1, "only lookup's set: {log:?}");

    // Removing a failure from `lookup`'s `fails` list changes what callers
    // handle: the caller is checked again.
    files[0].set_text(&mut db).to(LOOKUP
        .replace("fails LookupMissing, LookupRefused", "fails LookupMissing")
        .replace(
            "    if (key == \"refused\") {\n        reject LookupRefused\n    }\n",
            "",
        ));
    let problems: Vec<&str> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id)
        .collect();
    assert_eq!(problems, ["failure.stale_declaration"], "the caller's list");
    check_sets(&db);
    assert_eq!(executed(&db, "failure_sets"), 2);
}
