//! `jadpo flags` (area 16, "Changing a flag" and "Removing a flag"; area
//! 14; chunk B10): reads and changes `jadpo_flags` in the flags' store,
//! found as `jadpo schedules` finds its database, and rewrites the source
//! to remove a flag.
//!
//! - `jadpo flags` (or `list`) shows each declared flag's state: everyone's
//!   value, how many users are named, its shares, when it last changed,
//!   and whether it is ready to remove.
//! - `set <flag> [<value>]` changes one part of the state: everyone's
//!   value, named users' (`--users`), one share (`--percent`) or a split
//!   (`--split`). `reset <flag>` gives everyone the `default:` again, or
//!   with `--users` stops naming those users. Each change is logged in
//!   `jadpo_flag_changes` with who made it, in the same statement.
//! - `remove <flag> --keep <value>` rewrites the source, checked.
//!
//! None of it runs the app's code.

use std::path::PathBuf;

use jadpo_check::flags::Flag;
use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::flags::{Changed, Given, Stored};
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table};

use crate::database::Address;
use crate::run::{Call, Reply, quoted};

/// What `jadpo flags` does besides listing.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "list",
        description: "Show each flag's state",
    },
    crate::table::Value {
        name: "set",
        description: "Change who gets which value of a flag",
    },
    crate::table::Value {
        name: "reset",
        description: "Give everyone a flag's default again",
    },
    crate::table::Value {
        name: "remove",
        description: "Remove a flag from the source, keeping one value",
    },
];

/// How long every caller must have had one value before a flag is ready
/// to remove: seven days, in milliseconds.
pub const SETTLED_MS: i64 = 7 * 86_400_000;

/// The options each action takes; any other is refused rather than
/// ignored.
const OPTIONS: &[(&str, &[&str])] = &[
    ("list", &["--url"]),
    ("set", &["--url", "--users", "--percent", "--split"]),
    ("reset", &["--url", "--users"]),
    ("remove", &["--keep"]),
];

