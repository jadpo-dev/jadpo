//! `jadpo privacy` (area 15, "Deadlines and events"; area 14, "Operator
//! commands"): a person's data requests, read from the database their
//! records are in.
//!
//! - `jadpo privacy requests` lists those still to finish, the nearest
//!   deadline first, 20 a page; `--state` lists those in one state, and
//!   `--all` every request, newest first. Above the footer an alert says
//!   how many are past their deadline or within a week of it, and the
//!   footer says how many are open.
//! - It reads and changes nothing else, and never shows an address: only a
//!   request's `id`, what was asked, how it started and how it stands.
//!
//! The database is the one `--url` names, else the address the store's
//! connection's setting gives, else the one a running `jadpo dev` started.

use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::http::privacy_steps::{self as runtime, Counts, Listed};
use jadpo_std::http::PRIVACY_ALERT_BEFORE_MS;
use jadpo_std::privacy::{
    CANCELLED, COMPLETED, DOWNLOADED, ERASING, EXPIRED, READY, RECEIVED, STALLED,
};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table, Token};

use crate::database::Address;
use crate::run::{Call, Reply, quoted};
use crate::schedules::{now, plural, unreadable, url_of};

/// What `jadpo privacy` does.
pub const ACTIONS: &[crate::table::Value] = &[crate::table::Value {
    name: "requests",
    description: "List the data requests people have made",
}];

/// The states `--state` lists.
pub const STATES: &[crate::table::Value] = &[
    crate::table::Value {
        name: RECEIVED,
        description: "Waiting: an export being made, an erasure cooling off",
    },
    crate::table::Value {
        name: ERASING,
        description: "Erased here, waiting for the providers",
    },
    crate::table::Value {
        name: STALLED,
        description: "A provider gave up, and an operator must look at it",
    },
    crate::table::Value {
        name: READY,
        description: "An export whose link hasn't been used",
    },
    crate::table::Value {
        name: DOWNLOADED,
        description: "An export that was downloaded",
    },
    crate::table::Value {
        name: EXPIRED,
        description: "An export whose link ended unused",
    },
    crate::table::Value {
        name: COMPLETED,
        description: "An erasure every provider has confirmed",
    },
    crate::table::Value {
        name: CANCELLED,
        description: "An erasure cancelled in its cooling-off",
    },
];

/// How many requests a page shows.
pub const PAGE: u64 = 20;

/// The states of a request still to finish.
const OPEN: [&str; 3] = [RECEIVED, ERASING, STALLED];

pub fn privacy(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("requests") => {
            if let Some(extra) = call.arguments.get(1) {
                return Reply::failed(arguments(
                    format!("`jadpo privacy requests` lists them all, not `{extra}`"),
                    "It takes no id; `--state` and `--all` pick which to list.".to_string(),
                    "Run `jadpo privacy requests`, or `jadpo privacy requests --all`".to_string(),
                ));
            }
            requests(call)
        }
        Some(other) => Reply::failed(arguments(
            format!("`jadpo privacy` has no action `{other}`"),
            "It takes `requests`.".to_string(),
            "Run `jadpo privacy requests`".to_string(),
        )),
        None => Reply::failed(arguments(
            "`jadpo privacy` needs an action".to_string(),
            "It takes `requests`.".to_string(),
            "Run `jadpo privacy requests`".to_string(),
        )),
    }
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_PRIVACY_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// Which requests the command was asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Asked {
    Open,
    All,
    State(String),
}

fn asked(call: &Call) -> Result<Asked, Reply> {
    if call.value("--state").is_some() && call.flag("--all") {
        return Err(Reply::failed(arguments(
            "`--state` and `--all` both pick which requests to list".to_string(),
            "A list is one or the other.".to_string(),
            "Keep `--state` or `--all`, not both".to_string(),
        )));
    }
    if call.flag("--all") {
        return Ok(Asked::All);
    }
    match call.value("--state") {
        None => Ok(Asked::Open),
        Some(state) if STATES.iter().any(|known| known.name == state) => {
            Ok(Asked::State(state.to_string()))
        }
        Some(state) => {
            let names: Vec<&str> = STATES.iter().map(|known| known.name).collect();
            let near = jadpo_diag::nearest(state, names.iter().copied());
            Err(Reply::failed(arguments(
                format!("`{state}` isn't a state"),
                format!(
                    "The states listed are {}.{}",
                    crate::run::both(&quoted(&names)),
                    near.map(|near| format!(" Did you mean `{near}`?"))
                        .unwrap_or_default()
                ),
                match near {
                    Some(near) => format!("Use `--state {near}`"),
                    None => "Use `--state received`, or leave it out".to_string(),
                },
            )))
        }
    }
}

