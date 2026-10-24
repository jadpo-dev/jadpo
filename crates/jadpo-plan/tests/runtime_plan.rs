//! The runtime's plan in its canonical bytes (roadmap chunk 8.1a): compiled
//! code holds the plan as a constant and decodes it as it starts
//! (docs/compiler-runtime.md §2.5), so for every project of the
//! specification corpus (the language pages' `check` projects and the
//! teaching examples) the bytes must give back an equal plan, that plan
//! must write the same bytes again, and the same project must give the
//! same bytes however often it is planned.

use crate::syntax_examples as examples;

use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::Plan;
use jadpo_std::RuntimePlan;

/// The plan of `files`, as a page's checker loads them, when they check
/// without an error that stops a run.
fn planned(files: &[(String, String)]) -> Option<Vec<u8>> {
    let db = Database::default();
    let mut sorted = files.to_vec();
    sorted.sort();
    let inputs: Vec<File> = sorted
        .iter()
        .map(|(path, text)| File::new(&db, path.clone(), text.clone()))
        .collect();
    let layout = if files.iter().any(|(path, _)| path == "app.jadpo") {
        Layout::Project
    } else {
        Layout::Loose
    };
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        layout,
        Default::default(),
        Vec::new(),
    );
    bytes_of(&db, project)
}

/// The bytes of `project`'s runtime plan, checked to read back equal and
/// to write the same bytes again; `None` for a project with errors.
fn bytes_of(db: &Database, project: Project) -> Option<Vec<u8>> {
    let blocked = jadpo_check::check(db, project)
        .into_iter()
        .any(|diagnostic| jadpo_check::blocks_running(&diagnostic));
    if blocked {
        return None;
    }
    let plan = Plan::build(db, project);
    let bytes = plan
        .runtime()
        .encode()
        .expect("the plan has a canonical form");
    let read = RuntimePlan::decode(&bytes).expect("its bytes read back");
    assert!(read == *plan.runtime(), "the plan read back differs");
    assert_eq!(read.encode().expect("encodes"), bytes, "written again");
    Some(bytes)
}

#[test]
fn every_page_projects_plan_reads_back_from_its_bytes() {
    let mut planned_count = 0;
    let mut problems = Vec::new();
    for (name, files) in examples::projects() {
        if name.starts_with("examples ") {
            continue;
        }
        let result = std::panic::catch_unwind(|| planned(&files));
        match result {
            Ok(Some(_)) => planned_count += 1,
            Ok(None) => {}
            Err(_) => problems.push(name),
        }
    }
    assert!(
        problems.is_empty(),
        "plans that don't read back: {problems:?}"
    );
    assert!(planned_count > 150, "the pages were read: {planned_count}");
}

#[test]
fn every_teaching_examples_plan_reads_back_and_is_the_same_twice() {
    let root = examples::root().join("examples");
    let mut folders: Vec<_> = std::fs::read_dir(&root)
        .expect("examples/")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    folders.sort();
    assert!(folders.len() >= 28, "the examples were found: {folders:?}");
    for folder in folders {
        let first = jadpo_db::load(&folder);
        let bytes = bytes_of(&first.db, first.project)
            .unwrap_or_else(|| panic!("{} checks", folder.display()));
        // A second reading, into a database of its own: a plan holding
        // anything in hash order would come out in another order.
        let second = jadpo_db::load(&folder);
        let again = bytes_of(&second.db, second.project).expect("it checks again");
        assert!(bytes == again, "{} planned twice differs", folder.display());
        // Every example's plan holds something: the header at
        // least, and its routes or settings.
        assert!(
            bytes.starts_with(RuntimePlan::HEADER),
            "{}",
            folder.display()
        );
    }
}

