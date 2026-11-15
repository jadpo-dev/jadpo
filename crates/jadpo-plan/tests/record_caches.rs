//! An entity's record cache in the plan (area 5, "A record cache"; chunk
//! Q9b): a route's read of one record by `id` keeps it under its entity
//! alone, every caller's copy, filled by a read of the record by `id`
//! with no policy, which each read applies to the copy by its grants; a
//! copy holds no sensitive field; and every write of the entity clears
//! it, whether or not a route reads it.

use jadpo_db::{Database, File, Layout, Project};
use jadpo_std::policy::{CallerPart, RecordGrant};
use jadpo_std::read::Cardinality;
use jadpo_std::redis::{Element, Given};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "use notes/entities { Reader }

application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        cache: {
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
",
    ),
    ("config.jadpo", "config {\n    redis_url: RedisUrl\n}\n"),
    (
        "domains/notes/entities/reader.jadpo",
        "/// Someone who keeps notes.
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
",
    ),
    (
        "domains/notes/entities/note.jadpo",
        "use notes/entities { Reader }

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
",
    ),
    (
        "domains/notes/types/note_view.jadpo",
        "use notes/entities { Note }

/// A note as its owner sees it.
type NoteView = Output {
    pick: [Note.id, Note.title]
}
",
    ),
    (
        "domains/notes/queries/find_note.jadpo",
        "use notes/entities { Note, NoteNotFound }
use notes/types { NoteView }

/// One note, by its `id`.
query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
",
    ),
    (
        "domains/notes/actions/rename_note.jadpo",
        "use notes/entities { Note, NoteNotFound }

/// Gives a note a new title.
action rename_note(note_id: Note.id, title: Note.title) fails NoteNotFound -> Note {
    return attempt Note.update(note_id) {
        set: {
            title: title
        }
    }
}
",
    ),
    (
        "domains/notes/routes/notes.jadpo",
        "use notes/entities { Note }
use notes/types { NoteView }
use notes/queries { find_note }

routes Notes {
    prefix: /notes

    GET /{note_id: Note.id} {
        output: NoteView
        run: find_note(note_id)
    }
}
",
    ),
];

fn planned() -> jadpo_plan::Plan {
    let db = Database::default();
    let inputs = FILES
        .iter()
        .map(|(path, text)| File::new(&db, (*path).to_string(), (*text).to_string()))
        .collect();
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        Layout::Project,
        Default::default(),
        Vec::new(),
    );
    let mut problems = jadpo_check::check(&db, project);
    // A fixture is the part of an app its plan is about, with no records
    // or migrations kept, and a marked field needn't also be read.
    problems.retain(|problem| {
        !matches!(
            problem.rule.id,
            "data.personal_unread" | "migration.out_of_date"
        ) && !problem.rule.id.starts_with("record.")
    });
    assert!(problems.is_empty(), "the fixture checks: {problems:#?}");
    jadpo_plan::Plan::build(&db, project)
}

#[test]
fn a_read_of_one_record_by_id_is_kept_whole_but_for_its_sensitive_field() {
    let plan = planned();
    let route = plan
        .routes()
        .routes
        .iter()
        .find(|route| route.key.starts_with("route GET /notes/"))
        .expect("the route");
    let cache = route.cache.as_ref().expect("a cache");
    assert_eq!(cache.store, "cache");
    assert_eq!(cache.keep_ms, 3_600_000);
    assert_eq!(cache.tags, ["Note"], "its entity alone");
    assert!(cache.key.starts_with("Note records "), "{}", cache.key);
    let records = cache.records.as_ref().expect("read as a record");
    let kept: Vec<&str> = records
        .fields
        .iter()
        .map(|field| field.field.as_str())
        .collect();
    assert_eq!(kept, ["id", "owner_id", "title"]);
    assert_eq!(
        records.id,
        Given::Arg {
            index: 0,
            path: Vec::new()
        }
    );
    assert_eq!(
        records.element,
        Element::Picked {
            ty: "NoteView".into(),
            fields: vec![("id".into(), "id".into()), ("title".into(), "title".into())],
        }
    );
    assert_eq!(
        records.grants.grants,
        [RecordGrant::Caller {
            field: "owner_id".into(),
            part: CallerPart::User,
        }]
    );
    // The copy is filled with no policy, which each read applies to it,
    // and without the sensitive field.
    let fill = &records.fill;
    assert_eq!(fill.cardinality, Cardinality::Optional);
    assert_eq!(fill.select.unpoliced, None);
    assert_eq!(fill.select.permitted, None);
    assert!(!fill.select.sql.contains("symptoms"), "{}", fill.select.sql);
    assert!(!fill.select.sql.contains("$2"), "{}", fill.select.sql);
    assert_eq!(fill.select.sensitive, None);
    // A longest `keep:` for a write's revision cookie.
    assert_eq!(plan.routes().revisions, Some(3_600_000));
}

#[test]
fn every_write_of_the_entity_clears_its_record_cache() {
    let plan = planned();
    let renames: Vec<_> = plan
        .writes()
        .iter()
        .filter(|write| write.plan.sql.contains("UPDATE \"note\""))
        .collect();
    assert!(!renames.is_empty(), "a write of a note");
    for write in renames {
        assert_eq!(write.plan.clears.len(), 1, "{:?}", write.plan.clears);
        assert_eq!(write.plan.clears[0].store, "cache");
        assert_eq!(write.plan.clears[0].tags, ["Note"]);
    }
}
