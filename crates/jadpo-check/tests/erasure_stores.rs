//! Area 15, "What erasure reaches" and "What erasure doesn't rewrite"
//! (chunk P4c): a reference to a person may cross stores when it says
//! `erase:` and has no `on_delete:`, which isn't a foreign key; anything
//! else across stores stays `data.reference_across_stores`. Nothing reads
//! through such a reference, and a person referred to that way goes only by
//! erasure. `PersonErased` is a built-in event an app may subscribe to. An
//! erasure needs `privacy: { erasure_log: { store: } }`, a store that holds
//! no person's records, or a store of files, and that is all `privacy:`
//! holds yet.

use jadpo_db::{Database, Layout};

use crate::support;

const APP: &str = "application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
        erasures: {
            driver: Driver.postgres
        }
        scans: {
            driver: Driver.disk
            path: \"/var/lib/app/scans\"
        }
    }
    schedules: {
        store: main
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
}
";

const CONFIG: &str = "config {
    kept_for: Duration {
        default: 365d
    }
}
";

const USER: &str = "/// Someone with an account.
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    policy: {
        delete: [Access.public]
    }

    person: true
}
";

const NOTE: &str = "use accounts/entities { User }

/// What a person wrote, kept in the archive.
entity OldNote {
    id: Uuid
    owner_id: User.id {
        erase: Erasure.deleted
    }
    words: Email {
        personal: PersonalData.identifying
    }

    store: archive
}
";

const CLOSE: &str = "use accounts/entities { User, UserNotFound }

/// Erases someone.
action close(id: User.id) fails UserNotFound -> Unit {
    attempt User.erase(id)
}
";

const USER_PATH: &str = "domains/accounts/entities/user.jadpo";
const NOTE_PATH: &str = "domains/accounts/entities/old_note.jadpo";
const CLOSE_PATH: &str = "domains/accounts/actions/close.jadpo";

/// The project, each of `changed` replacing the file at its path or added;
/// an empty text removes it.
fn files<'a>(changed: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        ("config.jadpo", CONFIG),
        (USER_PATH, USER),
        (NOTE_PATH, NOTE),
        (CLOSE_PATH, CLOSE),
    ];
    for (path, text) in changed {
        out.retain(|(at, _)| at != path);
        if !text.is_empty() {
            out.push((path, text));
        }
    }
    out
}

fn problems(changed: &[(&str, &str)]) -> Vec<String> {
    support::problems(Layout::Project, &files(changed))
}

fn rules(changed: &[(&str, &str)]) -> Vec<String> {
    support::rules(Layout::Project, &files(changed))
}

const NONE: [&str; 0] = [];

#[test]
fn a_reference_to_a_person_crosses_stores_saying_what_erasure_does() {
    assert_eq!(problems(&[]), NONE);
    let kept = "use accounts/entities { User }

/// What a person wrote, kept in the archive without them.
entity OldNote {
    id: Uuid
    owner_id: User.id? {
        erase: Erasure.anonymised
    }
    words: Email? {
        personal: PersonalData.identifying
    }

    store: archive
}
";
    assert_eq!(problems(&[(NOTE_PATH, kept)]), NONE);
    let retained = "use accounts/entities { User }

/// A bill, kept in the archive for the tax years.
entity OldNote {
    id: Uuid
    owner_id: User.id? {
        erase: Erasure.retained
    }
    issued_at: Instant

    retention: {
        after: config.kept_for
        since: issued_at
    }

    store: archive
}
";
    assert_eq!(problems(&[(NOTE_PATH, retained)]), NONE);
}

/// It isn't a foreign key, so no table holds one, and it is indexed all the
/// same, since erasure looks records up by it.
#[test]
fn a_reference_across_stores_is_indexed_and_no_foreign_key() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&[]));
    let schema = jadpo_check::data::schema(&db, project);
    let table = schema
        .stores
        .get("archive")
        .and_then(|store| store.tables.get("old_note"))
        .expect("the table");
    assert!(table.foreign_keys.is_empty(), "{:?}", table.foreign_keys);
    assert!(
        table
            .indexes
            .values()
            .any(|index| index.columns == ["owner_id"]),
        "{:?}",
        table.indexes
    );
}

#[test]
fn anything_else_across_stores_is_refused() {
    let says_on_delete = "use accounts/entities { User }

/// A note.
entity OldNote {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.cascade
        erase: Erasure.deleted
    }

    store: archive
}
";
    assert_eq!(
        rules(&[(NOTE_PATH, says_on_delete)]),
        ["data.reference_across_stores"]
    );
    let says_nothing = "use accounts/entities { User }