#[test]
fn bytes_of_another_version_or_cut_short_are_refused() {
    let files = vec![(
        "main.jadpo".to_string(),
        "type Count = Int {\n    max: 12\n}\n\nfunction one() -> Count {\n    return 1\n}\n"
            .to_string(),
    )];
    let bytes = planned(&files).expect("it checks");
    assert!(RuntimePlan::decode(&bytes).is_ok());
    // The header names the Jadpo that wrote the plan, taken from its
    // version rather than a number bumped by hand.
    assert_eq!(
        RuntimePlan::HEADER,
        format!("jadpo runtime plan {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    let mut other = b"jadpo runtime plan 0.0.0-another\n".to_vec();
    other.extend_from_slice(&bytes[RuntimePlan::HEADER.len()..]);
    assert!(
        RuntimePlan::decode(&other).is_err(),
        "another version's header"
    );
    assert!(RuntimePlan::decode(&bytes[..bytes.len() - 1]).is_err());
    assert!(RuntimePlan::decode(b"").is_err());
}

/// What only `jadpo test` and `jadpo dev` read, each test's key, its
/// `config:` and `clock:` and the recordings, is the plan's dev and test
/// section (chunk 8.1e): written after the rest of the plan's bytes, and
/// left out of a deployable's, which then holds no test's name.
#[test]
fn a_plans_tests_are_its_dev_and_test_section_which_a_deployable_leaves_out() {
    let source = "type FromAddress = Email {}\n\nconfig {\n    mail_sender: FromAddress {\n        test: \"reminders@example.test\"\n    }\n}\n\nfunction address() -> FromAddress {\n    return config.mail_sender\n}\n\ntest \"reminders come from the test sender\" {\n    assert address() == \"reminders@example.test\"\n}\n\ntest \"a test can change the sender\" {\n    config: {\n        mail_sender: \"planning@example.test\"\n    }\n    clock: \"2026-01-15T12:00:00Z\"\n    assert address() == \"planning@example.test\"\n}\n";
    let db = Database::default();
    let project = Project::new(
        &db,
        vec![File::new(&db, "main.jadpo".to_string(), source.to_string())],
        Vec::new(),
        Layout::Loose,
        Default::default(),
        Vec::new(),
    );
    let bytes = bytes_of(&db, project).expect("it checks");
    let plan = Plan::build(&db, project);
    let runtime = plan.runtime();
    // Each test by its key, the other callables by theirs.
    let changed = runtime
        .testing
        .tests
        .iter()
        .find(|(key, _)| key.contains("a test can change the sender"))
        .map(|(_, number)| *number)
        .expect("the test is in the section");
    assert_eq!(runtime.testing.tests.len(), 2);
    assert_eq!(runtime.callable("function address"), plan.find("address"));
    assert!(
        runtime
            .callables
            .keys()
            .all(|key| !key.starts_with("test ")),
        "{:?}",
        runtime.callables
    );
    // What the test gives itself.
    let setup = runtime.testing.setups.get(&changed).expect("its setup");
    assert_eq!(setup.clock, Some(1_768_478_400_000));
    assert_eq!(setup.settings.len(), 1);
    assert_eq!(runtime.testing.setups.len(), 1);
    // A deployable's bytes are the rest of the plan's, and read back
    // with an empty section.
    let deployable = runtime.encode_deployable().expect("encodes");
    assert!(bytes.starts_with(&deployable) && bytes.len() > deployable.len());
    let read = RuntimePlan::decode(&deployable).expect("reads back");
    assert!(read.testing.is_empty());
    assert_eq!(read.callables, runtime.callables);
    let named = |bytes: &[u8], name: &[u8]| bytes.windows(name.len()).any(|at| at == name);
    assert!(named(&bytes, b"a test can change the sender"));
    assert!(!named(&deployable, b"a test can change the sender"));
}

/// A declared deployable's plan (chunk 8.1o2): the routes of the APIs it
/// serves, none of another's, and the background work it runs.
#[test]
fn a_deployables_plan_holds_only_the_routes_it_serves() {
    let route = |group: &str, api: &str, prefix: &str| {
        format!(
            "use desk/types {{ Words, Said }}\nuse desk/actions {{ echo }}\n\n/// {group}.\nroutes {group} {{\n    prefix: /{prefix}\n{api}\n    /// Says it back.\n    POST / {{\n        authentication: RouteAuthentication.none\n        input: Words\n        output: Said\n        run: echo(input)\n    }}\n}}\n"
        )
    };
    let files = [
        (
            "app.jadpo".to_string(),
            "application {\n    apis: {\n        main: {\n            prefix: /\n        }\n        staff: {\n            prefix: /staff\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/desk/types/line.jadpo".to_string(),
            "type Line = Text {\n    max_length: 40\n}\n".to_string(),
        ),
        (
            "domains/desk/types/words.jadpo".to_string(),
            "use desk/types { Line }\n\ntype Words = Input {\n    line: Line\n}\n".to_string(),
        ),
        (
            "domains/desk/types/said.jadpo".to_string(),
            "use desk/types { Line }\n\ntype Said = Output {\n    line: Line\n}\n".to_string(),
        ),
        (
            "domains/desk/actions/echo.jadpo".to_string(),
            "use desk/types { Said, Words }\n\naction echo(input: Words) -> Said {\n    return Said {\n        line: input.line\n    }\n}\n".to_string(),
        ),
        (
            "policy.jadpo".to_string(),
            "use desk\n\npolicy {\n    actions: {\n        desk.echo: [Access.public]\n    }\n}\n".to_string(),
        ),
        (
            "domains/desk/routes/front.jadpo".to_string(),
            route("Front", "", "front"),
        ),
        (
            "domains/desk/routes/staff.jadpo".to_string(),
            route("Staff", "    api: staff\n", "staff"),
        ),
    ];
    let db = Database::default();
    let inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(&db, path.clone(), text.clone()))
        .collect();
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        Layout::Project,
        Default::default(),
        Vec::new(),
    );
    assert!(bytes_of(&db, project).is_some(), "it checks");
    let paths = |plan: &Plan| {
        plan.runtime()
            .routes
            .routes
            .iter()
            .filter(|route| route.path().ends_with("/front") || route.path().ends_with("/staff"))
            .map(|route| route.path())
            .collect::<Vec<_>>()
    };
    let whole = Plan::build(&db, project);
    assert_eq!(paths(&whole), ["/front", "/staff"]);
    assert_eq!(whole.runtime().runs, jadpo_std::Runs::EVERYTHING);
    let runs = jadpo_std::Runs {
        schedules: false,
        subscribers: true,
    };
    let deployable = jadpo_check::deployables::Deployable {
        name: "kiosk".to_string(),
        at: None,
        serves: vec!["main".to_string()],
        runs,
        target: None,
    };
    let kiosk = Plan::for_deployable(&db, project, &deployable).expect("planned");
    assert_eq!(paths(&kiosk), ["/front"]);
    assert_eq!(kiosk.runtime().runs, runs);
    let read = RuntimePlan::decode(&kiosk.runtime().encode_deployable().expect("encodes"))
        .expect("reads back");
    assert_eq!(read.runs, runs);
    assert_eq!(
        read.routes.routes.len(),
        kiosk.runtime().routes.routes.len()
    );
}

