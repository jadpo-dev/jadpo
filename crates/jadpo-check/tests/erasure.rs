//! Area 15, "Erasure" (chunk P3): `User.erase(user_id)` erases a person,
//! deleting their record and deciding what happens to each record of
//! theirs by the reference that links it to a record erasure deletes: its
//! `erase:`, or what its `on_delete:` implies. A `restricted` reference
//! into records erasure deletes must say, a reference or marked field it
//! clears can be `none`, a record it keeps as it is has a `retention:`,
//! and an `erase:` it never reaches is refused. A record that belongs to
//! several people, those it refers to, keeps its marked fields until the
//! last of them is erased, and another person's own record loses only the
//! reference (chunks P4 and P4b). A person's own field that says
//! `erase: Erasure.retained` keeps their record field by field, every
//! other marked field cleared (chunk P4b), and only a query saying
//! `include_erased: true` finds what is kept, reading nothing else. As
//! policy sees it, an erasure is a `delete` of the person's record, and an
//! action erasing one runs at repeatable read.

use jadpo_db::{Database, Layout};

use crate::support;

/// The problems of `source`, after the settings, as `path rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", &with_settings(source))])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

/// The rules of each problem of `source`, after the settings, in order.
fn rules(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", &with_settings(source))])
}

fn with_settings(source: &str) -> String {
    format!("{SETTINGS}\n{source}")
}

const SETTINGS: &str = "config {
    invoice_retention: Duration {
        default: 2192d
    }
}
";

const CLEAN: [&str; 0] = [];

/// A person, and a type for their records' text.
const USER: &str = "type Caption = Text {
    max_length: 200
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
";

/// A user's todos, deleted with them, and each todo's attachments, which
/// cascade from it; comments kept without their author; and invoices kept
/// as they are until their own retention ends.
const RECORDS: &str = "entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    title: Caption
}

entity Attachment {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    caption: Caption {
        personal: PersonalData.identifying
    }
}

entity Comment {
    id: Uuid
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: Caption
    signature: Caption? {
        personal: PersonalData.identifying
    }
}

entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    billing_name: Caption {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}
";

/// An action erasing a user.
const FORGET: &str = "action forget(user_id: User.id) fails UserNotFound -> Unit {
    attempt User.erase(user_id)
}
";

fn project(parts: &[&str]) -> String {
    parts.join("\n")
}

#[test]
fn a_person_with_records_of_every_outcome_is_erased_cleanly() {
    assert_eq!(rules(&project(&[USER, RECORDS, FORGET])), CLEAN);
    assert_eq!(rules(&project(&[USER, FORGET])), CLEAN);
}

#[test]
fn a_restricted_reference_into_a_persons_records_says_what_erasure_does() {
    let source = project(&[
        USER,
        &RECORDS.replacen("        erase: Erasure.deleted\n", "", 1),
    ]);
    assert_eq!(
        problems(&source),
        [
            "main.jadpo data.erase_required: `Todo.owner_id` doesn't say what erasing a `User` does to it"
        ]
    );
}

#[test]
fn erasure_reaches_through_what_it_deletes_but_not_past_what_it_keeps() {
    let notes = "entity TodoNote {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.restricted
    }
}

entity Reaction {
    id: Uuid
    comment_id: Comment.id {
        on_delete: OnDelete.restricted
    }
}
";
    assert_eq!(
        problems(&project(&[USER, RECORDS, notes])),
        [
            "main.jadpo data.erase_required: `TodoNote.todo_id` doesn't say what erasing a `User` does to it"
        ],
        "a todo is deleted, so its notes need an outcome; a comment is kept, so its reactions stay with it"
    );
    assert_eq!(
        rules(&project(&[
            USER,
            RECORDS,
            &notes.replacen(
                "on_delete: OnDelete.restricted\n",
                "on_delete: OnDelete.restricted\n        erase: Erasure.deleted\n",
                1
            )
        ])),
        CLEAN
    );
}

