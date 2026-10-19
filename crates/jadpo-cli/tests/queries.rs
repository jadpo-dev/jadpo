//! Queries, `get` and `create` run against live PostgreSQL through `jadpo
//! test` (area 6): what a condition means where a field is `none`,
//! optional filters, pages and their cursors, a list included per record,
//! a read inside an action seeing its own writes, and the failures a read
//! raises. The server is the one `JADPO_TEST_DATABASE_URL` names or one
//! started from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1`
//! skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/planning/types/name.jadpo",
        "/// A name.\ntype Name = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/planning/types/status.jadpo",
        "/// Where a task stands.\ntype Status = Enum {\n    open\n    done\n}\n",
    ),
    (
        "domains/planning/types/day.jadpo",
        "/// A day a task is due.\ntype Day = CalendarDate {}\n",
    ),
    (
        "domains/planning/entities/project.jadpo",
        "use planning/types { Name }\n\n/// A project.\nentity Project {\n    id: Uuid\n    name: Name\n}\n",
    ),
    (
        "domains/planning/entities/task.jadpo",
        "use planning/entities { Project }\nuse planning/types { Day, Name, Status }\n\n/// A task.\nentity Task {\n    id: Uuid\n    project_id: Project.id {\n        on_delete: OnDelete.cascade\n    }\n    title: Name\n    status: Status\n    due_on: Day?\n}\n",
    ),
    (
        "domains/planning/failures/project_not_found.jadpo",
        "use planning/entities { Project }\n\n/// No such project.\ntype ProjectNotFound = Failure {\n    kind: FailureKind.not_found\n    client: {\n        project_id: Project.id\n    }\n}\n",
    ),
    (
        "domains/planning/types/task_view.jadpo",
        "use planning/entities { Task }\n\n/// A task in a list.\ntype TaskView = Output {\n    pick: [Task.title]\n}\n",
    ),
    (
        "domains/planning/types/board.jadpo",
        "use planning/entities { Project }\nuse planning/types { TaskView }\n\n/// A project and its first open task.\ntype Board = Output {\n    pick: [Project.name]\n    tasks: List<TaskView>\n}\n",
    ),
    (
        "domains/planning/types/filter.jadpo",
        "use planning/types { Status }\n\n/// Which tasks.\ntype Filter = Input {\n    status: Status {\n        optional: true\n    }\n    size: PageSize {\n        default: 2\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n",
    ),
    (
        "domains/planning/types/plan.jadpo",
        "use planning/entities { Project, Task }\n\n/// Two projects and their tasks, as the tests set them up.\ntype Plan = Object {\n    garden: Project\n    house: Project\n    tasks: List<Task>\n}\n",
    ),
    (
        "domains/planning/actions/add_project.jadpo",
        "use planning/entities { Project }\n\n/// Adds a project.\naction add_project(name: Project.name) -> Project {\n    return Project.create {\n        set: {\n            name: name\n        }\n    }\n}\n",
    ),
    (
        "domains/planning/actions/add_task.jadpo",
        "use planning/entities { Project, Task }\nuse planning/types { Status }\n\n/// Adds a task.\naction add_task(project: Project, title: Task.title, status: Status, due_on: Task.due_on) -> Task {\n    return Task.create {\n        set: {\n            project_id: project.id\n            title: title\n            status: status\n            due_on: due_on\n        }\n    }\n}\n",
    ),
    (
        "domains/planning/actions/add_and_count.jadpo",
        "use planning/entities { Project, Task }\nuse planning/types { Status, TaskView }\nuse planning/actions { add_task }\nuse planning/queries { tasks_due }\n\n/// Adds a task and reads the project's tasks in the same action.\naction add_and_count(project: Project) -> List<TaskView> {\n    var task: Task = add_task(project, \"Later\", Status.open, none)\n    return tasks_due(task.project_id, \"2000-01-01\")\n}\n",
    ),
    (
        "domains/planning/actions/find_project.jadpo",
        "use planning/entities { Project }\nuse planning/failures { ProjectNotFound }\n\n/// A project by id.\naction find_project(project_id: Project.id) fails ProjectNotFound -> Project {\n    return attempt Project.get(project_id)\n}\n",
    ),
    (
        "domains/planning/fixtures/two_projects.jadpo",
        "use planning/entities { Project }\nuse planning/types { Plan, Status }\nuse planning/actions { add_project, add_task }\n\n/// Garden with three tasks and House with two.\nfixture two_projects() -> Plan {\n    var garden: Project = add_project(\"Garden\")\n    var house: Project = add_project(\"House\")\n    return Plan {\n        garden: garden\n        house: house\n        tasks: [\n            add_task(garden, \"Dig\", Status.open, \"2026-11-01\"),\n            add_task(garden, \"Plant\", Status.done, \"2026-11-02\"),\n            add_task(garden, \"Water\", Status.open, none),\n            add_task(house, \"Paint\", Status.open, \"2026-12-01\"),\n            add_task(house, \"Sweep\", Status.open, none),\n        ]\n    }\n}\n",
    ),
    (
        "domains/planning/queries/tasks_due.jadpo",
        "use planning/entities { Project, Task }\nuse planning/types { Day, TaskView }\n\n/// A project's tasks not due after a day, an undated one included.\nquery tasks_due(project_id: Project.id, day: Day) -> List<TaskView> {\n    from: Task\n    where: Task.project_id == project_id\n    where: not (Task.due_on > day)\n    order: [Task.title asc]\n    limit: 10\n}\n",
    ),
    (
        "domains/planning/queries/tasks_not_on.jadpo",
        "use planning/entities { Project, Task }\nuse planning/types { Day, TaskView }\n\n/// A project's tasks due any day but this one, an undated one included.\nquery tasks_not_on(project_id: Project.id, day: Day) -> List<TaskView> {\n    from: Task\n    where: Task.project_id == project_id\n    where: Task.due_on != day\n    order: [Task.title asc]\n    limit: 10\n}\n",
    ),
    (
        "domains/planning/queries/tasks_page.jadpo",
        "use planning/entities { Task }\nuse planning/types { Filter, TaskView }\n\n/// Every task, a page at a time.\nquery tasks_page(filter: Filter) -> Page<TaskView> {\n    from: Task\n    where_present: Task.status == filter.status\n    order: [Task.title desc]\n    page: {\n        size: filter.size\n        after: filter.after\n    }\n}\n",
    ),
    (
        "domains/planning/queries/other_page.jadpo",
        "use planning/entities { Task }\nuse planning/types { Filter, TaskView }\n\n/// Every task by status, a page at a time.\nquery other_page(filter: Filter) -> Page<TaskView> {\n    from: Task\n    order: [Task.title asc]\n    page: {\n        size: filter.size\n        after: filter.after\n    }\n}\n",
    ),
    (
        "domains/planning/queries/boards.jadpo",
        "use planning/entities { Project, Task }\nuse planning/types { Board, Status }\n\n/// Every project with its first open task.\nquery boards() -> List<Board> {\n    from: Project\n    order: [Project.name asc]\n    limit: 10\n    include: {\n        tasks: {\n            via: Task.project_id\n            where: Task.status == Status.open\n            order: [Task.title asc]\n            limit: 1\n        }\n    }\n}\n",
    ),
    (
        "domains/planning/queries/project_board.jadpo",
        "use planning/entities { Project, Task }\nuse planning/types { Board }\nuse planning/failures { ProjectNotFound }\n\n/// One project.\nquery project_board(project_id: Project.id) fails ProjectNotFound -> Board {\n    from: Project\n    where: Project.id == project_id\n    missing: ProjectNotFound {\n        project_id: project_id\n    }\n    include: {\n        tasks: {\n            via: Task.project_id\n            order: [Task.title asc]\n            limit: 1\n        }\n    }\n}\n",
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("queries-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    root
}

#[test]
fn queries_read_what_area_6_says() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("semantics");
    write(
        &root,
        "domains/planning/queries/tasks_due.test.jadpo",
        TESTS_FILE,
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("7 passed"), "{}", tested.stdout);
}

