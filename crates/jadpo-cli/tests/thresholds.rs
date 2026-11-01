//! Monitoring thresholds (area 16, "Monitoring thresholds"; chunk B13),
//! end to end: `jadpo check` reads each domain's `thresholds { }`,
//! refuses one that watches nothing its domain has or gives a limit that
//! doesn't apply, writes the across-request limits and the disk floors to
//! `build/monitoring/` as Prometheus and OpenSLO rules and the manifest,
//! and, against the before side read from git, refuses a limit raised or
//! removed and a floor lowered until an acceptance names it, while a
//! tightened threshold passes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Jadpo Tests",
            "-c",
            "user.email=tests@jadpo.dev",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
    fs::write(path, text).expect("written");
}

fn edit(root: &Path, path: &str, from: &str, to: &str) {
    let text = fs::read_to_string(root.join(path)).expect("read");
    assert_eq!(text.matches(from).count(), 1, "{from:?} in {path}");
    fs::write(root.join(path), text.replacen(from, to, 1)).expect("written");
}

const MONITORING: &str = r#"use hello/actions { greet }

thresholds {
    /// A visitor is greeted quickly.
    greeting_route: {
        route: {
            method: HttpMethod.post
            path: "/hello"
        }
        duration: 800ms
        queries: 2
        p95: 300ms
        server_failures: 1%
    }

    /// Working out a greeting stays quick.
    greeting: {
        watches: greet
        p99: 50ms
        within: 10m
    }
}
"#;

/// A greeter whose `hello` domain watches its route and its action, with
/// a disk store of uploads kept at `uploads` with a 5% floor.
fn greeter(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("thresholds-{name}"));
    let _ = fs::remove_dir_all(&root);
    let uploads = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("thresholds-{name}-uploads"));
    let app = format!(
        "application {{\n    connections: {{\n        uploads: {{\n            driver: Driver.disk\n            path: \"{}\"\n            free_space_floor: 5%\n        }}\n    }}\n}}\n",
        uploads.display()
    );
    let files: Vec<(&str, &str)> = vec![
        ("app.jadpo", &app),
        (
            "policy.jadpo",
            "use hello\n\npolicy {\n    actions: {\n        hello.greet: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/hello/types/visitor.jadpo",
            "/// Who is calling.\ntype Visitor = Input {\n    name: Text {\n        max_length: 40\n    }\n}\n",
        ),
        (
            "domains/hello/types/greeting.jadpo",
            "/// What a visitor is told.\ntype Greeting = Output {\n    salutation: Text\n}\n",
        ),
        (
            "domains/hello/actions/greet.jadpo",
            "use hello/types { Greeting, Visitor }\n\n/// Greets a visitor by name.\naction greet(input: Visitor) -> Greeting {\n    return Greeting {\n        salutation: \"Hello, {input.name}\"\n    }\n}\n",
        ),
        (
            "domains/hello/routes/hello.jadpo",
            "use hello/types { Greeting, Visitor }\nuse hello/actions { greet }\n\n/// Greetings.\nroutes Hello {\n    prefix: /hello\n\n    /// Greets whoever calls it.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Visitor\n        output: Greeting\n        run: greet(input)\n    }\n}\n",
        ),
        ("domains/hello/monitoring.jadpo", MONITORING),
    ];
    for (path, text) in files {
        write(&root, path, text);
    }
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    root
}