pub fn flags(call: &Call) -> Reply {
    let action = call.arguments.first().map(String::as_str);
    let shown = action.unwrap_or("list");
    if let Some((_, allowed)) = OPTIONS.iter().find(|(name, _)| *name == shown)
        && let Some((given, _)) = call
            .values
            .iter()
            .find(|(option, _)| !allowed.contains(option))
    {
        return Reply::failed(arguments(
            format!("`jadpo flags {shown}` takes no `{given}`"),
            format!(
                "`jadpo flags {shown}` takes {}.",
                crate::run::either(&quoted(allowed.iter()))
            ),
            format!("Leave out `{given}`"),
        ));
    }
    match action {
        None | Some("list") => {
            if let Some(extra) = call.arguments.get(1) {
                return Reply::failed(arguments(
                    format!("`jadpo flags list` takes no flag, not `{extra}`"),
                    "It lists every flag the project declares.".to_string(),
                    "Run `jadpo flags list`".to_string(),
                ));
            }
            match Project::load(call) {
                Ok(project) => list(call, &project),
                Err(reply) => reply,
            }
        }
        Some(action @ ("set" | "reset" | "remove")) => {
            let project = match Project::load(call) {
                Ok(project) => project,
                Err(reply) => return reply,
            };
            let flag = match project.named(call.arguments.get(1), action) {
                Ok(flag) => flag,
                Err(reply) => return reply,
            };
            match action {
                "set" => set(call, &project, &flag),
                "reset" => reset(call, &project, &flag),
                _ => remove(call, project, &flag),
            }
        }
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            let reason = format!(
                "`jadpo flags` lists the flags, and {} act on one.",
                crate::run::both(&quoted(names.iter().skip(1)))
            );
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo flags` does"),
                match near {
                    Some(near) => format!("{reason} Did you mean `{near}`?"),
                    None => reason,
                },
                match near {
                    Some(near) => format!("Use `{near}` instead of `{other}`"),
                    None => "Run `jadpo flags`, or `jadpo flags set <flag> <value>`".to_string(),
                },
            ))
        }
    }
}

/// The project's flags, by name, for Tab completion.
pub(crate) fn names(cwd: &std::path::Path) -> Vec<(String, &'static str)> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let loaded = jadpo_db::load(&root);
    jadpo_check::flags::flags(&loaded.db, loaded.project)
        .flags
        .iter()
        .map(|flag| (flag.name.clone(), "Flag"))
        .collect()
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_FLAGS_ARGUMENTS, message)
        .reason(reason)
        .next(next)
}

/// The project as these commands need it: its flags and where their state
/// is kept.
struct Project {
    root: PathBuf,
    cwd: PathBuf,
    loaded: jadpo_db::Loaded,
    flags: Vec<Flag>,
    store: Option<String>,
}

impl Project {
    fn load(call: &Call) -> Result<Project, Reply> {
        let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
        let Some(root) = jadpo_db::find_root(&cwd) else {
            return Err(crate::fmt::project_needed("flags"));
        };
        let loaded = jadpo_db::load(&root);
        let flags = jadpo_check::flags::flags(&loaded.db, loaded.project)
            .flags
            .clone();
        let store = jadpo_check::flags::store(&loaded.db, loaded.project);
        Ok(Project {
            root,
            cwd,
            loaded,
            flags,
            store,
        })
    }

    /// The flag `name` names.
    fn named(&self, name: Option<&String>, action: &str) -> Result<Flag, Reply> {
        let Some(name) = name else {
            return Err(Reply::failed(arguments(
                format!("`jadpo flags {action}` needs a flag"),
                "It acts on one flag, named as `flags.jadpo` declares it.".to_string(),
                format!("Run `jadpo flags {action} <flag>`"),
            )));
        };
        if let Some(flag) = self.flags.iter().find(|flag| flag.name == *name) {
            return Ok(flag.clone());
        }
        let names: Vec<&str> = self.flags.iter().map(|flag| flag.name.as_str()).collect();
        if names.is_empty() {
            return Err(Reply::failed(arguments(
                format!("`{name}` isn't a flag"),
                "The project declares no flags.".to_string(),
                "Declare one in `flags { }` in `flags.jadpo`".to_string(),
            )));
        }
        let near = jadpo_diag::nearest(name, names.iter().copied());
        Err(Reply::failed(arguments(
            format!("`{name}` isn't a flag"),
            match near {
                Some(near) => format!("`flags.jadpo` declares no `{name}`. Did you mean `{near}`?"),
                None => format!(
                    "`flags.jadpo` declares {}.",
                    crate::run::both(&quoted(names.iter()))
                ),
            },
            match near {
                Some(near) => format!("Use `{near}` instead of `{name}`"),
                None => "Run `jadpo flags list` to see the flags".to_string(),
            },
        )))
    }

    fn database(&self) -> Result<String, Reply> {
        self.store.clone().ok_or_else(|| {
            Reply::failed(
                Diagnostic::new(
                    &rule::FLAGS_UNAVAILABLE,
                    "The flags have no database to keep their state in",
                )
                .reason("`jadpo check` says why: `flag.store_required`.")
                .next("Run `jadpo check`, and fix what it reports"),
            )
        })
    }

    fn address(&self, call: &Call) -> Result<(String, Address), Reply> {
        let store = self.database()?;
        match crate::schedules::url_of(call, &self.root, &self.loaded, &store) {
            Ok(Some(url)) => Ok((store, url)),
            Ok(None) => Err(Reply::failed(arguments(
                format!("`jadpo flags` needs the address of the database `{store}`"),
                "No variable gives the address of its connection, and no `jadpo dev` is running with one: a connection reads its address from the setting its `url:` names (area 10), and `--url` names a database directly.".to_string(),
                "Run `jadpo config set <the connection's setting>`, start `jadpo dev`, or add `--url postgresql://…`".to_string(),
            ))),
            Err(reply) => Err(reply),
        }
    }
}

fn with_database<T>(
    store: &str,
    url: &Address,
    work: impl for<'d> FnOnce(
        &'d dyn jadpo_runtime::Database,
    )
        -> jadpo_runtime::LocalFuture<'d, Result<T, jadpo_runtime::StoreError>>,
) -> Result<T, Reply> {
    let failed = |error: jadpo_runtime::StoreError| {
        Reply::failed(crate::schedules::unreadable(
            &rule::FLAGS_UNAVAILABLE,
            store,
            "flags",
            &error,
        ))
    };
    let runtime = jadpo_host_native::Runtime::new()
        .map_err(|error| failed(jadpo_runtime::StoreError::Fault(error.to_string())))?;
    let database = url.open().map_err(failed)?;
    runtime.block_on(work(&database)).map_err(failed)
}

