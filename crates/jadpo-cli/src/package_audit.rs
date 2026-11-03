//! `jadpo package audit` (area 11, "Yanks, takedowns and advisories";
//! chunk R5): lists the advisories against the app's packages from the
//! registry and says whether the app reaches each. It is the one check that
//! goes online, and CI runs it: an advisory the app reaches fails it, and one
//! it doesn't reach is only listed.
//!
//! ```text
//! error: `Postcodes.uk.parse` has an advisory: ADV-7
//!   reason: Reads the wrong outward code. It is against 2.0.0 to 2.1.2, and the app has 2.0.0 and reaches it.
//!   next: Run `jadpo package update postcodes`, or `--to` a release that fixes it (human)
//!   rule: package.advisory
//!
//! Not reached
//!   ADV-9  Postcodes.OutwardCode, 2.0.0 to 2.1.2: Is too short.
//!
//! ✗ advisories the app reaches  1 reached · 1 not reached · 1 package
//!
//! → jadpo package update postcodes  takes a release that fixes it
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use jadpo_db::{Locked, Source};
use jadpo_diag::{Decider, Diagnostic, rule};
use jadpo_registry::names::{ScopedName, Version};
use jadpo_registry::protocol::{Advisory, AdvisoryAnswer, AdvisoryQuery, Installed, Request};
use jadpo_term::{Block, Fact, Json, Outcome, Row, Screen};

use crate::install;
use crate::package::{arguments, plural};
use crate::registry::{Asked, Client};
use crate::run::{Call, Reply};

/// The lock's lines from the registry, and how many it has from elsewhere.
fn from_registry(lines: &[Locked]) -> Result<(Vec<(&Locked, ScopedName, Version)>, usize), Reply> {
    let mut found = Vec::new();
    for line in lines {
        if let Source::Registry { version, name, .. } = &line.source {
            let unread = |why: String| {
                Reply::failed(arguments(
                    format!("`{}` holds a line that can't be read", jadpo_db::LOCK),
                    why,
                    "Run `jadpo records` to see what is wrong with it, then put it right"
                        .to_string(),
                ))
            };
            let name = ScopedName::parse(name).map_err(|why| unread(format!("{why}.")))?;
            let version = Version::parse(version).map_err(|why| unread(format!("{why}.")))?;
            found.push((line, name, version));
        }
    }
    let others = lines.len() - found.len();
    Ok((found, others))
}

/// Whether the app reaches a declaration: `Postcodes.uk.parse`, or with its
/// version label, against what it reaches of the package.
fn is_reached(declaration: &str, reached: &std::collections::BTreeSet<String>) -> bool {
    let after = declaration
        .split_once('.')
        .map_or(declaration, |(_, rest)| rest);
    let after = match after.split_once('.') {
        Some((label, rest)) if jadpo_db::is_version_label(label) => rest,
        _ => after,
    };
    reached.contains(after)
}

pub(crate) fn audit(call: &Call) -> Reply {
    let root = match install::root_of(call, "package audit") {
        Ok(root) => root,
        Err(reply) => return reply,
    };
    let lines = match install::read_lock(&root) {
        Ok(lines) => lines,
        Err(reply) => return reply,
    };
    let (installed, others) = match from_registry(&lines) {
        Ok(found) => found,
        Err(reply) => return reply,
    };
    let elsewhere = (others > 0).then(|| {
        Fact::plain(format!(
            "{} from elsewhere not checked",
            plural(others, "package", "packages")
        ))
    });
    if installed.is_empty() {
        let screen = Screen::new().with(Block::summary(
            Outcome::Success,
            "no packages from the registry",
            elsewhere.into_iter().collect::<Vec<_>>(),
        ));
        return Reply::screen(
            screen,
            Json::object([
                ("advisories", Json::array(Vec::<Json>::new())),
                ("packages", Json::from(0_i64)),
                ("not_checked", Json::from(others as i64)),
            ]),
        );
    }
    let client = Client::of(&call.environment);
    let answer: AdvisoryAnswer = match client.ask(&Request::QueryAdvisories(AdvisoryQuery {
        packages: installed
            .iter()
            .map(|(_, name, version)| Installed {
                package: name.clone(),
                version: version.clone(),
            })
            .collect(),
    })) {
        Ok(answer) => answer,
        Err(asked) => return Asked::reply(asked),
    };
    report(&root, &installed, &answer.advisories, elsewhere, others)
}

/// One advisory against one declaration of an installed package.
struct Found<'a> {
    advisory: &'a Advisory,
    declaration: &'a str,
    line: &'a Locked,
    version: &'a Version,
    reached: bool,
}

