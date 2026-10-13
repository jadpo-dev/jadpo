//! Area 4, failures, and the failure side of area 3: the decided examples
//! check cleanly, each error is caught on its own mistake, one mistake
//! gives one error, each callable's failure set is a query of its own, and
//! a body edit reruns only the failure sets that depend on it.

mod support;

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
            "function line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)\n    return total\n}\n\nfunction doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge\n    return result\n}\n\nfunction halved(quantity: Quantity) -> Pence {\n    return Pence(quantity / 2)\n}\n\nfunction capped(quantity: Quantity) -> Quantity {\n    return Quantity(quantity)\n}\n"
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
            "type Short = Text {\n    max_length: 10\n}\n\ntype Long = Text {\n    max_length: 80\n}\n\nfunction widen(text: Short) -> Long {\n    return Long(text)\n}\n\nfunction narrow(text: Long) -> Short {\n    return Short(text)\n}\n"
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
    // Without one, the compiler provides it (area 6): nothing is known.
    assert_eq!(
        one(
            "entity Note {\n    id: Uuid\n}\n\naction load(note_id: Note.id) -> Note {\n    return attempt Note.get(note_id)\n}\n"
        ),
        CLEAN
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
            "function f(key: LookupKey) fails LookupMissing, LookupRefused -> LookupResult {\n    return attempt lookup(key) invalid: LookupMissing\n}\n",
            "failure.unknown_mapping",
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
            "type QuantityRefused = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction doubled(quantity: Quantity) fails QuantityRefused -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityRefused\n    return result\n}\n",
            "failure.mapping_kind",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) missing: QuantityTooLarge\n    return result\n}\n",
            "failure.unknown_mapping",
        ),
        (
            "function doubled(quantity: Quantity) fails QuantityTooLarge -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2)\n    return result\n}\n",
            "failure.invalid_mapping_required",
        ),
        (
            "function doubled(quantity: Quantity) -> Quantity {\n    return Quantity(quantity * 2)\n}\n",
            "failure.attempt_required",
        ),
        (
            "function doubled(quantity: Quantity) -> Quantity {\n    var result: Quantity = attempt Quantity(quantity * 2) invalid: QuantityTooLarge\n    return result\n}\n",
            "failure.undeclared_propagation",
        ),
    ];
    for (source, rule) in quantity_cases {
        assert_eq!(with_quantity(source), [*rule], "for:\n{source}");
    }
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
    db.take_executions();

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
    assert_eq!(count("body_check"), 1, "only lookup's body: {log:?}");
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
    assert_eq!(count("body_check"), 1, "only lookup's body: {log:?}");
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
