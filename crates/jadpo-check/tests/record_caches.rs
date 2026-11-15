//! Area 5, "A record cache"; area 9, "Caches and personal data"; area 15,
//! "Sensitive fields" (chunk Q9b): an entity's `caches: { records: }`
//! keeps each record whole, by `id`, but for a sensitive field, an
//! encrypted one and a `File`; a route's query that reads one record by
//! `id` and gives only what it keeps is served from it; and a record
//! whose policy a copy can't be held to, a role's, a tenant's or one
//! through a field it leaves out, is refused (`data.cache_invalid`), as
//! is a block that names no cache or keeps a copy too long.

use jadpo_check::caches::{RecordCache, caches_holding, record_cache, record_lookup};
use jadpo_db::{Database, Layout};
use jadpo_graph::{Detail, EdgeKind, Key, Kind, Status};

use crate::support;

const APP: &str = "use notes/entities { Reader }

application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
        drafts: {
            driver: Driver.redis
            url: config.redis_url
        }
    }
}

principal {
    user: {
        subject: Reader.email
    }
}

authentication {
    api_token: {}
}
";

const CONFIG: &str = "config {
    redis_url: RedisUrl
}
";

const READER: &str = "/// Someone who keeps notes.
entity Reader {
    id: Uuid
    email: Email {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }

    person: true
}
";

/// What the cache of `NOTE` says, which the refusals replace.
const CACHES: &str = "    caches: {
        records: {
            store: cache
            keep: 1h
        }
    }
";

const NOTE: &str = "use notes/entities { Reader }

/// Something a reader wrote down, and how they felt.
entity Note {
    id: Uuid
    owner_id: Reader.id {
        on_delete: OnDelete.cascade
    }
    title: Text
    symptoms: Text? {
        personal: PersonalData.health
    }

    policy: {
        create: [owner_id]
        read: [owner_id]
        update: [owner_id]
    }
    caches: {
        records: {
            store: cache
            keep: 1h
        }
    }
}
";

const NOTE_VIEW: &str = "use notes/entities { Note }

/// A note as its owner sees it.
type NoteView = Output {
    pick: [Note.id, Note.title]
}
";

const NOTE_HEALTH: &str = "use notes/entities { Note }

/// How the owner felt.
type NoteHealth = Output {
    pick: [Note.id, Note.symptoms]
}
";

const FIND_NOTE: &str = "use notes/entities { Note, NoteNotFound }
use notes/types { NoteView }

/// One note, by its `id`.
query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
";

const NOTE_HEALTH_QUERY: &str = "use notes/entities { Note, NoteNotFound }
use notes/types { NoteHealth }

