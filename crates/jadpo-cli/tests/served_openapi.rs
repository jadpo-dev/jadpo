//! Each API serves its own OpenAPI document at `<prefix>/openapi.json`
//! (area 7, "OpenAPI"; chunk 7.2): the runtime answers with the document
//! `jadpo check` writes to `build/openapi/<api>.json`, byte for byte,
//! under the API's own default authentication, and `jadpo routes` lists
//! the route as generated. No store is used, so nothing here is live.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Env, NoStore};
use jadpo_term::Environment;
use jadpo_test::host::Harness;

use crate::http::tempdir;
use crate::support;

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    apis: {\n        main: {\n            prefix: /v1\n        }\n    }\n}\n",
    ),
    (
        "domains/memos/types/memo.jadpo",
        "type Memo = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/memos/actions/add_memo.jadpo",
        r#"use memos/types { Memo }

type AddMemo = Input {
    text: Memo
}

type MemoView = Output {
    text: Memo
}

action add_memo(input: AddMemo) -> MemoView {
    return MemoView {
        text: input.text
    }
}
"#,
    ),
    (
        "domains/memos/routes/v1_memos.jadpo",
        r#"use memos/actions { AddMemo, MemoView, add_memo }

/// Memos, kept for a moment.
routes V1Memos {
    prefix: /v1/memos

    POST / {
        authentication: RouteAuthentication.none
        input: AddMemo
        output: MemoView
        status: SuccessStatus.created
        run: add_memo(input)
    }
}
"#,
    ),
    (
        "policy.jadpo",
        "use memos\n\npolicy {\n    actions: {\n        memos.add_memo: [Access.public]\n    }\n}\n",
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
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn project(files: &[(&str, &str)]) -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("served");
    let root = dir.path().join("memos");
    for (path, text) in files {
        write(&root, path, text);
    }
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", updated.stdout);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    (dir, root)
}

fn get(root: &Path, path: &str, query: Option<&str>) -> jadpo_runtime::http::Response {
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
            method: "GET".to_string(),
            path: path.to_string(),
            query: query.map(str::to_string),
            ..Default::default()
        };
        jadpo_runtime::http::handle(&program.app(), &program, &mut ctx, &request, "req_1").await
    })
}

#[test]
fn the_api_answers_with_its_own_document() {
    let (_dir, root) = project(FILES);
    let written = fs::read_to_string(root.join("build/openapi/main.json")).expect("the document");
    let response = get(&root, "/v1/openapi.json", None);
    assert_eq!(response.status, 200);
    assert_eq!(String::from_utf8_lossy(&response.body), written);
    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value == "application/json"),
        "{:?}",
        response.headers
    );
    // It is in the document it serves, and its title is the application's.
    let document: serde_json::Value = serde_json::from_str(&written).expect("JSON");
    assert_eq!(document["info"]["title"], "memos");
    assert_eq!(
        document["paths"]["/v1/openapi.json"]["get"]["x-jadpo-generated"],
        true
    );
    // Like any route that takes no input, it refuses a query string.
    let refused = get(&root, "/v1/openapi.json", Some("all=1"));
    assert_eq!(
        refused.status,
        422,
        "{}",
        String::from_utf8_lossy(&refused.body)
    );
}

#[test]
fn jadpo_routes_lists_it_as_generated() {
    let (_dir, root) = project(FILES);
    let listed = jadpo(&root, &["routes"]);
    assert_eq!(listed.code, 0, "{}", listed.stdout);
    assert!(
        listed
            .stdout
            .contains("GET   /v1/openapi.json  generated openapi  none  generated"),
        "{}",
        listed.stdout
    );
}

#[test]
fn serve_openapi_false_leaves_the_path_unanswered() {
    let mut files = FILES.to_vec();
    files[0] = (
        "app.jadpo",
        "application {\n    apis: {\n        main: {\n            prefix: /v1\n            serve_openapi: false\n        }\n    }\n}\n",
    );
    let (_dir, root) = project(&files);
    assert_eq!(get(&root, "/v1/openapi.json", None).status, 404);
}
