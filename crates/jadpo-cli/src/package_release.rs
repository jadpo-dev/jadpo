//! What `jadpo package check` shows of a release and `jadpo package verify`
//! (area 11, "What a release shows", "The changelog" and "The verifier";
//! chunk R3).
//!
//! `check` prints a release's facts, then what its operations send, then
//! its change record, above the bar they lead to, exactly as the registry
//! will show them:
//!
//! ```text
//! Facts
//!   kind      provider
//!   hosts     https://api.acme-pay.example
//!             https://sandbox.acme-pay.example
//!   changes   customers.create
//!   erasure   not declared
//!   storable  CustomerId
//!   failures  customers.create: CardDeclined
//!   contract  1 of 2 operations confirmed
//!
//! Changes since 1.0.0
//!   + added              type Plan
//!   + new host           provider          https://eu.acme-pay.example
//!   ~ behaviour changed  customers.create  example of customers.create
//!
//! Publishing bar
//! ✓ behaviour  passed  1 of 4 fails, marked a fix
//! ```
//!
//! A row is left out only where the package's kind can't have it, so a
//! library's block is its kind, what an app may store and what can fail;
//! `--json` always holds every fact, whole.
//!
//! `verify <folder> --previous <folder> --json` is the registry's verifier:
//! the same bar, on a fetched commit it never writes to, so the surface and
//! changelog list the commit carries have to be what its code gives
//! (`publish.stale_files`), and it adds each public declaration's
//! fingerprint and the Jadpo release it checked with.
//!
//! The previous release's examples and tests run against the new code in a
//! scratch app of their own: the new package's files with none of its own
//! tests, and the old release's tests and examples placed beside them. One
//! that no longer compiles counts as failed, so a release can't hide a
//! change by removing what an old test leans on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_check::publishing;
use jadpo_check::release::{Change, ChangeEntry, Facts, Fingerprint};
use jadpo_check::surface::{Surface, What};
use jadpo_db::PACKAGE_INDEX;
use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{self, AstNode, Decl};
use jadpo_term::{Block, ChangeMark, Json};

use crate::package::{self, Compiled, Git, Mode, Release, Scratch, arguments, plural};
use crate::run::{Call, Reply};

// --- verify ----------------------------------------------------------------

/// `jadpo package verify <folder> [--previous <folder>]`.
pub(crate) fn verify(call: &Call) -> Reply {
    if call.flag("--serve") {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                "`jadpo package verify --serve` isn't built yet",
            )
            .reason(
                "Area 11 decides it, and it comes with the verifier the registry reaches over HTTPS.",
            )
            .next("Run `jadpo package verify <folder> --previous <folder> --json` meanwhile"),
        );
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let usage = "Run `jadpo package verify <folder> [--previous <folder>]`";
    let Some(folder) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package verify` needs a package's folder".to_string(),
            "It checks one package, given by its folder, as the registry checks a release."
                .to_string(),
            usage.to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo package verify` takes one folder, not `{extra}`"),
            "The previous release's folder goes after `--previous`.".to_string(),
            usage.to_string(),
        ));
    }
    let previous = match call.value("--previous") {
        None => None,
        Some(earlier) => match read_previous(&cwd.join(earlier), earlier) {
            Ok(previous) => Some(Box::new(previous)),
            Err(reply) => return reply,
        },
    };
    package::bar(call, &cwd.join(folder), Mode::Verify { previous })
}

/// The folder given as the previous release: its name and the version its
/// committed surface says.
fn read_previous(folder: &Path, shown: &str) -> Result<Previous, Reply> {
    let refuse = |reason: String| {
        Reply::failed(arguments(
            format!("`{shown}` isn't a released package"),
            reason,
            "Give the folder of the release before this one with `--previous`".to_string(),
        ))
    };
    let compiled = match package::compile(folder, "package verify") {
        Ok(compiled) => compiled,
        Err(Some(reply)) => return Err(reply),
        Err(None) => {
            return Err(refuse(format!(
                "It has no `{PACKAGE_INDEX}` naming a package."
            )));
        }
    };
    let surface = std::fs::read_to_string(folder.join(jadpo_db::record::SURFACE_FILE))
        .map(|text| jadpo_db::record::normalise(&text).into_owned())
        .ok();
    let version = surface.as_deref().and_then(crate::install::surface_version);
    let (Some(surface), Some(version)) = (surface, version) else {
        return Err(refuse(format!(
            "Its committed {} names no version.",
            jadpo_db::record::SURFACE_FILE
        )));
    };
    Ok(Previous::new(
        Release {
            tag: String::new(),
            version,
            surface,
        },
        compiled.package.clone(),
        Some(compiled),
    ))
}

