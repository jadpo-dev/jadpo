//! Area 16, "Search" (chunk B12): `search: { fields: language: }` on an
//! entity keeps its text fields' words in a generated `tsvector` with a
//! GIN index, the fields listed first weighted most; a query finds its
//! records with `where: Entity matches phrase`, a `SearchText`, and with
//! no `order:` gives the best matches first. Each search field must be one
//! every caller may read, as a condition's must (area 9). What isn't
//! built is refused where it is written: an engine's `store:`, and a
//! field of a sensitive category (area 15), which needs access records.

use jadpo_db::{Database, Layout};

use crate::support;

/// The rules of each problem of `source` alone, in order.
fn rules(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", source)])
}

/// The problems of `source` alone, as `rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", source)])
        .into_iter()
        .map(|problem| {
            let problem = problem.trim_start();
            problem
                .strip_prefix("main.jadpo ")
                .unwrap_or(problem)
                .to_string()
        })
        .collect()
}

const CLEAN: [&str; 0] = [];

const TYPES: &str = "type TodoTitle = Text {
    max_length: 200
}

type TodoNotes = Text {
    max_length: 2000
}

type Priority = Int {
    min: 1
    max: 5
}
";

/// A todo searched by its title and notes, with `search` in place of
/// `extra` and a query in place of `query`.
fn todo(search: &str, query: &str) -> String {
    format!(
        "{TYPES}
entity Todo {{
    id: Uuid
    title: TodoTitle
    notes: TodoNotes?
    priority: Priority
{search}}}

type TodoView = Output {{
    pick: [Todo.id, Todo.title]
}}

type TodoSearch = Input {{
    phrase: SearchText
    page_size: PageSize {{
        default: 20
    }}
    after: PageCursor {{
        optional: true
    }}
}}
{query}"
    )
}

const SEARCH: &str = "
    search: {
        fields: [title, notes]
        language: SearchLanguage.english
    }
";

const PAGED: &str = "
query search_todos(search: TodoSearch) -> Page<TodoView> {
    from: Todo
    where: Todo matches search.phrase
    page: {
        size: search.page_size
        after: search.after
    }
}
";

#[test]
fn a_search_checks_and_needs_no_order() {
    assert_eq!(rules(&todo(SEARCH, PAGED)), CLEAN);
    // A list too, and a search beside other conditions, ordered as written.
    let listed = "
query urgent_matches(phrase: SearchText) -> List<TodoView> {
    from: Todo
    where: Todo matches phrase and Todo.priority == 1
    limit: 20
}

query by_title(phrase: SearchText) -> List<TodoView> {
    from: Todo
    where: Todo matches phrase
    order: [Todo.title asc]
    limit: 20
}
";
    assert_eq!(rules(&todo(SEARCH, listed)), CLEAN);
    // A field may still be named `search`.
    let named = "    search: TodoNotes?\n";
    assert_eq!(rules(&todo(named, "")), CLEAN);
}

#[test]
fn the_words_are_a_generated_column_with_a_gin_index() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Project,
        &[
            support::APP,
            (
                "domains/planning/entities/todo.jadpo",
                "type TodoTitle = Text {\n    max_length: 200\n}\n\ntype TodoNotes = Text {\n    max_length: 2000\n}\n\nentity Todo {\n    id: Uuid\n    title: TodoTitle\n    notes: TodoNotes?\n\n    search: {\n        fields: [title, notes]\n        language: SearchLanguage.english\n    }\n}\n",
            ),
        ],
    );
    let schema = jadpo_check::data::schema(&db, project);
    let table = &schema.stores["main"].tables["todo"];
    let column = &table.columns["jadpo_search"];
    assert_eq!(column.sql_type, "tsvector");
    assert_eq!(column.field, None);
    assert_eq!(
        column.generated.as_deref(),
        Some(
            "setweight(to_tsvector('english'::regconfig, coalesce(\"title\", '')), 'A') || setweight(to_tsvector('english'::regconfig, coalesce(\"notes\", '')), 'B')"
        )
    );
    let index = &table.indexes["todo_jadpo_search_idx"];
    assert_eq!(index.columns, ["jadpo_search"]);
    assert_eq!(index.method.as_ref().map(|m| m.name.as_str()), Some("gin"));
    let sql = jadpo_graph::migration::migration_sql(&Default::default(), schema, &|_| None)
        .expect("migrates")
        .to_document("initial");
    assert!(
        sql.contains("    \"jadpo_search\" tsvector GENERATED ALWAYS AS (setweight(to_tsvector('english'::regconfig, coalesce(\"title\", '')), 'A') || setweight(to_tsvector('english'::regconfig, coalesce(\"notes\", '')), 'B')) STORED,\n"),
        "{sql}"
    );
    assert!(
        sql.contains(
            "CREATE INDEX \"todo_jadpo_search_idx\" ON \"todo\" USING gin (\"jadpo_search\");"
        ),
        "{sql}"
    );
    assert!(jadpo_check::check(&db, project).is_empty());
}

