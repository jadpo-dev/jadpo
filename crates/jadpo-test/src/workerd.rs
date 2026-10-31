//! Route tests on workerd (roadmap chunk 8.2e; docs/compiler-runtime.md
//! §2.5, "Differential test", and §2.7, "The differential run"): a
//! project's route tests run on its compiled program inside workerd,
//! Cloudflare's own Workers runtime, and each test's record compared with
//! the interpreter's, as [`crate::compiled`] compares compiled code's run
//! natively.
//!
//! [`differential_all`] compiles each project as a module of one crate
//! with the `testing` feature, as the native harness does, beside a test
//! Worker's crate whose `fetch` is `jadpo_host_workers::testing::fetch`
//! over those programs; builds it for `wasm32-unknown-unknown` in the dev
//! profile, offline, in a workspace it keeps, so a run with nothing
//! changed compiles nothing; binds the module with wasm-bindgen's library
//! (`jadpo_codegen::workers::bind`); and serves it under the workerd
//! pinned in `tools/fixtures/workers/`, its network the machine's own and nothing
//! past it. Then, for each project, it runs the route tests on the
//! interpreter with records, seed 1, and asks the Worker to run each one
//! by its key: on a copy of each store's database made on a server the
//! Worker reaches over `connect()` (the run's when it is named by a URL,
//! else one of this process's own on `127.0.0.1`), a fresh copy for each
//! run, crashed again at each step it reached as `jadpo test` crashes a
//! run of its own, and judged as one here is. Each test's record, the
//! Worker's lines with how it ended and the rows its stores held, is
//! compared with the interpreter's ([`crate::differential::compare`]).

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use jadpo_diag::{Rule, rule};
use jadpo_host_workers::testing::{Answered, Asked};
use jadpo_runtime::crashes::Boundary;

use crate::compiled::{CRATE, Compared, Problem, done, generated, profiles, sources, write};
use crate::database::TestDatabases;
use crate::outcome::{Ending, fake_rule};
use crate::postgres::{ADMIN_DATABASE, Server};
use crate::run::{CrashRun, Crashed, crash_runs};
use crate::{Options, Ran, Test, TestKind, TestRecord};

/// The test Worker's package.
const WORKER: &str = "jadpo-test-worker";

/// The module's name, beside the shim: `tests.wasm` and `tests.js`.
const MODULE: &str = "tests";

/// The order seed both runs use.
const SEED: u64 = 1;

/// Compiles each project at `folders` into a test Worker kept at `work`,
/// serves it under workerd, and compares each project's route tests run
/// inside it with the interpreter's: what each compared, or why it
/// couldn't, in `folders`' order. A project that doesn't check or isn't
/// compiled is that project's problem; a Worker that isn't built or
/// served is every project's.
pub fn differential_all(
    folders: &[PathBuf],
    work: &Path,
) -> Result<Vec<Result<Compared, Problem>>, Problem> {
    let sources = sources();
    let crate_folder = work.join(CRATE);
    let mut results: Vec<Option<Result<Compared, Problem>>> = Vec::new();
    let mut compiled: Vec<usize> = Vec::new();
    for (place, folder) in folders.iter().enumerate() {
        match generated(folder, place) {
            Ok(generated) => {
                generated
                    .write(&crate_folder)
                    .map_err(|error| Problem::Write(error.to_string()))?;
                compiled.push(place);
                results.push(None);
            }
            Err(problem) => results.push(Some(Err(problem))),
        }
    }
    if !compiled.is_empty() {
        jadpo_codegen::holding(
            &compiled,
            &jadpo_codegen::Options {
                name: "jadpo-compiled".into(),
                runtime: sources.display().to_string(),
            },
        )
        .map_err(Problem::Generate)?
        .write(&crate_folder)
        .map_err(|error| Problem::Write(error.to_string()))?;
        crate::compiled::remove_stale_modules(&crate_folder, &compiled)?;
        let out = worker(work, &sources)?;
        let server = tcp_server().map_err(Problem::Harness)?;
        let workerd = Workerd::start(&out).map_err(Problem::Harness)?;
        for (program, place) in compiled.iter().copied().enumerate() {
            if let Some(folder) = folders.get(place) {
                results[place] = Some(compare(folder, program, &workerd, &server));
            }
        }
    }
    Ok(results
        .into_iter()
        .map(|result| {
            result.unwrap_or_else(|| Err(Problem::Harness("the project wasn't run".into())))
        })
        .collect())
}