#[test]
fn a_reference_erasure_clears_can_be_none() {
    let required = RECORDS.replace("customer_id: User.id? {", "customer_id: User.id {");
    assert_eq!(
        problems(&project(&[USER, &required])),
        ["main.jadpo data.erase_field_required: `Invoice.customer_id` can't be cleared"]
    );
    let anonymised = RECORDS
        .replace("customer_id: User.id? {", "customer_id: User.id {")
        .replace("erase: Erasure.retained", "erase: Erasure.anonymised");
    assert_eq!(
        rules(&project(&[USER, &anonymised])),
        ["data.erase_field_required", "data.erase_field_required"],
        "the reference and the marked billing name"
    );
}

#[test]
fn a_marked_field_of_a_record_kept_without_the_person_can_be_none() {
    let required = RECORDS.replace("signature: Caption? {", "signature: Caption {");
    assert_eq!(
        problems(&project(&[USER, &required])),
        ["main.jadpo data.erase_field_required: Erasing a `User` can't clear `Comment.signature`"]
    );
    // Unmarked fields stay as they are: the body isn't personal data.
    assert_eq!(rules(&project(&[USER, RECORDS])), CLEAN);
}

#[test]
fn a_record_kept_as_it_is_has_a_retention_of_its_own() {
    let forever = RECORDS.replace(
        "\n    retention: {\n        after: config.invoice_retention\n        since: issued_at\n    }\n",
        "",
    );
    assert_eq!(
        problems(&project(&[USER, &forever])),
        [
            "main.jadpo data.erase_retention_required: `Invoice` has no retention, so an invoice kept after erasure would never go"
        ]
    );
}

#[test]
fn an_erase_that_erasure_never_reaches_is_refused() {
    let category = "entity Category {
    id: Uuid
}

entity Tag {
    id: Uuid
    category_id: Category.id {
        on_delete: OnDelete.cascade
        erase: Erasure.deleted
    }
    label: Caption {
        erase: Erasure.deleted
    }
}
";
    assert_eq!(
        problems(&project(&[USER, category])),
        [
            "main.jadpo data.erase_unreached: Erasing a person never reaches `Tag.category_id`",
            "main.jadpo data.erase_unreached: `Tag.label` isn't a reference, so `erase:` does nothing",
        ]
    );
    let event = "type TodoAdded = Event {
    owner_id: User.id {
        erase: Erasure.deleted
    }
}
";
    assert_eq!(
        rules(&project(&[USER, event])),
        ["data.erase_unreached"],
        "an event is sent as it was written"
    );
}

/// Erasure deletes a person's record and what its references say go with
/// it, and walks on only through those: an `erase:` on a reference to a
/// record it keeps would never be read.
#[test]
fn an_erase_on_a_reference_to_a_record_erasure_keeps_is_refused() {
    let reactions = "entity Reaction {
    id: Uuid
    comment_id: Comment.id? {
        on_delete: OnDelete.cleared
        erase: Erasure.anonymised
    }
}
";
    assert_eq!(
        problems(&project(&[USER, RECORDS, reactions])),
        ["main.jadpo data.erase_unreached: Erasing a person never reaches `Reaction.comment_id`"]
    );
}

#[test]
fn erase_takes_an_erasure() {
    for value in ["true", "Erasure.forgotten", "OnDelete.cascade"] {
        let source = RECORDS.replacen("erase: Erasure.deleted", &format!("erase: {value}"), 1);
        assert_eq!(
            rules(&project(&[USER, &source])),
            ["data.erase_required", "type.setting_value"],
            "{value}"
        );
    }
}

/// A customer an insurer must keep part of: the policy number stays, and
/// the name and address go.
const CUSTOMER: &str = "type Caption = Text {
    max_length: 200
}

entity Customer {
    id: Uuid
    email: Email? {
        personal: PersonalData.contact
    }
    policy_number: Caption {
        personal: PersonalData.identifying
        erase: Erasure.retained
    }
    plan: Caption
    ended_at: Instant?

    retention: {
        after: config.invoice_retention
        since: ended_at
    }

    person: true
}

