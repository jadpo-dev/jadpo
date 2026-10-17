//! `jadpo translations missing`, `unused`, `update`, `accept`, `builtin`,
//! `export` and `import` (area 10, "Tooling"; chunks 5.4d, 5.4e and 5.4f):
//! what is left to translate, which messages nothing calls, the record of
//! which default wording each translation was made from and who accepted
//! it, Jadpo's own messages, and the exchange with translation services
//! (`export` and `import`, designed in [`crate::exchange::command`]). Every
//! command that prints takes `--json`.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, run unattended, so its assumptions are marked):
//!
//! - *Job.* An agent that has changed wording, or a person asked to review
//!   a locale, needs to know what is outstanding and in which file, to do
//!   it; an agent that has written translations records them; a person
//!   who has read a locale accepts it. Mode: operate. Scanning beats
//!   reading, and the file is what is acted on.
//! - `missing` groups by language file, the file's path as the bold
//!   heading (never cut, so it copies whole) and a line under it for each
//!   member: its name, its state (`missing`, `stale`, `unaccepted`) and
//!   one note. A locale in progress says what the missing wording falls
//!   back to (`falls back to en_gb`); a complete one says the check fails
//!   until it is worded. A file that doesn't exist yet is the heading with
//!   `no file yet` after it. The footer counts what is outstanding and
//!   breaks it down; counts that make `jadpo check` fail take the problem
//!   colour. The one hint, only when something is unaccepted, is the
//!   `accept` command for its first locale. *Assumption:* the command
//!   succeeds whatever it lists, as `jadpo routes` and `declines list`
//!   do; it answers a question, and the check is what fails.
//! - `unused` is one list, as `declines list` is: the bold heading
//!   `Unused`, a row for each message or enum (its type's name, then its
//!   place, `path:line`, never cut), and a footer that counts what is
//!   unused and how many of all are used. No hint.
//! - `update` and `accept` are a status line for each record written,
//!   `✓ Wrote domains/shipping/translations/sources.jadpo_record`, and a
//!   row under it for each translation that changed (`de_de.parcels` and
//!   what happened). *Assumption:* a translation left stale because its
//!   own wording was not touched is shown (`still stale`) rather than
//!   hidden, since the check keeps failing on it; the hint is then
//!   `jadpo translations missing`, otherwise `git diff` of the folder.
//! - `accept` needs the locale. *Assumption:* it accepts what `update`
//!   would record for the locale first, so a person need not run both,
//!   and it never accepts a stale translation.
//! - `builtin` (5.4e; impeccable's shape pass again, unattended). *Job:*
//!   an agent or a person who must word Jadpo's messages for a locale
//!   Jadpo does not ship, or reword what it ships, needs the names, the
//!   fields each may write and the English to translate from, for the
//!   messages of the features this app has on. Mode: operate. One bold
//!   heading for each feature (`Requests`, `Signing in`), a line for each
//!   message (its name, `{fields}` when it has any, and `needs fr_fr` when
//!   a supported locale that is not in progress still lacks it), and the
//!   shipped wording under it, wrapped, never cut, since it is what is
//!   translated. The footer counts the messages and, in the problem
//!   colour, how many need wording; the one hint is `jadpo check`, which
//!   fails until they are worded, and only appears then. Nothing is a
//!   table: a wording is prose and would be cut. *Assumption:* the
//!   command succeeds whatever it lists, as `missing` does. `--json`
//!   carries the description translators read, the fields with their
//!   kinds, the wording, where Jadpo ships it and the locales that still
//!   need it.
//! - *States.* Outstanding work; nothing outstanding (`✓ Nothing
//!   outstanding`); no translations at all; a locale that is not
//!   supported, is the default or is missing (`cli.translations_arguments`,
//!   naming the locales that can be accepted); a project whose files
//!   don't parse (`update` and `accept` stop on the syntax errors, as every
//!   command that writes does).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_check::translations::outstanding::{self, Gap};
use jadpo_check::translations::sources::{self, Change};
use jadpo_check::translations::{Kind, scopes};
use jadpo_db::{Db, Loaded, Project};
use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_not_found;
use crate::migrate::blocked;
use crate::run::{Call, Reply, either, quoted};
use crate::suggest::nearest;
use crate::table::Value;