fn page_of(call: &Call) -> Result<u64, Reply> {
    match call.value("--page") {
        None => Ok(1),
        Some(written) => match written.parse::<u64>() {
            Ok(page) if page >= 1 => Ok(page),
            _ => Err(Reply::failed(arguments(
                format!("`{written}` isn't a page"),
                "Pages are counted from 1.".to_string(),
                "Use a page such as `--page 2`".to_string(),
            ))),
        },
    }
}

/// The store a project's data requests are kept in.
fn store_of(call: &Call) -> Result<(PathBuf, jadpo_db::Loaded, String), Reply> {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return Err(crate::fmt::project_needed("privacy"));
    };
    let loaded = jadpo_db::load(&root);
    let schema = jadpo_check::data::schema(&loaded.db, loaded.project);
    let store = schema
        .stores
        .iter()
        .find(|(_, store)| {
            store
                .tables
                .contains_key(jadpo_std::system::PRIVACY_REQUESTS.table)
        })
        .map(|(name, _)| name.clone());
    match store {
        Some(store) => Ok((root, loaded, store)),
        None => Err(Reply::failed(arguments(
            "This application takes no data requests".to_string(),
            "`privacy:` in `application { }` turns them on, once it says more than `erasure_log:`."
                .to_string(),
            "Add `privacy:` to `application { }` in `app.jadpo`".to_string(),
        ))),
    }
}

fn requests(call: &Call) -> Reply {
    let asked = match asked(call) {
        Ok(asked) => asked,
        Err(reply) => return reply,
    };
    let page = match page_of(call) {
        Ok(page) => page,
        Err(reply) => return reply,
    };
    let (root, loaded, store) = match store_of(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let url = match url_of(call, &root, &loaded, &store) {
        Ok(Some(url)) => url,
        Ok(None) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::PRIVACY_UNAVAILABLE,
                    format!("Nothing says where the database `{store}` is"),
                )
                .reason("Its connection's setting has no value here, and no `jadpo dev` is running for this project.".to_string())
                .next("Pass its address with `--url`, or start `jadpo dev`"),
            );
        }
        Err(reply) => return reply,
    };
    let now = now(call);
    let limit = i64::try_from(PAGE).unwrap_or(i64::MAX);
    let offset = i64::try_from((page - 1).saturating_mul(PAGE)).unwrap_or(i64::MAX);
    let states: Vec<String> = match &asked {
        Asked::Open => OPEN.iter().map(|state| state.to_string()).collect(),
        Asked::All => Vec::new(),
        Asked::State(state) => vec![state.clone()],
    };
    let latest_first = asked != Asked::Open;
    let read = read_requests(&store, &url, states, latest_first, limit, offset, now);
    let (listed, counts) = match read {
        Ok(read) => read,
        Err(reply) => return reply,
    };
    let total = match &asked {
        Asked::Open => counts.open(),
        Asked::All => counts.total(),
        Asked::State(state) => counts.of(state),
    };
    let pages = total.div_ceil(PAGE).max(1);
    if page > pages && total > 0 {
        return Reply::failed(arguments(
            format!("There is no page {page}"),
            format!(
                "{} make {}.",
                plural(total, "request", "requests"),
                plural(pages, "page", "pages")
            ),
            format!("Use a page from 1 to {pages}"),
        ));
    }
    let data = data_of(&listed, &counts, (total, page, pages));
    Reply::screen(
        screen_of(&asked, &listed, &counts, (total, page, pages), now),
        data,
    )
}