action forget(customer_id: Customer.id) fails CustomerNotFound -> Unit {
    attempt Customer.erase(customer_id)
}
";

#[test]
fn a_persons_own_field_keeps_their_record_field_by_field() {
    assert_eq!(problems(CUSTOMER), CLEAN);
    let db = Database::default();
    let source = with_settings(CUSTOMER);
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let erasures = jadpo_check::erasure::erasures(&db, project);
    let erasure = &erasures.people["Customer"];
    assert_eq!(erasure.deleted, ["Customer"]);
    assert_eq!(erasure.own.len(), 1, "{erasure:?}");
    assert_eq!(erasure.own[0].entity, 0);
    assert_eq!(erasure.own[0].retained, ["policy_number"]);
    assert_eq!(erasure.own[0].clears, ["email"], "the plan isn't marked");
    assert!(erasure.unbuilt.is_empty());
}

#[test]
fn a_person_with_no_retained_field_is_deleted() {
    let db = Database::default();
    let source = with_settings(&project(&[USER, FORGET]));
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let erasures = jadpo_check::erasure::erasures(&db, project);
    assert!(erasures.people["User"].own.is_empty());
}

#[test]
fn a_persons_own_field_says_only_that_it_is_retained() {
    for (value, message) in [
        (
            "Erasure.deleted",
            "`Customer.policy_number` is the person's own field, so only `Erasure.retained` means anything on it",
        ),
        (
            "Erasure.anonymised",
            "`Customer.policy_number` is the person's own field, so only `Erasure.retained` means anything on it",
        ),
    ] {
        let source = CUSTOMER.replace("erase: Erasure.retained", &format!("erase: {value}"));
        assert_eq!(
            problems(&source),
            [format!("main.jadpo data.erase_own_field: {message}")],
            "{value}"
        );
    }
}

#[test]
fn a_persons_unmarked_own_field_stays_without_saying_so() {
    let source = CUSTOMER.replace(
        "    plan: Caption\n",
        "    plan: Caption {\n        erase: Erasure.retained\n    }\n",
    );
    assert_eq!(
        problems(&source),
        [
            "main.jadpo data.erase_own_field: `Customer.plan` isn't marked `personal:`, so it stays without saying `erase:`"
        ]
    );
}

#[test]
fn a_persons_kept_record_says_when_it_goes() {
    let source = CUSTOMER.replace(
        "    retention: {\n        after: config.invoice_retention\n        since: ended_at\n    }\n\n",
        "",
    );
    assert_eq!(
        problems(&source),
        [
            "main.jadpo data.erase_retention_required: `Customer` has no retention, so what a customer's record keeps after erasure would never go"
        ]
    );
}

#[test]
fn a_persons_other_marked_fields_can_be_cleared() {
    let source = CUSTOMER.replace("email: Email? {", "email: Email {");
    assert_eq!(
        problems(&source),
        ["main.jadpo data.erase_field_required: Erasing a `Customer` can't clear `Customer.email`"]
    );
}

/// What someone signs in with must always have a value, so erasure can't
/// clear it, and the error says what can be done instead.
#[test]
fn what_a_kept_person_signs_in_with_stays_or_holds_nothing_personal() {
    let source = format!(
        "{}
principal {{
    user: {{
        subject: Customer.email
    }}
}}

authentication {{
    api_token: {{}}
}}
",
        CUSTOMER.replace(
            "email: Email? {\n        personal: PersonalData.contact",
            "email: Email {\n        unique: true\n        personal: PersonalData.contact"
        )
    );
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &with_settings(&source))],
    );
    let found = jadpo_check::check(&db, project);
    let problem = found
        .iter()
        .find(|problem| problem.rule.id == "data.erase_field_required")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert!(
        problem
            .next
            .as_ref()
            .is_some_and(|next| next.text.starts_with("Sign in with a field")),
        "{problem:?}"
    );
}

