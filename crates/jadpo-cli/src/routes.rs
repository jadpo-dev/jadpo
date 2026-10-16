//! `jadpo routes` (area 7, question 15): every route with its full method
//! and path, what it runs, its authentication and where it is written,
//! grouped by API. Authentication is `required` for a route that needs a
//! signed-in caller, `none` for one anyone may call and `fresh` for one
//! that reads the caller's record now (area 8).
//!
//! The screen is one block per API, its name and prefix in bold, then a
//! row per route in the order a URL is read; the footer counts them. A
//! route the runtime answers itself, the key exchange a revocation delay
//! and service keys bring (area 8), is listed in each API that accepts
//! service keys, running `generated exchange`, written `generated`.

use std::path::PathBuf;

use jadpo_check::routes::{Groups, api_table, groups};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_not_found;
use crate::migrate::blocked;
use crate::run::{Call, Reply};

pub fn routes(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("routes");
    };
    let loaded = jadpo_db::load(&root);
    let (db, project) = (&loaded.db, loaded.project);
    if !jadpo_check::routes::routes_clean(db, project) {
        let problems = jadpo_check::check(db, project)
            .into_iter()
            .filter(|d| d.severity() == jadpo_diag::Severity::Error && d.rule.area == 7)
            .collect::<Vec<_>>();
        if !problems.is_empty() {
            return blocked(&loaded, &cwd, problems, "routes stopped");
        }
    }
    let apis = api_table(db, project);
    let groups: &Groups = groups(db, project);
    let graph = jadpo_check::graph::graph(db, project);
    let mut lines = listing(groups, graph);
    if jadpo_check::identity::identity(db, project).exchanges() {
        for api in &apis.apis {
            let methods = jadpo_check::routes::apis::accepted_methods(db, project, &api.name);
            if methods.contains(&jadpo_std::AuthenticationMethod::ServiceKey) {
                lines.push(Line {
                    api: api.name.clone(),
                    method: "POST".to_string(),
                    path: format!("/{}", jadpo_std::http::EXCHANGE_PATH.join("/")),
                    runs: "generated exchange".to_string(),
                    authentication: "required",
                    file: String::new(),
                    line: 0,
                    place: "generated".to_string(),
                });
            }
        }
        lines.sort_by(|a, b| (&a.api, &a.path, &a.method).cmp(&(&b.api, &b.path, &b.method)));
    }
    let mut screen = Screen::new();
    let mut data = Vec::new();
    let mut count = 0usize;
    for api in &apis.apis {
        let rows: Vec<&Line> = lines.iter().filter(|line| line.api == api.name).collect();
        if rows.is_empty() {
            continue;
        }
        let widths = [
            rows.iter().map(|r| r.method.len()).max().unwrap_or(0),
            rows.iter().map(|r| r.path.len()).max().unwrap_or(0),
            rows.iter().map(|r| r.runs.len()).max().unwrap_or(0),
            rows.iter()
                .map(|r| r.authentication.len())
                .max()
                .unwrap_or(0),
        ];
        let shown = rows
            .iter()
            .map(|r| {
                format!(
                    "{:w0$}  {:w1$}  {:w2$}  {:w3$}  {}",
                    r.method,
                    r.path,
                    r.runs,
                    r.authentication,
                    r.place,
                    w0 = widths[0],
                    w1 = widths[1],
                    w2 = widths[2],
                    w3 = widths[3]
                )
            })
            .collect();
        screen.push(Block::Section {
            heading: format!("{}  {}", api.name, api.path()),
            lines: shown,
        });
        count += rows.len();
        for r in rows {
            data.push(Json::object([
                ("api", Json::from(r.api.as_str())),
                ("method", Json::from(r.method.as_str())),
                ("path", Json::from(r.path.as_str())),
                ("runs", Json::from(r.runs.as_str())),
                ("authentication", Json::from(r.authentication)),
                ("file", Json::from(r.file.as_str())),
                ("line", Json::from(r.line as usize)),
                ("generated", Json::from(r.file.is_empty())),
            ]));
        }
    }
    let apis_shown = apis
        .apis
        .iter()
        .filter(|api| lines.iter().any(|line| line.api == api.name))
        .count();
    screen.push(Block::summary(
        Outcome::Success,
        if count == 1 {
            "1 route".to_string()
        } else {
            format!("{count} routes")
        },
        [Fact::plain(if apis_shown == 1 {
            "1 API".to_string()
        } else {
            format!("{apis_shown} APIs")
        })],
    ));
    Reply::screen(screen, Json::object([("routes", Json::array(data))]))
}

struct Line {
    api: String,
    method: String,
    path: String,
    runs: String,
    authentication: &'static str,
    file: String,
    line: u32,
    place: String,
}

fn listing(groups: &Groups, graph: &jadpo_graph::Graph) -> Vec<Line> {
    let mut out: Vec<Line> = groups
        .routes()
        .map(|(group, route)| {
            let line = graph
                .get(&route.key)
                .and_then(|node| node.place.as_ref())
                .map_or(0, |place| place.lines.0);
            Line {
                api: group.api.clone(),
                method: route.method.word().to_string(),
                path: route.path(),
                runs: format!(
                    "{} {}",
                    if route.run.query { "query" } else { "action" },
                    route.run.name
                ),
                authentication: match route.authentication {
                    jadpo_std::http::Authentication::Required => "required",
                    jadpo_std::http::Authentication::Public => "none",
                    jadpo_std::http::Authentication::Fresh => "fresh",
                },
                file: group.path.clone(),
                line,
                place: format!("{}:{line}", group.path),
            }
        })
        .collect();
    out.sort_by(|a, b| (&a.api, &a.path, &a.method).cmp(&(&b.api, &b.path, &b.method)));
    out
}