fn read_requests(
    store: &str,
    url: &Address,
    states: Vec<String>,
    latest_first: bool,
    limit: i64,
    offset: i64,
    now: i64,
) -> Result<(Vec<Listed>, Counts), Reply> {
    let failed = |error: jadpo_runtime::StoreError| {
        Reply::failed(unreadable(
            &rule::PRIVACY_UNAVAILABLE,
            store,
            "data requests",
            &error,
        ))
    };
    let runtime_host = jadpo_host_native::Runtime::new()
        .map_err(|error| failed(jadpo_runtime::StoreError::Fault(error.to_string())))?;
    let database = url.open().map_err(failed)?;
    runtime_host
        .block_on(async {
            let listed = runtime::list(&database, &states, latest_first, limit, offset).await?;
            let counts = runtime::counts(&database, now).await?;
            Ok::<_, jadpo_runtime::StoreError>((listed, counts))
        })
        .map_err(failed)
}

fn instant(at: Option<i64>) -> Json {
    at.map_or(Json::Null, |at| Json::from(jadpo_std::format_instant(at)))
}

fn number(count: u64) -> Json {
    Json::from(i64::try_from(count).unwrap_or(i64::MAX))
}

fn data_of(listed: &[Listed], counts: &Counts, (total, page, pages): (u64, u64, u64)) -> Json {
    Json::object([
        ("page", number(page)),
        ("pages", number(pages)),
        ("total", number(total)),
        ("open", number(counts.open())),
        ("overdue", number(counts.overdue)),
        ("nearing", number(counts.nearing)),
        (
            "requests",
            Json::array(listed.iter().map(|request| {
                Json::object([
                    ("id", Json::from(request.id.as_str())),
                    ("kind", Json::from(request.kind.as_str())),
                    ("source", Json::from(request.source.as_str())),
                    ("state", Json::from(request.state.as_str())),
                    ("person", Json::from(request.person.as_str())),
                    (
                        "createdBy",
                        request.created_by.as_deref().map_or(Json::Null, Json::from),
                    ),
                    (
                        "identityCheck",
                        request
                            .identity_check
                            .as_deref()
                            .map_or(Json::Null, Json::from),
                    ),
                    ("requestedAt", instant(Some(request.requested_at))),
                    ("deadline", instant(Some(request.deadline))),
                    ("erasesAt", instant(request.erases_at)),
                    ("linkEndsAt", instant(request.link_ends_at)),
                    ("completedAt", instant(request.completed_at)),
                ])
            })),
        ),
    ])
}

/// How a request started, in a word.
fn from_word(source: &str) -> &'static str {
    match source {
        "self_service" => "email",
        "signed_in" => "signed in",
        "assisted" => "staff",
        _ => "unknown",
    }
}

/// A request's state as the list shows it.
fn state_cell(request: &Listed) -> Cell {
    match (request.kind.as_str(), request.state.as_str()) {
        ("erasure", RECEIVED) => Cell::state(Symbol::Pending, "cooling off"),
        (_, RECEIVED) => Cell::state(Symbol::Pending, "being made"),
        (_, ERASING) => Cell::state(Symbol::Running, "erasing"),
        (_, STALLED) => Cell::state(Symbol::Decision, "stalled"),
        (_, READY) => Cell::state(Symbol::Success, "ready"),
        (_, DOWNLOADED) => Cell::state(Symbol::Success, "downloaded"),
        (_, COMPLETED) => Cell::state(Symbol::Success, "completed"),
        (_, EXPIRED) => Cell::state(Symbol::Skipped, "expired"),
        (_, CANCELLED) => Cell::state(Symbol::Skipped, "cancelled"),
        (_, other) => Cell::state(Symbol::Skipped, other.replace('_', " ")),
    }
}

/// How long there is left, in whole days: rounded down when there is time
/// left, so a deadline is never made to look further away than it is.
fn days_word(deadline: i64, now: i64) -> String {
    const DAY: i64 = 86_400_000;
    if deadline <= now {
        let late = (now - deadline).div_euclid(DAY).max(0).unsigned_abs();
        return if late == 0 {
            "due today".to_string()
        } else {
            format!("{} overdue", plural(late, "day", "days"))
        };
    }
    let left = (deadline - now).div_euclid(DAY).unsigned_abs();
    if left == 0 {
        "due today".to_string()
    } else {
        format!("in {}", plural(left, "day", "days"))
    }
}