/// A reference on a person is a reference, as it is on any entity: its
/// `erase:` says what happens when what it refers to is erased.
#[test]
fn a_persons_own_reference_keeps_what_erase_means_on_a_reference() {
    let invited = CUSTOMER.replace(
        "    plan: Caption\n",
        "    plan: Caption\n    invited_by_id: Customer.id? {\n        on_delete: OnDelete.cleared\n        erase: Erasure.anonymised\n    }\n",
    );
    assert_eq!(problems(&invited), CLEAN);
    let db = Database::default();
    let source = with_settings(&invited);
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let erasures = jadpo_check::erasure::erasures(&db, project);
    let erasure = &erasures.people["Customer"];
    assert_eq!(erasure.own[0].retained, ["policy_number"]);
    assert_eq!(erasure.kept[0].entity, "Customer");
    assert!(
        erasure.kept[0].clears.is_empty(),
        "another person's own record loses only the reference"
    );
}

/// A reference that deletes the record when erasure deletes what it refers
/// to would take what must stay with it. One to the same entity reaches
/// only someone else whose record stays too.
#[test]
fn a_kept_person_isnt_deleted_with_another_record() {
    let household = |outcome: &str| {
        format!(
            "{}
entity Household {{
    id: Uuid
    owner_id: User.id {{
        on_delete: OnDelete.cascade
    }}
}}

{USER_ONLY}
",
            CUSTOMER.replace(
                "    plan: Caption\n",
                &format!(
                    "    plan: Caption\n    household_id: Household.id? {{\n        on_delete: OnDelete.cascade\n{outcome}    }}\n"
                )
            )
        )
    };
    assert_eq!(
        problems(&household("")),
        [
            "main.jadpo data.erase_own_field: `Customer.household_id` deletes a `Customer` that `Customer.policy_number` says stays"
        ]
    );
    let cleared = household("        erase: Erasure.anonymised\n");
    assert_eq!(problems(&cleared), CLEAN);
    let db = Database::default();
    let source = with_settings(&cleared);
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let erasures = jadpo_check::erasure::erasures(&db, project);
    assert!(
        erasures.people["User"].own.is_empty(),
        "a customer is kept when a user is erased, not erased with them"
    );
    let parent = CUSTOMER.replace(
        "    plan: Caption\n",
        "    plan: Caption\n    parent_id: Customer.id? {\n        on_delete: OnDelete.cascade\n    }\n",
    );
    assert_eq!(problems(&parent), CLEAN);
}

/// The history an `audit:` keeps would still hold what erasure clears, so
/// keeping an audited person's record isn't built yet.
#[test]
fn keeping_an_audited_persons_record_is_not_built_yet() {
    let source = CUSTOMER.replace("    person: true", "    audit: {}\n\n    person: true");
    let found = problems(&source);
    assert!(
        found.contains(
            &"main.jadpo toolchain.not_built_yet: Erasing a `Customer` here isn't built yet"
                .to_string()
        ),
        "{found:?}"
    );
}

/// A person whose records a customer's household belongs to.
const USER_ONLY: &str = "entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
";

/// What a query reads of a person whose record is kept.
fn with_query(query: &str) -> String {
    format!(
        "{CUSTOMER}
type KeptPolicy = Output {{
    pick: [Customer.id, Customer.policy_number]
}}

type CustomerCard = Output {{
    pick: [Customer.id, Customer.email]
}}

{query}
"
    )
}

#[test]
fn a_query_opts_in_to_what_erasure_keeps() {
    let source = with_query(
        "query kept_policy(number: Customer.policy_number) -> List<KeptPolicy> {
    from: Customer
    where: Customer.policy_number == number
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
",
    );
    assert_eq!(problems(&source), CLEAN);
    let db = Database::default();
    let source = with_settings(&source);
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let queries = jadpo_check::data::queries::query_models(&db, project);
    assert!(
        queries
            .get("kept_policy")
            .expect("the query")
            .include_erased
    );
}

