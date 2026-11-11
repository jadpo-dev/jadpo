//! The query report (area 14, "The `build/` folder"; area 6, "Where a
//! read comes from"; area 16, "Setting and changing them"):
//! `build/report/queries.md` and its `.json` twin. What each read costs:
//! each statement a query sends, with the table it reads, the most rows it
//! returns and the key or index that serves it; each query call, with
//! where it reads from; and the most queries each route, action, schedule
//! and subscriber sends in one request, which is where a threshold's
//! `queries:` starts. A statement no index serves is listed first, as the
//! problem to look at.
//!
//! Written only for a project with no errors, since the counts walk each
//! action's body as the plan holds it.

use std::collections::BTreeMap;

use jadpo_check::data::report::{StatementRead, reads, statement_name};
use jadpo_check::thresholds::{Limit, thresholds};
use jadpo_graph::json::Value;
use jadpo_graph::{EdgeKind, Graph, Key, Kind};
use jadpo_plan::{Decoded, Plan};

/// The report: its Markdown and its JSON twin.
pub struct Report {
    pub markdown: String,
    pub json: Value,
}

/// One place a query is called.
struct Call {
    query: String,
    /// What calls it: `action add_note`, `route GET /notes`.
    caller: String,
    place: String,
    /// Where it reads: the store, or the store in the action's
    /// transaction.
    from: &'static str,
}

/// One route, action, schedule or subscriber, with the most queries one
/// request or run of it sends.
struct Runs {
    label: String,
    place: String,
    queries: u64,
    /// The `queries:` a threshold watching it sets, and that threshold.
    limit: Option<(u64, String)>,
}

