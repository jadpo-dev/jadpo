//! The tenant background work names (area 9, "Background work"; chunk
//! 6.6c): an event's tenant field, and each value a batch's run saves of
//! the tenant's `id` type, come from a record of that tenant the action
//! read or wrote under its caller's policy; otherwise
//! `policy.event_tenant_unproved`, so a caller can't aim system work at a
//! tenant they don't belong to.

use crate::support;

use jadpo_db::Layout;

/// A household application: people, households and their memberships,
/// chores, `Nudged`, a page of chores and `tick`.
const BASE: &str = r#"type Handle = Text {
    min_length: 3
    max_length: 40
}

type Title = Text {
    min_length: 1
    max_length: 80
}

type HomeRole = Enum {
    member
}

type SweepPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
        tenant: Tenancy.shared
    }
}

principal {
    user: {
        subject: Person.handle
    }
}

entity Household {
    id: Uuid
    name: Title

    policy: {
        read: [HomeRole.member, Access.system]
    }
}

entity HomeMembership {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    person_id: Person.id {
        on_delete: OnDelete.cascade
    }
    role: HomeRole

    membership: {
        scope: household_id
        member: person_id
        role: role
    }

    policy: {
        create: [HomeRole.member]
        read: [person_id]
    }
}

entity Chore {
    id: Uuid
    household_id: Household.id {
        on_delete: OnDelete.cascade
    }
    title: Title {
        immutable: true
    }
    ticks: Int

    policy: {
        create: [HomeRole.member]
        read: [HomeRole.member, Access.system]
        update: [Access.system]
    }
}

type Nudged = Event {
    household_id: Household.id
}

policy {
    tenant: Household
}

query all_chores(page: SweepPage) -> Page<Chore> {
    from: Chore
    order: [Chore.title asc]
    page: {
        size: page.page_size
        after: page.after
    }
}

action tick(chore: Chore) fails ChoreNotFound -> Unit {
    attempt Chore.update(chore.id) {
        set: {
            ticks: Chore.ticks + 1
        }
    }
}

"#;

/// The `policy.event_tenant_unproved` problems `actions` give beside
/// [`BASE`], each with what it says.
fn unproved(actions: &str) -> Vec<String> {
    let source = format!("{BASE}\n{actions}");
    support::problems(Layout::Loose, &[("main.jadpo", source.as_str())])
        .into_iter()
        .filter(|problem| problem.contains("policy.event_tenant_unproved"))
        .collect()
}

#[test]
fn an_event_naming_a_tenant_its_caller_passed_is_refused() {
    // The soundness review's case: Bob, a member of Birch only, passes
    // Ash's id, and the subscriber would run as the system in Ash.
    let nudge = "action nudge(household_id: Household.id) -> Unit {\n    emit Nudged {\n        household_id: household_id\n    }\n}\n";
    assert_eq!(
        unproved(nudge),
        [
            "main.jadpo policy.event_tenant_unproved: `nudge` sends `Nudged` to a `Household` it never reached"
        ]
    );
    // Read through the caller's membership first: a caller who isn't a
    // member of it finds no household, and the action fails.
    let read = "action nudge(household_id: Household.id) fails HouseholdNotFound -> Unit {\n    var household: Household = attempt Household.get(household_id)\n    emit Nudged {\n        household_id: household.id\n    }\n}\n";
    assert_eq!(unproved(read), Vec::<String>::new());
}

