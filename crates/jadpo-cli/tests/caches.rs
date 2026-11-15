//! Caches over real HTTP (area 5, "Caches"; area 6, "Where a read comes
//! from"; area 7, "Write revisions"; chunk Q9): `jadpo dev` keeps a
//! query's cache in its own memory, against live PostgreSQL. A display
//! read is served from its cache, so a change made behind the app's back
//! isn't seen; a request carrying a later revision skips it and keeps what
//! it read instead; a write answers with its revision as a header and a
//! cookie, and clears every cached result that reads what it changed, a
//! list's and one record's alike.

use std::path::{Path, PathBuf};

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    connections: {\n        cache: {\n            driver: Driver.redis\n            url: config.redis_url\n        }\n    }\n}\n",
    ),
    ("config.jadpo", "config {\n    redis_url: RedisUrl\n}\n"),
    (
        "domains/planning/actions/create_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoTitle, TodoView }

action create_todo(input: CreateTodo) -> TodoView {
    var todo: Todo = Todo.create {
        set: {
            title: input.title
        }
    }
    return TodoView {
        id: todo.id
        title: todo.title
    }
}

/// What creating a todo takes.
type CreateTodo = Input {
    title: TodoTitle
}
"#,
    ),
    (
        "domains/planning/actions/rename_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoTitle, TodoView }
use planning/failures { TodoNotFound }

action rename_todo(todo_id: Todo.id, input: RenameTodo) fails TodoNotFound -> TodoView {
    var todo: Todo = attempt Todo.update(todo_id) {
        set: {
            title: input.title
        }
    }
    return TodoView {
        id: todo.id
        title: todo.title
    }
}

/// What renaming a todo takes.
type RenameTodo = Input {
    title: TodoTitle
}
"#,
    ),
    (
        "domains/planning/entities/todo.jadpo",
        r#"use planning/types { TodoTitle }

entity Todo {
    id: Uuid
    title: TodoTitle

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
"#,
    ),
    (
        "domains/planning/failures/todo_not_found.jadpo",
        r#"use planning/entities { Todo }

type TodoNotFound = Failure {
    kind: FailureKind.not_found
    message: "Todo not found."
    client: {
        todo_id: Todo.id
    }
}
"#,
    ),
    (
        "domains/planning/queries/find_todo.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }
use planning/failures { TodoNotFound }

query find_todo(todo_id: Todo.id) fails TodoNotFound -> TodoView {
    from: Todo
    where: Todo.id == todo_id
    missing: TodoNotFound {
        todo_id: todo_id
    }
    cache: {
        store: cache
        keep: 10m
    }
}
"#,
    ),
    (
        "domains/planning/queries/list_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }

/// Todos, by title.
query list_todos() -> List<TodoView> {
    from: Todo
    order: [Todo.title asc]
    limit: 20
    cache: {
        store: cache
        keep: 1h
    }
}
"#,
    ),
    (
        "domains/planning/routes/todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }
use planning/actions { CreateTodo, RenameTodo, create_todo, rename_todo }
use planning/queries { find_todo, list_todos }

/// Todos: making one, renaming it and reading them back.
routes Todos {
    prefix: /todos

    /// Creates a todo.
    POST / {
        authentication: RouteAuthentication.none
        input: CreateTodo
        output: TodoView
        status: SuccessStatus.created
        run: create_todo(input)
    }

    /// Todos, by title.
    GET / {
        authentication: RouteAuthentication.none
        output: List<TodoView>
        run: list_todos()
    }

    /// One todo.
    GET /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        output: TodoView
        run: find_todo(todo_id)
    }

    /// Renames a todo.
    PATCH /{todo_id: Todo.id} {
        authentication: RouteAuthentication.none
        input: RenameTodo
        output: TodoView
        run: rename_todo(todo_id, input)
    }
}
"#,
    ),
    (
        "domains/planning/types/todo_title.jadpo",
        r#"type TodoTitle = Text {
    min_length: 1
    max_length: 200
}
"#,
    ),
    (
        "domains/planning/types/todo_view.jadpo",
        r#"use planning/entities { Todo }

/// A todo as clients see it.
type TodoView = Output {
    pick: [Todo.id, Todo.title]
}
"#,
    ),
];