fn read(store: &str, url: &Address) -> Result<Vec<Stored>, Reply> {
    with_database(store, url, |database| {
        Box::pin(async move { jadpo_runtime::flags::stored(database).await })
    })
}

// --- words ------------------------------------------------------------------

/// What a state gives, in words: `true for 2 named users; true for 10% of
/// users; false for everyone else`.
fn described(flag: &Flag, given: Option<&Given>) -> String {
    let default = default_name(flag);
    let Some(given) = given else {
        return format!("{default} for everyone (the default)");
    };
    if let Some(value) = given.uniform(&default) {
        return match &given.value {
            None => format!("{value} for everyone (the default)"),
            Some(_) => format!("{value} for everyone"),
        };
    }
    let mut parts = Vec::new();
    let mut by_value: Vec<(String, usize)> = Vec::new();
    for (_, value) in &given.users {
        match by_value.iter_mut().find(|(seen, _)| seen == value) {
            Some((_, count)) => *count += 1,
            None => by_value.push((value.clone(), 1)),
        }
    }
    for (value, count) in by_value {
        parts.push(format!(
            "{value} for {count} named user{}",
            if count == 1 { "" } else { "s" }
        ));
    }
    let shares: Vec<String> = given
        .shares
        .iter()
        .filter(|(_, percent)| *percent > 0)
        .map(|(value, percent)| format!("{value} for {percent}%"))
        .collect();
    if !shares.is_empty() {
        parts.push(format!("{} of users", shares.join(", ")));
    }
    let everyone = match &given.value {
        Some(value) => value.clone(),
        None => format!("{default} (the default)"),
    };
    if parts.is_empty() {
        parts.push(format!("{everyone} for everyone"));
    } else {
        parts.push(format!("{everyone} for everyone else"));
    }
    parts.join("; ")
}

fn default_name(flag: &Flag) -> String {
    flag.values
        .get(flag.default as usize)
        .cloned()
        .unwrap_or_default()
}

/// The place among `flag`'s values `written` names: a value as a test's
/// `flags:` writes it, `true` or a variant's name, or `Type.variant`.
fn value_of(flag: &Flag, written: &str) -> Option<String> {
    let bare = match &flag.enum_type {
        Some(own) => written.strip_prefix(&format!("{own}.")).unwrap_or(written),
        None => written,
    };
    flag.values.iter().find(|value| *value == bare).cloned()
}

fn not_a_value(flag: &Flag, written: &str) -> Reply {
    let near = jadpo_diag::nearest(written, flag.values.iter().map(String::as_str));
    Reply::failed(arguments(
        format!("`{written}` isn't a value of `{}`", flag.name),
        format!(
            "`{}` is {}.{}",
            flag.name,
            crate::run::either(&quoted(flag.values.iter())),
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        ),
        match near {
            Some(near) => format!("Use `{near}` instead of `{written}`"),
            None => format!("Give one of the values of `{}`", flag.name),
        },
    ))
}

