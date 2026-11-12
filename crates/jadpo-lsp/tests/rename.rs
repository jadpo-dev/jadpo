//! Rename in the editor (area 14, "Editor support"; chunk D8b):
//! `textDocument/prepareRename` and `textDocument/rename` run
//! `jadpo_check::rename`, the code `jadpo rename` runs, so the editor
//! renames the same places, keeps a stored field's column through the same
//! identity line and refuses the same names.

use std::collections::BTreeMap;
use std::path::Path;

use jadpo_check::rename;
use jadpo_lsp::Server;
use lsp_server::{Message, Request, RequestId, ResponseError};
use serde_json::{Value, json};

use crate::support::{at, project, start, uri};

/// A planning app: a todo's priority stored, written by an action, read by
/// a query ordered by it and copied into the view the query returns, and
/// read in a test.
const PLANNING: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/planning/entities/todo.jadpo",
        "/// Something to do.\nentity Todo {\n    id: Uuid\n    title: Text\n    priority: Int\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/planning/types/todo_view.jadpo",
        "use planning/entities { Todo }\n\n/// A todo in a list.\ntype TodoView = Output {\n    pick: [Todo.title, Todo.priority]\n}\n",
    ),
    (
        "domains/planning/actions/add_todo.jadpo",
        "use planning/entities { Todo }\n\n/// Adds a todo.\naction add_todo(title: Todo.title, priority: Todo.priority) -> Todo {\n    return Todo.create {\n        set: {\n            title: title\n            priority: priority\n        }\n    }\n}\n",
    ),
    (
        "domains/planning/queries/todos.jadpo",
        "use planning/entities { Todo }\nuse planning/types { TodoView }\n\n/// Every todo, the most pressing first.\nquery todos() -> List<TodoView> {\n    from: Todo\n    order: [Todo.priority desc]\n    limit: 50\n}\n",
    ),
    (
        "domains/planning/actions/list_todos.jadpo",
        "use planning/types { TodoView }\nuse planning/queries { todos }\n\n/// Every todo, as the list shows them.\naction list_todos() -> List<TodoView> {\n    return todos()\n}\n",
    ),
    (
        "domains/planning/actions/add_todo.test.jadpo",
        "use planning/entities { Todo }\nuse planning/types { TodoView }\nuse planning/queries { todos }\n\ntest \"a todo keeps its priority\" {\n    var todo: Todo = add_todo(\"Write\", 3)\n    assert todo.priority == 3\n    var listed: List<TodoView> = todos()\n    assert list.count(listed) == 1\n}\n",
    ),
];

const ADD_TODO: &str = "domains/planning/actions/add_todo.jadpo";
const QUERY: &str = "domains/planning/queries/todos.jadpo";

fn text_of(path: &str) -> &'static str {
    PLANNING
        .iter()
        .find(|(file, _)| *file == path)
        .map(|(_, text)| *text)
        .expect("a planning file")
}

/// An editor that asks before it renames, as VS Code does.
fn editor(root: &Path) -> Server {
    start(
        root,
        json!({"textDocument": {"rename": {"prepareSupport": true}}}),
    )
}

/// The answer to one request: its result, or its error.
fn answer(server: &mut Server, method: &str, params: Value) -> Result<Value, ResponseError> {
    let answer = server.handle(Message::Request(Request::new(
        RequestId::from(1),
        method.to_string(),
        params,
    )));
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    response.response_result.clone()
}

fn prepare(server: &mut Server, root: &Path, path: &str, position: Value) -> Result<Value, String> {
    answer(
        server,
        "textDocument/prepareRename",
        json!({"textDocument": {"uri": uri(root, path)}, "position": position}),
    )
    .map_err(|error| error.message)
}

fn rename_at(
    server: &mut Server,
    root: &Path,
    path: &str,
    position: Value,
    new: &str,
) -> Result<Value, String> {
    answer(
        server,
        "textDocument/rename",
        json!({
            "textDocument": {"uri": uri(root, path)},
            "position": position,
            "newName": new,
        }),
    )
    .map_err(|error| error.message)
}

/// Each file's text after, by project path, `None` for one removed.
type Texts = BTreeMap<String, Option<String>>;

/// What a workspace edit does to each file, by project path: its new
/// text, or `None` for a file deleted or renamed away; and the renames.
fn applied(root: &Path, edit: &Value) -> (Texts, Vec<(String, String)>) {
    let prefix = format!("file://{}/", root.display());
    let path = |uri: &Value| {
        uri.as_str()
            .expect("uri")
            .trim_start_matches(&prefix)
            .to_string()
    };
    let mut files = BTreeMap::new();
    let mut renames = Vec::new();
    for change in edit["documentChanges"].as_array().expect("changes") {
        match change["kind"].as_str() {
            Some("delete") => {
                files.insert(path(&change["uri"]), None);
            }
            Some("rename") => {
                let (from, to) = (path(&change["oldUri"]), path(&change["newUri"]));
                files.insert(from.clone(), None);
                renames.push((from, to));
            }
            Some(_) => {}
            None => {
                let edits = change["edits"].as_array().expect("edits");
                assert_eq!(edits.len(), 1, "each file replaced whole: {change}");
                files.insert(
                    path(&change["textDocument"]["uri"]),
                    edits[0]["newText"].as_str().map(str::to_string),
                );
            }
        }
    }
    (files, renames)
}