#[test]
fn a_cursor_from_another_query_is_invalid_input() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("cursor");
    write(
        &root,
        "domains/planning/queries/other_page.test.jadpo",
        "use planning/types { Filter, TaskView }\nuse planning/queries { tasks_page }\nuse planning/fixtures { two_projects }\n\ntest \"a cursor names its query\" {\n    assert list.count(two_projects().tasks) == 5\n    var first: Page<TaskView> = tasks_page(Filter {})\n    var wrong: Page<TaskView> = other_page(Filter { after: first.next })\n    assert list.count(wrong.items) == 2\n}\n",
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 1, "{}{}", tested.stdout, tested.stderr);
    let shown = format!("{}{}", tested.stdout, tested.stderr);
    assert!(shown.contains("invalid_input"), "{shown}");
    assert!(shown.contains("test.unexpected_failure"), "{shown}");
}

const TESTS_FILE: &str = "use planning/entities { Project }
use planning/types { Board, Filter, Status, TaskView }
use planning/failures { ProjectNotFound }
use planning/actions { add_and_count, find_project }
use planning/queries { boards, project_board, tasks_not_on, tasks_page }
use planning/fixtures { two_projects }

test \"not keeps a field that is none\" {
    var garden: Project = two_projects().garden
    var due: List<TaskView> = tasks_due(garden.id, \"2026-11-01\")
    assert list.map(due, task -> task.title) == [\"Dig\", \"Water\"]
}

test \"!= is met by a field that is none\" {
    var garden: Project = two_projects().garden
    var others: List<TaskView> = tasks_not_on(garden.id, \"2026-11-01\")
    assert list.map(others, task -> task.title) == [\"Plant\", \"Water\"]
}

test \"pages follow each other, a filter left out applying to nothing\" {
    assert list.count(two_projects().tasks) == 5
    var first: Page<TaskView> = tasks_page(Filter {})
    assert list.map(first.items, task -> task.title) == [\"Water\", \"Sweep\"]
    var second: Page<TaskView> = tasks_page(Filter { after: first.next })
    assert list.map(second.items, task -> task.title) == [\"Plant\", \"Paint\"]
    var last: Page<TaskView> = tasks_page(Filter { after: second.next })
    assert list.map(last.items, task -> task.title) == [\"Dig\"]
    assert last.next == none
    var done: Page<TaskView> = tasks_page(Filter { status: Status.done })
    assert list.map(done.items, task -> task.title) == [\"Plant\"]
}

test \"each record's list is ordered and limited on its own\" {
    assert list.count(two_projects().tasks) == 5
    var lists: List<List<TaskView>> = list.map(boards(), board -> board.tasks)
    assert lists == [[TaskView { title: \"Dig\" }], [TaskView { title: \"Paint\" }]]
}

test \"a read in an action sees the action's own writes\" {
    var garden: Project = two_projects().garden
    var due: List<TaskView> = add_and_count(garden)
    assert list.map(due, task -> task.title) == [\"Later\", \"Water\"]
}

test \"exactly one record, or the missing failure with its fields\" {
    var garden: Project = two_projects().garden
    var shown: Board = attempt project_board(garden.id)
    assert shown.tasks == [TaskView { title: \"Dig\" }]
    assert project_board(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\") fails ProjectNotFound
}

test \"get reads a record by id, or fails with its id\" {
    var garden: Project = two_projects().garden
    var found: Project = attempt find_project(garden.id)
    assert found == garden
    assert find_project(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\") fails ProjectNotFound
}
";
