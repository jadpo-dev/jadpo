//! Fields and variants named with reserved words (area 2, "Names from
//! reserved words") through everything that reads a name: `jadpo check`, the
//! formatter, the API record, the OpenAPI document, a body on the wire in
//! the interpreter, a column in a migration, a policy line, a query and a
//! wording. Running them compiled, native and on workerd, is the corpus's
//! differential in `compiled.rs`, which takes area 2's project.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Env, NoStore};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use serde_json::Value;

use crate::http::tempdir;

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/registry/types/name.jadpo",
        "type Name = Text {\n    min_length: 1\n    max_length: 80\n}\n",
    ),
    (
        "domains/registry/types/kind.jadpo",
        "use registry/types { Name }\n\ntype Kind = Enum {\n    library\n    package\n    type\n    in\n    public\n    use\n    test: {\n        for: Name\n    }\n}\n",
    ),
    (
        "domains/registry/entities/release.jadpo",
        "use registry/types { Name }\n\nentity Release {\n    id: Uuid\n    package: Name\n    type: Name\n    in: Name?\n    true: Name?\n    select: Name?\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n\n    unique: {\n        one_of_each: [package, type]\n    }\n}\n",
    ),
    (
        "domains/registry/types/echoed.jadpo",
        "use registry/types { Kind, Name }\n\ntype Echoed = Output {\n    type: Name\n    if: Name?\n    match: Kind\n    none: Name?\n    true: Name\n    package: Name\n}\n",
    ),
    (
        "domains/registry/actions/echo.jadpo",
        "use registry/types { Echoed, Kind, Name }\n\ntype Echo = Input {\n    type: Name\n    if: Name?\n    kind: Kind\n}\n\naction echo(input: Echo) -> Echoed {\n    return Echoed {\n        type: input.type\n        if: input.if\n        match: input.kind\n        none: none\n        true: input.type\n        package: input.type\n    }\n}\n",
    ),
    (
        "domains/registry/routes/echoes.jadpo",
        "use registry/types { Echoed }\nuse registry/actions { Echo, echo }\n\nroutes Echoes {\n    prefix: /echoes\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Echo\n        output: Echoed\n        run: echo(input)\n    }\n}\n",
    ),
    (
        "policy.jadpo",
        "use registry\n\npolicy {\n    actions: {\n        registry.echo: [Access.public]\n    }\n}\n",
    ),
];

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("JADPO_PLAIN", "1");
    run(&args, &env.with_cwd(cwd))
}

fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("reserved");
    let root = dir.path().join("registry");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}", made.stdout);
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", updated.stdout);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    (dir, root)
}

fn post(root: &Path, path: &str, body: &str) -> jadpo_runtime::http::Response {
    let loaded = jadpo_db::load(root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Harness::new(NoStore);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut ctx = Ctx::of(
            &program,
            &host,
            Env {
                seed: Some(1),
                clock: Clock::TEST,
            },
        );
        let request = jadpo_runtime::http::Request {
            method: "POST".to_string(),
            path: path.to_string(),
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: body.as_bytes().to_vec(),
            ..Default::default()
        };
        jadpo_runtime::http::handle(&program.app(), &program, &mut ctx, &request, "req_1").await
    })
}