/// Who the operator is, as the machine names them: `USER`, else
/// `LOGNAME` or `USERNAME`.
fn operator(call: &Call) -> String {
    ["USER", "LOGNAME", "USERNAME"]
        .iter()
        .find_map(|name| {
            call.environment
                .var(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// The ids `--users` gives, each a `Uuid`, in lowercase.
fn users(call: &Call) -> Result<Vec<String>, Reply> {
    let Some(written) = call.value("--users") else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for id in written
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if !is_uuid(id) {
            return Err(Reply::failed(arguments(
                format!("`{id}` isn't a user's id"),
                "`--users` names users by their `id`, a `Uuid` such as `0192f1c4-7a3e-7c1d-9b2a-5e6f7a8b9c0d`, separated by commas.".to_string(),
                "Give each user's `id` in full".to_string(),
            )));
        }
        let id = id.to_ascii_lowercase();
        if !out.contains(&id) {
            out.push(id);
        }
    }
    if out.is_empty() {
        return Err(Reply::failed(arguments(
            "`--users` names no user".to_string(),
            "`--users` names users by their `id`, separated by commas.".to_string(),
            "Give at least one user's `id` after `--users`".to_string(),
        )));
    }
    Ok(out)
}

/// Whether `text` is a `Uuid` as text writes one: 32 hex digits in groups
/// of 8, 4, 4, 4 and 12.
fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A percentage `--percent` or `--split` gives: a whole number from 0 to
/// 100.
fn percent(written: &str, option: &str) -> Result<u8, Reply> {
    match written.trim().trim_end_matches('%').parse::<u8>() {
        Ok(percent) if percent <= 100 => Ok(percent),
        _ => Err(Reply::failed(arguments(
            format!("`{written}` isn't a percentage"),
            format!("`{option}` takes a whole percentage of users, from 0 to 100."),
            format!("Give `{option}` a whole number from 0 to 100"),
        ))),
    }
}

// --- list -------------------------------------------------------------------

fn list(call: &Call, project: &Project) -> Reply {
    if project.flags.is_empty() {
        let screen = Screen::new().with(Block::summary(
            Outcome::Success,
            "0 flags",
            [Fact::plain("`flags.jadpo` declares none")],
        ));
        return Reply::screen(
            screen,
            Json::object([("flags", Json::array(Vec::<Json>::new()))]),
        );
    }
    let (store, url) = match project.address(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let stored = match read(&store, &url) {
        Ok(stored) => stored,
        Err(reply) => return reply,
    };
    let now = crate::schedules::now(call);
    let mut table = Table::new(["flag", "everyone", "users", "shares", "changed", "state"])
        .right(2)
        .cut(3);
    let mut ready = Vec::new();
    let mut data = Vec::new();
    for flag in &project.flags {
        let row = stored.iter().find(|row| row.flag == flag.name);
        let default = default_name(flag);
        let given = row.map(|row| &row.kept);
        let everyone = given
            .and_then(|given| given.value.clone())
            .unwrap_or_else(|| default.clone());
        let users = given.map_or(0, |given| given.users.len());
        let shares: Vec<String> = given
            .map(|given| {
                given
                    .shares
                    .iter()
                    .filter(|(_, percent)| *percent > 0)
                    .map(|(value, percent)| format!("{value} {percent}%"))
                    .collect()
            })
            .unwrap_or_default();
        let changed_ms = row.map(|row| row.set_at / 1000);
        let uniform = given.and_then(|given| given.uniform(&default).map(str::to_string));
        let is_ready =
            matches!((&uniform, changed_ms), (Some(_), Some(at)) if now - at >= SETTLED_MS);
        let state = match (row, &uniform) {
            (None, _) => Cell::state(Symbol::Pending, "default"),
            (Some(_), _) if is_ready => Cell::state(Symbol::Success, "ready to remove"),
            (Some(_), Some(value)) => Cell::state(Symbol::Running, format!("{value} for everyone")),
            (Some(_), None) => Cell::state(Symbol::Running, "rolling out"),
        };
        if is_ready && let Some(value) = &uniform {
            ready.push((flag.name.clone(), value.clone()));
        }
        table = table.row(vec![
            Cell::text(flag.name.as_str()),
            Cell::text(everyone.as_str()),
            if users == 0 {
                Cell::muted("–")
            } else {
                Cell::text(users.to_string())
            },
            if shares.is_empty() {
                Cell::muted("–")
            } else {
                Cell::text(shares.join(", "))
            },
            match changed_ms {
                Some(at) => Cell::muted(day(at)),
                None => Cell::muted("–"),
            },
            state,
        ]);
        data.push(Json::object([
            ("flag", Json::from(flag.name.as_str())),
            ("default", Json::from(default.as_str())),
            ("everyone", Json::from(everyone.as_str())),
            ("users", Json::from(users)),
            (
                "shares",
                Json::array(given.map_or_else(Vec::new, |given| {
                    given
                        .shares
                        .iter()
                        .map(|(value, percent)| {
                            Json::object([
                                ("value", Json::from(value.as_str())),
                                ("percent", Json::from(u32::from(*percent))),
                            ])
                        })
                        .collect()
                })),
            ),
            (
                "setAt",
                changed_ms.map_or(Json::Null, |at| Json::from(instant(at))),
            ),
            ("readyToRemove", Json::from(is_ready)),
            ("owner", Json::from(flag.owner.as_str())),
            ("expires", Json::from(flag.expires.as_str())),
        ]));
    }
    let mut screen = Screen::new().with(Block::Table(table));
    let mut facts = Vec::new();
    if !ready.is_empty() {
        facts.push(Fact::plain(format!("{} ready to remove", ready.len())));
    }
    facts.push(Fact::plain(format!(
        "{store}  {}",
        crate::seed::shown_address(&url.url)
    )));
    screen.push(Block::summary(
        Outcome::Success,
        crate::schedules::plural(project.flags.len() as u64, "flag", "flags"),
        facts,
    ));
    if let Some((name, value)) = ready.first() {
        screen.push(Block::hint(
            format!("jadpo flags remove {name} --keep {value}"),
            "rewrites the source to keep it",
        ));
    }
    Reply::screen(screen, Json::object([("flags", Json::array(data))]))
}

/// `2026-10-31`, the UTC day of `at`.
fn day(at: i64) -> String {
    instant(at).chars().take(10).collect()
}

/// `2026-10-31T09:12:00Z`.
fn instant(at: i64) -> String {
    jadpo_std::format_instant(at)
}

// --- set and reset ----------------------------------------------------------

fn set(call: &Call, project: &Project, flag: &Flag) -> Reply {
    if let Some(extra) = call.arguments.get(3) {
        return Reply::failed(arguments(
            format!("`jadpo flags set` takes one value, not `{extra}` too"),
            "It gives one value, to everyone or to the users or share its options name."
                .to_string(),
            format!("Run `jadpo flags set {} <value>`", flag.name),
        ));
    }
    let users = match users(call) {
        Ok(users) => users,
        Err(reply) => return reply,
    };
    let percent_given = call.value("--percent");
    let split = call.value("--split");
    let written = call.arguments.get(2);
    if split.is_some() && (percent_given.is_some() || written.is_some()) {
        return Reply::failed(arguments(
            if written.is_some() {
                format!("`--split` gives its own values, so `{}` can't be given too", written.map_or("", String::as_str))
            } else {
                "`--split` and `--percent` can't both be given".to_string()
            },
            "`--split` replaces the shares with one for each value it names, `old=50,new=50`; `--percent` replaces them with one share of one value.".to_string(),
            format!("Run `jadpo flags set {} --split <value>=<percent>,…` alone", flag.name),
        ));
    }
    if split.is_some() && !users.is_empty() {
        return Reply::failed(arguments(
            "`--split` and `--users` can't both be given".to_string(),
            "`--split` gives several values, so it can't say which one the named users get."
                .to_string(),
            format!(
                "Name the users in a `jadpo flags set {} <value> --users …` of their own",
                flag.name
            ),
        ));
    }
    let targeted = !users.is_empty() || percent_given.is_some();
    let value = match written {
        Some(written) => match value_of(flag, written) {
            Some(value) => Some(value),
            None => return not_a_value(flag, written),
        },
        None if targeted && flag.enum_type.is_none() => Some("true".to_string()),
        None if split.is_some() => None,
        None => {
            return Reply::failed(arguments(
                format!("`jadpo flags set {}` needs a value", flag.name),
                format!(
                    "It gives a value, {}, to everyone, or to the users or share its options name.",
                    crate::run::either(&quoted(flag.values.iter()))
                ),
                format!(
                    "Run `jadpo flags set {} {}`",
                    flag.name,
                    flag.values.last().map_or("<value>", String::as_str)
                ),
            ));
        }
    };
    let shares = match split {
        Some(split) => {
            let mut shares = Vec::new();
            for part in split
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
            {
                let Some((name, share)) = part.split_once('=') else {
                    return Reply::failed(arguments(
                        format!("`{part}` isn't a value and its percentage"),
                        "`--split` names each value with its percentage of users, `old=50,new=50`."
                            .to_string(),
                        "Write each share as `<value>=<percent>`".to_string(),
                    ));
                };
                let Some(value) = value_of(flag, name.trim()) else {
                    return not_a_value(flag, name.trim());
                };
                let share = match percent(share, "--split") {
                    Ok(share) => share,
                    Err(reply) => return reply,
                };
                shares.push((value, share));
            }
            let total: u32 = shares.iter().map(|(_, share)| u32::from(*share)).sum();
            if total > 100 {
                return Reply::failed(arguments(
                    format!("`--split` gives {total}% of users"),
                    "Each user is in one share at most, so the shares add up to 100% or less; the users in none get everyone's value.".to_string(),
                    "Make the percentages add up to 100 or less".to_string(),
                ));
            }
            Some(shares)
        }
        None => match percent_given {
            Some(written) => match percent(written, "--percent") {
                Ok(0) => Some(Vec::new()),
                Ok(share) => Some(vec![(value.clone().unwrap_or_default(), share)]),
                Err(reply) => return reply,
            },
            None => None,
        },
    };
    let (store, url) = match project.address(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    change(call, flag, (&store, &url), "set", |was| {
        let mut now = was.cloned().unwrap_or_default();
        if !users.is_empty() {
            let value = value.clone().unwrap_or_default();
            for id in &users {
                match now.users.iter_mut().find(|(seen, _)| seen == id) {
                    Some((_, held)) => *held = value.clone(),
                    None => now.users.push((id.clone(), value.clone())),
                }
            }
            now.users.sort();
        }
        if let Some(shares) = &shares {
            now.shares = shares.clone();
        }
        if !targeted && shares.is_none() {
            now.value = value.clone();
        }
        Some(now)
    })
}

fn reset(call: &Call, project: &Project, flag: &Flag) -> Reply {
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo flags reset` takes no value, not `{extra}`"),
            "It gives everyone the flag's `default:` again, or with `--users` stops naming those users.".to_string(),
            format!("Run `jadpo flags reset {}`", flag.name),
        ));
    }
    if call.value("--percent").is_some() || call.value("--split").is_some() {
        return Reply::failed(arguments(
            "`jadpo flags reset` takes no `--percent` or `--split`".to_string(),
            "Shares are removed with `jadpo flags set <flag> --percent 0`, or all at once with `reset`.".to_string(),
            format!("Run `jadpo flags set {} --percent 0`", flag.name),
        ));
    }
    let users = match users(call) {
        Ok(users) => users,
        Err(reply) => return reply,
    };
    let (store, url) = match project.address(call) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    change(call, flag, (&store, &url), "reset", |was| {
        if users.is_empty() {
            return None;
        }
        let mut now = was.cloned().unwrap_or_default();
        now.users.retain(|(id, _)| !users.contains(id));
        if now == Given::default() {
            None
        } else {
            Some(now)
        }
    })
}

/// Reads `flag`'s row, works out its new state with `change` (`None` for
/// no row) and makes the change, logged, unless it changes nothing.
fn change(
    call: &Call,
    flag: &Flag,
    (store, url): (&str, &Address),
    action: &'static str,
    next: impl FnOnce(Option<&Given>) -> Option<Given>,
) -> Reply {
    let stored = match read(store, url) {
        Ok(stored) => stored,
        Err(reply) => return reply,
    };
    let was = stored.into_iter().find(|row| row.flag == flag.name);
    let now = next(was.as_ref().map(|row| &row.kept)).filter(|now| *now != Given::default());
    let before = described(flag, was.as_ref().map(|row| &row.kept));
    let after = described(flag, now.as_ref());
    let unchanged = was.as_ref().map(|row| &row.kept) == now.as_ref();
    if unchanged {
        let screen = Screen::new().with(Block::Status {
            outcome: Outcome::Success,
            text: format!("`{}` already gives {after}", flag.name),
            rows: Vec::new(),
        });
        return Reply::screen(
            screen,
            Json::object([
                ("flag", Json::from(flag.name.as_str())),
                ("changed", Json::from(false)),
            ]),
        );
    }
    let operator = operator(call);
    let at = crate::schedules::now(call);
    let name = flag.name.clone();
    let done = with_database(store, url, |database| {
        let was = was.clone();
        let now = now.clone();
        let operator = operator.clone();
        Box::pin(async move {
            let host = jadpo_host_native::NativeHost::new();
            jadpo_runtime::flags::change(
                database,
                &host,
                &name,
                was.as_ref(),
                now.as_ref(),
                action,
                &operator,
                at,
            )
            .await
        })
    });
    let role = match done {
        Ok(Changed::Logged { role }) => role,
        Ok(Changed::Moved) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::FLAGS_UNAVAILABLE,
                    format!("`{}` changed while this command ran", flag.name),
                )
                .reason("Another change reached the database between reading the flag and changing it, so this one was left undone rather than overwrite it.")
                .next("Run `jadpo flags list` to see what it is now, then run the command again"),
            );
        }
        Err(reply) => return reply,
    };
    let text = match action {
        "reset" if now.is_none() => format!("Reset `{}`: everyone gets its default", flag.name),
        "reset" => format!("Stopped naming those users for `{}`", flag.name),
        _ => format!("Set `{}`", flag.name),
    };
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text,
        rows: vec![
            ("was".to_string(), before.clone()),
            ("now".to_string(), after.clone()),
            ("logged".to_string(), format!("{operator} · role {role}")),
            (
                "applies".to_string(),
                "within 10 s, on every server".to_string(),
            ),
        ],
    });
    Reply::screen(
        screen,
        Json::object([
            ("flag", Json::from(flag.name.as_str())),
            ("changed", Json::from(true)),
            ("action", Json::from(action)),
            ("was", Json::from(before)),
            ("now", Json::from(after)),
            ("operator", Json::from(operator)),
            ("role", Json::from(role)),
        ]),
    )
}

// --- remove -----------------------------------------------------------------

fn remove(call: &Call, mut project: Project, flag: &Flag) -> Reply {
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo flags remove` takes one flag, not `{extra}` too"),
            "It removes one flag, keeping the value `--keep` names.".to_string(),
            format!("Run `jadpo flags remove {} --keep <value>`", flag.name),
        ));
    }
    let Some(written) = call.value("--keep") else {
        return Reply::failed(arguments(
            format!("`jadpo flags remove {}` needs `--keep`", flag.name),
            format!(
                "Removing a flag keeps one of its values, {}, everywhere it is read; only you know which one every user should keep.",
                crate::run::either(&quoted(flag.values.iter()))
            ),
            format!(
                "Run `jadpo flags remove {} --keep <value>`, with the value every user has now",
                flag.name
            ),
        ));
    };
    let Some(value) = value_of(flag, written) else {
        return not_a_value(flag, written);
    };
    let keep = flag.values.iter().position(|v| *v == value).unwrap_or(0) as u32;
    let problems = jadpo_check::check(&project.loaded.db, project.loaded.project);
    if !problems.is_empty() {
        return crate::migrate::blocked(
            &project.loaded,
            &project.cwd,
            problems,
            "flags remove stopped",
        );
    }
    let shown_root = jadpo_db::shown_root(&project.cwd, &project.loaded.root);
    let removed = match jadpo_check::flag_remove::apply(&mut project.loaded, flag, keep) {
        Err(jadpo_check::flag_remove::Refused::RouteReads { route, path }) => {
            return Reply::failed(arguments(
                format!("`{}` is passed on by the route `{route}`", flag.name),
                format!(
                    "`{path}` passes `flags.{}` to what the route runs, and a route's `run:` passes no value written out, so there is nothing to put in its place.",
                    flag.name
                ),
                format!(
                    "Make the action the route runs read `flags.{}` itself, then run `jadpo flags remove {} --keep {value}` again",
                    flag.name, flag.name
                ),
            ));
        }
        Ok(Err(stopped)) => {
            let mut screen = Screen::new();
            for problem in &stopped.problems {
                screen.push(Block::diagnostic_from(
                    problem,
                    &stopped.sources,
                    shown_root.as_deref(),
                ));
            }
            let errors = stopped.problems.len();
            screen.push(Block::summary(
                Outcome::Failure,
                "flags remove stopped",
                [
                    Fact::problem(format!(
                        "{errors} {} after the removal",
                        if errors == 1 { "error" } else { "errors" }
                    )),
                    Fact::plain("nothing changed"),
                ],
            ));
            return Reply::Report {
                screen,
                data: Json::object([("removed", false.into())]),
                diagnostics: stopped.problems,
                sources: stopped.sources,
            };
        }
        Ok(Ok(removed)) => removed,
    };
    if let Err(problem) =
        crate::config::write_files(&project.root, &removed.files, "jadpo flags remove")
    {
        return Reply::failed(*problem);
    }
    let groups = crate::config::change_groups(&removed.files);
    let mut rows = vec![("kept".to_string(), flag.written(keep))];
    if !removed.tests.is_empty() {
        rows.push((
            "tests".to_string(),
            format!(
                "{} removed, each tested a branch that went",
                removed.tests.len()
            ),
        ));
    }
    let screen = Screen::new()
        .with(Block::Changes { note: None, groups })
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Removed `{}`", flag.name),
            rows,
        })
        .with(Block::hint("jadpo test", "runs the tests that are left"));
    Reply::screen(
        screen,
        Json::object([
            ("removed", true.into()),
            ("flag", Json::from(flag.name.as_str())),
            ("kept", Json::from(value.as_str())),
            (
                "files",
                Json::array(crate::config::files_json(&removed.files)),
            ),
            (
                "testsRemoved",
                Json::array(removed.tests.iter().map(|(path, sentence)| {
                    Json::object([
                        ("path", Json::from(path.as_str())),
                        ("test", Json::from(sentence.as_str())),
                    ])
                })),
            ),
        ]),
    )
}