/// The project, migrated and recorded, checking cleanly, its Redis address
/// one no one listens at, which `jadpo dev` never asks.
fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("caches");
    let root = dir.path().to_path_buf();
    for (path, text) in FILES {
        write(&root, path, text);
    }
    write(&root, ".env.local", "REDIS_URL=redis://127.0.0.1:9\n");
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"][..],
        &["check"][..],
    ] {
        let ran = jadpo(&root, args);
        assert_eq!(ran.code, 0, "{args:?}: {}{}", ran.stdout, ran.stderr);
    }
    (dir, root)
}

/// Renames todo `id` to `title` behind the app's back, as another program
/// writing to its database would.
fn renamed_behind(url: &str, id: &str, title: &str) {
    let rows = crate::deployable::rows(
        url,
        &format!(
            "UPDATE \"todo\" SET \"title\" = '{title}' WHERE \"id\" = '{id}' RETURNING \"title\""
        ),
    );
    assert_eq!(rows.len(), 1, "{rows:?}");
}

fn title(dev: &Dev, path: &str, headers: &[(&str, &str)]) -> String {
    let read = dev.send("GET", path, headers, "");
    assert_eq!(read.status, 200, "{read:?}");
    let json = read.json();
    let one = if json.is_array() { &json[0] } else { &json };
    one["title"].as_str().expect("a title").to_string()
}