#[test]
fn the_wire_has_the_names_as_written() {
    let (_dir, root) = project();
    let response = post(
        &root,
        "/echoes",
        r#"{"type":"a","if":"b","kind":{"tag":"test","for":"c"}}"#,
    );
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let body: Value = serde_json::from_slice(&response.body).expect("JSON");
    assert_eq!(body["type"], "a");
    assert_eq!(body["if"], "b");
    assert_eq!(body["match"]["tag"], "test");
    assert_eq!(body["match"]["for"], "c");
    assert_eq!(body["true"], "a");
    assert_eq!(body["package"], "a");
    // A plain variant is its name; `in` is no operator on the way in.
    let response = post(&root, "/echoes", r#"{"type":"a","if":null,"kind":"in"}"#);
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).expect("JSON");
    assert_eq!(body["match"], "in");
    // A variant the enum doesn't have is still refused by name.
    let response = post(&root, "/echoes", r#"{"type":"a","if":null,"kind":"if"}"#);
    assert_eq!(response.status, 422);
}

#[test]
fn the_record_and_openapi_have_the_names_as_written() {
    let (_dir, root) = project();
    let record = fs::read_to_string(root.join("api/main.jadpo_record")).expect("the record");
    assert!(record.contains("    type: Text"), "{record}");
    assert!(record.contains("    if: Text"), "{record}");
    assert!(record.contains("    match: Kind"), "{record}");
    assert!(record.contains("    variant library"), "{record}");
    assert!(record.contains("    variant type"), "{record}");
    let document = fs::read_to_string(root.join("build/openapi/main.json")).expect("the document");
    let document: Value = serde_json::from_str(&document).expect("JSON");
    let schemas = &document["components"]["schemas"];
    let echoed = &schemas["Echoed"]["properties"];
    for field in ["type", "if", "match", "none", "true", "package"] {
        assert!(echoed.get(field).is_some(), "{field}: {echoed}");
    }
    let kind = serde_json::to_string(&schemas["Kind"]).expect("JSON");
    for variant in ["library", "package", "type", "in", "public", "use", "test"] {
        assert!(
            kind.contains(&format!("\"{variant}\"")),
            "{variant}: {kind}"
        );
    }
}

#[test]
fn a_column_is_quoted_and_the_formatter_keeps_the_names() {
    let (_dir, root) = project();
    let mut sql = String::new();
    for entry in fs::read_dir(root.join("database/migrations")).expect("migrations") {
        let path = entry.expect("an entry").path().join("postgres.sql");
        sql.push_str(&fs::read_to_string(path).expect("its statements"));
    }
    for column in [
        "\"package\"",
        "\"type\"",
        "\"in\"",
        "\"true\"",
        "\"select\"",
    ] {
        assert!(sql.contains(column), "{column}:\n{sql}");
    }
    let formatted = jadpo(&root, &["fmt", "--check"]);
    assert_eq!(formatted.code, 0, "{}", formatted.stdout);
}

#[test]
fn a_policy_line_and_a_search_name_the_field() {
    let dir = tempdir::Dir::new("reserved-policy");
    let root = dir.path().join("notes");
    for (path, text) in [
        (
            "app.jadpo",
            "use notes/entities { Member }\n\napplication {}\n\nprincipal {\n    user: {\n        subject: Member.use\n    }\n}\n",
        ),
        (
            "authentication.jadpo",
            "authentication {\n    browser_session: {\n        cookie: \"notes_session\"\n    }\n}\n",
        ),
        (
            "domains/notes/types/handle.jadpo",
            "type Handle = Text {\n    min_length: 1\n    max_length: 80\n}\n",
        ),
        (
            "domains/notes/entities/member.jadpo",
            "use notes/types { Handle }\n\nentity Member {\n    id: Uuid\n    use: Handle {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [id]\n    }\n}\n",
        ),
        (
            "domains/notes/entities/note.jadpo",
            "use notes/entities { Member }\nuse notes/types { Handle }\n\nentity Note {\n    id: Uuid\n    for: Member.id {\n        on_delete: OnDelete.cascade\n    }\n    in: Handle\n\n    policy: {\n        create: [for]\n        read: [for]\n    }\n\n    search: {\n        fields: [in]\n        language: SearchLanguage.english\n    }\n}\n",
        ),
    ] {
        write(&root, path, text);
    }
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}", made.stdout);
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", updated.stdout);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let access = fs::read_to_string(root.join("access.jadpo_record")).expect("the record");
    assert!(access.contains("for"), "{access}");
}

#[test]
fn a_wording_names_the_variant_and_a_placeholder_the_field() {
    let dir = tempdir::Dir::new("reserved-wording");
    let root = dir.path().join("shop");
    for (path, text) in [
        (
            "app.jadpo",
            "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
        ),
        (
            "domains/shop/types/reason.jadpo",
            "type Reason = Text {\n    max_length: 80\n}\n",
        ),
        (
            "domains/shop/types/stage.jadpo",
            "use shop/types { Reason }\n\ntype Stage = Enum {\n    library\n    type\n    test: {\n        for: Reason\n    }\n}\n",
        ),
        (
            "domains/shop/translations/en_gb.jadpo",
            "translations {\n    stage: {\n        library: \"In the library\"\n        type: \"A type\"\n        test: \"For {for}\"\n    }\n}\n",
        ),
        (
            "domains/shop/actions/describe.jadpo",
            "use shop/types { Stage }\n\ntype Described = Output {\n    if: PresentationText\n}\n\naction describe(stage: Stage, reader: Locale) -> Described {\n    return Described {\n        if: translations.shop.stage(stage, locale: reader)\n    }\n}\n",
        ),
        (
            "domains/shop/actions/describe.test.jadpo",
            "use shop/types { Stage }\n\ntest \"a variant is worded by its name\" {\n    assert translations.shop.stage(Stage.type, locale: Locale.en_gb) == \"A type\"\n    assert translations.shop.stage(\n        Stage.test {\n            for: \"you\"\n        },\n        locale: Locale.en_gb\n    ) == \"For you\"\n}\n",
        ),
    ] {
        write(&root, path, text);
    }
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", updated.stdout);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}", tested.stdout);
}

#[test]
fn a_path_parameter_is_not_named_with_a_reserved_word() {
    let dir = tempdir::Dir::new("reserved-path");
    let root = dir.path().join("shop");
    for (path, text) in [
        ("app.jadpo", "application {}\n"),
        (
            "domains/shop/routes/items.jadpo",
            "routes Items {\n    prefix: /items\n\n    GET /{type: Uuid} {\n        authentication: RouteAuthentication.none\n        run: nothing()\n    }\n}\n",
        ),
    ] {
        write(&root, path, text);
    }
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: syntax.expected_name"),
        "{}",
        checked.stdout
    );
}