/// Writes the test Worker's workspace around the crate holding every
/// project, builds its module for wasm32 and binds it beside its shim:
/// the folder workerd serves.
fn worker(work: &Path, sources: &Path) -> Result<PathBuf, Problem> {
    let root = sources.parent().unwrap_or(sources);
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    write(
        &work.join("Cargo.toml"),
        format!(
            "# The test Worker's workspace (docs/compiler-runtime.md §2.5), written by jadpo-test; don't edit it.\n\
             [workspace]\n\
             members = [\"{CRATE}\", \"worker\"]\n\
             resolver = \"3\"\n\
             \n\
             {}",
            profiles(&manifest)
        )
        .as_bytes(),
    )?;
    // The workspace's own lock, so every version is the one it builds
    // with and nothing is fetched; cargo adds the generated packages to it.
    let lock = fs::read(root.join("Cargo.lock")).unwrap_or_default();
    write(&work.join("Cargo.lock"), &lock)?;
    write(
        &work.join("worker/Cargo.toml"),
        format!(
            "# The test Worker, written by jadpo-test; don't edit it.\n\
             [package]\n\
             name = \"{WORKER}\"\n\
             version = \"0.0.0\"\n\
             edition = \"2024\"\n\
             publish = false\n\
             \n\
             [lib]\n\
             path = \"src/lib.rs\"\n\
             crate-type = [\"cdylib\"]\n\
             \n\
             [dependencies]\n\
             jadpo-compiled = {{ path = \"../{CRATE}\", features = [\"testing\"] }}\n\
             jadpo-host-workers = {{ path = \"{}\", features = [\"testing\"] }}\n",
            sources.join("jadpo-host-workers").display()
        )
        .as_bytes(),
    )?;
    write(
        &work.join("worker/src/lib.rs"),
        "//! A test Worker: each request runs one test of the programs it holds\n\
         //! (docs/compiler-runtime.md §2.5); written by jadpo-test.\n\
         \n\
         use jadpo_host_workers::wasm_bindgen::JsValue;\n\
         \n\
         #[jadpo_host_workers::wasm_bindgen::prelude::wasm_bindgen(\n    \
             wasm_bindgen = jadpo_host_workers::wasm_bindgen\n\
         )]\n\
         pub fn fetch(request: JsValue, env: JsValue, ctx: JsValue) -> JsValue {\n    \
             jadpo_host_workers::testing::fetch(jadpo_compiled::PROGRAMS, request, env, ctx)\n\
         }\n"
        .as_bytes(),
    )?;
    let module = jadpo_codegen::cargo::build_module(&jadpo_codegen::cargo::Module {
        workspace: work,
        package: WORKER,
        target: &work.join("target"),
    })
    .map_err(Problem::Build)?;
    let bound = jadpo_codegen::workers::bind(&module, MODULE, jadpo_codegen::workers::Names::Kept)
        .map_err(|why| Problem::Harness(format!("the test Worker wasn't bound: {why}")))?;
    let out = work.join("out");
    write(&out.join(format!("{MODULE}.js")), bound.js.as_bytes())?;
    write(&out.join(format!("{MODULE}.wasm")), &bound.wasm)?;
    write(
        &out.join("worker.mjs"),
        format!(
            "// The test Worker, written by jadpo-test; don't edit it. Its module is\n\
             // instantiated once per isolate, and each request runs one test.\n\
             import module from \"./{MODULE}.wasm\";\n\
             import {{ initSync, fetch as handleFetch }} from \"./{MODULE}.js\";\n\
             \n\
             initSync({{ module }});\n\
             \n\
             export default {{\n\
             \x20 fetch(request, env, ctx) {{\n\
             \x20   return handleFetch(request, env, ctx);\n\
             \x20 }},\n\
             }};\n"
        )
        .as_bytes(),
    )?;
    Ok(out)
}

/// The server the Worker's copies are made on: the run's when a Worker
/// reaches it over TCP, as CI's, named by a URL; else one of this
/// process's own listening on `127.0.0.1`, since a local run's listens on
/// a Unix socket, which `connect()` can't open.
fn tcp_server() -> Result<Arc<Server>, String> {
    let run = crate::database::server()?;
    if run.tcp_url(ADMIN_DATABASE).is_some() {
        return Ok(run);
    }
    static TCP: OnceLock<Result<Arc<Server>, String>> = OnceLock::new();
    TCP.get_or_init(|| {
        Server::start_on_tcp(None)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    })
    .clone()
}