/// The project's query report; the plan is the project's, which checked.
pub fn report(db: &dyn jadpo_db::Db, project: jadpo_db::Project, graph: &Graph) -> Report {
    let plan = Plan::build(db, project);
    let statements = reads(db, project);
    // How many statements each query sends: its own and its includes'.
    let mut sends: BTreeMap<&str, u64> = BTreeMap::new();
    for read in &statements {
        *sends.entry(read.query.as_str()).or_default() += 1;
    }
    let query_sends = |name: &str| sends.get(name).copied().unwrap_or(1);

    // Each action's reads, its own and those of the actions it calls; no
    // action is in a cycle of calls, and no read sits in a loop.
    let callables = plan.callables();
    let mut counted: BTreeMap<u32, u64> = BTreeMap::new();
    fn count(
        plan: &Plan,
        at: u32,
        sends: &dyn Fn(&str) -> u64,
        counted: &mut BTreeMap<u32, u64>,
        depth: usize,
    ) -> u64 {
        if let Some(n) = counted.get(&at) {
            return *n;
        }
        let Some(callable) = plan.callable(at) else {
            return 0;
        };
        if depth > 64 {
            return 0;
        }
        let mut total = 0;
        for site in &callable.sites {
            total += match &site.decoded {
                Decoded::Query(Some(q)) => plan.query(*q).map_or(1, |query| sends(&query.name)),
                Decoded::Get(Some(_)) | Decoded::History(Some(_)) => 1,
                Decoded::Callable(Some(n)) => count(plan, *n, sends, counted, depth + 1),
                _ => 0,
            };
        }
        counted.insert(at, total);
        total
    }

    let place = |file: &str, line: u32| format!("{file}:{line}");
    let mut calls: Vec<Call> = Vec::new();
    for callable in callables {
        if callable.kind != Kind::Action {
            continue;
        }
        for (at, site) in callable.sites.iter().enumerate() {
            if let Decoded::Query(Some(q)) = &site.decoded
                && let Some(query) = plan.query(*q)
            {
                let line = callable
                    .body
                    .sites
                    .get(at)
                    .map_or(callable.line, |s| s.line);
                calls.push(Call {
                    query: query.name.clone(),
                    caller: format!("action {}", callable.key.name()),
                    place: place(&callable.file, line),
                    from: "the store, in the action's transaction",
                });
            }
        }
    }

    // What a threshold watching each node sets for `queries:`.
    let limits: BTreeMap<Key, (u64, String)> = thresholds(db, project)
        .thresholds
        .iter()
        .filter_map(|threshold| {
            let limit = threshold.limits.get(&Limit::Queries)?;
            Some((
                threshold.target.key.clone(),
                (*limit, threshold.subject.clone()),
            ))
        })
        .collect();
    let node_place = |key: &Key| {
        graph
            .get(key)
            .and_then(|node| node.place.as_ref())
            .map_or_else(String::new, |at| place(&at.file, at.lines.0))
    };
    let mut runs: Vec<Runs> = Vec::new();
    for kind in [Kind::Route, Kind::Schedule, Kind::Subscriber] {
        let mut found: Vec<_> = graph
            .ids()
            .filter(|&id| graph.node(id).kind == kind)
            .collect();
        found.sort_by(|a, b| graph.node(*a).name.cmp(&graph.node(*b).name));
        for id in found {
            let node = graph.node(id);
            let mut queries = 0;
            for target in graph.outgoing(id, EdgeKind::Runs) {
                let target = graph.node(target);
                match target.kind {
                    Kind::Query => {
                        queries += query_sends(&target.name);
                        calls.push(Call {
                            query: target.name.clone(),
                            caller: format!("{} {}", kind.word(), node.name),
                            place: node_place(&node.key),
                            from: "the store",
                        });
                    }
                    Kind::Action => {
                        if let Some(at) = plan.id(&target.key) {
                            queries += count(&plan, at, &query_sends, &mut counted, 0);
                        }
                    }
                    _ => {}
                }
            }
            runs.push(Runs {
                label: format!("{} {}", kind.word(), node.name),
                place: node_place(&node.key),
                queries,
                limit: limits.get(&node.key).cloned(),
            });
        }
    }
    let mut actions: Vec<(usize, &jadpo_plan::CallablePlan)> = callables
        .iter()
        .enumerate()
        .filter(|(_, callable)| callable.kind == Kind::Action)
        .collect();
    actions.sort_by(|a, b| a.1.key.name().cmp(b.1.key.name()));
    for (at, action) in actions {
        runs.push(Runs {
            label: format!("action {}", action.key.name()),
            place: place(&action.file, action.line),
            queries: count(&plan, at as u32, &query_sends, &mut counted, 0),
            limit: limits.get(&action.key).cloned(),
        });
    }
    calls.sort_by(|a, b| (&a.query, &a.caller, &a.place).cmp(&(&b.query, &b.caller, &b.place)));
    render(&statements, &calls, &runs)
}