#[test]
fn search_settings_are_checked() {
    let cases = [
        (
            "    search: {\n        fields: [title]\n    }\n",
            "search.settings_invalid: `Todo`'s `search:` doesn't say what language its words are in",
        ),
        (
            "    search: {\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `Todo`'s `search:` names no fields",
        ),
        (
            "    search: {\n        fields: [title, titel]\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `Todo` has no field `titel`",
        ),
        (
            "    search: {\n        fields: [title, title]\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `title` is in `fields:` twice",
        ),
        (
            "    search: {\n        fields: [title, priority]\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `Todo.priority` isn't text, so it can't be searched by words",
        ),
        (
            "    search: {\n        fields: [Todo.title]\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `Todo.title` isn't one of `Todo`'s fields",
        ),
        (
            "    search: {\n        fields: []\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `Todo`'s `search:` `fields:` isn't a list of its fields",
        ),
        (
            "    search: {\n        fields: [title]\n        language: SearchLanguage.klingon\n    }\n",
            "search.settings_invalid: `SearchLanguage.klingon` isn't a language a search reads",
        ),
        (
            "    search: {\n        fields: [title]\n        language: \"english\"\n    }\n",
            "search.settings_invalid: `Todo`'s `search:` `language:` isn't a `SearchLanguage`",
        ),
        (
            "    search: {\n        fields: [title]\n        language: SearchLanguage.english\n        weights: [1]\n    }\n",
            "search.settings_invalid: `weights` isn't something `search:` holds",
        ),
        (
            "    search: {\n        fields: [title]\n        language: SearchLanguage.english\n        store: products_index\n    }\n",
            "toolchain.not_built_yet: `Todo`'s search in an engine isn't built yet",
        ),
        (
            "    jadpo_search: TodoNotes?\n\n    search: {\n        fields: [title]\n        language: SearchLanguage.english\n    }\n",
            "search.settings_invalid: `Todo` has a field `jadpo_search`, the column its searchable words are kept in",
        ),
    ];
    for (search, expected) in cases {
        assert_eq!(problems(&todo(search, "")), [expected], "{search}");
    }
}

#[test]
fn a_sensitive_field_is_not_searched_yet() {
    let source = format!(
        "{TYPES}
entity Patient {{
    id: Uuid
    name: TodoTitle
    history: TodoNotes {{
        personal: PersonalData.health
    }}

    search: {{
        fields: [name, history]
        language: SearchLanguage.english
    }}
}}
"
    );
    let found: Vec<String> = problems(&source)
        .into_iter()
        .filter(|p| !p.contains("data.personal_unreachable"))
        .collect();
    assert_eq!(
        found,
        ["toolchain.not_built_yet: Searching `Patient.history`, `health` data, isn't built yet"]
    );
    // Personal data of a category that isn't sensitive may be searched.
    let named = source.replace("PersonalData.health", "PersonalData.identifying");
    let found: Vec<String> = problems(&named)
        .into_iter()
        .filter(|p| !p.contains("data.personal_unreachable"))
        .collect();
    assert_eq!(found, CLEAN);
}