#[test]
fn include_erased_takes_only_true() {
    for written in ["false", "1", "\"yes\""] {
        let source = with_query(&format!(
            "query kept_policy(number: Customer.policy_number) -> List<KeptPolicy> {{
    from: Customer
    where: Customer.policy_number == number
    order: [Customer.id asc]
    limit: 10
    include_erased: {written}
}}
"
        ));
        assert_eq!(
            problems(&source),
            ["main.jadpo type.setting_value: `include_erased:` takes only `true`"],
            "{written}"
        );
    }
}

#[test]
fn a_query_with_erased_people_reads_only_what_is_kept() {
    // Its result.
    let result = with_query(
        "query kept_cards() -> List<CustomerCard> {
    from: Customer
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
",
    );
    assert_eq!(
        problems(&result),
        [
            "main.jadpo query.include_erased_reads: `kept_cards` reads `Customer.email`, which isn't `erase: Erasure.retained`"
        ]
    );
    // Its conditions.
    let condition = with_query(
        "query kept_for(plan: Customer.plan) -> List<KeptPolicy> {
    from: Customer
    where: Customer.plan == plan
    order: [Customer.id asc]
    limit: 10
    include_erased: true
}
",
    );
    assert_eq!(
        problems(&condition),
        [
            "main.jadpo query.include_erased_reads: `kept_for` reads `Customer.plan`, which isn't `erase: Erasure.retained`"
        ]
    );
    // Its order.
    let order = with_query(
        "query kept_by_end() -> List<KeptPolicy> {
    from: Customer
    order: [Customer.ended_at asc]
    limit: 10
    include_erased: true
}
",
    );
    assert_eq!(
        problems(&order),
        [
            "main.jadpo query.include_erased_reads: `kept_by_end` reads `Customer.ended_at`, which isn't `erase: Erasure.retained`"
        ]
    );
    // An aggregate's group.
    let aggregate = with_query(
        "type PlanCount = Output {
    plan: Customer.plan
    customers: Int
}

query kept_counts() -> List<PlanCount> {
    from: Customer
    group: {
        plan: Customer.plan
    }
    aggregate: {
        customers: count()
    }
    order: [plan asc]
    limit: 10
    include_erased: true
}
",
    );
    assert_eq!(
        rules(&aggregate),
        ["query.include_erased_reads"],
        "{:?}",
        problems(&aggregate)
    );
}

#[test]
fn a_query_with_erased_people_brings_no_related_records() {
    let source = format!(
        "{CUSTOMER}
entity Claim {{
    id: Uuid
    customer_id: Customer.id? {{
        on_delete: OnDelete.cleared
    }}
}}

type ClaimRow = Output {{
    pick: [Claim.id]
}}

type KeptWithClaims = Output {{
    pick: [Customer.id, Customer.policy_number]
    claims: List<ClaimRow>
}}

query kept_with_claims(customer_id: Customer.id) -> KeptWithClaims? {{
    from: Customer
    where: Customer.id == customer_id
    include: {{
        claims: {{
            via: Claim.customer_id
            order: [Claim.id asc]
            limit: 5
        }}
    }}
    include_erased: true
}}
"
    );
    assert_eq!(
        problems(&source),
        [
            "main.jadpo query.include_erased_reads: `kept_with_claims` brings other records with what erasure keeps of a `Customer`"
        ]
    );
}

#[test]
fn include_erased_is_for_an_entity_that_keeps_someones_record() {
    let invoice = "type KeptRow = Output {
    pick: [User.id]
}

query everyone() -> List<KeptRow> {
    from: User
    order: [User.id asc]
    limit: 10
    include_erased: true
}
";
    assert_eq!(
        problems(&project(&[USER, invoice])),
        [
            "main.jadpo query.include_erased_unused: `everyone` includes erased people, and erasing a `User` keeps nothing to find"
        ]
    );
}