fn report(
    root: &Path,
    installed: &[(&Locked, ScopedName, Version)],
    advisories: &[Advisory],
    elsewhere: Option<Fact>,
    others: usize,
) -> Reply {
    let loaded = jadpo_db::load(root);
    // An app with errors has a graph with holes in it: not reached can't be
    // said of it, so every advisory counts.
    let whole = jadpo_check::check(&loaded.db, loaded.project).is_empty();
    let mut reach: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    let mut found: Vec<Found<'_>> = Vec::new();
    for advisory in advisories {
        // The package itself, or one carrying a copy of what it is against.
        let holder = installed
            .iter()
            .find(|(_, name, _)| *name == advisory.package)
            .or_else(|| {
                installed
                    .iter()
                    .find(|(_, name, _)| advisory.copies.contains(name))
            });
        let Some((line, _, version)) = holder else {
            continue;
        };
        let reached = reach.entry(line.package.clone()).or_insert_with(|| {
            jadpo_check::release::reached(
                &loaded.db,
                loaded.project,
                &jadpo_db::package_folder(&line.package),
            )
        });
        for declaration in &advisory.declarations {
            found.push(Found {
                advisory,
                declaration,
                line,
                version,
                reached: !whole || is_reached(declaration, reached),
            });
        }
    }
    let sources = jadpo_db::source_texts(&loaded.db, loaded.project);
    let mut screen = Screen::new();
    let mut diagnostics = Vec::new();
    for entry in found.iter().filter(|entry| entry.reached) {
        let advisory = entry.advisory;
        let mut reason = format!(
            "{} It is against {} to {}, and the app has {}",
            advisory.summary, advisory.from, advisory.to, entry.version
        );
        reason.push_str(if whole {
            " and reaches it."
        } else {
            ". The app has errors, so what it reaches can't be worked out."
        });
        if !advisory.copies.is_empty() {
            let copies: Vec<String> = advisory.copies.iter().map(|c| format!("`{c}`")).collect();
            reason.push_str(&format!(
                " {} a copy.",
                match copies.as_slice() {
                    [one] => format!("{one} carries"),
                    many => format!("{} carry", crate::package::and_list(many)),
                }
            ));
        }
        let name = install::folder_name(&entry.line.package);
        let diagnostic = Diagnostic::new(
            &rule::PACKAGE_ADVISORY,
            format!("`{}` has an advisory: {}", entry.declaration, advisory.id),
        )
        .reason(reason)
        .next_for(
            format!("Run `jadpo package update {name}`, or give `--to` a release that fixes it"),
            Decider::Human,
        );
        screen.push(Block::diagnostic_from(&diagnostic, &sources, None));
        diagnostics.push(diagnostic);
    }
    let quiet: Vec<Row> = found
        .iter()
        .filter(|entry| !entry.reached)
        .map(|entry| {
            Row::new(
                entry.advisory.id.clone(),
                format!(
                    "{}, {} to {}: {}",
                    entry.declaration,
                    entry.advisory.from,
                    entry.advisory.to,
                    entry.advisory.summary
                ),
            )
        })
        .collect();
    let (reached, not_reached) = (diagnostics.len(), quiet.len());
    if not_reached > 0 {
        screen.push(Block::list("Not reached", quiet));
    }
    let mut facts = Vec::new();
    if reached > 0 {
        facts.push(Fact::problem(format!("{reached} reached")));
    }
    if not_reached > 0 {
        facts.push(Fact::plain(format!("{not_reached} not reached")));
    }
    facts.push(Fact::plain(plural(installed.len(), "package", "packages")));
    facts.extend(elsewhere);
    screen.push(Block::summary(
        if reached > 0 {
            Outcome::Failure
        } else {
            Outcome::Success
        },
        match (reached, not_reached) {
            (0, 0) => "no advisories",
            (0, _) => "no advisories the app reaches",
            _ => "advisories the app reaches",
        },
        facts,
    ));
    if let Some(first) = found.iter().find(|entry| entry.reached) {
        screen.push(Block::hint(
            format!(
                "jadpo package update {}",
                install::folder_name(&first.line.package)
            ),
            "takes a release that fixes it",
        ));
    }
    let data = Json::object([
        (
            "advisories",
            Json::array(found.iter().map(|entry| {
                Json::object([
                    ("id", Json::from(entry.advisory.id.as_str())),
                    (
                        "package",
                        Json::from(entry.advisory.package.to_string().as_str()),
                    ),
                    ("installed", Json::from(entry.line.package.as_str())),
                    ("version", Json::from(entry.version.as_str())),
                    ("declaration", Json::from(entry.declaration)),
                    ("from", Json::from(entry.advisory.from.as_str())),
                    ("to", Json::from(entry.advisory.to.as_str())),
                    ("summary", Json::from(entry.advisory.summary.as_str())),
                    ("reached", entry.reached.into()),
                    (
                        "copies",
                        Json::array(
                            entry
                                .advisory
                                .copies
                                .iter()
                                .map(|copy| Json::from(copy.to_string().as_str())),
                        ),
                    ),
                ])
            })),
        ),
        ("packages", Json::from(installed.len() as i64)),
        ("not_checked", Json::from(others as i64)),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}