#[test]
fn a_record_read_or_written_names_its_tenant() {
    // A chore changed under the caller's grants is of a household they
    // belong to, and so is one they created there.
    let changed = "action nudge(chore_id: Chore.id) fails ChoreNotFound -> Unit {\n    var chore: Chore = attempt Chore.update(chore_id) {\n        set: {\n            ticks: Chore.ticks + 1\n        }\n    }\n    emit Nudged {\n        household_id: chore.household_id\n    }\n}\n";
    assert_eq!(unproved(changed), Vec::<String>::new());
    let created = "action nudge(household_id: Household.id, title: Title) fails HouseholdNotFound -> Unit {\n    var chore: Chore = attempt Chore.create {\n        set: {\n            household_id: household_id\n            title: title\n            ticks: 0\n        }\n    }\n    emit Nudged {\n        household_id: chore.household_id\n    }\n}\n";
    assert_eq!(unproved(created), Vec::<String>::new());
    // The id the create was given proves it too, before the create or
    // after: the action keeps its event only if the create succeeds.
    let before = "action nudge(household_id: Household.id, title: Title) fails HouseholdNotFound -> Unit {\n    emit Nudged {\n        household_id: household_id\n    }\n    var chore: Chore = attempt Chore.create {\n        set: {\n            household_id: household_id\n            title: title\n            ticks: 0\n        }\n    }\n    assert chore.ticks == 0\n}\n";
    assert_eq!(unproved(before), Vec::<String>::new());
    // A record handed whole was read by whoever handed it.
    let handed = "action nudge(chore: Chore) -> Unit {\n    emit Nudged {\n        household_id: chore.household_id\n    }\n}\n";
    assert_eq!(unproved(handed), Vec::<String>::new());
}

#[test]
fn a_create_proves_its_tenant_only_when_it_always_runs() {
    // A create that may not run proves nothing when it doesn't.
    let sometimes = "action nudge(household_id: Household.id, title: Title, add: Bool) fails HouseholdNotFound -> Unit {\n    if (add) {\n        var chore: Chore = attempt Chore.create {\n            set: {\n                household_id: household_id\n                title: title\n                ticks: 0\n            }\n        }\n        assert chore.ticks == 0\n    }\n    emit Nudged {\n        household_id: household_id\n    }\n}\n";
    assert_eq!(
        unproved(sometimes),
        [
            "main.jadpo policy.event_tenant_unproved: `nudge` sends `Nudged` to a `Household` it never reached"
        ]
    );
    // Nor does one after a `return` that may leave first.
    let after_return = "action nudge(household_id: Household.id, title: Title, add: Bool) fails HouseholdNotFound -> Unit {\n    emit Nudged {\n        household_id: household_id\n    }\n    if (add) {\n        return\n    }\n    var chore: Chore = attempt Chore.create {\n        set: {\n            household_id: household_id\n            title: title\n            ticks: 0\n        }\n    }\n    assert chore.ticks == 0\n}\n";
    assert_eq!(
        unproved(after_return),
        [
            "main.jadpo policy.event_tenant_unproved: `nudge` sends `Nudged` to a `Household` it never reached"
        ]
    );
}

#[test]
fn a_subscribers_action_passing_its_events_tenant_on_reads_it_first() {
    // A subscriber's action is an action a route may call too, so the id
    // it is passed proves nothing there.
    let relay = "subscriber noted {\n    on: Nudged\n    run: relay(event.household_id)\n}\n\naction relay(household_id: Household.id) -> Unit {\n    emit Nudged {\n        household_id: household_id\n    }\n}\n";
    assert_eq!(
        unproved(relay),
        [
            "main.jadpo policy.event_tenant_unproved: `relay` sends `Nudged` to a `Household` it never reached"
        ]
    );
}

#[test]
fn a_batch_naming_a_tenant_its_caller_passed_is_refused() {
    // The review's batch case: the run's `key:` limits it to the household
    // the caller named.
    let sweep = "action sweep_house(household_id: Household.id, page: SweepPage) -> BatchRun {\n    return batches.each(from: all_chores(page), each: chore -> tick(chore), key: household_id)\n}\n";
    assert_eq!(
        unproved(sweep),
        [
            "main.jadpo policy.event_tenant_unproved: `sweep_house` starts a batch in a `Household` it never reached"
        ]
    );
    // Each saved value naming it is reported: here a capture as well.
    let captured = "action keep(chore: Chore, household_id: Household.id) -> Unit {\n    assert chore.household_id == household_id\n}\n\naction sweep_house(household_id: Household.id, page: SweepPage) -> BatchRun {\n    return batches.each(from: all_chores(page), each: chore -> keep(chore, household_id), key: household_id)\n}\n";
    assert_eq!(unproved(captured).len(), 2);
    let read = "action sweep_house(household_id: Household.id, page: SweepPage) fails HouseholdNotFound -> BatchRun {\n    var household: Household = attempt Household.get(household_id)\n    return batches.each(from: all_chores(page), each: chore -> tick(chore), key: household.id)\n}\n";
    assert_eq!(unproved(read), Vec::<String>::new());
}