#[test]
fn erase_is_a_persons_operation() {
    let source = project(&[
        USER,
        RECORDS,
        "action forget_todo(todo_id: Todo.id) -> Unit {\n    Todo.erase(todo_id)\n}\n",
    ]);
    assert_eq!(
        problems(&source),
        ["main.jadpo semantic.unknown_callee: `Todo` has no operation `erase`"]
    );
    // A transition of that name on an entity that is no person is the
    // transition.
    let archived = "entity Note {
    id: Uuid
    gone_at: Instant?

    lifecycle: {
        visible: gone_at == none
        transitions: {
            erase: {
                set: {
                    gone_at: clock.now
                }
            }
        }
    }
}

action drop_note(note_id: Note.id) fails NoteNotFound -> Note {
    return attempt Note.erase(note_id)
}
";
    assert_eq!(rules(&project(&[USER, archived])), CLEAN);
    // On a person, the name is erasure's.
    let person = USER.replace(
        "    person: true",
        "    gone_at: Instant?\n\n    lifecycle: {\n        visible: gone_at == none\n        transitions: {\n            erase: {\n                set: {\n                    gone_at: clock.now\n                }\n            }\n        }\n    }\n\n    person: true",
    );
    assert_eq!(rules(&person), ["data.lifecycle_transition_name"]);
}

#[test]
fn erase_takes_the_persons_id_and_no_block() {
    let with = |body: &str| {
        rules(&project(&[
            USER,
            &format!(
                "action forget(user: User, user_id: User.id) fails UserNotFound -> Unit {{\n{body}}}\n"
            ),
        ]))
    };
    assert_eq!(with("    attempt User.erase(user_id)\n"), CLEAN);
    assert_eq!(with("    attempt User.erase(user)\n"), ["type.mismatch"]);
    assert_eq!(
        with("    attempt User.erase(user_id, user_id)\n"),
        ["type.argument_count"]
    );
    assert_eq!(
        with(
            "    attempt User.erase(user_id) {\n        set: {\n            email: \"ada@example.com\"\n        }\n    }\n"
        ),
        ["data.write_section_unexpected"]
    );
    let found = problems(&project(&[
        USER,
        "action forget(user: User) fails UserNotFound -> Unit {\n    attempt User.erase(user)\n}\n",
    ]));
    assert_eq!(
        found,
        ["main.jadpo type.mismatch: `User.erase` takes a `User.id`, and this is `User`"]
    );
}

#[test]
fn erase_fails_with_the_persons_not_found_failure() {
    let unhandled = project(&[
        USER,
        "action forget(user_id: User.id) -> Unit {\n    User.erase(user_id)\n}\n",
    ]);
    assert!(
        !rules(&unhandled).is_empty(),
        "a failure nothing handles is reported"
    );
}

#[test]
fn a_test_erases_through_an_action() {
    let found = support::problems(
        Layout::Loose,
        &[
            ("main.jadpo", &with_settings(&project(&[USER, FORGET]))),
            (
                "main.test.jadpo",
                "test \"a user is erased\" {\n    attempt User.erase(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\")\n}\n",
            ),
        ],
    );
    assert!(
        found.iter().any(
            |problem| problem.contains("semantic.not_callable: A test can't call `User.erase`")
        ),
        "{found:?}"
    );
}

#[test]
fn erasing_a_file_or_another_persons_own_record_is_built() {
    let app = "application {\n    connections: {\n        uploads: {\n            driver: Driver.disk\n            path: \"/var/lib/todos\"\n        }\n    }\n}\n";
    let scans = "entity Scan {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    file: File {
        store: uploads
        max_size: 5mb
        types: [FileType.pdf]
    }
}