// --- the previous release --------------------------------------------------

/// The release a new one is compared with.
pub(crate) struct Previous {
    pub release: Release,
    /// Its name and version label, `Postcodes.v2`.
    pub package: String,
    /// Its code, placed in a scratch app of its own; `None` when it holds
    /// no package this Jadpo can read.
    pub compiled: Option<Compiled>,
    pub facts: Option<Facts>,
    /// Holds a release read from a git tag.
    _exported: Option<Scratch>,
}

impl Previous {
    fn new(release: Release, package: String, compiled: Option<Compiled>) -> Previous {
        // A release this Jadpo can't compile says nothing of what it sent:
        // the facts are left out, and everything now sent counts as new.
        let facts = compiled.as_ref().and_then(|compiled| {
            compiled.problems.is_empty().then(|| {
                jadpo_check::release::facts(
                    &compiled.loaded.db,
                    compiled.loaded.project,
                    &compiled.mount,
                )
            })?
        });
        Previous {
            release,
            package,
            compiled,
            facts,
            _exported: None,
        }
    }

    /// The release at a git tag, written out of the repository.
    pub(crate) fn from_tag(git: &Git, release: Release, package: &str) -> Previous {
        let exported = Scratch::new()
            .ok()
            .filter(|scratch| git.export(&release.tag, &scratch.0));
        let compiled = exported
            .as_ref()
            .and_then(|scratch| package::compile(&scratch.0, "package check").ok());
        let mut previous = Previous::new(release, package.to_string(), compiled);
        previous._exported = exported;
        previous
    }
}

/// What running the previous release's examples and tests gave.
pub(crate) struct Behaviour {
    /// How many ran, or tried to.
    pub total: usize,
    pub failed: Vec<Behaved>,
}

/// A previous release's example or test that fails against the new code.
pub(crate) struct Behaved {
    /// Its name: `example of uk.parse`, or a test's sentence.
    pub test: String,
    /// What it calls, or its own name when it calls nothing public.
    pub declaration: String,
    pub kind: String,
    /// What went wrong, as a sentence.
    pub reason: String,
}

/// One example or test to run, as text to place in a file.
struct Unit {
    /// The test file it goes in, relative to the package's folder.
    file: String,
    name: String,
    text: String,
    /// The declaration an example shows, with its kind.
    shows: Option<(String, String)>,
}

/// Where a unit landed in the scratch app.
struct Placed {
    unit: usize,
    /// Relative to the scratch app, as a check or a test run names it.
    file: String,
    first: u32,
    last: u32,
}