#[test]
fn a_display_read_is_served_from_its_cache_until_a_change_clears_it() {
    if !jadpo_test::live() {
        return;
    }
    let (_dir, root) = project();
    let dev = Dev::start(&root);
    let created = dev.send("POST", "/todos", &[], r#"{"title":"Buy milk"}"#);
    assert_eq!(created.status, 201, "{created:?}");
    // A write's revision, as a header and as a cookie lasting the longest
    // `keep:`, an hour.
    let revision: i64 = created
        .header("jadpo-revision")
        .expect("a revision")
        .parse()
        .expect("a number");
    let cookie = created
        .headers("set-cookie")
        .into_iter()
        .find(|cookie| cookie.starts_with("jadpo_revision="))
        .expect("a revision cookie")
        .to_string();
    assert_eq!(
        cookie,
        format!(
            "jadpo_revision={revision}; Path=/; Max-Age=3600; HttpOnly; Secure; SameSite=Strict"
        )
    );
    let id = created.json()["id"].as_str().expect("an id").to_string();
    let one = format!("/todos/{id}");

    // Read once, then kept: a change behind the app's back isn't seen.
    assert_eq!(title(&dev, &one, &[]), "Buy milk");
    assert_eq!(title(&dev, "/todos", &[]), "Buy milk");
    renamed_behind(dev.url(), &id, "Buy oat milk");
    assert_eq!(title(&dev, &one, &[]), "Buy milk", "served from the cache");
    assert_eq!(
        title(&dev, "/todos", &[]),
        "Buy milk",
        "served from the cache"
    );
    // A revision kept before the entry was read is older than it.
    let older = revision.to_string();
    assert_eq!(title(&dev, &one, &[("Jadpo-Revision", &older)]), "Buy milk");

    // A request carrying a later revision skips the cache, by header or by
    // cookie, and keeps what it read for everyone after it.
    let later = (revision + 3_600_000).to_string();
    assert_eq!(
        title(&dev, &one, &[("Jadpo-Revision", &later)]),
        "Buy oat milk"
    );
    assert_eq!(title(&dev, &one, &[]), "Buy oat milk");
    let cookie = format!("jadpo_revision={later}");
    assert_eq!(
        title(&dev, "/todos", &[("Cookie", &cookie)]),
        "Buy oat milk"
    );

    // A write through the app clears every cached result reading todos.
    renamed_behind(dev.url(), &id, "Stale");
    assert_eq!(title(&dev, &one, &[]), "Buy oat milk");
    let renamed = dev.send("PATCH", &one, &[], r#"{"title":"Buy cream"}"#);
    assert_eq!(renamed.status, 200, "{renamed:?}");
    assert!(renamed.header("jadpo-revision").is_some(), "{renamed:?}");
    assert_eq!(title(&dev, &one, &[]), "Buy cream");
    assert_eq!(title(&dev, "/todos", &[]), "Buy cream");
    // A read sends no revision back.
    let read = dev.send("GET", &one, &[], "");
    assert!(read.header("jadpo-revision").is_none(), "{read:?}");
}

/// A project whose queries' caches name what isn't a cache, or keep their
/// entries too long, is refused where each is written.
#[test]
fn a_cache_names_a_cache_and_keeps_for_a_day_at_most() {
    let dir = tempdir::Dir::new("caches-refused");
    let root: &Path = dir.path();
    for (path, text) in FILES {
        write(root, path, text);
    }
    write(
        root,
        "domains/planning/queries/list_todos.jadpo",
        r#"use planning/entities { Todo }
use planning/types { TodoView }

/// Todos, by title.
query list_todos() -> List<TodoView> {
    from: Todo
    order: [Todo.title asc]
    limit: 20
    cache: {
        store: main
        keep: 2d
    }
}
"#,
    );
    jadpo(root, &["records", "update"]);
    let checked = jadpo(root, &["check", "--json"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout).expect("JSON");
    let rules: Vec<&str> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|diagnostic| diagnostic["rule"].as_str())
        .filter(|rule| rule.starts_with("query."))
        .collect();
    assert_eq!(
        rules,
        ["query.cache_invalid", "query.cache_invalid"],
        "{}",
        checked.stdout
    );
}

/// A person's erasure clears every cache (area 15, "What erasure reaches";
/// chunk Q9): a public list of reviews, cached, is read once with Ada's
/// review in it, and after Ada closes her account reads without it, where
/// a cached copy surviving the erasure would still show it.
const ERASED: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use accounts/entities { User }

application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        cache: {
            driver: Driver.redis
            url: config.redis_url
        }
        erasures: {
            driver: Driver.postgres
        }
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
}

principal {
    user: {
        subject: User.email
    }
}

authentication {
    api_token: {}
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    redis_url: RedisUrl
}
"#,
    ),
    (
        "domains/accounts/entities/user.jadpo",
        r#"/// Someone with an account, who may erase only themselves.
entity User {
    id: Uuid
    email: Email {
        unique: true
        personal: PersonalData.contact
    }

    policy: {
        create: [Access.public]
        read: [id]
        delete: [id]
    }

    person: true
}
"#,
    ),
    (
        "domains/accounts/types/review_text.jadpo",
        r#"/// What a review says.
type ReviewText = Text {
    max_length: 500
}
"#,
    ),
    (
        "domains/accounts/entities/review.jadpo",
        r#"use accounts/entities { User }
use accounts/types { ReviewText }

/// What someone thought of the tea, which anyone may read.
entity Review {
    id: Uuid
    author_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    text: ReviewText

    policy: {
        create: [author_id]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/accounts/types/review_card.jadpo",
        r#"use accounts/entities { Review }

/// A review as the list shows it.
type ReviewCard = Output {
    pick: [Review.text]
}
"#,
    ),
    (
        "domains/accounts/queries/all_reviews.jadpo",
        r#"use accounts/entities { Review }
use accounts/types { ReviewCard }

/// Every review, newest words first.
query all_reviews() -> List<ReviewCard> {
    from: Review
    order: [Review.text asc]
    limit: 20
    cache: {
        store: cache
        keep: 1h
    }
}
"#,
    ),
    (
        "domains/accounts/routes/reviews.jadpo",
        r#"use accounts/types { ReviewCard }
use accounts/queries { all_reviews }

routes Reviews {
    prefix: /reviews

    GET / {
        authentication: RouteAuthentication.none
        output: List<ReviewCard>
        run: all_reviews()
    }
}
"#,
    ),
    (
        "domains/accounts/failures/email_taken.jadpo",
        r#"/// Someone has that address already.
type EmailTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/accounts/actions/sign_up.jadpo",
        r#"use accounts/entities { User }
use accounts/failures { EmailTaken }

/// Adds a user.
action sign_up(email: User.email) fails EmailTaken -> User {
    return attempt User.create {
        set: {
            email: email
        }
        conflict: {
            User.email: EmailTaken
        }
    }
}
"#,
    ),
    (
        "domains/accounts/actions/write_review.jadpo",
        r#"use accounts/entities { Review }

/// Adds a review.
action write_review(me: Principal.user, words: Review.text) -> Review {
    return Review.create {
        set: {
            author_id: me.id
            text: words
        }
    }
}
"#,
    ),
    (
        "domains/accounts/actions/close_account.jadpo",
        r#"use accounts/entities { User, UserNotFound }

/// Erases the signed-in user, and their reviews with them.
action close_account(me: Principal.user) fails UserNotFound -> Unit {
    attempt User.erase(me.id)
}
"#,
    ),
    (
        "domains/accounts/routes/reviews.test.jadpo",
        r#"use accounts/entities { Review, User }
use accounts/types { ReviewCard }
use accounts/actions { close_account, sign_up, write_review }

test "erasing someone takes their reviews out of the cached list" {
    var ada: User = attempt sign_up("ada@example.com")
    var me: Principal.user = Principal.user(ada)
    var review: Review = write_review(me, "Lovely tea")
    assert review.text == "Lovely tea"
    var before: Response = request {
        method: HttpMethod.get
        path: "/reviews"
    }
    assert before.output == [
        ReviewCard {
            text: "Lovely tea"
        }
    ]
    attempt close_account(me)
    var after: Response = request {
        method: HttpMethod.get
        path: "/reviews"
    }
    var none_left: List<ReviewCard> = []
    assert after.output == none_left
}
"#,
    ),
];

#[test]
fn an_erasure_leaves_no_cached_copy() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("caches-erased");
    let root: &Path = dir.path();
    for (path, text) in ERASED {
        write(root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"][..],
        &["check"][..],
        &["test", "--seed", "1"][..],
    ] {
        let ran = jadpo(root, args);
        assert_eq!(ran.code, 0, "{args:?}: {}{}", ran.stdout, ran.stderr);
    }
}

/// The rules `jadpo check --json` reports for the project at `root` once
/// its records are written, but a migration's.
fn rules(root: &Path) -> Vec<String> {
    jadpo(root, &["records", "update"]);
    let checked = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|diagnostic| diagnostic["rule"].as_str())
        .filter(|rule| !rule.starts_with("migration."))
        .map(str::to_string)
        .collect()
}

