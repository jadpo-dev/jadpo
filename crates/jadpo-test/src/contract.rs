//! A contract run, `jadpo test --kind contract` (area 11, "Tests and
//! fakes"; area 13, "Kinds of test"; chunk K4c, as Richard decided it on
//! 2026-11-27): the app's own tests that reach a provider operation, its
//! cases, run with each call their `fakes:` doesn't script sent through
//! the connection to the provider's sandbox and each scripted one still
//! scripted.
//!
//! What the run goes through is worked out before anything is sent: the
//! cases, from the graph (a test's calls, the triggers it runs and the
//! subscribers its events reach, to an operation of a connection the app
//! calls through); the connections they reach; and the suites of the
//! capabilities the app's own integrations fill and the app uses through
//! them (`ai:` and `email:`), which the command runs through `jadpo
//! package conform --live` once the cases have run. Each of those
//! connections' settings is read again as `jadpo dev` reads them, the
//! environment over `.env.local`, over the values a test run reads; one
//! that nothing sets, that holds nothing valid, or whose environment
//! chooses `live` refuses the run (`test.contract_refused`).
//!
//! Once the cases have run, each operation of the app's own integrations
//! that the app calls, written from the provider's documentation and
//! confirmed by no recording, that no case sent to the sandbox fails the
//! run (`test.contract_uncovered`). That is judged only when every case
//! ran: a path, line, name, `--failed` or `--affected` leaves cases out.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::{EdgeKind, Graph, Kind, NodeId};
use jadpo_interp::Prepared;
use jadpo_runtime::config::{ConfigValues, ProblemKind, Variables};
use jadpo_std::provider::Address;

use crate::Test;

/// What a contract run reads its connections' settings from: the
/// environment over `.env.local`, as `jadpo dev` reads them.
pub struct Contract {
    pub variables: Box<dyn Variables + Send + Sync>,
}

impl std::fmt::Debug for Contract {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Contract")
    }
}

impl PartialEq for Contract {
    fn eq(&self, other: &Contract) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Contract {}

/// What a contract run found besides how its cases went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContractRun {
    /// Whether every case ran, so which operations none sent was judged.
    pub whole: bool,
    /// The operations no case sent to the sandbox, in name order.
    pub uncovered: Vec<Uncovered>,
    /// The suites to run against the provider through the app's
    /// connection, in package and capability order.
    pub suites: Vec<LiveSuite>,
    /// The files the uncovered operations are declared in, for their code
    /// frames.
    pub sources: BTreeMap<String, String>,
}

/// An operation no case of a contract run sent to the sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uncovered {
    /// As the app calls it: `couriers.bookings.cancel`.
    pub call: String,
    pub connection: String,
    /// `bookings.cancel`.
    pub operation: String,
    /// Where it is declared: the file and the byte range of its name.
    pub at: Option<(String, u32, u32)>,
}

impl Uncovered {
    /// Its error, `test.contract_uncovered`.
    pub fn diagnostic(&self) -> Diagnostic {
        let (connection, operation) = (&self.connection, &self.operation);
        let mut diagnostic = Diagnostic::new(
            &rule::TEST_CONTRACT_UNCOVERED,
            format!("`{}` has no contract case", self.call),
        );
        if let Some((file, start, end)) = &self.at {
            diagnostic = diagnostic.at(
                Span::new(file.as_str(), *start, *end),
                "no case sent it to the sandbox",
            );
        }
        diagnostic
            .reason("It was written from the provider's documentation and no recording confirms it, so a contract run is what checks it against the provider, and none of its cases sent it to the sandbox: no case reaches it, or each that does scripts its answer.")
            .next(format!(
                "Write a test that reaches `{}` with no `fakes:` answer for it, or confirm it with `jadpo package record {connection} {operation}`",
                self.call
            ))
    }
}

/// A capability's conformance suite a contract run takes through
/// `--live`: one the app's own integration fills, used through its
/// connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveSuite {
    /// `AcmeMail.v1`.
    pub package: String,
    /// `email`.
    pub capability: String,
    /// The app's connection to it.
    pub connection: String,
}

/// What a contract run goes through, worked out before anything is sent.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    /// Each case's key, and the connections it reaches.
    pub cases: BTreeMap<String, BTreeSet<String>>,
    pub suites: Vec<LiveSuite>,
    /// The operations a case must send: unconfirmed, written from the
    /// provider's documentation, called by the app through its own
    /// integration.
    pub needed: Vec<Uncovered>,
    pub sources: BTreeMap<String, String>,
}