entity Photo {
    id: Uuid
    taken_by_id: User.id? {
        on_delete: OnDelete.cleared
    }
    picture: File? {
        store: uploads
        max_size: 5mb
        types: [FileType.png]
        personal: PersonalData.identifying
    }
}
";
    let found = support::rules(
        Layout::Loose,
        &[
            ("app.jadpo", app),
            (
                "main.jadpo",
                &with_settings(&project(&[USER, RECORDS, scans, FORGET])),
            ),
        ],
    );
    assert_eq!(found, CLEAN, "a deleted record's file and a cleared one");
    let invited = USER.replace(
        "    person: true",
        "    invited_by_id: User.id? {\n        on_delete: OnDelete.cleared\n    }\n\n    person: true",
    );
    assert_eq!(rules(&project(&[&invited, FORGET])), CLEAN);
    let kept = invited.replace(
        "on_delete: OnDelete.cleared",
        "on_delete: OnDelete.restricted\n        erase: Erasure.deleted",
    );
    assert_eq!(rules(&project(&[&kept, FORGET])), CLEAN);
}

/// Records a user shares with others: a letter between two users, and a
/// reply on someone's todo.
const SHARED: &str = "entity Letter {
    id: Uuid
    from_id: User.id? {
        on_delete: OnDelete.cleared
    }
    to_id: User.id? {
        on_delete: OnDelete.cleared
    }
    signed_as: Caption? {
        personal: PersonalData.identifying
    }
}

entity Reply {
    id: Uuid
    todo_id: Todo.id? {
        on_delete: OnDelete.cleared
    }
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    signed_as: Caption? {
        personal: PersonalData.identifying
    }
}
";

/// A record that belongs to several people keeps its marked fields while
/// any other of them remains (area 15's notes, "Records that belong to
/// more than one person"): erasure looks for another person through the
/// record's own references to people, and nowhere else. Another person's
/// own record loses only the reference.
#[test]
fn a_record_belonging_to_several_people_is_cleared_only_by_the_last() {
    let invited = USER.replace(
        "    person: true",
        "    invited_by_id: User.id? {\n        on_delete: OnDelete.cleared\n    }\n\n    person: true",
    );
    let source = with_settings(&project(&[&invited, RECORDS, SHARED, FORGET]));
    assert_eq!(
        problems(&project(&[&invited, RECORDS, SHARED, FORGET])),
        CLEAN
    );
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let erasures = jadpo_check::erasure::erasures(&db, project);
    let user = &erasures.people["User"];
    let kept = |name: &str| {
        user.kept
            .iter()
            .find(|kept| kept.entity == name)
            .unwrap_or_else(|| panic!("{name} kept"))
    };
    let through = |name: &str| -> Vec<(String, String)> { kept(name).through.clone() };
    // Another person's own record loses only the reference.
    assert!(kept("User").clears.is_empty(), "{:?}", kept("User"));
    assert!(kept("User").through.is_empty());
    // A comment belongs to its author alone, so it is cleared with them.
    assert_eq!(kept("Comment").clears, ["signature"]);
    assert!(kept("Comment").through.is_empty());
    // A letter belongs to both its users.
    assert_eq!(kept("Letter").clears, ["signed_as"]);
    assert_eq!(
        through("Letter"),
        [
            ("from_id".to_string(), "User".to_string()),
            ("to_id".to_string(), "User".to_string())
        ]
    );
    // A reply belongs to its author alone: its todo's owner isn't someone
    // it refers to, so only its author keeps its marked fields.
    assert_eq!(kept("Reply").clears, ["signed_as"]);
    assert_eq!(
        through("Reply"),
        [("author_id".to_string(), "User".to_string())]
    );
}

#[test]
fn an_action_erasing_a_person_runs_at_repeatable_read() {
    let db = Database::default();
    let source = with_settings(&project(&[USER, RECORDS, FORGET]));
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let decided = jadpo_check::types::isolation(&db, project);
    let forget = decided
        .iter()
        .find(|(key, _)| key.name() == "forget")
        .map(|(_, decided)| decided)
        .expect("forget's isolation");
    assert_eq!(forget.isolation, jadpo_std::Isolation::RepeatableRead);
    assert_eq!(
        forget.because,
        Some(jadpo_check::types::Loads::Erases("User".to_string()))
    );
}