/// `Driver.memory` keeps a cache in each server's own memory: refused
/// until a person declines it, and on Workers whatever is declined, since
/// every deployable keeps or clears each cache.
#[test]
fn a_cache_in_memory_is_declined_and_never_on_workers() {
    let dir = tempdir::Dir::new("caches-memory");
    let root: &Path = dir.path();
    for (path, text) in FILES {
        write(root, path, text);
    }
    let memory = |declined: &str, deployables: &str| {
        format!(
            "application {{\n    connections: {{\n{declined}        cache: {{\n            driver: Driver.memory\n        }}\n    }}\n{deployables}}}\n"
        )
    };
    write(root, "config.jadpo", "config {}\n");
    write(root, "app.jadpo", &memory("", ""));
    assert_eq!(rules(root), ["connection.memory_cache"]);
    let declined = "        decline connection.memory_cache: \"The app runs on one server.\"\n";
    write(root, "app.jadpo", &memory(declined, ""));
    assert_eq!(rules(root), Vec::<String>::new());
    let edge = "    deployables: {\n        edge: {\n            serves: [main]\n            target: Target.wasm\n        }\n    }\n    rate_limits: {\n        store: limits\n    }\n";
    let app = memory(declined, edge).replace(
        "    connections: {\n",
        "    connections: {\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n",
    );
    write(root, "app.jadpo", &app);
    assert_eq!(rules(root), ["deployable.target_unsupported"]);
}