/// Runs `previous`'s examples and tests against the package in `folder`;
/// fails only when the scratch app can't be written.
pub(crate) fn run_previous(
    folder: &Path,
    compiled: &Compiled,
    previous: &Compiled,
) -> std::io::Result<Behaviour> {
    let scratch = Scratch::new()?;
    std::fs::write(scratch.0.join("app.jadpo"), "application {}\n")?;
    let mount = scratch.0.join(&compiled.mount);
    for file in package::package_files(folder) {
        if file.ends_with(".test.jadpo") && !in_own_app(&file) {
            continue;
        }
        let to = mount.join(&file);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(folder.join(&file), to)?;
    }
    let surface = previous.surface();
    let (mut preambles, units) = units_of(previous, surface.as_ref());
    let mut reasons: BTreeMap<usize, String> = BTreeMap::new();
    let mut placed = Vec::new();
    // Each pass drops what doesn't compile, so the rest can run; a test
    // that breaks only once another is gone is found on the next pass.
    for _ in 0..=units.len() {
        placed = place(&scratch.0, &compiled.mount, &preambles, &units, &reasons)?;
        let loaded = jadpo_db::load(&scratch.0);
        let mut broke = false;
        for diagnostic in jadpo_check::check(&loaded.db, loaded.project) {
            let Some(label) = &diagnostic.primary else {
                continue;
            };
            let text =
                std::fs::read_to_string(scratch.0.join(&label.span.path)).unwrap_or_default();
            let line = text[..(label.span.start as usize).min(text.len())]
                .matches('\n')
                .count() as u32
                + 1;
            let reason = format!("It no longer compiles: {}", sentence(&diagnostic.message));
            if let Some(at) = placed_at(&placed, &label.span.path, line) {
                reasons.entry(at.unit).or_insert(reason);
                broke = true;
            } else if let Some(relative) = compiled.mount_relative(&label.span.path)
                && (preambles.contains_key(relative)
                    || units.iter().any(|unit| unit.file == relative))
            {
                // Something else about the test file: none of its tests can
                // run while it is there.
                if preambles
                    .insert(relative.to_string(), String::new())
                    .is_some_and(|text| !text.is_empty())
                {
                    broke = true;
                }
                for (at, unit) in units.iter().enumerate() {
                    if unit.file == relative && !reasons.contains_key(&at) {
                        reasons.insert(at, reason.clone());
                        broke = true;
                    }
                }
            }
        }
        if !broke {
            break;
        }
    }
    let mut failed: BTreeMap<usize, (String, Option<u32>)> = reasons
        .into_iter()
        .map(|(at, reason)| (at, (reason, None)))
        .collect();
    if !placed.is_empty() {
        match jadpo_test::run(jadpo_test::Options {
            cwd: scratch.0.clone(),
            ..jadpo_test::Options::default()
        }) {
            jadpo_test::Ran::Done(run) => {
                for result in &run.results {
                    let Some(at) = placed_at(&placed, &result.test.file, result.test.line) else {
                        continue;
                    };
                    if let Some(failure) = &result.failure {
                        let (_, reason) = package::what_failed(failure);
                        failed.insert(at.unit, (reason, Some(failure.location.line)));
                    }
                }
            }
            // Nothing ran, so nothing can be said to still hold.
            other => {
                let reason = match other {
                    jadpo_test::Ran::Refused { diagnostics, .. } => diagnostics
                        .first()
                        .map(|d| format!("It couldn't run: {}", sentence(&d.message))),
                    jadpo_test::Ran::Stopped(diagnostic) => Some(format!(
                        "It couldn't run: {}",
                        sentence(&diagnostic.message)
                    )),
                    _ => None,
                }
                .unwrap_or_else(|| "It couldn't run.".to_string());
                for at in &placed {
                    failed.insert(at.unit, (reason.clone(), None));
                }
            }
        }
    }
    let failed = failed
        .into_iter()
        .map(|(at, (reason, line))| {
            let unit = &units[at];
            // The line it failed on says what it was calling, when it can.
            let on_line = line.and_then(|line| {
                let file = placed.iter().find(|placed| placed.unit == at)?;
                let text = std::fs::read_to_string(scratch.0.join(&file.file)).ok()?;
                text.lines().nth(line as usize - 1).map(str::to_string)
            });
            let (declaration, kind) = unit.shows.clone().unwrap_or_else(|| {
                exercised(on_line.as_deref(), &unit.text, surface.as_ref())
                    .unwrap_or_else(|| (unit.name.clone(), "test".to_string()))
            });
            Behaved {
                test: unit.name.clone(),
                declaration,
                kind,
                reason,
            }
        })
        .collect();
    Ok(Behaviour {
        total: units.len(),
        failed,
    })
}

/// Whether a package's file belongs to its `sample/` app or a recipe, whose
/// tests aren't the package's.
fn in_own_app(file: &str) -> bool {
    file.starts_with("sample/") || file.starts_with("recipes/")
}