fn render(statements: &[StatementRead], calls: &[Call], runs: &[Runs]) -> Report {
    let mut markdown = String::from(
        "# Queries\n\n\
         What each read costs: the most rows each statement returns and the key or index \
         that serves it, where each query is called, and the most queries one request or \
         run sends. Written by `jadpo check`.\n\n",
    );
    let unserved: Vec<&StatementRead> = statements.iter().filter(|r| r.index.is_none()).collect();
    if !unserved.is_empty() {
        markdown.push_str("## Reads every record\n\nNo key or index serves these statements, so each reads every record of its table: add a `where:` on an indexed field, or an `indexes:` entry.\n\n");
        for read in &unserved {
            markdown.push_str(&format!(
                "- {} reads every `{}` in `{}`\n",
                statement_name(read),
                read.entity,
                read.table
            ));
        }
        markdown.push('\n');
    }
    markdown.push_str("## Statements\n\n");
    if statements.is_empty() {
        markdown.push_str("This project declares no queries yet.\n\n");
    } else {
        markdown.push_str("| statement | reads | store | rows | index |\n|---|---|---|---|---|\n");
        for read in statements {
            markdown.push_str(&format!(
                "| {} | `{}` | `{}` | {} | {} |\n",
                statement_name(read),
                read.entity,
                read.store.as_deref().unwrap_or("–"),
                read.rows,
                read.index
                    .as_ref()
                    .map_or_else(|| "none: every record".to_string(), |i| format!("`{i}`")),
            ));
        }
        markdown.push('\n');
    }
    markdown.push_str("## Calls\n\n");
    if calls.is_empty() {
        markdown.push_str("Nothing calls a query yet.\n\n");
    } else {
        markdown.push_str(
            "A route's own read may come from its query's cache once caches are built; every read \
             inside an action is current.\n\n| query | called by | at | reads from |\n|---|---|---|---|\n",
        );
        for call in calls {
            markdown.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} |\n",
                call.query, call.caller, call.place, call.from
            ));
        }
        markdown.push('\n');
    }
    markdown.push_str("## Queries per request\n\n");
    if runs.is_empty() {
        markdown.push_str("This project has no routes, actions, schedules or subscribers yet.\n");
    } else {
        markdown.push_str(
            "The most queries one request or run sends: each query with one more for each include, \
             and each `get` and `history`, along every branch, through every action it calls. No \
             read can sit in a loop, so it never sends more. Writes aren't counted. A threshold's \
             `queries:` starts at this number.\n\n| runs | queries | threshold |\n|---|---|---|\n",
        );
        for run in runs {
            let threshold = match (&run.limit, run.queries) {
                (Some((limit, subject)), n) if *limit < n => {
                    format!("`queries: {limit}` in `{subject}`, below the {n} it can send")
                }
                (Some((limit, subject)), _) => format!("`queries: {limit}` in `{subject}`"),
                (None, 0) => "–".to_string(),
                (None, n) => format!("start at `queries: {n}`"),
            };
            markdown.push_str(&format!(
                "| `{}` | {} | {threshold} |\n",
                run.label, run.queries
            ));
        }
    }
    let mut json = Value::object();
    json.set("format", jadpo_graph::manifest::FORMAT);
    json.set(
        "statements",
        statements
            .iter()
            .map(|read| {
                let mut entry = Value::object();
                entry.set("query", read.query.as_str());
                entry.set_some(
                    "include",
                    read.include.iter().map(String::as_str).collect::<Vec<_>>(),
                );
                entry.set("entity", read.entity.as_str());
                entry.set_some("store", read.store.as_deref());
                entry.set("table", read.table.as_str());
                entry.set("rows", read.rows.as_str());
                entry.set_some("most", read.most.and_then(|n| i64::try_from(n).ok()));
                entry.set("index", read.index.as_deref());
                entry
            })
            .collect::<Vec<_>>(),
    );
    json.set(
        "calls",
        calls
            .iter()
            .map(|call| {
                let mut entry = Value::object();
                entry.set("query", call.query.as_str());
                entry.set("calledBy", call.caller.as_str());
                entry.set("at", call.place.as_str());
                entry.set(
                    "readsFrom",
                    if call.from == "the store" {
                        "store"
                    } else {
                        "transaction"
                    },
                );
                entry
            })
            .collect::<Vec<_>>(),
    );
    json.set(
        "runs",
        runs.iter()
            .map(|run| {
                let mut entry = Value::object();
                entry.set("runs", run.label.as_str());
                entry.set_some("at", (!run.place.is_empty()).then_some(run.place.as_str()));
                entry.set("queries", i64::try_from(run.queries).unwrap_or(i64::MAX));
                match &run.limit {
                    Some((limit, subject)) => {
                        let mut threshold = Value::object();
                        threshold.set("name", subject.as_str());
                        threshold.set("queries", i64::try_from(*limit).unwrap_or(i64::MAX));
                        entry.set("threshold", threshold);
                    }
                    None if run.queries > 0 => entry.set(
                        "startingQueries",
                        i64::try_from(run.queries).unwrap_or(i64::MAX),
                    ),
                    None => {}
                }
                entry
            })
            .collect::<Vec<_>>(),
    );
    Report { markdown, json }
}