/// A note.
entity OldNote {
    id: Uuid
    owner_id: User.id

    store: archive
}
";
    assert_eq!(
        rules(&[(NOTE_PATH, says_nothing)]),
        ["data.reference_across_stores"],
        "not asked for an `on_delete:` it may not have"
    );
    let not_a_person = "/// Somewhere a person might live.
entity Town {
    id: Uuid
}
";
    let to_a_town = "use accounts/entities { Town }

/// A note.
entity OldNote {
    id: Uuid
    town_id: Town.id {
        erase: Erasure.deleted
    }

    store: archive
}
";
    let found = rules(&[
        ("domains/accounts/entities/town.jadpo", not_a_person),
        (NOTE_PATH, to_a_town),
    ]);
    assert!(
        found.contains(&"data.reference_across_stores".to_string()),
        "{found:?}"
    );
}

/// In one store a reference is a foreign key, and `erase:` doesn't stand in
/// for its `on_delete:`.
#[test]
fn in_one_store_a_reference_still_says_on_delete() {
    let same_store = "use accounts/entities { User }

/// A note.
entity OldNote {
    id: Uuid
    owner_id: User.id {
        erase: Erasure.deleted
    }
}
";
    assert_eq!(
        rules(&[(NOTE_PATH, same_store)]),
        ["data.on_delete_required"]
    );
}

/// What erasure checks of a reference holds across stores too.
#[test]
fn erasures_rules_hold_across_stores() {
    let not_nullable = "use accounts/entities { User }

/// A note.
entity OldNote {
    id: Uuid
    owner_id: User.id {
        erase: Erasure.anonymised
    }

    store: archive
}
";
    assert_eq!(
        rules(&[(NOTE_PATH, not_nullable)]),
        ["data.erase_field_required"]
    );
    let no_retention = "use accounts/entities { User }

/// A note.
entity OldNote {
    id: Uuid
    owner_id: User.id? {
        erase: Erasure.retained
    }

    store: archive
}
";
    assert_eq!(
        rules(&[(NOTE_PATH, no_retention)]),
        ["data.erase_retention_required"]
    );
}

/// A record erasure keeps in another store, audited, would keep the person
/// in its history: refused where it is erased, as in one store.
#[test]
fn keeping_an_audited_record_in_another_store_is_not_built_yet() {
    let audited = "use accounts/entities { User }

/// A note.
entity OldNote {
    id: Uuid
    owner_id: User.id? {
        erase: Erasure.anonymised
    }
    written_at: Instant

    audit: {
        retention: config.kept_for
    }

    store: archive
}
";
    let found = rules(&[(NOTE_PATH, audited)]);
    assert!(
        found.contains(&"toolchain.not_built_yet".to_string()),
        "{found:?}"
    );
}

/// No statement reads two databases, so nothing reads through it.
#[test]
fn nothing_reads_through_a_reference_across_stores() {
    let include = "use accounts/entities { OldNote }

/// Each note with who wrote it.
query notes() -> List<OldNote> {
    from: OldNote
    include: {
        owner: {
            via: OldNote.owner_id
        }
    }
    order: [OldNote.id asc]
    limit: 10
}
";
    let found = problems(&[("domains/accounts/queries/notes.jadpo", include)]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("query.include_unknown")
                && problem.contains("another store")),
        "{found:?}"
    );
    let children = "use accounts/entities { OldNote, User }

/// Someone with what they wrote.
query with_notes(id: User.id) -> User {
    from: User
    where: User.id == id
    include: {
        notes: {
            via: OldNote.owner_id
            order: [OldNote.id asc]
            limit: 10
        }
    }
    missing: UserNotFound
}
";
    let found = problems(&[("domains/accounts/queries/with_notes.jadpo", children)]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("query.include_unknown")
                && problem.contains("another store")),
        "{found:?}"
    );
}

