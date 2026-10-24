//! Compiled code (roadmap chunks 8.1f to 8.1j; docs/compiler-runtime.md
//! §2.5, "Compiled code" and "Differential test"). Every project of the
//! specification corpus, the language pages' and the teaching examples',
//! written by `jadpo-codegen` as modules of one crate, one rustc
//! invocation, beside one harness built with cargo, and run beside the
//! interpreter: every test gives the same record both ways, a failed
//! assertion's values among them. Area 7's `todos` project, a route that
//! creates a todo and one that reads it back, is the first route. Its
//! route tests and teaching example 23's run inside workerd too, on each
//! one's compiled program built for wasm32, with the same records (chunk
//! 8.2e). The gate runs it as a group of its own, `compiled`, which CI
//! runs as a job of its own (chunk 8.1j).
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;
use jadpo_test::compiled::{Compared, Problem, differential_all};

use crate::support;
use crate::syntax_examples as examples;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A project of the specification corpus with `files`, written into a
/// folder of its own, `folder`, with its migration and its API's record
/// when it is laid out as a project, as the page's checker writes them;
/// `None` when it doesn't check, so nothing runs it.
fn corpus_project(files: &[(String, String)], folder: &str) -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(folder);
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    // A project with errors writes no migration, and is left out below.
    if files.iter().any(|(path, _)| path == "app.jadpo") {
        for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
            if jadpo(&root, args).code != 0 {
                return None;
            }
        }
    }
    let loaded = jadpo_db::load(&root);
    let blocked = jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .any(jadpo_check::blocks_running);
    (!blocked).then_some(root)
}

/// Copies the folder `from` into `to`, every file but what a run keeps
/// beside it (`.jadpo/`).
fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folders");
    for entry in fs::read_dir(from).expect("a folder") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            if entry.file_name() != ".jadpo" {
                copy_folder(&path, &target);
            }
        } else {
            fs::copy(&path, &target).expect("a copy");
        }
    }
}

/// The teaching example `name` (`examples/<name>/`), copied whole into a
/// folder of its own, `folder`, with its migrations, prompts, templates,
/// recordings and lock as committed, rather than its `.jadpo` files alone;
/// `None` when it doesn't check.
fn example_project(name: &str, folder: &str) -> Option<PathBuf> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(folder);
    let _ = fs::remove_dir_all(&root);
    copy_folder(&examples.join(name), &root);
    let loaded = jadpo_db::load(&root);
    let blocked = jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .any(jadpo_check::blocks_running);
    (!blocked).then_some(root)
}

/// Where the harness for `project` is kept between runs: beside the
/// build's own folders, so a second run compiles only what changed.
fn harness_folder(project: &str) -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled")
        .join(project)
}

/// The corpus's projects whose names start with one of `prefixes` and that
/// check, each written into a folder of its own: their names and folders.
fn corpus_projects(prefixes: &[&str]) -> Vec<(String, PathBuf)> {
    let projects: Vec<(String, Vec<(String, String)>)> = examples::projects()
        .into_iter()
        .filter(|(name, _)| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .collect();
    // Written and migrated a few at a time.
    let next = AtomicUsize::new(0);
    let mut written: Vec<(usize, String, PathBuf)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some((name, files)) = projects.get(at) else {
                            break;
                        };
                        let folder = format!("compiled-corpus-{at}");
                        let written = match name.strip_prefix("examples ") {
                            Some(example) => example_project(example, &folder),
                            None => corpus_project(files, &folder),
                        };
                        if let Some(folder) = written {
                            done.push((at, name.clone(), folder));
                        }
                    }
                    done
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker"))
            .collect()
    });
    written.sort_by_key(|(at, _, _)| *at);
    written
        .into_iter()
        .map(|(_, name, folder)| (name, folder))
        .collect()
}