/// The cases among `tests`, the suites, and the operations a case must
/// send, for the project `loaded` holds, which `prepared` runs.
pub(crate) fn scope(loaded: &jadpo_db::Loaded, prepared: &Prepared, tests: &[Test]) -> Scope {
    let (db, project) = (&loaded.db, loaded.project);
    let graph = jadpo_check::graph::graph(db, project);
    let plan = prepared.plan();
    let through = jadpo_check::graph::areas::connection_packages(db, project);
    // The connections whose calls a test's host answers otherwise: the
    // email connection's mail is caught whole, and the models `ai:` asks
    // are answered by the `ai` fake.
    let mail = plan.events().emails.connection.clone();
    let models = plan.ai().map(|ai| ai.connection.clone());
    let answered_otherwise =
        |name: &str| mail.as_deref() == Some(name) || models.as_deref() == Some(name);
    // Each operation's node name, `{through}.{resource}.{operation}`, and
    // the connection the app calls it through.
    let mut operations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for op in &plan.providers().operations {
        let Some(connection) = plan.connection_name(op.connection) else {
            continue;
        };
        if answered_otherwise(connection) {
            continue;
        }
        let Some(package) = through.get(connection) else {
            continue;
        };
        operations
            .entry(format!("{package}.{}", op.name))
            .or_default()
            .insert(connection.to_string());
    }
    let keyed: BTreeMap<&str, NodeId> = graph
        .ids()
        .filter(|&id| graph.node(id).kind == Kind::Test)
        .map(|id| (graph.node(id).key.as_str(), id))
        .collect();
    let mut cases = BTreeMap::new();
    for test in tests {
        if test.generated() || test.kind == crate::TestKind::Recording {
            continue;
        }
        let Some(&id) = keyed.get(test.key.as_str()) else {
            continue;
        };
        let connections: BTreeSet<String> = reached(graph, id)
            .into_iter()
            .filter(|&node| graph.node(node).kind == Kind::Operation)
            .filter_map(|node| operations.get(&graph.node(node).name))
            .flatten()
            .cloned()
            .collect();
        if !connections.is_empty() {
            cases.insert(test.key.clone(), connections);
        }
    }

    // The suites: a capability whose suite is built, filled by one of the
    // app's own integrations, used through the app's connection to it.
    let homes = jadpo_check::surface::homes(db, project);
    let files: Vec<String> = project
        .files(db)
        .iter()
        .map(|file| file.path(db).clone())
        .collect();
    let mut suites = Vec::new();
    for home in homes.values().filter(|home| !home.installed) {
        let package = home.versioned();
        for (_, name, connection) in plan.package_connections() {
            if connection.package != package {
                continue;
            }
            for (capability, through) in [("ai", &models), ("email", &mail)] {
                let fills = files
                    .iter()
                    .any(|path| *path == format!("{}/provides/{capability}.jadpo", home.folder));
                if fills && through.as_deref() == Some(name) {
                    suites.push(LiveSuite {
                        package: package.clone(),
                        capability: capability.to_string(),
                        connection: name.to_string(),
                    });
                }
            }
        }
    }
    suites.sort_by(|a, b| (&a.package, &a.capability).cmp(&(&b.package, &b.capability)));

    // The operations a case must send.
    let texts = jadpo_db::source_texts(db, project);
    let mut needed = Vec::new();
    let mut sources = BTreeMap::new();
    for review in jadpo_check::review::review(db, project) {
        for call in review
            .calls
            .iter()
            .filter(|call| !call.confirmed && call.written)
        {
            let at = through
                .get(&call.connection)
                .and_then(|package| {
                    graph.named(Kind::Operation, &format!("{package}.{}", call.operation))
                })
                .and_then(|node| graph.node(node).place.clone())
                // The operation's name, where its member starts.
                .map(|place| {
                    let name = call.operation.rsplit('.').next().unwrap_or_default();
                    let end = place.start + u32::try_from(name.len()).unwrap_or(0);
                    (place.file, place.start, end.min(place.end))
                });
            if let Some((file, _, _)) = &at
                && let Some(text) = texts.get(file)
            {
                sources.insert(file.clone(), text.clone());
            }
            needed.push(Uncovered {
                call: call.call.clone(),
                connection: call.connection.clone(),
                operation: call.operation.clone(),
                at,
            });
        }
    }
    needed.sort_by(|a, b| a.call.cmp(&b.call));
    Scope {
        cases,
        suites,
        needed,
        sources,
    }
}

/// Every node a test reaches as it runs: what it and each callable it
/// reaches calls, the actions each trigger runs, and the subscribers of
/// each event they emit.
fn reached(graph: &Graph, from: NodeId) -> BTreeSet<NodeId> {
    let mut seen = BTreeSet::from([from]);
    let mut queue = vec![from];
    while let Some(id) = queue.pop() {
        let mut next: Vec<NodeId> = graph
            .outgoing(id, EdgeKind::Calls)
            .chain(graph.outgoing(id, EdgeKind::Runs))
            .collect();
        for event in graph.outgoing(id, EdgeKind::Emits) {
            next.extend(graph.incoming(event, EdgeKind::Subscribes));
        }
        for node in next {
            if seen.insert(node) {
                queue.push(node);
            }
        }
    }
    seen
}