/// A role is read in the one statement that reads the record, so what holds
/// it can't refer across stores.
#[test]
fn a_membership_doesnt_refer_across_stores() {
    let roles = "/// What someone may do.
type DeskRole = Enum {
    clerk
}
";
    let membership = "use accounts/entities { User }
use accounts/types { DeskRole }

/// Who may work the desk.
entity Clerk {
    id: Uuid
    user_id: User.id {
        erase: Erasure.deleted
    }
    role: DeskRole

    membership: {
        member: user_id
        scope: MembershipScope.application
        role: role
    }

    store: archive
}
";
    let app = format!(
        "use accounts/entities {{ User }}\n\n{APP}\nprincipal {{\n    user: {{\n        subject: User.email\n    }}\n}}\n\nauthentication {{\n    api_token: {{}}\n}}\n"
    );
    let unique = USER.replace(
        "        personal: PersonalData.contact\n",
        "        unique: true\n        personal: PersonalData.contact\n",
    );
    let found = problems(&[
        ("app.jadpo", &app),
        (USER_PATH, &unique),
        ("domains/accounts/types/desk_role.jadpo", roles),
        ("domains/accounts/entities/clerk.jadpo", membership),
    ]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("clerk.jadpo") && problem.contains("another store")),
        "{found:?}"
    );
}

/// Erasure applies a reference across stores and nothing else does: no
/// foreign key would, so a person another store refers to goes only by
/// erasure, whatever else could delete them.
#[test]
fn a_person_another_store_refers_to_goes_only_by_erasure() {
    let delete = "use accounts/entities { User, UserNotFound }

/// Deletes someone.
action remove(id: User.id) fails UserNotFound -> Unit {
    attempt User.delete(id)
}
";
    assert_eq!(
        rules(&[("domains/accounts/actions/remove.jadpo", delete)]),
        ["data.person_removal_across_stores"]
    );
    let retained = "/// Someone with an account, deleted after a while.
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }
    joined_at: Instant

    policy: {
        delete: [Access.public]
    }

    person: true

    retention: {
        after: config.kept_for
        since: joined_at
    }
}
";
    assert_eq!(
        rules(&[(USER_PATH, retained)]),
        ["data.person_removal_across_stores"]
    );
    let purged = "/// Someone with an account, who can leave.
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }
    left_at: Instant?

    policy: {
        delete: [Access.public]
    }

    lifecycle: {
        visible: left_at == none
        transitions: {
            delete: {
                when: left_at == none
                set: {
                    left_at: clock.now
                }
            }
        }
        purge: {
            after: config.kept_for
            since: left_at
        }
    }

    person: true
}
";
    assert_eq!(
        rules(&[(USER_PATH, purged)]),
        ["data.person_removal_across_stores"]
    );
    // Without the purge a delete hides them, and erasure still finds them.
    let hidden = purged.replace(
        "        purge: {\n            after: config.kept_for\n            since: left_at\n        }\n",
        "",
    );
    assert_eq!(problems(&[(USER_PATH, &hidden)]), NONE);
    // Through a cascade too.
    let town = "/// Somewhere people live.
entity Town {
    id: Uuid
    founded_at: Instant

    retention: {
        after: config.kept_for
        since: founded_at
    }
}
";
    let in_town = "use accounts/entities { Town }

/// Someone in a town, deleted with it.
entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }
    town_id: Town.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        delete: [Access.public]
    }

    person: true
}
";
    assert_eq!(
        rules(&[
            ("domains/accounts/entities/town.jadpo", town),
            (USER_PATH, in_town),
        ]),
        ["data.person_removal_across_stores"]
    );
}