#[test]
fn external_tokens_plan_one_subject_lookup_without_credential_tables() {
    let db = Database::default();
    let source = r#"config {
    issuer: Url
    audience: Text
}
principal {
    user: {
        subject: Reader.handle
        active: Reader.active == true
    }
}
authentication {
    external_token: {
        issuer: config.issuer
        audience: config.audience
    }
}
entity Reader {
    id: Uuid
    handle: Text {
        unique: true
    }
    active: Bool
}
"#;
    let project = Project::new(
        &db,
        vec![File::new(&db, "main.jadpo".into(), source.into())],
        Vec::new(),
        Layout::Loose,
        Default::default(),
        Vec::new(),
    );
    let problems = jadpo_check::check(&db, project);
    assert!(
        !problems.iter().any(jadpo_check::blocks_running),
        "{problems:?}"
    );
    let plan = Plan::build(&db, project);
    let identity = plan.routes().identity.as_ref().expect("identity");
    let external = identity.external_token.as_ref().expect("external tokens");
    assert_eq!(external.issuer, "issuer");
    assert_eq!(external.audience, "audience");
    assert!(
        external.lookup.contains("\"handle\" = $1"),
        "{}",
        external.lookup
    );
    assert!(
        external.lookup.contains("\"active\""),
        "{}",
        external.lookup
    );
    assert!(
        !external.lookup.contains("jadpo_sessions"),
        "{}",
        external.lookup
    );
    assert!(!external.lookup.contains("JOIN"), "{}", external.lookup);
    assert!(identity.user.as_ref().expect("user").lookup.is_none());
    assert!(!identity.api_tokens);
    assert!(identity.signing_keys.is_empty());
    assert!(
        !jadpo_check::data::schema(&db, project)
            .to_document()
            .contains("jadpo_sessions")
    );
    let bytes = bytes_of(&db, project).expect("encodes");
    let restored = RuntimePlan::decode(&bytes).expect("decodes");
    assert_eq!(restored.routes.identity, plan.runtime().routes.identity);
}