/// The project at `folder`, the Worker's `program`th: its route tests run
/// on the interpreter and inside the Worker, their records compared.
fn compare(
    folder: &Path,
    program: usize,
    workerd: &Workerd,
    server: &Arc<Server>,
) -> Result<Compared, Problem> {
    let interpreted = match crate::run(Options {
        cwd: folder.to_path_buf(),
        target: Some(".".into()),
        kind: Some(TestKind::Route),
        seed: Some(SEED),
        record: true,
        ..Options::default()
    }) {
        // A project with no route tests compares none.
        Ran::Stopped(diagnostic) if diagnostic.rule.id == rule::TEST_NO_TESTS.id => {
            return Ok(Compared {
                tests: 0,
                passed: 0,
                requests: 0,
            });
        }
        ran => done(ran)?,
    };
    let loaded = jadpo_db::load(folder);
    let checked =
        crate::prepare(&loaded, folder).map_err(|ran| Problem::NotRun(format!("{ran:?}")))?;
    let databases = (!checked.steps.is_empty())
        .then(|| TestDatabases::on(checked.steps.clone(), Arc::clone(server)));
    let remote = Remote {
        workerd,
        program,
        server,
        databases: databases.as_ref(),
    };
    let mut ran: Vec<(Test, TestRecord, bool)> = Vec::new();
    for result in &interpreted.results {
        let (record, passed) = remote.test(&result.test).map_err(Problem::Harness)?;
        ran.push((result.test.clone(), record, passed));
    }
    drop(databases);
    let recorded: Vec<(&Test, Option<&TestRecord>)> = ran
        .iter()
        .map(|(test, record, _)| (test, Some(record)))
        .collect();
    let tests =
        crate::differential::compare(&crate::differential::recorded(&interpreted), &recorded)
            .map_err(|difference| Problem::Differs(difference.to_string()))?;
    Ok(Compared {
        tests,
        passed: ran.iter().filter(|(_, _, passed)| *passed).count(),
        requests: 0,
    })
}

/// What runs a project's tests inside the Worker.
struct Remote<'r> {
    workerd: &'r Workerd,
    program: usize,
    server: &'r Server,
    databases: Option<&'r TestDatabases>,
}

impl Remote<'_> {
    /// Runs `test` inside the Worker as `jadpo test` runs one: its record,
    /// and whether it passed.
    fn test(&self, test: &Test) -> Result<(TestRecord, bool), String> {
        let first = self.once(test, None, true)?;
        let mut failed = judged(&first, true).map(|rule| rule.id.to_string());
        // A test that reaches an action with several changing calls runs
        // again with a crash at each boundary of each step it reached
        // (chunk 7.5d), as natively.
        if failed.is_none() {
            let mut trouble = None;
            let crashed = crash_runs(&first.reached, &first.effects, |at, boundary| {
                match self.once(test, Some((at, boundary)), false) {
                    Ok(ran) => CrashRun {
                        failure: judged(&ran, false),
                        effects: ran.effects,
                        crashed: ran.crashed,
                    },
                    Err(why) => {
                        trouble.get_or_insert(why);
                        CrashRun {
                            failure: None,
                            effects: first.effects.clone(),
                            crashed: false,
                        }
                    }
                }
            });
            if let Some(why) = trouble {
                return Err(why);
            }
            failed = match crashed {
                None => None,
                Some(Crashed::Failed(point, rule)) => Some(format!("{} {}", rule.id, point.text())),
                Some(Crashed::Acted(point, _)) => {
                    Some(format!("{} {}", rule::TEST_FAULT.id, point.text()))
                }
            };
        }
        let verdict = failed.clone().unwrap_or_else(|| "passed".to_string());
        Ok((
            TestRecord::of_lines(first.entries, &verdict, &first.rows),
            failed.is_none(),
        ))
    }

    /// One run of `test` inside the Worker, on a fresh copy of each store's
    /// database when it has one, crashing at `crash`, keeping its record
    /// when `record`.
    fn once(
        &self,
        test: &Test,
        crash: Option<(usize, Boundary)>,
        record: bool,
    ) -> Result<Answered, String> {
        let stored = matches!(
            test.kind,
            TestKind::Action | TestKind::Route | TestKind::Schedule
        );
        let copy = match self.databases {
            Some(databases) if stored => Some(databases.acquire()?),
            _ => None,
        };
        let mut stores = BTreeMap::new();
        for (store, (name, _)) in copy.iter().flat_map(|copy| &copy.stores) {
            let url = self
                .server
                .tcp_url(name)
                .ok_or("the test server can't be reached over TCP")?;
            stores.insert(store.clone(), url);
        }
        // Only a test with a database delivers between its steps, runs the
        // batches its steps start and counts the steps it reaches, as
        // natively.
        let asked = Asked {
            program: self.program,
            key: test.key.clone(),
            order: copy.as_ref().map(|_| SEED),
            stores,
            crash,
            record: record.then(crate::differential::digest_key),
        };
        let answered = self.workerd.post("/run", &asked.to_json());
        if let (Some(databases), Some(copy)) = (self.databases, copy) {
            databases.release(copy, false);
        }
        let (status, body) = answered?;
        if status != 200 {
            return Err(format!(
                "the test Worker answered {status} for {}: {body}\n{}",
                test.key,
                self.workerd.log()
            ));
        }
        Answered::read(&body).map_err(|why| format!("the test Worker's answer: {why}: {body}"))
    }
}