#[test]
fn matches_stands_on_its_own_line_of_a_search_query() {
    let undeclared = "
query search_todos(phrase: SearchText) -> List<TodoView> {
    from: Todo
    where: Todo matches phrase
    limit: 20
}
";
    assert_eq!(
        problems(&todo("", undeclared)),
        ["search.undeclared: `Todo` can't be searched by words"]
    );
    let refused = [
        (
            "    where: Todo matches phrase or Todo.priority == 1\n",
            "query.condition_unsupported: `Todo matches phrase` isn't a condition a query can use",
        ),
        (
            "    where: not (Todo matches phrase)\n",
            "query.condition_unsupported: `Todo matches phrase` isn't a condition a query can use",
        ),
        (
            "    where: Todo.title matches phrase\n",
            "query.condition_unsupported: `Todo.title matches phrase` isn't a condition a query can use",
        ),
        (
            "    where: Todo matches phrase\n    where: Todo matches phrase\n",
            "query.condition_unsupported: `search_todos` searches by words twice",
        ),
        (
            "    where: Todo matches title\n",
            "type.mismatch: `title` isn't a `SearchText`",
        ),
    ];
    for (line, expected) in refused {
        let query = format!(
            "
query search_todos(phrase: SearchText, title: TodoTitle) -> List<TodoView> {{
    from: Todo
{line}    limit: 20
}}
"
        );
        let found = problems(&todo(SEARCH, &query));
        assert_eq!(found, [expected], "{line}");
    }
    let present = "
type MaybeSearch = Input {
    phrase: SearchText {
        optional: true
    }
}

query search_todos(search: MaybeSearch) -> List<TodoView> {
    from: Todo
    where_present: Todo matches search.phrase
    limit: 20
}
";
    assert_eq!(
        rules(&todo(SEARCH, present)),
        ["query.condition_unsupported"]
    );
}

#[test]
fn a_full_text_index_is_the_entity_s_search() {
    let index = "
    indexes: {
        by_words: {
            fields: [title]
            kind: IndexKind.full_text
        }
    }
";
    assert_eq!(
        problems(&todo(index, "")),
        ["type.setting_value: A full-text index is the entity's `search:`"]
    );
}

#[test]
fn a_search_field_must_be_one_every_caller_may_read() {
    let files = |reference: &str| {
        vec![
            (
                "app.jadpo".to_string(),
                "use notes/entities { Member }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Member.handle\n    }\n}\n".to_string(),
            ),
            (
                "authentication.jadpo".to_string(),
                "authentication {\n    browser_session: {\n        cookie: \"notes_session\"\n    }\n}\n".to_string(),
            ),
            (
                "domains/notes/entities/member.jadpo".to_string(),
                "type Handle = Text {\n    min_length: 3\n    max_length: 40\n}\n\nentity Member {\n    id: Uuid\n    handle: Handle {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n    }\n}\n".to_string(),
            ),
            (
                "domains/notes/entities/note.jadpo".to_string(),
                format!("use notes/entities {{ Member }}\n\ntype NoteText = Text {{\n    max_length: 2000\n}}\n\nentity Note {{\n    id: Uuid\n    owner_id: Member.id {{\n        on_delete: OnDelete.cascade\n    }}\n    body: NoteText\n    import_reference: NoteText? {{\n        policy: FieldAccess.server_only\n    }}\n\n    search: {{\n        fields: [{reference}]\n        language: SearchLanguage.english\n    }}\n\n    policy: {{\n        read: [owner_id]\n    }}\n}}\n"),
            ),
            (
                "domains/notes/types/note_view.jadpo".to_string(),
                "use notes/entities { Note }\n\ntype NoteView = Output {\n    pick: [Note.id, Note.body]\n}\n".to_string(),
            ),
            (
                "domains/notes/queries/search_notes.jadpo".to_string(),
                "use notes/entities { Note }\nuse notes/types { NoteView }\n\n/// The caller's notes that match a search.\nquery search_notes(user: Principal.user, phrase: SearchText) -> List<NoteView> {\n    from: Note\n    where: Note matches phrase\n    limit: 20\n}\n".to_string(),
            ),
        ]
    };
    let found = |reference: &str| -> Vec<String> {
        let files = files(reference);
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        support::problems(Layout::Project, &borrowed)
            .into_iter()
            .filter(|p| !p.contains("record.") && !p.contains("migration."))
            .collect()
    };
    assert_eq!(found("body"), CLEAN);
    assert_eq!(
        found("body, import_reference"),
        [
            "domains/notes/queries/search_notes.jadpo policy.output_field_unproved: `search_notes` searches `Note.import_reference`, which not every caller may read"
        ]
    );
}
