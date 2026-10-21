//! Provider call sites by number (roadmap chunk 8.1d2): a provider call's
//! site plan names its operation by its number in the plan's provider
//! operations, or a package action's own operation by its place among its
//! package's, and the runtime's own calls (a mail package's email, a
//! queue's receive and acknowledgement, a poll) carry their operations'
//! numbers in their plans, so nothing at a call finds an operation by
//! name. For every project of the specification corpus, each number must
//! name the operation the code or the plan names.

use std::collections::BTreeMap;

use crate::syntax_examples as examples;

use jadpo_db::{Database, File, Layout, Project};
use jadpo_graph::body::{Callee, Expr, Node, OWN_CONNECTION};
use jadpo_plan::{Decoded, Plan};
use jadpo_std::provider::Called;
use jadpo_std::receiving::QueueSource;

/// How many of each kind of call a project's plan names by number.
#[derive(Debug, Default)]
struct Counted {
    named: usize,
    own: usize,
    runtime: usize,
}

/// The name of the operation numbered `number`, with its connection's.
fn named(plan: &Plan, number: u32) -> Option<(String, String)> {
    let op = plan.providers().operations.get(number as usize)?;
    let connection = plan.connection_name(op.connection)?;
    Some((connection.to_string(), op.name.clone()))
}

/// Checks every provider call `project`'s plan names by number, adding
/// what it found to `counted`; `None` for a project with errors.
fn checked(db: &Database, project: Project, counted: &mut Counted) -> Option<()> {
    let blocked = jadpo_check::check(db, project)
        .into_iter()
        .any(|diagnostic| jadpo_check::blocks_running(&diagnostic));
    if blocked {
        return None;
    }
    let plan = Plan::build(db, project);
    let providers = plan.providers();
    // The connections each package action is called through.
    let mut through: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for callable in plan.callables() {
        for site in &callable.body.sites {
            if let Node::Expr(Expr::Call {
                callee: Callee::PackageAction { connection, action },
                ..
            }) = &site.node
            {
                through
                    .entry(action.as_str().to_string())
                    .or_default()
                    .push(connection.clone());
            }
        }
    }
    for callable in plan.callables() {
        for (at, site) in callable.body.sites.iter().enumerate() {
            let Node::Expr(Expr::Call {
                callee:
                    Callee::Provider {
                        connection,
                        operation,
                    },
                ..
            }) = &site.node
            else {
                continue;
            };
            let Some(Decoded::Provider(call)) = callable.sites.get(at).map(|site| &site.decoded)
            else {
                panic!("{}: a provider call not decoded as one", callable.label);
            };
            match call.called {
                Called::Operation(number) => {
                    assert_ne!(connection, OWN_CONNECTION, "{}", callable.label);
                    assert_eq!(
                        named(&plan, number),
                        Some((connection.clone(), operation.clone())),
                        "{}: `{connection}.{operation}`",
                        callable.label
                    );
                    counted.named += 1;
                }
                Called::Own(_) => {
                    assert_eq!(connection, OWN_CONNECTION, "{}", callable.label);
                    let callers = through
                        .get(callable.key.as_str())
                        .cloned()
                        .unwrap_or_default();
                    for caller in callers {
                        let at = plan
                            .connections()
                            .iter()
                            .position(|known| known.name == caller)
                            .and_then(|at| u32::try_from(at).ok());
                        let number = providers.called(call.called, at);
                        assert_eq!(
                            number.and_then(|number| named(&plan, number)),
                            Some((caller.clone(), operation.clone())),
                            "{}: its own `{operation}` through `{caller}`",
                            callable.label
                        );
                    }
                    counted.own += 1;
                }
            }
        }
    }
    // The runtime's own calls.
    let mut own_calls: Vec<(u32, String, String)> = Vec::new();
    for poll in &providers.polls {
        own_calls.push((poll.called, poll.connection.clone(), poll.operation.clone()));
    }
    for queue in &providers.queues {
        if let QueueSource::Package {
            receive,
            acknowledge,
        } = &queue.source
        {
            own_calls.push((
                receive.called,
                queue.connection.clone(),
                receive.operation.clone(),
            ));
            own_calls.push((
                acknowledge.called,
                queue.connection.clone(),
                acknowledge.operation.clone(),
            ));
        }
    }
    let emails = &plan.events().emails;
    if let (Some(package), Some(connection)) = (&emails.package, &emails.connection) {
        own_calls.push((
            package.called,
            connection.clone(),
            package.operation.clone(),
        ));
    }
    for (number, connection, operation) in own_calls {
        assert_eq!(
            named(&plan, number),
            Some((connection.clone(), operation.clone())),
            "the runtime's call of `{connection}.{operation}`"
        );
        counted.runtime += 1;
    }
    Some(())
}

#[test]
fn every_provider_call_names_its_operation_by_number() {
    let mut counted = Counted::default();
    for (_, files) in examples::projects() {
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
        checked(&db, project, &mut counted);
    }
    let root = examples::root().join("examples");
    let mut folders: Vec<_> = std::fs::read_dir(&root)
        .expect("examples/")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    folders.sort();
    for folder in folders {
        let loaded = jadpo_db::load(&folder);
        checked(&loaded.db, loaded.project, &mut counted);
    }
    // The corpus calls providers each way.
    assert!(counted.named > 5, "{counted:?}");
    assert!(counted.own > 0, "{counted:?}");
    assert!(counted.runtime > 0, "{counted:?}");
}