/// The previous release's own tests and its examples, with what else each
/// test file holds.
fn units_of(
    previous: &Compiled,
    surface: Option<&Surface>,
) -> (BTreeMap<String, String>, Vec<Unit>) {
    let sources = previous.sources();
    let mut preambles: BTreeMap<String, String> = BTreeMap::new();
    let mut units = Vec::new();
    for (path, text) in &sources {
        if !path.ends_with(".test.jadpo") || in_own_app(path) {
            continue;
        }
        let parse = jadpo_syntax::parse_file(text);
        let Some(source) = ast::SourceFile::cast(parse.syntax()) else {
            continue;
        };
        let mut preamble = text.clone();
        let mut cuts: Vec<(usize, usize)> = Vec::new();
        for decl in source.decls() {
            let Decl::TestDecl(test) = decl else { continue };
            let Some(name) = test.string_token() else {
                continue;
            };
            let range = test.syntax().text_range();
            let (start, end) = (usize::from(range.start()), usize::from(range.end()));
            cuts.push((start, end));
            units.push(Unit {
                file: path.clone(),
                name: name.text().trim_matches('"').to_string(),
                text: format!("{}\n", &text[start..end]),
                shows: None,
            });
        }
        for (start, end) in cuts.into_iter().rev() {
            preamble.replace_range(start..end, "");
        }
        preambles.insert(path.clone(), preamble.trim().to_string());
    }
    for declaration in surface.iter().flat_map(|surface| &surface.declarations) {
        let (Some(line), Some(text)) = (declaration.line, sources.get(&declaration.file)) else {
            continue;
        };
        for example in publishing::examples(text, &declaration.name, line) {
            let mut body = format!("test \"{}\" {{\n", example.name());
            for (_, statement) in &example.lines {
                body.push_str(&format!("    {statement}\n"));
            }
            body.push_str("}\n");
            let file = format!(
                "{}.test.jadpo",
                declaration
                    .file
                    .strip_suffix(".jadpo")
                    .unwrap_or(&declaration.file)
            );
            units.push(Unit {
                file,
                name: example.name(),
                text: body,
                shows: Some((
                    declaration.name.clone(),
                    declaration.what.word().to_string(),
                )),
            });
        }
    }
    (preambles, units)
}

/// Writes the test files, leaving out each unit in `dropped`, and says
/// where each of the rest landed.
fn place(
    root: &Path,
    mount: &str,
    preambles: &BTreeMap<String, String>,
    units: &[Unit],
    dropped: &BTreeMap<usize, String>,
) -> std::io::Result<Vec<Placed>> {
    let files: BTreeSet<&String> = preambles
        .keys()
        .chain(units.iter().map(|unit| &unit.file))
        .collect();
    let mut placed = Vec::new();
    for file in files {
        let mut text = preambles.get(file).cloned().unwrap_or_default();
        if !text.is_empty() {
            text.push('\n');
        }
        for (at, unit) in units.iter().enumerate() {
            if unit.file != *file || dropped.contains_key(&at) {
                continue;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            let first = text.matches('\n').count() as u32 + 1;
            text.push_str(&unit.text);
            placed.push(Placed {
                unit: at,
                file: format!("{mount}/{file}"),
                first,
                last: text.matches('\n').count() as u32,
            });
        }
        let path = root.join(mount).join(file);
        if text.is_empty() {
            let _ = std::fs::remove_file(path);
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, text)?;
        }
    }
    Ok(placed)
}

fn placed_at<'a>(placed: &'a [Placed], file: &str, line: u32) -> Option<&'a Placed> {
    placed
        .iter()
        .find(|placed| placed.file == file && (placed.first..=placed.last).contains(&line))
}

fn sentence(message: &str) -> String {
    if message.ends_with('.') {
        message.to_string()
    } else {
        format!("{message}.")
    }
}

/// The public call a test exercises: one on the line it failed on, or else
/// the first it makes.
fn exercised(
    line: Option<&str>,
    text: &str,
    surface: Option<&Surface>,
) -> Option<(String, String)> {
    let callables: Vec<_> = surface?
        .declarations
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.what,
                What::Function { .. } | What::Action { .. } | What::Operation { .. }
            )
        })
        .collect();
    let calling = |within: &str| {
        callables
            .iter()
            .find(|declaration| within.contains(&format!("{}(", declaration.name)))
    };
    line.and_then(calling)
        .or_else(|| calling(text))
        .map(|declaration| {
            (
                declaration.name.clone(),
                declaration.what.word().to_string(),
            )
        })
}