/// What `jadpo rename <name> <new>` writes over the same files: each
/// changed file's text after, or `None` for one it removes.
fn command(root: &Path, name: &str, new: &str) -> Texts {
    let mut loaded = jadpo_db::load(root);
    let graph = jadpo_check::graph::graph(&loaded.db, loaded.project);
    let key = graph
        .nodes()
        .iter()
        .find(|node| node.name == name)
        .map(|node| node.key.clone())
        .expect("declared");
    let plan = rename::plan(&loaded.db, loaded.project, &key, new).expect("planned");
    let renamed = rename::apply(&mut loaded, &plan).expect("renamed");
    renamed
        .files
        .iter()
        .map(|change| (change.path.clone(), change.after.clone()))
        .collect()
}

#[test]
fn the_server_says_it_renames_and_asks_first_when_the_editor_can() {
    let root = project("lsp-rename-capability", PLANNING);
    let params = |capabilities: Value| {
        serde_json::from_value::<lsp_types::InitializeParams>(json!({
            "processId": null,
            "rootUri": format!("file://{}", root.display()),
            "capabilities": capabilities,
        }))
        .expect("initialize params")
    };
    let provider = |capabilities: Value| {
        let (_, result) = Server::initialize(&params(capabilities), &root);
        serde_json::to_value(result).expect("result")["capabilities"]["renameProvider"].clone()
    };
    assert_eq!(
        provider(json!({"textDocument": {"rename": {"prepareSupport": true}}})),
        json!({"prepareProvider": true})
    );
    assert_eq!(provider(json!({})), json!(true));
}

#[test]
fn prepare_rename_gives_the_name_under_the_cursor() {
    let root = project("lsp-rename-prepare", PLANNING);
    let mut server = editor(&root);
    let query = text_of(QUERY);
    // On `priority` in `order: [Todo.priority desc]`.
    let found = prepare(&mut server, &root, QUERY, at(query, "priority", 0, 2)).expect("found");
    assert_eq!(found["placeholder"], "priority");
    assert_eq!(found["range"]["start"], at(query, "priority", 0, 0));
    assert_eq!(found["range"]["end"], at(query, "priority desc", 0, 8));
    // On the entity's name, its own.
    let found = prepare(&mut server, &root, QUERY, at(query, "from: Todo", 0, 7)).expect("found");
    assert_eq!(found["placeholder"], "Todo");
    // Nowhere near a name.
    assert_eq!(
        prepare(&mut server, &root, QUERY, at(query, "limit: 50", 0, 8)),
        Ok(Value::Null)
    );
}

#[test]
fn a_stored_field_renamed_in_the_editor_gets_the_edits_and_the_identity_line_of_the_command() {
    let root = project("lsp-rename-field", PLANNING);
    let mut server = editor(&root);
    let add_todo = text_of(ADD_TODO);
    // On `priority` in `priority: Todo.priority`, the field.
    let edit = rename_at(
        &mut server,
        &root,
        ADD_TODO,
        at(add_todo, "Todo.priority", 0, 6),
        "importance",
    )
    .expect("renamed");
    let (files, renames) = applied(&root, &edit);
    assert!(renames.is_empty(), "{renames:?}");
    assert_eq!(files, command(&root, "Todo.priority", "importance"));
    assert_eq!(
        files["domains/planning/actions/add_todo.jadpo"].as_deref(),
        Some(
            "use planning/entities { Todo }\n\n/// Adds a todo.\naction add_todo(title: Todo.title, priority: Todo.importance) -> Todo {\n    return Todo.create {\n        set: {\n            title: title\n            importance: priority\n        }\n    }\n}\n"
        )
    );
    // The line that keeps the column: the registry is created.
    let registry = files["database/identities.jadpo_record"]
        .as_deref()
        .expect("the registry");
    assert!(
        registry.ends_with("\nfield Todo.priority -> importance\n"),
        "{registry}"
    );
    let creates: Vec<&Value> = edit["documentChanges"]
        .as_array()
        .expect("changes")
        .iter()
        .filter(|change| change["kind"] == "create")
        .collect();
    assert_eq!(creates.len(), 1, "{creates:?}");
    assert!(
        creates[0]["uri"]
            .as_str()
            .is_some_and(|uri| uri.ends_with("/database/identities.jadpo_record"))
    );

    // The server's texts are as the editor has them until it applies the
    // edit.
    let loaded = server.loaded().expect("project");
    assert_eq!(
        loaded
            .file(ADD_TODO)
            .map(|file| file.text(&loaded.db).clone()),
        Some(add_todo.to_string())
    );
    assert_eq!(loaded.record("database/identities.jadpo_record"), None);
}