/// The greeter committed on `main`, then a branch `change` checked out.
fn committed(name: &str) -> PathBuf {
    let root = greeter(name);
    git(&root, &["init", "-q"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "The greeter"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    git(&root, &["checkout", "-q", "-b", "change"]);
    root
}

/// The rules and messages `jadpo check --json` reports.
fn problems(root: &Path) -> Vec<String> {
    let output = jadpo(root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|d| {
            format!(
                "{}: {}",
                d["rule"].as_str().unwrap_or(""),
                d["message"].as_str().unwrap_or("")
            )
        })
        .collect()
}

#[test]
fn check_writes_the_rules_and_the_manifest_carries_every_threshold() {
    let root = greeter("rules");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let prometheus = fs::read_to_string(root.join("build/monitoring/prometheus.yaml"))
        .expect("prometheus rules");
    for line in [
        "  - name: \"jadpo.hello\"",
        "      - alert: hello_greeting_route_p95",
        "        expr: \"histogram_quantile(0.95, sum by (le) (rate(http_server_request_duration_seconds_bucket{http_request_method=\\\"POST\\\", http_route=\\\"/hello\\\"}[5m]))) > 0.3\"",
        "      - alert: hello_greeting_route_server_failures",
        "      - alert: hello_greeting_p99",
        "        expr: \"histogram_quantile(0.99, sum by (le) (rate(jadpo_run_duration_seconds_bucket{jadpo_kind=\\\"action\\\", jadpo_name=\\\"hello.greet\\\"}[10m]))) > 0.05\"",
        "          summary: \"Working out a greeting stays quick.\"",
        "  - name: \"jadpo.files\"",
        "      - alert: uploads_free_space_floor",
        "        expr: \"min(jadpo_files_free_ratio{jadpo_connection=\\\"uploads\\\"}) < 0.05\"",
    ] {
        assert!(
            prometheus.lines().any(|l| l == line),
            "{line}\n{prometheus}"
        );
    }
    // `duration:` and `queries:` are judged per request, in the runtime.
    assert!(!prometheus.contains("_duration\n"), "{prometheus}");
    assert!(!prometheus.contains("queries"), "{prometheus}");
    let openslo = fs::read_to_string(root.join("build/monitoring/openslo.yaml")).expect("openslo");
    for line in [
        "kind: Service",
        "  name: \"hello-greeting-route-p95\"",
        "  budgetingMethod: Occurrences",
        "    - duration: 10m",
        "      target: 0.95",
        "      target: 0.99",
    ] {
        assert!(openslo.lines().any(|l| l == line), "{line}\n{openslo}");
    }
    assert!(
        !openslo.contains("uploads"),
        "a floor is a condition: {openslo}"
    );
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/manifest.json")).expect("manifest"),
    )
    .expect("JSON");
    let nodes = manifest["nodes"].as_array().expect("nodes");
    let node = |id: &str| {
        nodes
            .iter()
            .find(|node| node["id"] == id)
            .unwrap_or_else(|| panic!("{id} in the manifest"))
            .clone()
    };
    let route = node("threshold hello.greeting_route");
    assert_eq!(route["watches"], "route");
    assert_eq!(route["uses"], serde_json::json!(["route POST /hello"]));
    assert_eq!(
        route["perRequest"],
        serde_json::json!({"duration": "800ms", "queries": "2"})
    );
    assert_eq!(
        route["acrossRequests"],
        serde_json::json!({"p95": "300ms", "server_failures": "1%"})
    );
    assert_eq!(route["within"], "5m");
    let action = node("threshold hello.greeting");
    assert_eq!(action["uses"], serde_json::json!(["action greet"]));
    assert_eq!(action["within"], "10m");
    let floor = node("threshold uploads.free_space_floor");
    assert_eq!(floor["watches"], "disk");
    assert_eq!(
        floor["acrossRequests"],
        serde_json::json!({"free_space_floor": "5%"})
    );
    // With no thresholds and no disk store, there are no rules to apply.
    fs::remove_file(root.join("domains/hello/monitoring.jadpo")).expect("removed");
    let app = fs::read_to_string(root.join("app.jadpo")).expect("app");
    let start = app.find("        uploads: {").expect("uploads");
    let end = app[start..].find("        }\n").expect("its end") + start + "        }\n".len();
    fs::write(
        root.join("app.jadpo"),
        format!("{}{}", &app[..start], &app[end..]),
    )
    .expect("app");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert!(!root.join("build/monitoring/prometheus.yaml").exists());
    assert!(!root.join("build/monitoring/openslo.yaml").exists());
}

#[test]
fn a_threshold_must_watch_something_its_domain_has_with_limits_that_apply() {
    let root = greeter("mistakes");
    write(
        &root,
        "domains/hello/monitoring.jadpo",
        r#"use hello/actions { greet }

thresholds {
    wrong_path: {
        route: {
            method: HttpMethod.post
            path: "/helo"
        }
        p95: 300ms
    }

    nothing: {
        watches: grete
        p95: 300ms
    }

    waiting: {
        watches: greet
        oldest_waiting: 5m
    }

    past_its_deadline: {
        route: {
            method: HttpMethod.post
            path: "/hello"
        }
        duration: 10s
    }

    judged_alone: {
        watches: greet
        duration: 20ms
        within: 5m
    }

    no_limit: {
        watches: greet
    }
}
"#,
    );
    assert_eq!(
        problems(&root),
        [
            "threshold.subject_unknown: No route answers `POST /helo`",
            "threshold.subject_unknown: Nothing called `grete` to watch",
            "threshold.declaration_invalid: `oldest_waiting:` can't watch an action",
            "threshold.declaration_invalid: `duration:` on `past_its_deadline` isn't shorter than `POST /hello`'s deadline",
            "threshold.declaration_invalid: `within:` on `judged_alone` judges nothing",
            "threshold.declaration_invalid: `no_limit` has no limit",
        ]
    );
}

#[test]
fn a_raised_or_removed_threshold_needs_a_person_and_a_tightened_one_does_not() {
    let root = committed("loosened");
    // Tightened: lower limits, and a new threshold, need nothing.
    edit(
        &root,
        "domains/hello/monitoring.jadpo",
        "p95: 300ms",
        "p95: 250ms",
    );
    edit(
        &root,
        "domains/hello/monitoring.jadpo",
        "within: 10m",
        "within: 5m",
    );
    edit(
        &root,
        "app.jadpo",
        "free_space_floor: 5%",
        "free_space_floor: 10%",
    );
    assert_eq!(problems(&root), Vec::<String>::new());
    // Loosened: a limit raised, a limit gone, a threshold gone, a floor
    // lowered.
    let monitoring = MONITORING
        .replace("p95: 300ms", "p95: 400ms")
        .replace("        queries: 2\n", "");
    let start = monitoring.find("\n    /// Working out").expect("second");
    let end = monitoring.rfind("}\n}\n").expect("end") + 2;
    let monitoring = format!("{}\n{}", &monitoring[..start], &monitoring[end..])
        .replace("use hello/actions { greet }\n\n", "");
    write(&root, "domains/hello/monitoring.jadpo", &monitoring);
    edit(
        &root,
        "app.jadpo",
        "free_space_floor: 10%",
        "free_space_floor: 2%",
    );
    assert_eq!(jadpo(&root, &["fmt"]).code, 0);
    assert_eq!(
        problems(&root),
        [
            "acceptance.missing: lowering a disk store's floor fills more of its disk: uploads.free_space_floor",
            "acceptance.missing: removing a threshold can silence its alert: hello.greeting, hello.greeting_route.queries",
            "acceptance.missing: raising a threshold can silence its alert: hello.greeting_route.p95",
        ]
    );
    let drafted = jadpo(&root, &["acceptance", "new", "quieter_greetings"]);
    assert_eq!(drafted.code, 0, "{}", drafted.stdout);
    let file = fs::read_dir(root.join("acceptances"))
        .expect("acceptances")
        .filter_map(|entry| Some(entry.ok()?.path()))
        .next()
        .expect("an acceptance");
    let text = fs::read_to_string(&file).expect("read");
    assert_eq!(
        text,
        r#"acceptance {
    changes: {
        hello.greeting: ThresholdChange.removed
        hello.greeting_route.p95: ThresholdChange.raised
        hello.greeting_route.queries: ThresholdChange.removed
        uploads.free_space_floor: ThresholdChange.lowered
    }
}
"#
    );
    fs::write(
        &file,
        format!("/// Greetings are slower now that they are translated; the floor moved to a larger disk.\n{text}"),
    )
    .expect("written");
    assert_eq!(problems(&root), Vec::<String>::new());
}