/// `publish.behaviour_changed` for one example or test.
pub(crate) fn behaviour_changed(
    failed: &Behaved,
    since: &str,
    version: &str,
    next_major: &str,
) -> Diagnostic {
    let name = &failed.declaration;
    Diagnostic::new(
        &rule::PUBLISH_BEHAVIOUR_CHANGED,
        format!("`{name}` behaves differently from {since}"),
    )
    .reason(format!(
        "`{}`, from {since}, now fails against the new code. {} An app on {since} may rely on what it showed, and a change the types don't show reaches it with nothing to review.",
        failed.test, failed.reason
    ))
    .next(format!(
        "Put back what it did, publish the change as {next_major}, or, if {since} was wrong, mark it a fix under `## {version}` in {}: Fix `{name}`: why it had to change",
        publishing::CHANGELOG
    ))
}

/// `publish.stale_files`: what a commit carries isn't what its code gives.
pub(crate) fn stale(file: &str, reason: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::PUBLISH_STALE_FILES,
        format!("`{file}` isn't what the package's code gives"),
    )
    .reason(format!(
        "{reason} A release shows what its code gives, so the file a commit carries has to match; `jadpo package verify` checks a commit as it stands and writes nothing."
    ))
    .next(format!(
        "Run `jadpo package check`, commit the {file} it writes, and tag the release again"
    ))
}

// --- screens ---------------------------------------------------------------

/// The facts, on the grid.
pub(crate) fn facts_block(facts: &Facts) -> Block {
    let list = |items: &[String]| {
        if items.is_empty() {
            "none".to_string()
        } else {
            items.join(", ")
        }
    };
    let mut fields = vec![(
        "kind".to_string(),
        if facts.pure {
            format!("{}, pure", facts.kind)
        } else {
            facts.kind.to_string()
        },
    )];
    if facts.kind == "provider" {
        // A host on a line of its own, so it copies whole.
        let hosts: Vec<&str> = facts.hosts.iter().map(|host| host.host.as_str()).collect();
        fields.push(("hosts".to_string(), hosts.join("\n")));
        fields.push(("changes".to_string(), list(&facts.changing_calls)));
        fields.push((
            "erasure".to_string(),
            if facts.erasure {
                "declared"
            } else {
                "not declared"
            }
            .to_string(),
        ));
    }
    fields.push(("storable".to_string(), list(&facts.storable_types)));
    let failures: Vec<String> = facts
        .failures
        .iter()
        .map(|fact| format!("{}: {}", fact.declaration, fact.failures.join(", ")))
        .collect();
    fields.push((
        "failures".to_string(),
        if failures.is_empty() {
            "none".to_string()
        } else {
            failures.join("\n")
        },
    ));
    if facts.kind == "provider" {
        fields.push((
            "contract".to_string(),
            format!(
                "{} of {} confirmed",
                facts.confirmed_operations.len(),
                plural(facts.operations, "operation", "operations")
            ),
        ));
    }
    if !facts.standard_candidates.is_empty() {
        let candidates: Vec<String> = facts
            .standard_candidates
            .iter()
            .map(|fact| format!("{}: {}", fact.declaration, fact.standard))
            .collect();
        fields.push(("standard".to_string(), candidates.join("\n")));
    }
    Block::Answer {
        headline: "Facts".to_string(),
        fields,
    }
}

/// The change record, a line an entry.
pub(crate) fn changes_block(since: &str, record: &[ChangeEntry]) -> Block {
    let rows = record
        .iter()
        .map(|entry| {
            let mark = match entry.change {
                Change::Added | Change::NewHost | Change::PersonalDataSent => ChangeMark::Added,
                Change::Removed => ChangeMark::Removed,
                Change::Retyped | Change::NewFailure | Change::BehaviourChanged => {
                    ChangeMark::Changed
                }
            };
            let what = match entry.change {
                Change::Added | Change::Removed | Change::Retyped if entry.kind != "provider" => {
                    format!("{} {}", entry.kind, entry.declaration)
                }
                _ => entry.declaration.clone(),
            };
            let mut detail = entry.detail.clone().unwrap_or_default();
            if entry.why.is_some() {
                detail.push_str(", marked a fix");
            }
            (mark, vec![entry.change.word().to_string(), what, detail])
        })
        .collect();
    Block::ChangeList {
        heading: format!("Changes since {since}"),
        rows,
    }
}