#[test]
fn a_declaration_in_its_own_file_renames_the_file_and_its_test() {
    let root = project("lsp-rename-file", PLANNING);
    let mut server = editor(&root);
    let list = text_of("domains/planning/actions/list_todos.jadpo");
    // On the query's name where it is called.
    let edit = rename_at(
        &mut server,
        &root,
        "domains/planning/actions/list_todos.jadpo",
        at(list, "return todos()", 0, 8),
        "pending_todos",
    )
    .expect("renamed");
    let (files, renames) = applied(&root, &edit);
    assert_eq!(
        renames,
        [(
            QUERY.to_string(),
            "domains/planning/queries/pending_todos.jadpo".to_string()
        )]
    );
    assert_eq!(files, command(&root, "todos", "pending_todos"));

    // An action with a test beside it: both files follow it.
    let root = project("lsp-rename-file-test", PLANNING);
    let mut server = editor(&root);
    let add_todo = text_of(ADD_TODO);
    let edit = rename_at(
        &mut server,
        &root,
        ADD_TODO,
        at(add_todo, "add_todo(", 0, 0),
        "add_task",
    )
    .expect("renamed");
    let (files, renames) = applied(&root, &edit);
    assert_eq!(
        renames,
        [
            (
                ADD_TODO.to_string(),
                "domains/planning/actions/add_task.jadpo".to_string()
            ),
            (
                "domains/planning/actions/add_todo.test.jadpo".to_string(),
                "domains/planning/actions/add_task.test.jadpo".to_string()
            ),
        ]
    );
    assert_eq!(files, command(&root, "add_todo", "add_task"));
    // Each renamed file's edit replaces the text it had under its old name.
    let changes = edit["documentChanges"].as_array().expect("changes");
    let renamed_at = changes
        .iter()
        .position(|change| change["kind"] == "rename")
        .expect("a rename");
    let edited_at = changes
        .iter()
        .position(|change| {
            change["textDocument"]["uri"]
                .as_str()
                .is_some_and(|uri| uri.ends_with("/actions/add_task.jadpo"))
        })
        .expect("an edit of the new file");
    assert!(renamed_at < edited_at, "renamed before it is edited");
    let lines = add_todo.lines().count() as u64;
    assert_eq!(
        changes[edited_at]["edits"][0]["range"]["end"]["line"],
        lines
    );
}

#[test]
fn a_new_name_the_command_refuses_is_refused_with_why() {
    let root = project("lsp-rename-refused", PLANNING);
    let mut server = editor(&root);
    let add_todo = text_of(ADD_TODO);
    let position = at(add_todo, "add_todo(", 0, 0);
    assert_eq!(
        rename_at(&mut server, &root, ADD_TODO, position.clone(), "AddTask"),
        Err("`AddTask` isn't a name for an action. An action's name is written in lower_snake_case, as `add_task` is.".to_string())
    );
    assert_eq!(
        rename_at(&mut server, &root, ADD_TODO, position.clone(), "add_todo"),
        Err("`add_todo` is the name it has. A rename gives a declaration or a field a new name, and `add_todo` already has this one.".to_string())
    );
    let taken =
        rename_at(&mut server, &root, ADD_TODO, position, "list_todos").expect_err("a name in use");
    assert!(taken.starts_with("`list_todos` "), "{taken}");
}

#[test]
fn what_the_command_doesnt_rename_is_refused_before_a_name_is_asked_for() {
    let root = project("lsp-rename-not-renamed", PLANNING);
    let mut server = editor(&root);
    let add_todo = text_of(ADD_TODO);
    // The parameter `title`, where the body reads it.
    assert_eq!(
        prepare(&mut server, &root, ADD_TODO, at(add_todo, "title: title", 0, 8)),
        Err("`title` is a parameter. `jadpo rename` renames entities, types, functions, constants, actions, queries, fixtures, seeds, schedules, subscribers and libraries, and their fields and variants.".to_string())
    );
    assert_eq!(
        rename_at(
            &mut server,
            &root,
            ADD_TODO,
            at(add_todo, "title: title", 0, 8),
            "heading"
        ),
        Err("`title` is a parameter. `jadpo rename` renames entities, types, functions, constants, actions, queries, fixtures, seeds, schedules, subscribers and libraries, and their fields and variants.".to_string())
    );
}

#[test]
fn a_file_that_doesnt_parse_stops_it_as_it_stops_the_command() {
    let mut files = PLANNING.to_vec();
    files.push((
        "domains/planning/actions/broken.jadpo",
        "action broken( {\n",
    ));
    let root = project("lsp-rename-unparsed", &files);
    let mut server = editor(&root);
    let add_todo = text_of(ADD_TODO);
    let refused = prepare(
        &mut server,
        &root,
        ADD_TODO,
        at(add_todo, "add_todo(", 0, 0),
    )
    .expect_err("stopped");
    assert!(
        refused.starts_with("domains/planning/actions/broken.jadpo doesn't parse, so nothing is renamed until it does: "),
        "{refused}"
    );
}