/// The rule a run inside the Worker fails with, as a run here is judged
/// (`jadpo test`'s rules, chunks 6.11 and 7.2d), its fakes' use judged
/// when `fakes`: `None` when it passed.
fn judged(answered: &Answered, fakes: bool) -> Option<&'static Rule> {
    let ending = match answered.ending.as_str() {
        "value" => Ending::Value,
        "assertion" => Ending::Assertion,
        "failed" => Ending::Failed,
        _ => Ending::Fault,
    };
    let failed = ending.rule(answered.undelivered);
    if fakes && let Some((_, rule)) = fake_rule(answered.fakes.iter().copied(), failed.is_some()) {
        return Some(rule);
    }
    failed
}

/// workerd serving the test Worker on a port of this machine, stopped
/// when dropped.
struct Workerd {
    child: Child,
    port: u16,
    log: PathBuf,
}

impl Workerd {
    /// The Worker in `out` (its shim, its module and the module's
    /// JavaScript), at the Workers host's compatibility date, its network
    /// this machine's alone.
    fn start(out: &Path) -> Result<Workerd, String> {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|error| format!("no port for workerd: {error}"))?
            .port();
        fs::write(
            out.join("config.capnp"),
            format!(
                "# The test Worker's workerd configuration, written by jadpo-test.\n\
                 using Workerd = import \"/workerd/workerd.capnp\";\n\n\
                 const config :Workerd.Config = (\n\
                 \x20 services = [\n\
                 \x20   (name = \"tests\", worker = .tests),\n\
                 \x20   (name = \"machine\", network = (allow = [\"local\"])),\n\
                 \x20 ],\n\
                 \x20 sockets = [(name = \"http\", address = \"127.0.0.1:{port}\", http = (), service = \"tests\")],\n\
                 );\n\n\
                 const tests :Workerd.Worker = (\n\
                 \x20 modules = [\n\
                 \x20   (name = \"worker.mjs\", esModule = embed \"worker.mjs\"),\n\
                 \x20   (name = \"{MODULE}.js\", esModule = embed \"{MODULE}.js\"),\n\
                 \x20   (name = \"{MODULE}.wasm\", wasm = embed \"{MODULE}.wasm\"),\n\
                 \x20 ],\n\
                 \x20 compatibilityDate = \"{}\",\n\
                 \x20 globalOutbound = \"machine\",\n\
                 );\n",
                jadpo_codegen::workers::COMPATIBILITY_DATE
            ),
        )
        .map_err(|error| format!("workerd's configuration wasn't written: {error}"))?;
        let log = out.join("workerd.log");
        let file = fs::File::create(&log).map_err(|error| error.to_string())?;
        let child = Command::new(crate::workerd_install::workerd()?)
            .args(["serve", "config.capnp"])
            .current_dir(out)
            .stdin(Stdio::null())
            .stdout(file.try_clone().map_err(|error| error.to_string())?)
            .stderr(file)
            .spawn()
            .map_err(|error| format!("workerd didn't start: {error}"))?;
        let mut workerd = Workerd { child, port, log };
        let until = Instant::now() + Duration::from_secs(60);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if let Ok(Some(status)) = workerd.child.try_wait() {
                return Err(format!("workerd ended ({status}):\n{}", workerd.log()));
            }
            if Instant::now() > until {
                return Err(format!(
                    "workerd didn't listen within a minute:\n{}",
                    workerd.log()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(workerd)
    }

    /// What workerd has logged.
    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// One HTTP/1.1 exchange posting `body` as JSON to `path`: the status
    /// and the answer's body.
    fn post(&self, path: &str, body: &str) -> Result<(u16, String), String> {
        let failed = |error: std::io::Error| format!("workerd's answer: {error}");
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).map_err(failed)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(failed)?;
        write!(
            stream,
            "POST {path} HTTP/1.1\r\nHost: tests\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .map_err(failed)?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(failed)?;
        let status = line
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse().ok())
            .ok_or_else(|| format!("workerd's answer isn't HTTP: {line}\n{}", self.log()))?;
        let mut length = None;
        let mut chunked = false;
        loop {
            line.clear();
            reader.read_line(&mut line).map_err(failed)?;
            let header = line.trim_end().to_ascii_lowercase();
            if header.is_empty() {
                break;
            }
            if let Some(value) = header.strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().ok();
            }
            if header.starts_with("transfer-encoding:") && header.contains("chunked") {
                chunked = true;
            }
        }
        let mut answer = Vec::new();
        if chunked {
            loop {
                line.clear();
                reader.read_line(&mut line).map_err(failed)?;
                let size = usize::from_str_radix(line.trim(), 16)
                    .map_err(|_| format!("workerd's chunk: {line}"))?;
                let mut piece = vec![0; size + 2];
                reader.read_exact(&mut piece).map_err(failed)?;
                if size == 0 {
                    break;
                }
                answer.extend_from_slice(&piece[..size]);
            }
        } else if let Some(length) = length {
            answer.resize(length, 0);
            reader.read_exact(&mut answer).map_err(failed)?;
        } else {
            reader.read_to_end(&mut answer).map_err(failed)?;
        }
        Ok((status, String::from_utf8_lossy(&answer).into_owned()))
    }
}