/// The deadline column: its date, and for a request still to finish how
/// close it is.
fn deadline_cell(request: &Listed, now: i64) -> Cell {
    let date = jadpo_std::format_instant(request.deadline);
    let date = date.get(..10).unwrap_or_default();
    if OPEN.contains(&request.state.as_str()) {
        Cell::text(date).then(format!("  {}", days_word(request.deadline, now)))
    } else {
        Cell::muted(date)
    }
}

fn screen_of(
    asked: &Asked,
    listed: &[Listed],
    counts: &Counts,
    (total, page, pages): (u64, u64, u64),
    now: i64,
) -> Screen {
    let mut screen = Screen::new();
    if listed.is_empty() {
        let (headline, facts) = match asked {
            Asked::Open => (
                "No data requests are open".to_string(),
                if counts.total() > 0 {
                    vec![Fact::plain(format!("{} in all", counts.total()))]
                } else {
                    Vec::new()
                },
            ),
            Asked::All => ("No data requests yet".to_string(), Vec::new()),
            Asked::State(state) => (format!("No data requests are {state}"), Vec::new()),
        };
        screen.push(Block::summary(Outcome::Success, headline, facts));
        return screen;
    }
    let mut table = Table::new(["id", "kind", "from", "state", "deadline"]).cut(1);
    for request in listed {
        table = table.row(vec![
            Cell::text(request.short_id()),
            Cell::text(request.kind.as_str()),
            Cell::muted(from_word(&request.source)),
            state_cell(request),
            deadline_cell(request, now),
        ]);
    }
    screen.push(Block::Table(table));
    // What needs someone's attention soonest, whatever list this is.
    let mut alerts = Vec::new();
    if counts.overdue > 0 {
        alerts.push(format!(
            "{} past {} legal deadline",
            plural(counts.overdue, "request is", "requests are"),
            if counts.overdue == 1 { "its" } else { "their" },
        ));
    }
    if counts.nearing > 0 {
        alerts.push(format!(
            "{} due within {} days",
            plural(counts.nearing, "request is", "requests are"),
            PRIVACY_ALERT_BEFORE_MS / 86_400_000,
        ));
    }
    if !alerts.is_empty() {
        screen.push(Block::Alerts(alerts));
    }
    let mut facts = Vec::new();
    let (outcome, headline) = match asked {
        Asked::Open if counts.overdue > 0 => {
            facts.push(Fact::plain(format!("{} open", counts.open())));
            (Outcome::Failure, format!("{} overdue", counts.overdue))
        }
        Asked::Open => (Outcome::Success, format!("{} open", counts.open())),
        Asked::All => (Outcome::Success, plural(total, "request", "requests")),
        Asked::State(state) => (Outcome::Success, format!("{total} {state}")),
    };
    if counts.stalled() > 0 {
        facts.push(Fact {
            text: format!("{} stalled", counts.stalled()),
            tone: Some(Token::Decision),
        });
    }
    if *asked == Asked::Open && counts.total() > counts.open() {
        facts.push(Fact::plain(format!("{} in all", counts.total())));
    }
    if pages > 1 {
        facts.push(Fact::plain(format!("page {page} of {pages}")));
    }
    screen.push(Block::summary(outcome, headline, facts).with_plain_headline("data requests"));
    if counts.stalled() > 0 {
        screen.push(Block::hint(
            "jadpo deliveries",
            "shows what a stalled erasure is waiting for",
        ));
    }
    screen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deadline_is_said_in_days_and_never_further_off_than_it_is() {
        let now = 1_000 * 86_400_000;
        assert_eq!(days_word(now + 30 * 86_400_000, now), "in 30 days");
        assert_eq!(days_word(now + 86_400_000 + 1, now), "in 1 day");
        assert_eq!(days_word(now + 3_600_000, now), "due today");
        assert_eq!(days_word(now - 3_600_000, now), "due today");
        assert_eq!(days_word(now - 3 * 86_400_000, now), "3 days overdue");
    }

    #[test]
    fn a_request_says_how_it_started_and_how_it_stands() {
        assert_eq!(from_word("self_service"), "email");
        assert_eq!(from_word("signed_in"), "signed in");
        assert_eq!(from_word("assisted"), "staff");
    }
}