/// A desk's notes, each cached whole by `id` and read by its owner alone
/// (area 5, "A record cache"; chunk Q9b), its staff signing in with tokens
/// from `jadpo dev`'s local issuer.
const RECORDS: &[(&str, &str)] = &[
    (
        "config.jadpo",
        r#"config {
    issuer: Url {
        default: "https://id.lending.example/"
    }
    audience: Text {
        default: "lending-desk"
    }
    redis_url: RedisUrl
}
"#,
    ),
    (
        "app.jadpo",
        r#"use desk/entities { Librarian }

application {
    apis: {
        main: {
            prefix: /
            authentication: [AuthenticationMethod.browser_session]
        }
        staff: {
            prefix: /staff
            authentication: [AuthenticationMethod.external_token]
        }
    }
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
        subject: Librarian.subject
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "desk_session"
    }
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}
"#,
    ),
    (
        "domains/desk/entities/librarian.jadpo",
        r#"entity Librarian {
    id: Uuid
    subject: Text {
        unique: true
    }

    policy: {
        create: [Access.system]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/desk/entities/note.jadpo",
        r#"use desk/entities { Librarian }

/// Something a librarian wrote down for themselves.
entity Note {
    id: Uuid
    owner_id: Librarian.id {
        on_delete: OnDelete.cascade
    }
    title: Text

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
"#,
    ),
    (
        "domains/desk/types/note_view.jadpo",
        r#"use desk/entities { Note }

/// A note as its owner sees it.
type NoteView = Output {
    pick: [Note.id, Note.title]
}
"#,
    ),
    (
        "domains/desk/types/retitle.jadpo",
        r#"use desk/entities { Note }

/// A note's new title.
type Retitle = Input {
    title: Note.title
}
"#,
    ),
    (
        "domains/desk/queries/find_note.jadpo",
        r#"use desk/entities { Note, NoteNotFound }
use desk/types { NoteView }

/// One note, by its `id`.
query find_note(note_id: Note.id) fails NoteNotFound -> NoteView {
    from: Note
    where: Note.id == note_id
    missing: NoteNotFound
}
"#,
    ),
    (
        "domains/desk/actions/retitle_note.jadpo",
        r#"use desk/entities { Note, NoteNotFound }
use desk/types { NoteView, Retitle }

/// Gives a note a new title.
action retitle_note(note_id: Note.id, input: Retitle) fails NoteNotFound -> NoteView {
    var note: Note = attempt Note.update(note_id) {
        set: {
            title: input.title
        }
    }
    return NoteView {
        id: note.id
        title: note.title
    }
}
"#,
    ),
    (
        "domains/desk/routes/staff.jadpo",
        r#"use desk/entities { Note }
use desk/types { NoteView, Retitle }
use desk/queries { find_note }
use desk/actions { retitle_note }

/// A librarian's notes.
routes Staff {
    api: staff
    prefix: /staff

    GET /notes/{note_id: Note.id} {
        output: NoteView
        run: find_note(note_id)
    }

    PATCH /notes/{note_id: Note.id} {
        input: Retitle
        output: NoteView
        run: retitle_note(note_id, input)
    }
}
"#,
    ),
];

/// A row `insert` makes in the session's database, giving its `id`.
fn inserted(url: &str, insert: &str) -> String {
    let rows = crate::deployable::rows(url, insert);
    assert_eq!(rows.len(), 1, "{rows:?}");
    rows[0][0].clone()
}

/// A record is cached whole, one copy for every caller, under `jadpo dev`
/// against live PostgreSQL: its owner reads it from the cache, so a change
/// behind the app's back isn't seen; someone else is refused it from that
/// copy, as policy is applied each time it is read; a later revision
/// skips the copy; and a write through the app clears it. The query
/// report says where the route reads from.
#[test]
fn a_record_is_cached_whole_and_held_to_policy_each_time_it_is_read() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("caches-records");
    let root = dir.path().join("desk");
    for (path, text) in RECORDS {
        write(&root, path, text);
    }
    write(&root, ".env.local", "REDIS_URL=redis://127.0.0.1:9\n");
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"][..],
        &["check"][..],
    ] {
        let ran = jadpo(&root, args);
        assert_eq!(ran.code, 0, "{args:?}: {}{}", ran.stdout, ran.stderr);
    }
    let report = std::fs::read_to_string(root.join("build/report/queries.md")).expect("the report");
    assert!(
        report.contains("`Note`'s record cache in `cache`, up to `1h` old, else the store"),
        "{report}"
    );
    let dev = Dev::start(&root);
    let token = |id: &str| {
        let all = ["dev", "token", id, "--url", dev.url()].map(str::to_string);
        let printed = jadpo_cli::run(&all, &jadpo_term::Environment::empty().with_cwd(&root));
        assert_eq!(printed.code, 0, "{}{}", printed.stdout, printed.stderr);
        format!("Bearer {}", printed.stdout.trim_end())
    };
    let ann = inserted(
        dev.url(),
        "INSERT INTO librarian (id, subject) VALUES (gen_random_uuid(), 'google-ann') RETURNING id::text",
    );
    let bo = inserted(
        dev.url(),
        "INSERT INTO librarian (id, subject) VALUES (gen_random_uuid(), 'google-bo') RETURNING id::text",
    );
    let note = inserted(
        dev.url(),
        &format!(
            "INSERT INTO note (id, owner_id, title) VALUES (gen_random_uuid(), '{ann}', 'Buy tea') RETURNING id::text"
        ),
    );
    let (ann, bo) = (token(&ann), token(&bo));
    let path = format!("/staff/notes/{note}");
    let read = |authorization: &str, more: &[(&str, &str)]| {
        let mut headers = vec![("authorization", authorization)];
        headers.extend_from_slice(more);
        dev.send("GET", &path, &headers, "")
    };
    let title = |authorization: &str, more: &[(&str, &str)]| {
        let answer = read(authorization, more);
        assert_eq!(answer.status, 200, "{answer:?}");
        answer.json()["title"]
            .as_str()
            .expect("a title")
            .to_string()
    };
    assert_eq!(title(&ann, &[]), "Buy tea");
    // Kept whole: a change behind the app's back isn't seen.
    let changed = crate::deployable::rows(
        dev.url(),
        &format!("UPDATE note SET title = 'Buy green tea' WHERE id = '{note}' RETURNING title"),
    );
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(title(&ann, &[]), "Buy tea", "served from the record cache");
    // One copy, held to policy as it is read: someone else finds nothing.
    let refused = read(&bo, &[]);
    assert_eq!(refused.status, 404, "{refused:?}");
    // A later revision skips the copy, and keeps what it read.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_millis();
    let later = (now + 3_600_000).to_string();
    assert_eq!(title(&ann, &[("Jadpo-Revision", &later)]), "Buy green tea");
    assert_eq!(title(&ann, &[]), "Buy green tea");
    // A write through the app clears it.
    crate::deployable::rows(
        dev.url(),
        &format!("UPDATE note SET title = 'Stale' WHERE id = '{note}' RETURNING title"),
    );
    let retitled = dev.send(
        "PATCH",
        &path,
        &[("authorization", &ann)],
        r#"{"title":"Buy oolong"}"#,
    );
    assert_eq!(retitled.status, 200, "{retitled:?}");
    assert_eq!(title(&ann, &[]), "Buy oolong");
    assert_eq!(read(&bo, &[]).status, 404);
}