/// Every project in `projects` compiled into one crate of a harness kept
/// at `work` and compared: each one's name with what it compared, and the
/// names of those a developer's server can't migrate (an extension missing
/// or too old: skipped, saying so, but never under CI). Any other problem
/// fails the test, named by its project, a form the generator refused
/// among them (chunk 8.1i).
fn compared(projects: &[(String, PathBuf)], work: &str) -> (Vec<(String, Compared)>, Vec<String>) {
    let folders: Vec<PathBuf> = projects.iter().map(|(_, folder)| folder.clone()).collect();
    let results = differential_all(&folders, &harness_folder(work))
        .unwrap_or_else(|problem| panic!("{problem}"));
    let mut problems = Vec::new();
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    for ((name, _), result) in projects.iter().zip(results) {
        match result {
            Ok(compared) => done.push((name.clone(), compared)),
            Err(Problem::Unserved(why)) => {
                jadpo_test::skip_for_version(format_args!("{name}: {why}"));
                skipped.push(name.clone());
            }
            Err(problem) => problems.push(format!("{name}: {problem}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
    (done, skipped)
}

/// A `return` inside an inline function, which the checker allows in a
/// `match` arm's block there: it leaves the function, as it does anywhere
/// else, rather than giving the item's value.
const RETURNS: &str = r#"type Count = Int {}

function picked(counts: List<Count>) -> List<Count> {
    return list.map(counts, count -> match count > 3 {
        true -> {
            return counts
        }
        false -> count
    })
}
"#;

/// Its tests, one failing so its assertion's values are compared too.
const RETURNS_TEST: &str = r#"test "a return inside an inline function leaves the function" {
    assert picked([1, 5]) == [1, 5]
    assert picked([1, 2]) == [1, 2]
}

test "a failed assertion shows its values" {
    assert picked([1, 2]) == [2, 1]
}
"#;

/// A package's action whose failure the calling action handles with an
/// outcome `match` after reading a plan, so its snapshot is open: the call
/// runs in a savepoint its failure returns to, compiled as interpreted
/// (chunk 8.4a), the failing and the succeeding call alike.
const PACKAGE_CAUGHT: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        pay: {
            package: AcmePay.v1
            domains: [billing]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/package.jadpo",
        r#"/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/provider.jadpo",
        r#"/// Acme Pay's payments API.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    error_code: "error.code"
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/types/pay_request.jadpo",
        "/// What each call takes.\npublic type PayRequest = Object {\n    reference: Reference\n}\n",
    ),
    (
        "shared/providers/acme_pay/v1/types/pay_reply.jadpo",
        "/// What each call gives.\npublic type PayReply = Object {\n    id: Reference\n}\n",
    ),
    (
        "shared/providers/acme_pay/v1/types/reference.jadpo",
        "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n",
    ),
    (
        "shared/providers/acme_pay/v1/failures/declined_card.jadpo",
        "/// The card was declined.\npublic type DeclinedCard = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was declined.\"\n}\n",
    ),
    (
        "shared/providers/acme_pay/v1/resources/cards.jadpo",
        r#"resource cards {
    attach: {
        method: HttpMethod.post
        path: "/v1/cards"
        input: PayRequest
        output: PayReply
        failures: [DeclinedCard]
        source: PackageSource.written
    }
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/actions/check_card.jadpo",
        r#"/// Checks the card on file for a reference.
public action check_card(input: PayRequest) fails DeclinedCard -> PayReply {
    return attempt cards.attach(input)
}
"#,
    ),
    (
        "domains/billing/types/reference.jadpo",
        // This domain's name is valid beside AcmePay.Reference. Both its
        // create and its post-provider update must retain their SQL plans.
        "type Reference = Text { max_length: 40 }\n",
    ),
    (
        "domains/billing/types/card_state.jadpo",
        "/// What the card on a plan did.\ntype CardState = Enum {\n    unchecked\n    accepted\n    declined\n}\n",
    ),
    (
        "domains/billing/entities/plan.jadpo",
        r#"use billing/types { CardState, Reference }
use shared/providers/acme_pay

entity Plan {
    id: Uuid
    tier: AcmePay.Reference
    own_tier: Reference
    card: CardState

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
    }
}
"#,
    ),
    (
        "domains/billing/actions/add_plan.jadpo",
        r#"use billing/entities { Plan }
use billing/types { CardState, Reference }
use shared/providers/acme_pay

/// Adds a plan whose card isn't checked yet.
action add_plan(tier: AcmePay.Reference) -> Plan {
    return Plan.create {
        set: {
            tier: tier
            own_tier: Reference(tier)
            card: CardState.unchecked
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/check_plan.jadpo",
        r#"use app { pay }
use billing/entities { Plan, PlanNotFound }
use billing/types { CardState }
use shared/providers/acme_pay

/// Checks a plan's card, keeping whether it went through: a declined card
/// is handled here, not passed on.
action check_plan(plan_id: Plan.id) fails PlanNotFound -> Plan {
    var plan: Plan = attempt Plan.get(plan_id)
    var state: CardState = match pay.check_card(
        AcmePay.PayRequest {
            reference: plan.tier
        }
    ) {
        success -> CardState.accepted
        AcmePay.DeclinedCard -> CardState.declined
    }
    return Plan.update(plan) {
        set: {
            card: state
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/check_plan.test.jadpo",
        r#"use app { pay }
use billing/entities { Plan }
use billing/types { CardState }
use billing/actions { add_plan }

test "a declined card is kept on the plan" {
    fakes: {
        pay.cards.attach: [declined_card]
    }
    var plan: Plan = add_plan("gold")
    var checked: Plan = attempt check_plan(plan.id)
    assert checked.card == CardState.declined
    assert checked.own_tier == "gold"
}

test "an accepted card is kept on the plan" {
    var plan: Plan = add_plan("gold")
    var checked: Plan = attempt check_plan(plan.id)
    assert checked.card == CardState.accepted
    assert checked.own_tier == "gold"
}
"#,
    ),
];

/// A recursion to the limits (area 3, "Recursion"; chunk 8.4a): a
/// thousand frames, and four hundred inline functions running at once,
/// each a level through `list.map`; one level more is the
/// `recursion_limit` fault, its site and stack compared both ways.
const RECURSION: &str = r#"type Depth = Int {}

/// One frame a level.
function down(depth: Depth) -> Depth {
    if (depth == 0) {
        return 0
    }
    return Depth(down(Depth(depth - 1)) + 1)
}

/// One frame a level, each through an inline function.
function down_mapped(depth: Depth) -> Depth {
    if (depth == 0) {
        return 0
    }
    return Depth(list.sum(list.map([depth], level -> down_mapped(Depth(level - 1)))) + 1)
}
"#;

/// Its tests, the test's own frame the first of the thousand.
const RECURSION_TEST: &str = r#"test "a thousand frames are the most a recursion takes" {
    assert down(998) == 998
}

test "a frame more is the recursion limit" {
    assert down(999) == 999
}

test "four hundred inline functions are the most running at once" {
    assert down_mapped(400) == 400
}

test "one more is the recursion limit" {
    assert down_mapped(401) == 401
}
"#;

/// Area 19's export (`docs/language/19-standard-library.md csv-export`)
/// with what a test of it needs: a loaf added, a price list started, and
/// a test that starts the export. The differential run runs each batch a
/// test started to its end (chunk 8.1h), so each loaf's row is worked out
/// by compiled code and its file and `finished:` compared.
const EXPORT: &[(&str, &str)] = &[
    (
        "domains/bakery/actions/add_loaf.jadpo",
        r#"use bakery/entities { Loaf }
use bakery/types { LoafCode, LoafPrice }

/// Adds a loaf to the bakery's list.
action add_loaf(code: LoafCode, price: LoafPrice) -> Loaf {
    return Loaf.create {
        set: {
            code: code
            price: price
        }
    }
}
"#,
    ),
    (
        "domains/bakery/actions/new_price_list.jadpo",
        r#"use bakery/entities { PriceList }

/// Starts a price list, not written yet.
action new_price_list() -> PriceList {
    return PriceList.create {
        set: {
            list_file: none
            list_run: none
            loaves_listed: none
        }
    }
}
"#,
    ),
    (
        "domains/bakery/actions/list_prices.test.jadpo",
        r#"use bakery/entities { Loaf, PriceList }
use bakery/actions { add_loaf, new_price_list }

test "a price list's export starts" {
    var rye: Loaf = add_loaf("rye", 3.50)
    var spelt: Loaf = add_loaf("spelt", 4.20)
    var sour: Loaf = add_loaf("=sour", 5.00)
    var price_list: PriceList = new_price_list()
    var listed: PriceList = attempt list_prices(price_list.id)
    assert list.count([rye.id, spelt.id, sour.id]) == 3
    assert listed.list_run != none
}
"#,
    ),
];

/// The projects chunk 8.4a compares: a package's action caught, and a
/// recursion to the limits.
fn parity_projects() -> Vec<(String, PathBuf)> {
    let caught: Vec<(String, String)> = PACKAGE_CAUGHT
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    let recursion = [
        ("main.jadpo".to_string(), RECURSION.to_string()),
        ("main.test.jadpo".to_string(), RECURSION_TEST.to_string()),
    ];
    vec![
        (
            "a package's action caught".into(),
            corpus_project(&caught, "compiled-package-caught").expect("it checks"),
        ),
        (
            "a recursion to the limits".into(),
            corpus_project(&recursion, "compiled-recursion").expect("it checks"),
        ),
    ]
}

/// The export project, written into a folder of its own.
fn export_project() -> PathBuf {
    let mut files = examples::projects()
        .into_iter()
        .find(|(name, _)| name == "docs/language/19-standard-library.md csv-export")
        .map(|(_, files)| files)
        .expect("area 19's export");
    files.extend(
        EXPORT
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
    corpus_project(&files, "compiled-export").expect("it checks")
}

/// Every project of the specification corpus, the pages' and the
/// teaching examples', built as modules of one crate (chunk 8.1j) and
/// compared (chunks 8.1f to 8.1i): every one compiles (since chunk 8.1i,
/// translations, provider operations with their fakes and a package's
/// actions, a store's files, prompts, `ai.*` with the `ai` fake and email
/// templates among them), gives the same records both ways, and every
/// teaching example's tests pass. Area 7's first routes pass as they did,
/// and named projects of each area pass compiled.
#[test]
fn the_corpus_passes_compiled_with_the_interpreters_records() {
    if !jadpo_test::live() {
        return;
    }
    let mut projects = corpus_projects(&["docs/language/", "examples "]);
    let returns = [
        ("main.jadpo".to_string(), RETURNS.to_string()),
        ("main.test.jadpo".to_string(), RETURNS_TEST.to_string()),
    ];
    projects.push((
        "a return inside an inline function".into(),
        corpus_project(&returns, "compiled-returns").expect("it checks"),
    ));
    let export = export_project();
    projects.push(("an export run to its end".into(), export.clone()));
    projects.extend(parity_projects());
    let (done, skipped) = compared(&projects, "corpus");
    // Sessions, tokens and keys, a test's passkeys, callers made from
    // their records, a test's clock, a batch started, read and run to its
    // end with a schedule run by hand (chunk 8.1h); translations, provider
    // operations with a test's fakes and their failures mapped by code, a
    // package's action, a store's put and read, `ai.*` with the `ai` fake
    // and `.requests`, email templates and prompts (chunk 8.1i): each
    // passing compiled.
    for name in [
        "docs/language/08-identity.md sign-in",
        "docs/language/08-identity.md tokens-and-keys",
        "docs/language/08-identity.md passkeys",
        "docs/language/09-policy.md policy-end-to-end",
        "docs/language/09-policy.md tenant-references",
        "docs/language/10-configuration-and-time.md deliveries",
        "docs/language/13-testing.md events-in-tests",
        "docs/language/10-configuration-and-time.md translation-messages",
        "docs/language/12-jobs-and-events.md several-steps",
        "docs/language/13-testing.md fakes-in-tests",
        "docs/language/21-ai.md ai-embed",
        "examples 11-cake-shop",
        "examples 12-recipe-book",
        "examples 15-parcel-tracker",
        "examples 22-plant-helpline",
        "examples 24-seed-library",
        "examples 25-allotment-rents",
        "examples 26-delivery-desk",
        "examples 27-guest-house",
        "examples 28-lost-property",
    ] {
        if skipped.iter().any(|skipped| skipped == name) {
            continue;
        }
        let compared = done
            .iter()
            .find(|(done, _)| done == name)
            .map(|(_, compared)| *compared);
        assert!(
            compared
                .is_some_and(|compared| compared.tests > 0 && compared.passed == compared.tests),
            "{name}: {compared:?}"
        );
    }
    // Every teaching example's tests pass, both ways.
    let failing: Vec<&(String, Compared)> = done
        .iter()
        .filter(|(name, compared)| {
            name.starts_with("examples ") && compared.passed != compared.tests
        })
        .collect();
    assert!(failing.is_empty(), "{failing:?}");
    let pages = done
        .iter()
        .filter(|(name, _)| name.starts_with("docs/language/"))
        .count();
    assert!(pages > 200, "{done:?}");
    // Tests compared and passed, by project.
    let counted = |project: &str| {
        done.iter()
            .find(|(name, _)| name == project)
            .map(|(_, compared)| (compared.tests, compared.passed))
    };
    assert_eq!(counted("docs/language/07-http.md todos"), Some((3, 3)));
    assert_eq!(counted("a return inside an inline function"), Some((2, 1)));
    assert_eq!(counted("an export run to its end"), Some((1, 1)));
    assert_eq!(counted("a package's action caught"), Some((2, 2)));
    assert_eq!(counted("a recursion to the limits"), Some((4, 2)));
    // Every route's typed entry read requests and wrote values as the
    // codec walk does (chunk 8.1p): the first route's among them.
    let todos = done
        .iter()
        .find(|(name, _)| name == "docs/language/07-http.md todos")
        .map(|(_, compared)| compared.requests);
    assert!(todos.is_some_and(|requests| requests > 50), "{todos:?}");
    exported_to_its_end(&export);
    let tests: usize = done.iter().map(|(_, compared)| compared.tests).sum();
    assert!(tests > 500, "{done:?}");
    let requests: usize = done.iter().map(|(_, compared)| compared.requests).sum();
    println!(
        "compared {} projects, {tests} tests, {requests} requests read and values written by typed entries; skipped for the server's extensions: {}",
        done.len(),
        skipped.len()
    );
    // The teaching examples on their own: how many pass both ways.
    let examples: Vec<&Compared> = done
        .iter()
        .filter(|(name, _)| name.starts_with("examples "))
        .map(|(_, compared)| compared)
        .collect();
    println!(
        "teaching examples: {} compared, {} with every test passing both ways ({} tests)",
        examples.len(),
        examples
            .iter()
            .filter(|compared| compared.passed == compared.tests)
            .count(),
        examples
            .iter()
            .map(|compared| compared.tests)
            .sum::<usize>(),
    );
    for name in &skipped {
        println!("  skipped: {name}");
    }
}

/// Area 7's `todos` and teaching example 23, the repair café: each
/// project's route tests run on its compiled program inside workerd,
/// built for wasm32 with the `testing` feature behind a test Worker that
/// runs one test by its key, against a copy of its database on a server
/// the Worker reaches over `connect()`, and each test's record the
/// interpreter's (chunk 8.2e): the café's through `settle:`'s subscribers,
/// what a request leaves for after its answer delivered before the test's
/// next line, and its fixtures.
#[test]
fn route_tests_on_workerd_give_the_interpreters_records() {
    if !jadpo_test::live() {
        return;
    }
    let started = std::time::Instant::now();
    let files = examples::projects()
        .into_iter()
        .find(|(name, _)| name == "docs/language/07-http.md todos")
        .map(|(_, files)| files)
        .expect("area 7's todos");
    let projects = [
        ("area 7's todos", corpus_project(&files, "workerd-todos")),
        (
            "the repair café",
            example_project("23-repair-cafe", "workerd-repair-cafe"),
        ),
        (
            "a write after an outside call",
            corpus_project(
                &crate::call_snapshots::compiled_route_files(),
                "workerd-version-write",
            ),
        ),
    ];
    let folders: Vec<PathBuf> = projects
        .iter()
        .map(|(name, folder)| folder.clone().unwrap_or_else(|| panic!("{name} checks")))
        .collect();
    let results = jadpo_test::workerd::differential_all(&folders, &harness_folder("workerd"))
        .unwrap_or_else(|problem| panic!("{problem}"));
    let compared: Vec<Compared> = projects
        .iter()
        .zip(results)
        .map(|((name, _), result)| result.unwrap_or_else(|problem| panic!("{name}: {problem}")))
        .collect();
    let counts: Vec<(usize, usize)> = compared
        .iter()
        .map(|compared| (compared.tests, compared.passed))
        .collect();
    assert_eq!(counts, [(3, 3), (4, 4), (1, 1)]);
    println!(
        "compared on workerd: {} route tests of {} projects with the interpreter's records, every one passing, in {:.0?}",
        counts.iter().map(|(tests, _)| tests).sum::<usize>(),
        counts.len(),
        started.elapsed()
    );
}

/// The export's test, run interpreted as the differential run runs it:
/// its run went to its end, each loaf's row written and `finished:`
/// keeping the file, all in the test's record, so compiled code's rows are
/// compared, not skipped.
fn exported_to_its_end(folder: &Path) {
    let ran = jadpo_test::run(jadpo_test::Options {
        cwd: folder.to_path_buf(),
        target: Some(".".into()),
        seed: Some(1),
        record: true,
        batches: true,
        ..jadpo_test::Options::default()
    });
    let jadpo_test::Ran::Done(run) = ran else {
        panic!("the export's test ran: {ran:?}");
    };
    let record = run
        .results
        .first()
        .and_then(|result| result.record.as_ref())
        .expect("its record");
    let lines = record.lines();
    let finished = lines.iter().any(|line| {
        line.starts_with("row ") && line.contains("jadpo_batch_runs") && line.contains("finished")
    });
    let listed = lines.iter().any(|line| {
        line.starts_with("row ") && line.contains("price_list") && line.contains("prices.csv")
    });
    assert!(finished && listed, "{}", lines.join("\n"));
}