/// The settings a contract run reads again from the environment, for the
/// connections `names`, over `values`, a test run's: their numbers, or
/// each refusal (`test.contract_refused`) when one is missing, invalid or
/// chooses `live`.
pub(crate) fn settings(
    prepared: &Prepared,
    names: &BTreeSet<String>,
    values: &mut ConfigValues,
    contract: &Contract,
) -> Result<Vec<u32>, Vec<Diagnostic>> {
    let plan = prepared.plan();
    let config = plan.config();
    let mut refusals = Vec::new();
    let mut read = Vec::new();
    for (_, name, connection) in plan.package_connections() {
        if !names.contains(name) {
            continue;
        }
        let mut fields: Vec<u32> = connection
            .credential
            .iter()
            .flat_map(|credential| credential.source.settings())
            .collect();
        fields.extend(connection.address_settings());
        let problems = jadpo_runtime::config::read_again(
            config,
            prepared.validators(),
            values,
            contract.variables.as_ref(),
            &fields,
        );
        if let Some(problem) = problems.first() {
            let variables: Vec<String> = problems
                .iter()
                .map(|problem| format!("`{}`", problem.variable))
                .collect();
            let listed = crate::screen::either_and(&variables);
            refusals.push(match &problem.kind {
                ProblemKind::Missing => Diagnostic::new(
                    &rule::TEST_CONTRACT_REFUSED,
                    format!("`{name}` has nothing set for the provider's sandbox"),
                )
                .reason(format!(
                    "A contract run sends its cases' calls through `{name}` to the provider's sandbox, with the settings `jadpo dev` reads, and nothing sets {listed}, so only a stand-in would be sent."
                ))
                .next(
                    format!(
                        "Set {listed} in `.env.local`, or in the job's environment, for the sandbox, then run it again"
                    ),
                ),
                ProblemKind::Invalid { .. } => Diagnostic::new(
                    &rule::TEST_CONTRACT_REFUSED,
                    format!("`{name}`'s settings for the provider's sandbox aren't valid"),
                )
                .reason(format!(
                    "A contract run sends its cases' calls through `{name}` to the provider's sandbox, with the settings `jadpo dev` reads, and {listed} {} a value its type takes.",
                    if variables.len() == 1 { "doesn't hold" } else { "don't hold" }
                ))
                .next(
                    format!("Correct {listed} in `.env.local`, or in the job's environment, then run it again"),
                ),
            });
            continue;
        }
        // Never live (area 11): the environment the connection's setting
        // chooses must be the sandbox.
        if let Address::Hosts { environment, .. } = &connection.address
            && values
                .text_at(*environment)
                .is_some_and(|chosen| chosen.eq_ignore_ascii_case("live"))
        {
            let variable = config
                .fields
                .get(*environment as usize)
                .map_or_else(String::new, |field| field.variable.clone());
            refusals.push(
                Diagnostic::new(
                    &rule::TEST_CONTRACT_REFUSED,
                    format!("`{name}`'s environment is live"),
                )
                .reason(format!(
                    "A contract run sends its cases' calls to the provider's sandbox, never its live API, and `{variable}` chooses `live`."
                ))
                .next(
                    format!("Set `{variable}` to `sandbox` for the run, then run it again"),
                ),
            );
            continue;
        }
        read.extend(fields);
    }
    if refusals.is_empty() {
        Ok(read)
    } else {
        Err(refusals)
    }
}

/// The operations in `needed` that no case sent, of those `sent` names.
pub(crate) fn uncovered(needed: Vec<Uncovered>, sent: &BTreeSet<String>) -> Vec<Uncovered> {
    needed
        .into_iter()
        .filter(|operation| !sent.contains(&operation.call))
        .collect()
}

/// Nothing for a contract run to do: no case, no suite and no operation
/// a case must send, or, with a selection, no case among the tests it
/// picks.
pub(crate) fn no_cases(tests: usize, selected: bool) -> Box<Diagnostic> {
    let reason = if selected {
        format!(
            "Of the {} asked for, none reaches a provider operation through one of the app's connections.",
            if tests == 1 {
                "1 test".to_string()
            } else {
                format!("{tests} tests")
            }
        )
    } else {
        "No test reaches a provider operation through one of the app's connections, and none of the app's own integrations fills `ai` or `email` through the connection the app uses.".to_string()
    };
    Box::new(
        Diagnostic::new(&rule::TEST_NO_TESTS, "No contract cases to run")
            .reason(reason)
            .next("Write a test that calls an action reaching a provider, or leave out `--kind contract`"),
    )
}