/// An erasure leaves an entry in the log, so an app that erases names where
/// it is kept.
#[test]
fn an_erasure_needs_a_log() {
    let without = APP.replace(
        "    privacy: {\n        erasure_log: {\n            store: erasures\n        }\n    }\n",
        "",
    );
    let found = problems(&[("app.jadpo", &without)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("data.erasure_log_required"), "{found:?}");
    assert!(found[0].contains(CLOSE_PATH), "{found:?}");
    // Nothing erases, so nothing is logged.
    assert_eq!(problems(&[("app.jadpo", &without), (CLOSE_PATH, "")]), NONE);
}

#[test]
fn the_log_is_kept_where_a_restore_cant_take_it_back() {
    let in_store = |store: &str| APP.replace("store: erasures", &format!("store: {store}"));
    // A person's own database, and one that holds records of theirs.
    assert_eq!(
        rules(&[("app.jadpo", &in_store("main"))]),
        ["data.erasure_log_store"]
    );
    assert_eq!(
        rules(&[("app.jadpo", &in_store("archive"))]),
        ["data.erasure_log_store"]
    );
    // Nothing of that name.
    assert_eq!(
        rules(&[("app.jadpo", &in_store("erasure"))]),
        ["data.erasure_log_store"]
    );
    // A store of files is out of a database restore's reach.
    assert_eq!(problems(&[("app.jadpo", &in_store("scans"))]), NONE);
    // A store that holds entities, none of them anyone's, will do.
    let town = "/// Somewhere.
entity Town {
    id: Uuid

    store: erasures
}
";
    assert_eq!(
        problems(&[("domains/accounts/entities/town.jadpo", town)]),
        NONE
    );
    let log_kept_with_queue = APP.replace(
        "        scans: {",
        "        cache: {\n            driver: Driver.redis\n        }\n        scans: {",
    );
    assert_eq!(
        rules(&[(
            "app.jadpo",
            &log_kept_with_queue.replace("store: erasures", "store: cache")
        )]),
        ["data.erasure_log_store"],
        "neither a database nor a store of files"
    );
}

#[test]
fn privacy_holds_only_the_erasure_log_yet() {
    let routes = APP.replace(
        "    privacy: {\n",
        "    privacy: {\n        export_link: \"/data\"\n",
    );
    assert_eq!(
        rules(&[("app.jadpo", &routes)]),
        ["toolchain.not_built_yet"]
    );
    let not_a_store = APP.replace("store: erasures", "folder: erasures");
    let found = rules(&[("app.jadpo", &not_a_store)]);
    assert!(
        found.contains(&"data.erasure_log_store".to_string()),
        "{found:?}"
    );
}

/// The log's table is in the store it names, which migrations create, and
/// in no other.
#[test]
fn the_logs_table_is_in_the_store_it_names() {
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files(&[]));
    let schema = jadpo_check::data::schema(&db, project);
    let table = jadpo_std::system::ERASURE_LOG.table;
    let holding: Vec<&String> = schema
        .stores
        .iter()
        .filter(|(_, store)| store.tables.contains_key(table))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(holding, ["erasures"]);
    // A store of files holds it as files.
    let db = Database::default();
    let in_files = APP.replace("store: erasures", "store: scans");
    let project = support::project(&db, Layout::Project, &files(&[("app.jadpo", &in_files)]));
    let schema = jadpo_check::data::schema(&db, project);
    assert!(
        schema
            .stores
            .values()
            .all(|store| !store.tables.contains_key(table))
    );
}

const DEPARTURE: &str = "/// Someone gone, noted.
entity Departure {
    id: Uuid
    who: Uuid
    at: Instant

    store: erasures

    policy: {
        create: [Access.system]
    }
}
";

const FORGET: &str = "use accounts/entities { Departure }

/// Says someone has gone.
action forget(event: PersonErased) -> Departure {
    return Departure.create {
        set: {
            who: event.id
            at: event.erased_at
        }
    }
}
";

const FORGET_THEM: &str = "use accounts/actions { forget }

subscriber forget_them {
    on: PersonErased
    run: forget(event)
}
";

/// The event names the person's entity, their `id` and when they were
/// erased, and only the runtime emits it.
#[test]
fn an_app_subscribes_to_person_erased() {
    let with = [
        ("domains/accounts/actions/forget.jadpo", FORGET),
        (
            "domains/accounts/subscribers/forget_them.jadpo",
            FORGET_THEM,
        ),
        ("domains/accounts/entities/departure.jadpo", DEPARTURE),
    ];
    assert_eq!(problems(&with), NONE);
    let no_person = USER.replace("    person: true\n", "");
    let found = rules(&[
        ("domains/accounts/actions/forget.jadpo", FORGET),
        (
            "domains/accounts/subscribers/forget_them.jadpo",
            FORGET_THEM,
        ),
        ("domains/accounts/entities/departure.jadpo", DEPARTURE),
        (USER_PATH, &no_person),
        (NOTE_PATH, ""),
        (CLOSE_PATH, ""),
    ]);
    assert!(
        found.contains(&"subscriber.event_invalid".to_string()),
        "{found:?}"
    );
}

/// Every start replays the log, so a deployable reaches where it is kept
/// whatever it serves or runs, and a Worker has no disk to keep it on.
#[test]
fn a_worker_keeps_the_log_in_a_database() {
    let declared = |store: &str| {
        APP.replace("store: erasures", &format!("store: {store}"))
            .replace(
                "    schedules: {",
                "    deployables: {\n        desk: {\n            runs: [Trigger.subscribers]\n            target: Target.wasm\n        }\n    }\n    schedules: {",
            )
    };
    let on_disk = declared("scans");
    let found = problems(&[("app.jadpo", &on_disk)]);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("deployable.target_unsupported")
                && problem.contains("`scans`")),
        "{found:?}"
    );
    let in_a_database = declared("erasures");
    let found = problems(&[("app.jadpo", &in_a_database)]);
    assert!(
        !found
            .iter()
            .any(|problem| problem.contains("deployable.target_unsupported")),
        "{found:?}"
    );
}