/// How the owner felt, by the note's `id`.
query symptoms_of(note_id: Note.id) fails NoteNotFound -> NoteHealth {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
";

const NOTES_OF: &str = "use notes/entities { Note, Reader }
use notes/types { NoteView }

/// A reader's notes.
query notes_of(owner_id: Reader.id) -> List<NoteView> {
    from: Note
    where: Note.owner_id == owner_id
    order: [Note.id asc]
    limit: 20
}
";

const OWN_CACHE: &str = "use notes/entities { Note }
use notes/types { NoteView }

/// One note, kept by the query for each caller.
query kept_note(note_id: Note.id) -> NoteView? {
    from: Note
    where: Note.id == note_id
    cache: {
        store: cache
        keep: 10m
    }
}
";

const NOTE_PATH: &str = "domains/notes/entities/note.jadpo";

fn files(changed: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = [
        ("app.jadpo", APP),
        ("config.jadpo", CONFIG),
        ("domains/notes/entities/reader.jadpo", READER),
        (NOTE_PATH, NOTE),
        ("domains/notes/types/note_view.jadpo", NOTE_VIEW),
        ("domains/notes/types/note_health.jadpo", NOTE_HEALTH),
        ("domains/notes/queries/find_note.jadpo", FIND_NOTE),
        ("domains/notes/queries/symptoms_of.jadpo", NOTE_HEALTH_QUERY),
        ("domains/notes/queries/notes_of.jadpo", NOTES_OF),
        ("domains/notes/queries/kept_note.jadpo", OWN_CACHE),
    ]
    .iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect();
    for (path, text) in changed {
        match out.iter_mut().find(|(at, _)| at == path) {
            Some(file) => file.1 = text.to_string(),
            None => out.push((path.to_string(), text.to_string())),
        }
    }
    out
}

fn problems(changed: &[(&str, &str)]) -> Vec<String> {
    let files = files(changed);
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    support::problems(Layout::Project, &borrowed)
}

/// `NOTE` with its `caches:` written as `caches`.
fn note_caching(caches: &str) -> String {
    NOTE.replace(CACHES, caches)
}

/// The one problem `caches` in place of `NOTE`'s gives, which is
/// `data.cache_invalid` with `message`.
fn refused(caches: &str, message: &str) {
    let note = note_caching(caches);
    let found = problems(&[(NOTE_PATH, &note)]);
    assert_eq!(
        found,
        [format!("{NOTE_PATH} data.cache_invalid: {message}")],
        "{caches}"
    );
}

#[test]
fn a_record_cache_keeps_every_field_but_a_sensitive_one() {
    assert_eq!(problems(&[]), Vec::<String>::new());
    let db = Database::default();
    let files = files(&[]);
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let project = support::project(&db, Layout::Project, &borrowed);
    assert_eq!(
        record_cache(&db, project, "Note"),
        Some(RecordCache {
            store: "cache".into(),
            keep_ms: 3_600_000,
            kept: vec!["id".into(), "owner_id".into(), "title".into()],
        })
    );
    assert_eq!(record_cache(&db, project, "Reader"), None, "none declared");
    // Where personal data goes (area 15): the cache holds each field it
    // keeps, and never the sensitive one.
    assert_eq!(caches_holding(&db, project, "Note", "title"), ["cache"]);
    assert_eq!(
        caches_holding(&db, project, "Note", "symptoms"),
        Vec::<String>::new()
    );
    // A route's read of one note by `id` is served from it; one that gives
    // the sensitive field, reads several or keeps its own results isn't.
    let queries = jadpo_check::data::queries::query_models(&db, project);
    let served =
        |name: &str| record_lookup(&db, project, queries.get(name).expect("the query")).is_some();
    assert!(served("find_note"));
    assert!(!served("symptoms_of"), "gives a sensitive field");
    assert!(!served("notes_of"), "reads several");
    assert!(!served("kept_note"), "keeps its own results");
}

#[test]
fn a_record_cache_is_a_node_using_its_connection() {
    assert_eq!(Kind::Cache.status(), Status::Built);
    let db = Database::default();
    let files = files(&[]);
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let project = support::project(&db, Layout::Project, &borrowed);
    let graph = jadpo_check::graph::graph(&db, project);
    let id = graph
        .find(&Key::new(Kind::Cache, "Note"))
        .expect("the record cache's node");
    let node = graph.node(id);
    assert_eq!(
        node.detail,
        Detail::Cache {
            store: "cache".into(),
            keep: "1h".into(),
        }
    );
    assert_eq!(node.parent, Some(Key::new(Kind::Entity, "Note")));
    let used: Vec<String> = graph
        .outgoing(id, EdgeKind::Uses)
        .map(|to| graph.node(to).key.to_string())
        .collect();
    assert_eq!(used, ["connection cache"]);
}

#[test]
fn a_record_cache_names_a_cache_and_keeps_a_copy_for_a_day_at_most() {
    refused(
        "    caches: {\n        records: {\n            store: main\n            keep: 1h\n        }\n    }\n",
        "`Note` caches its records in `main`, which isn't a cache",
    );
    refused(
        "    caches: {\n        records: {\n            store: cache\n            keep: 2d\n        }\n    }\n",
        "`Note`'s `keep:` must be a time from `1s` to `1d`",
    );
    refused(
        "    caches: {\n        records: {\n            store: cache\n        }\n    }\n",
        "`Note`'s `records:` doesn't give `keep:`",
    );
    refused(
        "    caches: {\n        records: {\n            store: cache\n            keep: 1h\n            kept: 1h\n        }\n    }\n",
        "`kept` isn't a cache setting",
    );
    refused(
        "    caches: {\n        queries: {\n            store: cache\n            keep: 1h\n        }\n    }\n",
        "`queries` isn't a cache an entity keeps",
    );
    refused("    caches: {}\n", "`Note`'s `caches:` keeps nothing");
}

#[test]
fn an_entity_in_redis_keeps_no_record_cache() {
    let draft = "/// A note not yet written down for good.
entity Draft {
    id: Uuid
    title: Text

    store: drafts
    expires_after: 1h
    policy: {
        read: [Access.public]
    }
    caches: {
        records: {
            store: cache
            keep: 1h
        }
    }
}
";
    let path = "domains/notes/entities/draft.jadpo";
    assert_eq!(
        problems(&[(path, draft)]),
        [format!(
            "{path} data.cache_invalid: `Draft` is kept in Redis already"
        )]
    );
}

#[test]
fn a_role_or_a_field_it_leaves_out_cant_be_applied_to_a_copy() {
    let roles = "/// What someone may do at the desk.
type DeskRole = Enum {
    clerk
}
";
    let clerk = "use notes/entities { Reader }
use notes/types { DeskRole }

/// Who may work the desk.
entity Clerk {
    id: Uuid
    reader_id: Reader.id {
        on_delete: OnDelete.cascade
    }
    role: DeskRole

    membership: {
        member: reader_id
        scope: MembershipScope.application
        role: role
    }

    policy: {
        read: [reader_id]
    }
}
";
    let by_role = NOTE
        .replace(
            "use notes/entities { Reader }\n",
            "use notes/entities { Reader }\nuse notes/types { DeskRole }\n",
        )
        .replace(
            "        read: [owner_id]\n",
            "        read: [owner_id, DeskRole.clerk]\n",
        );
    let found = problems(&[
        ("domains/notes/types/desk_role.jadpo", roles),
        ("domains/notes/entities/clerk.jadpo", clerk),
        (NOTE_PATH, &by_role),
    ]);
    assert_eq!(
        found,
        [format!(
            "{NOTE_PATH} data.cache_invalid: `Note`'s records can't be cached: its `read:` grants name a role"
        )]
    );
    let hidden_owner = NOTE.replace(
        "        on_delete: OnDelete.cascade\n    }\n    title",
        "        on_delete: OnDelete.cascade\n        personal: PersonalData.health\n    }\n    title",
    );
    let found = problems(&[(NOTE_PATH, &hidden_owner)]);
    assert_eq!(
        found,
        [format!(
            "{NOTE_PATH} data.cache_invalid: `Note`'s records can't be cached: its `read:` grants go through `owner_id`, which it leaves out"
        )]
    );
}