impl Drop for Workerd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(ending: &str, undelivered: bool, fakes: Vec<(bool, bool)>) -> Answered {
        Answered {
            entries: Vec::new(),
            rows: Vec::new(),
            ending: ending.into(),
            undelivered,
            fakes,
            reached: Vec::new(),
            crashed: false,
            effects: Vec::new(),
        }
    }

    /// A run inside the Worker is judged as a run here is: its outcome,
    /// a delivery that failed for good, then its fakes' use.
    #[test]
    fn a_run_in_the_worker_is_judged_as_one_here_is() {
        assert_eq!(judged(&answered("value", false, Vec::new()), true), None);
        assert_eq!(
            judged(&answered("assertion", false, Vec::new()), true).map(|rule| rule.id),
            Some(rule::TEST_ASSERTION_FAILED.id)
        );
        assert_eq!(
            judged(&answered("fault", true, Vec::new()), true).map(|rule| rule.id),
            Some(rule::TEST_SUBSCRIBER_FAILED.id)
        );
        assert_eq!(
            judged(&answered("value", true, Vec::new()), true),
            None,
            "a run that gave a value passed"
        );
        assert_eq!(
            judged(&answered("value", false, vec![(false, true)]), true).map(|rule| rule.id),
            Some(rule::TEST_FAKE_UNUSED.id)
        );
        assert_eq!(
            judged(&answered("failed", false, vec![(false, true)]), true).map(|rule| rule.id),
            Some(rule::TEST_UNEXPECTED_FAILURE.id),
            "answers left over don't hide a failure"
        );
        assert_eq!(
            judged(
                &answered("failed", false, vec![(false, true), (true, false)]),
                true
            )
            .map(|rule| rule.id),
            Some(rule::TEST_FAKE_EXHAUSTED.id),
            "a call past the script is what went wrong first"
        );
        assert_eq!(
            judged(&answered("value", false, vec![(true, false)]), false),
            None,
            "a crash run's fakes aren't judged"
        );
    }
}