// --- JSON ------------------------------------------------------------------

/// The facts, in the shape the registry's protocol names `Facts`.
pub(crate) fn facts_json(facts: &Facts) -> Json {
    let strings =
        |items: &[String]| Json::array(items.iter().map(|item| Json::from(item.as_str())));
    Json::object([
        ("kind", Json::from(facts.kind)),
        ("pure", facts.pure.into()),
        (
            "hosts",
            Json::array(facts.hosts.iter().map(|host| {
                Json::object([
                    ("host", Json::from(host.host.as_str())),
                    ("operations", strings(&host.operations)),
                ])
            })),
        ),
        ("changing_calls", strings(&facts.changing_calls)),
        (
            "personal_data",
            Json::array(facts.personal_data.iter().map(|sent| {
                Json::object([
                    ("field", Json::from(sent.field.as_str())),
                    ("host", Json::from(sent.host.as_str())),
                    ("operation", Json::from(sent.operation.as_str())),
                    ("categories", strings(&sent.categories)),
                    ("decided_only", sent.decided_only.into()),
                ])
            })),
        ),
        ("erasure", facts.erasure.into()),
        ("storable_types", strings(&facts.storable_types)),
        (
            "failures",
            Json::array(facts.failures.iter().map(|fact| {
                Json::object([
                    ("declaration", Json::from(fact.declaration.as_str())),
                    ("failures", strings(&fact.failures)),
                ])
            })),
        ),
        ("confirmed_operations", strings(&facts.confirmed_operations)),
        // The registry's scheduled runs: nothing a commit can say.
        ("last_contract_run", Json::Null),
        (
            "standard_candidates",
            Json::array(facts.standard_candidates.iter().map(|fact| {
                Json::object([
                    ("declaration", Json::from(fact.declaration.as_str())),
                    ("standard", Json::from(fact.standard.as_str())),
                ])
            })),
        ),
    ])
}

/// The change record, in the shape the registry's protocol names
/// `ChangeEntry`.
pub(crate) fn record_json(record: &[ChangeEntry]) -> Json {
    let text = |value: &Option<String>| value.as_deref().map_or(Json::Null, Json::from);
    Json::array(record.iter().map(|entry| {
        Json::object([
            ("declaration", Json::from(entry.declaration.as_str())),
            ("kind", Json::from(entry.kind.as_str())),
            ("change", Json::from(entry.change.name())),
            ("detail", text(&entry.detail)),
            ("why", text(&entry.why)),
        ])
    }))
}

/// What the previous release's examples and tests gave.
pub(crate) fn behaviour_json(since: &str, behaviour: &Behaviour) -> Json {
    Json::object([
        ("since", Json::from(since)),
        ("ran", Json::from(behaviour.total as i64)),
        (
            "failed",
            Json::array(behaviour.failed.iter().map(|failed| {
                Json::object([
                    ("test", Json::from(failed.test.as_str())),
                    ("declaration", Json::from(failed.declaration.as_str())),
                    ("reason", Json::from(failed.reason.as_str())),
                ])
            })),
        ),
    ])
}

/// Every fingerprint.
pub(crate) fn fingerprints_json(prints: &[Fingerprint]) -> Json {
    Json::array(prints.iter().map(|print| {
        Json::object([
            ("declaration", Json::from(print.declaration.as_str())),
            ("kind", Json::from(print.kind)),
            ("source", Json::from(print.source.as_str())),
            (
                "shape",
                print.shape.as_ref().map_or(Json::Null, |shape| {
                    Json::object([
                        (
                            "inputs",
                            Json::array(shape.inputs.iter().map(|ty| Json::from(ty.as_str()))),
                        ),
                        (
                            "output",
                            shape.output.as_deref().map_or(Json::Null, Json::from),
                        ),
                        (
                            "failures",
                            Json::array(
                                shape.failures.iter().map(|kind| Json::from(kind.as_str())),
                            ),
                        ),
                        (
                            "effects",
                            Json::array(shape.effects.iter().map(|effect| Json::from(*effect))),
                        ),
                        ("digest", Json::from(shape.digest.as_str())),
                    ])
                }),
            ),
        ])
    }))
}