/// What `jadpo translations` does.
pub const ACTIONS: &[Value] = &[
    Value {
        name: "missing",
        description: "List wording a locale lacks, is stale or waits for acceptance",
    },
    Value {
        name: "unused",
        description: "List messages no code calls",
    },
    Value {
        name: "update",
        description: "Record which wording each translation was made from",
    },
    Value {
        name: "accept",
        description: "Accept a locale's changed wording",
    },
    Value {
        name: "builtin",
        description: "List Jadpo's own messages the app can show",
    },
    Value {
        name: "export",
        description: "Write the wording for a translation service as XLIFF or JSON",
    },
    Value {
        name: "import",
        description: "Read a translation service's file into the language files",
    },
];

pub fn translations(call: &Call) -> Reply {
    let action = match call.arguments.first().map(String::as_str) {
        Some(
            action @ ("missing" | "unused" | "update" | "accept" | "builtin" | "export" | "import"),
        ) => action,
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let diagnostic = Diagnostic::new(
                &rule::CLI_TRANSLATIONS_ARGUMENTS,
                format!("`{other}` isn't something `jadpo translations` does"),
            );
            return Reply::failed(match nearest(other, names.iter().copied()) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo translations` can {}. Did you mean `{near}`?",
                        either(&quoted(&names))
                    ))
                    .next(format!("Use `jadpo translations {near}`")),
                None => diagnostic
                    .reason(format!(
                        "`jadpo translations` can {}.",
                        either(&quoted(&names))
                    ))
                    .next("Run `jadpo translations missing` to see what is left to translate"),
            });
        }
        None => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_TRANSLATIONS_ARGUMENTS,
                    "`jadpo translations` needs to know what to do",
                )
                .reason("`missing` lists what is left to translate, `unused` the messages no code calls, `update` records the wording each translation was made from, `accept` accepts a locale's changed wording, `builtin` lists Jadpo's own messages, and `export` and `import` exchange the wording with a translation service.")
                .next("Run `jadpo translations missing`"),
            );
        }
    };
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("translations");
    };
    let mut loaded = jadpo_db::load(&root);
    match action {
        "export" => return crate::exchange::command::export(&loaded, call, &cwd),
        "import" => return crate::exchange::command::import(&mut loaded, call, &cwd),
        _ => {}
    }
    let extra = call.arguments.get(1).map(String::as_str);
    if let Some(extra) = extra
        && matches!(action, "unused" | "update" | "builtin")
    {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_TRANSLATIONS_ARGUMENTS,
                format!("`jadpo translations {action}` takes nothing after it, not `{extra}`"),
            )
            .reason(match action {
                "unused" => "It looks at every message in the project.",
                "builtin" => "It lists every message of Jadpo's the application can show.",
                _ => "It records every locale at once.",
            })
            .next(format!("Run `jadpo translations {action}`")),
        );
    }
    if let Some(third) = call.arguments.get(2) {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_TRANSLATIONS_ARGUMENTS,
                format!("`{third}` is one argument too many"),
            )
            .reason(format!(
                "`jadpo translations {action}` takes at most a locale after it."
            ))
            .next(format!(
                "Run `jadpo translations {action} {}`",
                extra.unwrap_or("")
            )),
        );
    }
    match action {
        "missing" => missing(&loaded, extra),
        "unused" => unused(&loaded, &cwd),
        "builtin" => builtin(&loaded),
        "update" => write(&loaded, &cwd, None),
        _ => accept(&loaded, &cwd, extra),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn tag_of(locale: u32) -> &'static str {
    jadpo_std::LOCALES
        .get(locale as usize)
        .map_or("", |(variant, _)| *variant)
}

/// The supported locales that are not the default: the ones with
/// translations.
fn translated_locales(db: &dyn Db, project: Project) -> Vec<&'static str> {
    let Some(plan) = &jadpo_check::locales::locales(db, project).plan else {
        return Vec::new();
    };
    plan.supported
        .iter()
        .copied()
        .filter(|&locale| locale != plan.default && !jadpo_std::LocalePlan::is_pseudo(locale))
        .map(tag_of)
        .collect()
}

/// A locale given on the command line, or the error saying which can be.
fn locale_argument(
    loaded: &Loaded,
    action: &str,
    given: Option<&str>,
) -> Result<Option<String>, Box<Reply>> {
    let candidates = translated_locales(&loaded.db, loaded.project);
    let Some(given) = given else {
        if action == "accept" {
            return Err(Box::new(Reply::failed(
                Diagnostic::new(
                    &rule::CLI_TRANSLATIONS_ARGUMENTS,
                    "`jadpo translations accept` needs a locale",
                )
                .reason(if candidates.is_empty() {
                    "A person accepts one locale's wording at a time, and this project has no locale besides its default.".to_string()
                } else {
                    format!(
                        "A person accepts one locale's wording at a time: {}.",
                        either(&quoted(&candidates))
                    )
                })
                .next(match candidates.first() {
                    Some(first) => format!("Run `jadpo translations accept {first}`"),
                    None => "Add a locale to `supported:` in `locales { }`".to_string(),
                }),
            )));
        }
        return Ok(None);
    };
    if candidates.contains(&given) {
        return Ok(Some(given.to_string()));
    }
    let diagnostic = Diagnostic::new(
        &rule::CLI_TRANSLATIONS_ARGUMENTS,
        format!("`{given}` isn't a locale with translations"),
    );
    let reason = if candidates.is_empty() {
        "This project supports no locale besides its default, which is what the others are translated from.".to_string()
    } else {
        format!(
            "Translations are for the supported locales besides the default: {}.",
            either(&quoted(&candidates))
        )
    };
    Err(Box::new(Reply::failed(
        match nearest(given, candidates.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!("{reason} Did you mean `{near}`?"))
                .next(format!("Use `jadpo translations {action} {near}`")),
            None => diagnostic.reason(reason).next(match candidates.first() {
                Some(first) => format!("Run `jadpo translations {action} {first}`"),
                None => "Add a locale to `supported:` in `locales { }`".to_string(),
            }),
        },
    )))
}

// --- missing -------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Missing,
    Stale,
    Unaccepted,
}

impl State {
    fn word(self) -> &'static str {
        match self {
            State::Missing => "missing",
            State::Stale => "stale",
            State::Unaccepted => "unaccepted",
        }
    }
}

/// One line of the list.
struct Item {
    path: String,
    file_exists: bool,
    locale: String,
    member: String,
    state: State,
    note: String,
    /// Whether `jadpo check` fails on it.
    error: bool,
}

fn missing(loaded: &Loaded, locale: Option<&str>) -> Reply {
    let filter = match locale_argument(loaded, "missing", locale) {
        Ok(filter) => filter,
        Err(reply) => return *reply,
    };
    let (db, project) = (&loaded.db, loaded.project);
    let mut items = items(db, project);
    if let Some(filter) = &filter {
        items.retain(|item| &item.locale == filter);
    }
    items.sort_by(|a, b| (&a.path, a.state, &a.member).cmp(&(&b.path, b.state, &b.member)));
    let mut by_file: BTreeMap<&str, Vec<&Item>> = BTreeMap::new();
    for item in &items {
        by_file.entry(item.path.as_str()).or_default().push(item);
    }
    let mut screen = Screen::new();
    for (path, rows) in &by_file {
        let width = rows.iter().map(|row| row.member.len()).max().unwrap_or(0);
        let lines = rows
            .iter()
            .map(|row| {
                format!(
                    "{:width$}  {:10}  {}",
                    row.member,
                    row.state.word(),
                    row.note
                )
                .trim_end()
                .to_string()
            })
            .collect();
        let heading = if rows.first().is_some_and(|row| !row.file_exists) {
            format!("{path}  no file yet")
        } else {
            (*path).to_string()
        };
        screen.push(Block::Section { heading, lines });
    }
    let of = |state: State| items.iter().filter(|item| item.state == state).count();
    let errors = |state: State| {
        items
            .iter()
            .filter(|item| item.state == state && item.error)
            .count()
    };
    let locales: std::collections::BTreeSet<&str> =
        items.iter().map(|item| item.locale.as_str()).collect();
    let fact = |state: State, word: &str| {
        let n = of(state);
        (n > 0).then(|| {
            if errors(state) > 0 {
                Fact::problem(format!("{n} {word}"))
            } else {
                Fact::plain(format!("{n} {word}"))
            }
        })
    };
    if items.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "Nothing outstanding",
            filter.iter().map(|locale| Fact::plain(locale.clone())),
        ));
    } else {
        let mut facts: Vec<Fact> = [
            fact(State::Missing, "missing"),
            fact(State::Stale, "stale"),
            fact(State::Unaccepted, "unaccepted"),
        ]
        .into_iter()
        .flatten()
        .collect();
        facts.push(Fact::plain(count(locales.len(), "locale", "locales")));
        screen.push(Block::summary(
            Outcome::Success,
            format!("{} outstanding", items.len()),
            facts,
        ));
        if let Some(first) = items.iter().find(|item| item.state == State::Unaccepted) {
            screen.push(Block::hint(
                format!("jadpo translations accept {}", first.locale),
                "records a person's acceptance of the locale's wording",
            ));
        }
    }
    let data = Json::object([
        (
            "outstanding",
            Json::array(items.iter().map(|item| {
                Json::object([
                    ("locale", Json::from(item.locale.as_str())),
                    ("member", Json::from(item.member.as_str())),
                    ("state", Json::from(item.state.word())),
                    ("path", Json::from(item.path.as_str())),
                    ("fileExists", Json::from(item.file_exists)),
                    ("note", Json::from(item.note.as_str())),
                    ("failsCheck", Json::from(item.error)),
                ])
            })),
        ),
        ("missing", of(State::Missing).into()),
        ("stale", of(State::Stale).into()),
        ("unaccepted", of(State::Unaccepted).into()),
    ]);
    Reply::screen(screen, data)
}

/// Every outstanding item of the project.
fn items(db: &dyn Db, project: Project) -> Vec<Item> {
    let Some(plan) = &jadpo_check::locales::locales(db, project).plan else {
        return Vec::new();
    };
    let review = sources::review(db, project).required;
    let mut out = Vec::new();
    for gap in outstanding::gaps(db, project) {
        out.push(gap_item(&gap, plan));
    }
    let mut all: std::collections::BTreeSet<String> = scopes(db, project).into_iter().collect();
    all.extend(jadpo_check::translations::failures::failure_scopes(
        db, project,
    ));
    all.insert(sources::ROOT.to_string());
    for scope in all {
        let standings = sources::standings(
            db,
            project,
            jadpo_check::lookups::Name::new(db, scope.clone()),
        );
        for standing in &standings.list {
            if standing.stale {
                out.push(Item {
                    path: standing.path.clone(),
                    file_exists: true,
                    locale: standing.tag.clone(),
                    member: standing.member.clone(),
                    state: State::Stale,
                    note: format!("{} reworded since it was translated", standing.default_tag),
                    error: !standing.in_progress,
                });
            } else if review && !standing.accepted() {
                out.push(Item {
                    path: standing.path.clone(),
                    file_exists: true,
                    locale: standing.tag.clone(),
                    member: standing.member.clone(),
                    state: State::Unaccepted,
                    note: match &standing.recorded {
                        None => "new".to_string(),
                        Some(entry) if entry.wording != standing.wording_hash && entry.accepted => {
                            "reworded since it was accepted".to_string()
                        }
                        Some(entry) if entry.wording != standing.wording_hash => {
                            "reworded since it was recorded".to_string()
                        }
                        Some(_) => "waiting for acceptance".to_string(),
                    },
                    error: false,
                });
            }
        }
    }
    out
}

fn gap_item(gap: &Gap, plan: &jadpo_std::LocalePlan) -> Item {
    let note = if gap.in_progress {
        let chain = jadpo_value::locale::fallback_chain(gap.locale, plan);
        chain
            .into_iter()
            .skip(1)
            .find(|at| gap.worded_in.contains(at))
            .map_or_else(
                || "nothing to fall back on".to_string(),
                |at| format!("falls back to {}", tag_of(at)),
            )
    } else {
        "needed, or jadpo check fails".to_string()
    };
    Item {
        path: gap.path.clone(),
        file_exists: gap.file_exists,
        locale: gap.tag.clone(),
        member: gap.member.clone(),
        state: State::Missing,
        note,
        error: !gap.in_progress,
    }
}

// --- unused --------------------------------------------------------------

fn unused(loaded: &Loaded, cwd: &Path) -> Reply {
    let (db, project) = (&loaded.db, loaded.project);
    let (found, total) = outstanding::unused(db, project);
    let _ = cwd;
    let texts = jadpo_db::source_texts(db, project);
    let line_of = |path: &str, offset: u32| -> usize {
        texts.get(path).map_or(1, |text| {
            1 + text
                .bytes()
                .take(offset as usize)
                .filter(|&b| b == b'\n')
                .count()
        })
    };
    let places: Vec<String> = found
        .iter()
        .map(|item| format!("{}:{}", item.path, line_of(&item.path, item.span.start)))
        .collect();
    let width = found
        .iter()
        .map(|item| item.type_name.len())
        .max()
        .unwrap_or(0);
    let mut screen = Screen::new();
    if found.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            if total == 0 {
                "No messages"
            } else {
                "Nothing unused"
            },
            if total == 0 {
                Vec::new()
            } else {
                vec![Fact::plain(format!("{total} of {total} used"))]
            },
        ));
    } else {
        let lines = found
            .iter()
            .zip(&places)
            .map(|(item, place)| format!("{:width$}  {place}", item.type_name))
            .collect();
        screen.push(Block::Section {
            heading: "Unused".to_string(),
            lines,
        });
        screen.push(Block::summary(
            Outcome::Success,
            format!("{} unused", found.len()),
            [Fact::plain(format!(
                "{} of {total} used",
                total - found.len()
            ))],
        ));
    }
    let data = Json::object([
        (
            "unused",
            Json::array(found.iter().zip(&places).map(|(item, place)| {
                Json::object([
                    ("name", Json::from(item.type_name.as_str())),
                    ("member", Json::from(item.key.as_str())),
                    ("scope", Json::from(item.scope.as_str())),
                    (
                        "kind",
                        Json::from(match item.kind {
                            Kind::Enum => "enum",
                            _ => "message",
                        }),
                    ),
                    ("path", Json::from(item.path.as_str())),
                    ("place", Json::from(place.as_str())),
                ])
            })),
        ),
        ("total", total.into()),
    ]);
    Reply::screen(screen, data)
}

// --- update and accept ---------------------------------------------------

fn accept(loaded: &Loaded, cwd: &Path, locale: Option<&str>) -> Reply {
    match locale_argument(loaded, "accept", locale) {
        Ok(locale) => write(loaded, cwd, locale),
        Err(reply) => *reply,
    }
}

/// Brings every `sources.jadpo_record` up to date, accepting `accept`'s
/// wording when given.
fn write(loaded: &Loaded, cwd: &Path, accept: Option<String>) -> Reply {
    let (db, project) = (&loaded.db, loaded.project);
    let syntax: Vec<Diagnostic> = jadpo_check::check(db, project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect();
    if !syntax.is_empty() {
        let headline = if accept.is_some() {
            "translations accept stopped"
        } else {
            "translations update stopped"
        };
        return blocked(loaded, cwd, syntax, headline);
    }
    let mut all: std::collections::BTreeSet<String> = scopes(db, project).into_iter().collect();
    all.extend(jadpo_check::translations::failures::failure_scopes(
        db, project,
    ));
    all.insert(sources::ROOT.to_string());
    let mut screen = Screen::new();
    let mut data = Vec::new();
    let mut accepted = 0usize;
    let mut recorded = 0usize;
    let mut stale = 0usize;
    let mut touched = false;
    for scope in all {
        let standings = sources::standings(
            db,
            project,
            jadpo_check::lookups::Name::new(db, scope.clone()),
        );
        let updated = sources::updated(standings, accept.as_deref());
        let path = sources::record_path(&scope);
        let before = project
            .records(db)
            .iter()
            .find(|record| record.path == path)
            .and_then(|record| record.text.clone().ok());
        let after = (!updated.entries.is_empty()).then(|| sources::render(&updated.entries));
        let mut rows = Vec::new();
        for (locale, member, change) in &updated.changes {
            let what = match change {
                Change::Recorded => "recorded",
                Change::Reworded => "reworded, acceptance dropped",
                Change::Removed => "removed, its translation is gone",
                Change::Accepted => "accepted",
                Change::StillStale => "still stale, reword it first",
            };
            match change {
                Change::Recorded | Change::Reworded => recorded += 1,
                Change::Accepted => accepted += 1,
                Change::StillStale => stale += 1,
                Change::Removed => {}
            }
            rows.push((format!("{locale}.{member}"), what.to_string()));
        }
        let changed = before != after;
        touched |= changed || !rows.is_empty();
        if changed {
            let target = loaded.root.join(&path);
            let result = match &after {
                Some(text) => target
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&target, text)),
                None => std::fs::remove_file(&target),
            };
            if let Err(error) = result {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                        format!("`{path}` couldn't be written"),
                    )
                    .reason(format!("Writing it failed: {error}."))
                    .next_for(
                        "Check the folder can be written, then run the command again",
                        jadpo_diag::Decider::Human,
                    ),
                );
            }
            screen.push(Block::Status {
                outcome: Outcome::Success,
                text: match &after {
                    Some(_) => format!("Wrote {path}"),
                    None => format!("Removed {path}"),
                },
                rows: rows.clone(),
            });
        } else if !rows.is_empty() {
            // Nothing to write, but translations still waiting on a reword.
            screen.push(Block::Status {
                outcome: Outcome::Success,
                text: format!("{path} is up to date"),
                rows: rows.clone(),
            });
        }
        data.push(Json::object([
            ("path", Json::from(path.as_str())),
            ("written", Json::from(changed)),
            (
                "changes",
                Json::array(updated.changes.iter().map(|(locale, member, change)| {
                    Json::object([
                        ("locale", Json::from(locale.as_str())),
                        ("member", Json::from(member.as_str())),
                        (
                            "change",
                            Json::from(match change {
                                Change::Recorded => "recorded",
                                Change::Reworded => "reworded",
                                Change::Removed => "removed",
                                Change::Accepted => "accepted",
                                Change::StillStale => "still_stale",
                            }),
                        ),
                    ])
                })),
            ),
        ]));
    }
    if !touched {
        let headline = match &accept {
            Some(locale) => format!("Nothing to accept in {locale}"),
            None => "Already up to date".to_string(),
        };
        screen.push(Block::summary(Outcome::Success, headline, []));
    } else if stale > 0 {
        screen.push(Block::hint(
            "jadpo translations missing",
            "lists the translations to reword",
        ));
    } else {
        screen.push(Block::hint(
            "git diff",
            if accept.is_some() {
                "shows the acceptance a reviewer approves"
            } else {
                "shows what was recorded"
            },
        ));
    }
    let _ = (accepted, recorded);
    Reply::screen(
        screen,
        Json::object([
            ("records", Json::array(data)),
            ("accepted", accepted.into()),
            ("recorded", recorded.into()),
            ("stale", stale.into()),
        ]),
    )
}

// --- builtin ---------------------------------------------------------------

/// The wording wrapped to `width` columns, never cut.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Jadpo's own messages the application can show, by feature.
fn builtin(loaded: &Loaded) -> Reply {
    use jadpo_check::translations::jadpo as own;
    use jadpo_std::jadpo_text::{Feature, Shape, Ships};
    let (db, project) = (&loaded.db, loaded.project);
    let messages = own::messages_on(db, project);
    let needs = own::needs(db, project);
    let needing = |key: &str| -> Vec<&'static str> {
        needs
            .iter()
            .filter(|(_, missing)| missing.iter().any(|message| message.key == key))
            .map(|(locale, _)| tag_of(*locale))
            .collect()
    };
    let mut screen = Screen::new();
    if messages.is_empty() {
        screen.push(Block::summary(Outcome::Success, "No messages", []));
    }
    let width = messages.iter().map(|m| m.key.len()).max().unwrap_or(0);
    let mut features = 0usize;
    for feature in Feature::ALL {
        let of: Vec<_> = messages
            .iter()
            .filter(|message| message.feature == feature)
            .collect();
        if of.is_empty() {
            continue;
        }
        features += 1;
        let mut lines = Vec::new();
        for message in of {
            let fields: Vec<String> = message
                .fields
                .iter()
                .map(|(name, _)| format!("{{{name}}}"))
                .collect();
            let wanted = needing(message.key);
            let mut parts = vec![format!("{:width$}", message.key)];
            if !fields.is_empty() {
                parts.push(fields.join(" "));
            }
            if !wanted.is_empty() {
                parts.push(format!("needs {}", wanted.join(" ")));
            }
            lines.push(parts.join("  ").trim_end().to_string());
            match message.shape {
                Shape::Plain(text) => {
                    lines.extend(wrap(text, 76).into_iter().map(|line| format!("  {line}")));
                }
                Shape::Variants(variants) => {
                    let name_width = variants
                        .iter()
                        .map(|(name, _)| name.len())
                        .max()
                        .unwrap_or(0);
                    lines.extend(
                        variants
                            .iter()
                            .map(|(name, text)| format!("  {name:name_width$}  {text}")),
                    );
                }
            }
        }
        screen.push(Block::Section {
            heading: feature.heading().to_string(),
            lines,
        });
    }
    let needing_count = messages
        .iter()
        .filter(|message| !needing(message.key).is_empty())
        .count();
    if !messages.is_empty() {
        let mut facts = Vec::new();
        if needing_count > 0 {
            facts.push(Fact::problem(format!("{needing_count} need wording")));
        }
        facts.push(Fact::plain(count(features, "feature", "features")));
        screen.push(Block::summary(
            Outcome::Success,
            count(messages.len(), "message", "messages"),
            facts,
        ));
        if needing_count > 0 {
            screen.push(Block::hint(
                "jadpo check",
                "fails until the application words them",
            ));
        }
    }
    let kind_word = |kind: jadpo_std::jadpo_text::Kind| kind.word();
    let data = Json::object([
        (
            "messages",
            Json::array(messages.iter().map(|message| {
                let wording = match message.shape {
                    Shape::Plain(text) => Json::from(text),
                    Shape::Variants(variants) => Json::object(
                        variants
                            .iter()
                            .map(|(name, text)| (*name, Json::from(*text)))
                            .collect::<Vec<_>>(),
                    ),
                };
                Json::object([
                    ("key", Json::from(message.key)),
                    ("kind", Json::from(kind_word(message.kind))),
                    ("feature", Json::from(message.feature.word())),
                    ("description", Json::from(message.description)),
                    (
                        "fields",
                        Json::array(message.fields.iter().map(|(name, kind)| {
                            Json::object([
                                ("name", Json::from(*name)),
                                ("type", Json::from(kind.word())),
                            ])
                        })),
                    ),
                    ("wording", wording),
                    (
                        "shipsIn",
                        Json::array(
                            match message.ships {
                                Ships::English => jadpo_std::jadpo_text::SHIPS_IN.to_vec(),
                                Ships::Everywhere => Vec::new(),
                            }
                            .into_iter()
                            .map(Json::from),
                        ),
                    ),
                    (
                        "shipsEverywhere",
                        Json::from(message.ships == Ships::Everywhere),
                    ),
                    (
                        "needs",
                        Json::array(needing(message.key).into_iter().map(Json::from)),
                    ),
                ])
            })),
        ),
        (
            "features",
            Json::array(
                Feature::ALL
                    .iter()
                    .filter(|feature| messages.iter().any(|m| m.feature == **feature))
                    .map(|feature| Json::from(feature.word())),
            ),
        ),
        ("needWording", needing_count.into()),
    ]);
    Reply::screen(screen, data)
}
