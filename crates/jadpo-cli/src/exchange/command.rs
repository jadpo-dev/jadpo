//! `jadpo translations export` and `import` (area 10, "Tooling"; chunk
//! 5.4f): the wording a project has, written for a translation service,
//! and the translations the service returns, checked as the language files
//! are checked and written into them and into the record.
//!
//! Designed before it was built (impeccable's shape pass against the CLI
//! design language, run unattended, so its assumptions are marked):
//!
//! - *Job.* Someone about to hand wording to Crowdin or Lokalise needs the
//!   files and where they are; someone who has the service's answer needs
//!   to know what changed in which language file and, when it cannot go in,
//!   which unit of the file they hold is wrong and why. Mode: operate; the
//!   path is what is copied or uploaded, so it is never cut.
//! - `export` is the status line `update` has for each file written,
//!   `Wrote build/translations/de_de.xlf`, with one row under it saying how
//!   many units the file holds and how many are translated, to translate
//!   and stale; a summary counting the locales and units; and one hint, the
//!   `import` of the file with most left to translate, since that is the
//!   one going out first. *Assumption:* the hint names a path, not the
//!   service, since which one is not Jadpo's to say.
//! - `import` writes the same status line for each language file and the
//!   record, a row under a language file for each unit that changed
//!   (`order_shipped.formal  added`), and a summary headed `Imported
//!   pl_pl` that counts units added, reworded, unchanged, not translated
//!   yet and stale, the stale ones in the problem colour. Nothing to do is
//!   `Already up to date` with the counts that explain it. The hint is
//!   `jadpo check`, or, with something stale, `jadpo translations
//!   missing`, which lists it.
//! - A file that cannot go in is an error for each reason, as a
//!   diagnostic at the unit of the file the person holds (a language
//!   file's own rule when the wording breaks it, `translation.exchange_file_invalid`
//!   and `translation.exchange_unit_unknown` otherwise), a summary that says
//!   nothing was written, and no change to any file. *Assumption:* all
//!   reasons at once, as `config move` has them, because a service round
//!   trip is slow.
//! - *States.* Nothing to export; nothing to import; all of it unchanged;
//!   some translated and some not; wording a person has accepted
//!   (`reviewed` in the file); stale wording (a note); a locale that is
//!   not supported, is the default or is the pseudo-locale; a project
//!   whose files don't parse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_check::translations::exchange::{self, Catalogue, Entry, Shape, Wording};
use jadpo_check::translations::sources;
use jadpo_db::Loaded;
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use super::{Format, InFile, InUnit, OutFile, OutUnit};
use crate::run::{Call, Reply, either, quoted};
use crate::table::Value;

/// The formats `--format` takes.
pub const FORMATS: &[Value] = &[
    Value {
        name: "xliff",
        description: "XLIFF 2.0, with each unit's notes",
    },
    Value {
        name: "json",
        description: "JSON in the shape Chrome extensions use, with descriptions",
    },
];

/// Where `export` writes, under the project.
const OUT: &str = "build/translations";

/// `a`, `a and b`, `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn arguments(
    message: impl Into<String>,
    reason: impl Into<String>,
    next: impl Into<String>,
) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CLI_TRANSLATIONS_ARGUMENTS, message)
            .reason(reason)
            .next(next),
    )
}

/// The catalogue, or the reply saying why there is none.
fn catalogue_of(loaded: &Loaded) -> Result<Catalogue, Box<Reply>> {
    let Some(found) = exchange::catalogue(&loaded.db, loaded.project) else {
        return Err(Box::new(arguments(
            "This project has no locales to exchange translations for",
            "Translations are for the locales the application supports, and `locales { }` in `app.jadpo` declares none.",
            "Declare them, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb, Locale.fr_fr] }`",
        )));
    };
    if let Some(clash) = &found.clash {
        return Err(Box::new(arguments(
            "Jadpo's own messages and a domain would share unit ids",
            format!("Units are named `<domain>.<message>`, and {clash}."),
            "Rename the domain",
        )));
    }
    Ok(found)
}

/// The locale a command line names, among those with something to
/// exchange: the translated locales, and for `export` the default too.
fn locale_among(
    catalogue: &Catalogue,
    action: &str,
    given: &str,
    default_too: bool,
) -> Result<exchange::Target, Box<Reply>> {
    let mut candidates: Vec<&exchange::Target> = catalogue.targets.iter().collect();
    if default_too {
        candidates.insert(0, &catalogue.default);
    }
    let names: Vec<&str> = candidates.iter().map(|target| target.tag).collect();
    if let Some(found) = exchange::locale_named(given)
        && let Some(target) = candidates
            .iter()
            .find(|target| jadpo_std::locale_ordinal(target.tag) == Some(found))
    {
        return Ok((*target).clone());
    }
    let diagnostic = Diagnostic::new(
        &rule::CLI_TRANSLATIONS_ARGUMENTS,
        format!("`{given}` isn't a locale with translations"),
    );
    let reason = if names.is_empty() {
        "This project supports no locale besides its default, which is what the others are translated from.".to_string()
    } else {
        format!(
            "{} the supported locales besides the pseudo-locale: {}.",
            if default_too {
                "A file is exchanged for one of"
            } else {
                "Translations are for"
            },
            either(&quoted(&names))
        )
    };
    Err(Box::new(Reply::failed(
        match nearest(given, names.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!("{reason} Did you mean `{near}`?"))
                .next(format!("Use `jadpo translations {action} {near}`")),
            None => diagnostic.reason(reason).next(match names.first() {
                Some(first) => format!("Run `jadpo translations {action} {first}`"),
                None => "Add a locale to `supported:` in `locales { }`".to_string(),
            }),
        },
    )))
}

fn syntax_errors(loaded: &Loaded) -> Vec<Diagnostic> {
    jadpo_check::check(&loaded.db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.id.starts_with("syntax."))
        .collect()
}

// --- export ----------------------------------------------------------------------------

fn format_flag(call: &Call) -> Result<Format, Box<Reply>> {
    match call.value("--format") {
        None | Some("xliff") => Ok(Format::Xliff),
        Some("json") => Ok(Format::Json),
        Some(other) => Err(Box::new(arguments(
            format!("`{other}` isn't a format `export` writes"),
            "`export` writes `xliff`, XLIFF 2.0, or `json`, in the shape Chrome extensions use; both carry the descriptions.",
            "Use `--format xliff` or `--format json`",
        ))),
    }
}

/// The units of a locale's file.
fn units(
    loaded: &Loaded,
    catalogue: &Catalogue,
    target: &exchange::Target,
) -> (Vec<OutUnit>, usize, usize) {
    let (db, project) = (&loaded.db, loaded.project);
    let is_default = target.ordinal == catalogue.default.ordinal;
    let mut out = Vec::new();
    let mut stale = 0;
    for entry in &catalogue.entries {
        let existing = if is_default {
            None
        } else {
            exchange::existing(db, project, entry, target.tag)
        };
        for variant in entry.variants(target.ordinal) {
            let Some(source) = entry.source_for(variant.as_deref()) else {
                continue;
            };
            let have = existing.as_ref().and_then(|existing| {
                existing
                    .wording
                    .iter()
                    .find(|(seen, _)| *seen == variant)
                    .map(|(_, text)| text.clone())
            });
            let mut notes: Vec<(&'static str, String)> = Vec::new();
            if let Some(description) = &entry.description {
                notes.push(("description", description.clone()));
            }
            if let Some(context) = entry.context(variant.as_deref()) {
                notes.push(("context", context));
            }
            if !entry.placeholders.is_empty() {
                let listed: Vec<String> = entry
                    .placeholders
                    .iter()
                    .map(|name| format!("{{{name}}}"))
                    .collect();
                notes.push(("placeholders", listed.join(", ")));
            }
            let is_stale = have.is_some() && existing.as_ref().is_some_and(|e| e.stale);
            if is_stale {
                stale += 1;
                notes.push((
                    "stale",
                    "The default wording changed after this was translated.".to_string(),
                ));
            }
            out.push(OutUnit {
                scope: entry.scope.clone(),
                folder: folder_of(entry),
                id: match &variant {
                    Some(variant) => format!("{}.{variant}", entry.member),
                    None => entry.member.clone(),
                },
                notes,
                source: source.to_string(),
                reviewed: have.is_some() && existing.as_ref().is_some_and(|e| e.accepted),
                target: have,
            });
        }
    }
    let translated = out.iter().filter(|unit| unit.target.is_some()).count();
    (out, translated, stale)
}

fn folder_of(entry: &Entry) -> String {
    let path = entry.language_path("x");
    path.rsplit_once('/')
        .map_or(path.clone(), |(folder, _)| folder.to_string())
}

pub fn export(loaded: &Loaded, call: &Call, cwd: &Path) -> Reply {
    if let Some(extra) = call.arguments.get(2) {
        return arguments(
            format!("`{extra}` is one argument too many"),
            "`jadpo translations export` takes at most a locale after it.",
            "Run `jadpo translations export` for every locale, or name one",
        );
    }
    let format = match format_flag(call) {
        Ok(format) => format,
        Err(reply) => return *reply,
    };
    let syntax = syntax_errors(loaded);
    if !syntax.is_empty() {
        return blocked(
            loaded,
            cwd,
            syntax,
            "translations export stopped",
            &BTreeMap::new(),
        );
    }
    let catalogue = match catalogue_of(loaded) {
        Ok(found) => found,
        Err(reply) => return *reply,
    };
    let locales: Vec<exchange::Target> = match call.arguments.get(1) {
        Some(given) => match locale_among(&catalogue, "export", given, true) {
            Ok(target) => vec![target],
            Err(reply) => return *reply,
        },
        None if catalogue.targets.is_empty() => vec![catalogue.default.clone()],
        None => catalogue.targets.clone(),
    };
    let out_folder = call.value("--out").unwrap_or(OUT).trim_end_matches('/');
    let mut screen = Screen::new();
    let mut files = Vec::new();
    let mut total = 0;
    let mut first_to_translate: Option<String> = None;
    let mut first_written: Option<String> = None;
    for target in &locales {
        let is_default = target.ordinal == catalogue.default.ordinal;
        let (units, translated, stale) = units(loaded, &catalogue, target);
        let file = OutFile {
            source_language: catalogue.default.bcp47.to_string(),
            target_language: (!is_default).then(|| target.bcp47.to_string()),
            units,
        };
        if let Some(bad) = file.units.iter().find_map(|unit| {
            [Some(&unit.source), unit.target.as_ref()]
                .into_iter()
                .flatten()
                .find(|text| format == Format::Xliff && !text.chars().all(super::xml::allowed))
                .map(|_| unit.full_id())
        }) {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TRANSLATION_EXCHANGE_FILE_INVALID,
                    format!("`{bad}` has a character XLIFF cannot hold"),
                )
                .reason("XML 1.0 has no way to write most control characters, so a wording that holds one cannot be exported as XLIFF.")
                .next_for(
                    "Take the control character out of the wording, or export as JSON with `--format json`",
                    jadpo_diag::Decider::Human,
                ),
            );
        }
        let relative = format!("{out_folder}/{}.{}", target.tag, format.extension());
        let path = if Path::new(&relative).is_absolute() {
            PathBuf::from(&relative)
        } else {
            loaded.root.join(&relative)
        };
        let text = super::write(format, &file);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text));
        if let Err(error) = written {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{relative}` couldn't be written"),
                )
                .reason(format!("Writing it failed: {error}."))
                .next_for(
                    "Check the folder can be written, then run the command again",
                    jadpo_diag::Decider::Human,
                ),
            );
        }
        let to_translate = file.units.len() - translated;
        let detail = if is_default {
            format!(
                "{}, the default wording to translate from",
                count(file.units.len(), "unit", "units")
            )
        } else {
            let mut parts = vec![
                count(file.units.len(), "unit", "units"),
                format!("{translated} translated"),
            ];
            if to_translate > 0 {
                parts[1] = if translated > 0 {
                    format!("{translated} translated")
                } else {
                    String::new()
                };
                parts.push(format!("{to_translate} to translate"));
                parts.retain(|part| !part.is_empty());
            }
            if stale > 0 {
                parts.push(format!("{stale} stale"));
            }
            parts.join(", ")
        };
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Wrote {relative}"),
            rows: vec![(target.tag.to_string(), detail)],
        });
        total += file.units.len();
        if first_written.is_none() {
            first_written = Some(relative.clone());
        }
        if first_to_translate.is_none() && !is_default && to_translate > 0 {
            first_to_translate = Some(relative.clone());
        }
        files.push(Json::object([
            ("locale", Json::from(target.tag)),
            ("path", Json::from(relative.as_str())),
            ("format", Json::from(format.word())),
            ("units", file.units.len().into()),
            ("translated", translated.into()),
            ("toTranslate", to_translate.into()),
            ("stale", stale.into()),
        ]));
    }
    if catalogue.entries.is_empty() {
        screen.push(Block::summary(
            Outcome::Success,
            "No messages to export",
            [],
        ));
    } else {
        screen.push(Block::summary(
            Outcome::Success,
            format!("Exported {}", count(locales.len(), "locale", "locales")),
            [
                Fact::plain(count(total, "unit", "units")),
                Fact::plain(format.word()),
            ],
        ));
        let only_default = locales.len() == 1 && locales[0].ordinal == catalogue.default.ordinal;
        if !only_default && let Some(path) = first_to_translate.or(first_written) {
            screen.push(Block::hint(
                format!("jadpo translations import {path}"),
                "reads the translations back",
            ));
        }
    }
    Reply::screen(
        screen,
        Json::object([("files", Json::array(files)), ("units", total.into())]),
    )
}

// --- import ----------------------------------------------------------------------------

/// A screen of what stops the command, with the file it read among the
/// sources.
fn blocked(
    loaded: &Loaded,
    cwd: &Path,
    problems: Vec<Diagnostic>,
    headline: &str,
    extra: &BTreeMap<String, String>,
) -> Reply {
    let mut sources: BTreeMap<String, String> = jadpo_db::source_texts(&loaded.db, loaded.project);
    sources.extend(extra.clone());
    let root = jadpo_db::shown_root(cwd, &loaded.root);
    let mut screen = Screen::new();
    for problem in &problems {
        screen.push(Block::diagnostic_from(problem, &sources, root.as_deref()));
    }
    let errors = problems.len();
    screen.push(Block::summary(
        Outcome::Failure,
        headline,
        [Fact::problem(count(errors, "error", "errors"))],
    ));
    Reply::Report {
        screen,
        data: Json::object([("errors", errors.into())]),
        diagnostics: problems,
        sources,
    }
}

fn file_invalid(shown: &str, at: usize, text: &str, why: &str, headline: &str) -> Diagnostic {
    let start = at.min(text.len());
    let end = text[start..]
        .char_indices()
        .nth(1)
        .map_or(text.len(), |(next, _)| start + next)
        .max(start + 1);
    Diagnostic::new(&rule::TRANSLATION_EXCHANGE_FILE_INVALID, headline)
        .at(
            Span::new(shown, start as u32, end as u32),
            why.to_string(),
        )
        .reason("`jadpo translations import` reads XLIFF 2.0 or 2.1, and JSON in the shape `export` writes: an object with a unit for each key, each a string or `{ \"message\": ... }`.")
        .next_for(
            "Export again with `jadpo translations export`, upload that file and import what the service returns",
            jadpo_diag::Decider::Human,
        )
}

/// A change to one language file's member.
struct Change {
    entry: usize,
    wording: Wording,
    /// `(value or plural form, what happened, the unit of the file)`.
    units: Vec<(Option<String>, Kind, usize)>,
    /// Translated from wording that has since changed.
    stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Added,
    Reworded,
}

fn normalised(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

pub fn import(loaded: &mut Loaded, call: &Call, cwd: &Path) -> Reply {
    let Some(file_argument) = call.arguments.get(1) else {
        return arguments(
            "`jadpo translations import` needs a file",
            "It reads the XLIFF or JSON file a translation service returned.",
            "Run `jadpo translations import <file>`",
        );
    };
    if let Some(extra) = call.arguments.get(2) {
        return arguments(
            format!("`{extra}` is one argument too many"),
            "`jadpo translations import` takes one file; a locale a JSON file doesn't say comes from `--locale`.",
            format!("Run `jadpo translations import {file_argument} --locale {extra}`"),
        );
    }
    let path = if Path::new(file_argument).is_absolute() {
        PathBuf::from(file_argument)
    } else {
        cwd.join(file_argument)
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_SOURCE_READ_FAILED,
                    format!("`{file_argument}` couldn't be read"),
                )
                .reason(format!("Reading it failed: {error}."))
                .next("Check the path, and that the file is UTF-8 text"),
            );
        }
    };
    let shown = loaded
        .relative(&path)
        .unwrap_or_else(|| file_argument.clone());
    let mut file_sources = BTreeMap::new();
    file_sources.insert(shown.clone(), text.clone());
    let headline = format!("`{shown}` isn't a translation file Jadpo can read");
    let fail = |loaded: &Loaded, diagnostics: Vec<Diagnostic>| {
        blocked(
            loaded,
            cwd,
            diagnostics,
            "Import stopped: nothing written",
            &file_sources,
        )
    };

    let syntax = syntax_errors(loaded);
    if !syntax.is_empty() {
        return blocked(
            loaded,
            cwd,
            syntax,
            "translations import stopped",
            &BTreeMap::new(),
        );
    }
    let catalogue = match catalogue_of(loaded) {
        Ok(found) => found,
        Err(reply) => return *reply,
    };

    // What the file is, and the locale it is for.
    let format = match Format::detect(&text) {
        Some(format) => format,
        None => {
            return fail(
                loaded,
                vec![file_invalid(
                    &shown,
                    0,
                    &text,
                    "it starts with neither `<` (XLIFF) nor `{` (JSON)",
                    &headline,
                )],
            );
        }
    };
    let parsed: InFile = match super::read(format, &text) {
        Ok(parsed) => parsed,
        Err(problem) => {
            return fail(
                loaded,
                vec![file_invalid(
                    &shown,
                    problem.at,
                    &text,
                    &problem.why,
                    &headline,
                )],
            );
        }
    };
    let by_flag = match call.value("--locale") {
        Some(given) => match locale_among(&catalogue, "import", given, false) {
            Ok(target) => Some(target),
            Err(reply) => return *reply,
        },
        None => None,
    };
    let by_file = match &parsed.target_language {
        Some(language) => match locale_among(&catalogue, "import", language, false) {
            Ok(target) => Some(target),
            Err(reply) => return *reply,
        },
        None => None,
    };
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| locale_among(&catalogue, "import", stem, false).ok());
    let target = match (&by_flag, &by_file) {
        (Some(flag), Some(file)) if flag.ordinal != file.ordinal => {
            return arguments(
                format!(
                    "`--locale {}` doesn't match the file, which says it is `{}`",
                    flag.tag,
                    parsed.target_language.as_deref().unwrap_or_default()
                ),
                "An XLIFF file names the language it is translated into, and importing it as another would put its wording in the wrong language file.",
                format!(
                    "Run `jadpo translations import {file_argument}` without `--locale`, or with `--locale {}`",
                    file.tag
                ),
            );
        }
        (Some(target), _) | (None, Some(target)) => target.clone(),
        (None, None) => match stem {
            Some(target) => target,
            None => {
                let first = catalogue
                    .targets
                    .first()
                    .map_or("fr_fr", |target| target.tag);
                return arguments(
                    format!(
                        "`jadpo translations import` can't tell which locale `{file_argument}` is for"
                    ),
                    "An XLIFF file names its target language; a JSON file doesn't, so the locale is taken from the file's name (`pl_pl.json`) or from `--locale`.",
                    format!("Run `jadpo translations import {file_argument} --locale {first}`"),
                );
            }
        },
    };
    if let Some(language) = &parsed.source_language
        && exchange::locale_named(language) != Some(catalogue.default.ordinal)
    {
        return fail(
            loaded,
            vec![file_invalid(
                &shown,
                text.find("srcLang").unwrap_or(0),
                &text,
                &format!(
                    "it was translated from `{language}`, and this project's default locale is `{}`",
                    catalogue.default.bcp47
                ),
                &headline,
            )],
        );
    }

    // A record that can't be read is not written over.
    {
        let (db, project) = (&loaded.db, loaded.project);
        let unreadable: Vec<Diagnostic> = jadpo_check::check(db, project)
            .into_iter()
            .filter(|d| d.rule.id == "translation.sources_invalid")
            .collect();
        if !unreadable.is_empty() {
            return fail(loaded, unreadable);
        }
    }

    // Each unit of the file is a wording of this project's.
    let mut expected: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for (at, entry) in catalogue.entries.iter().enumerate() {
        for variant in entry.variants(target.ordinal) {
            expected.insert(entry.id(variant.as_deref()), (at, variant));
        }
    }
    let mut problems: Vec<Diagnostic> = Vec::new();
    let mut groups: BTreeMap<usize, Vec<(Option<String>, usize)>> = BTreeMap::new();
    let mut skipped = 0usize;
    for (at, unit) in parsed.units.iter().enumerate() {
        match expected.get(&unit.id) {
            None => problems.push(unknown_unit(
                &shown, &text, unit, &catalogue, &target, &expected,
            )),
            Some((entry, variant)) => {
                if unit.target.as_deref().is_none_or(str::is_empty) {
                    skipped += 1;
                } else {
                    groups
                        .entry(*entry)
                        .or_default()
                        .push((variant.clone(), at));
                }
            }
        }
    }
    if !problems.is_empty() {
        return fail(loaded, problems);
    }

    // What changes.
    let (db, project) = (&loaded.db, loaded.project);
    let mut changes: Vec<Change> = Vec::new();
    let mut unchanged = 0usize;
    let mut stale_units = 0usize;
    for (entry_at, imported) in &groups {
        let entry = &catalogue.entries[*entry_at];
        let existing = exchange::existing(db, project, entry, target.tag)
            .map(|existing| existing.wording)
            .unwrap_or_default();
        let mut merged: Vec<(Option<String>, String)> = existing.clone();
        let mut units = Vec::new();
        let mut stale = false;
        for (variant, unit_at) in imported {
            let unit = &parsed.units[*unit_at];
            let new = unit.target.clone().unwrap_or_default();
            match merged.iter_mut().find(|(seen, _)| seen == variant) {
                Some((_, text)) if *text == new => {
                    unchanged += 1;
                    continue;
                }
                Some((_, text)) => {
                    *text = new;
                    units.push((variant.clone(), Kind::Reworded, *unit_at));
                }
                None => {
                    merged.push((variant.clone(), new));
                    units.push((variant.clone(), Kind::Added, *unit_at));
                }
            }
            if let (Some(said), Some(now)) = (&unit.source, entry.source_for(variant.as_deref()))
                && normalised(said) != normalised(now)
            {
                stale = true;
                stale_units += 1;
            }
        }
        if units.is_empty() {
            continue;
        }
        // The expected order first, then anything else the file had.
        let order = entry.variants(target.ordinal);
        merged.sort_by_key(|(variant, _)| {
            order
                .iter()
                .position(|seen| seen == variant)
                .unwrap_or(order.len())
        });
        let wording = match entry.shape {
            Shape::Plain => Wording::Text(
                merged
                    .into_iter()
                    .next()
                    .map(|(_, text)| text)
                    .unwrap_or_default(),
            ),
            _ => Wording::Variants(
                merged
                    .into_iter()
                    .filter_map(|(variant, text)| Some((variant?, text)))
                    .collect(),
            ),
        };
        units.sort_by_key(|(variant, _, _)| {
            order
                .iter()
                .position(|seen| seen == variant)
                .unwrap_or(order.len())
        });
        changes.push(Change {
            entry: *entry_at,
            wording,
            units,
            stale,
        });
    }

    if changes.is_empty() {
        let mut screen = Screen::new();
        let mut facts = vec![Fact::plain(target.tag)];
        if unchanged > 0 {
            facts.push(Fact::plain(format!("{unchanged} unchanged")));
        }
        if skipped > 0 {
            facts.push(Fact::plain(format!("{skipped} not translated yet")));
        }
        screen.push(Block::summary(
            Outcome::Success,
            "Already up to date",
            facts,
        ));
        return Reply::screen(
            screen,
            import_data(&target, &[], &[], 0, 0, unchanged, skipped, 0),
        );
    }

    // The language files, as they would be.
    let texts = jadpo_db::source_texts(&loaded.db, loaded.project);
    let mut edited: BTreeMap<String, Edited> = BTreeMap::new();
    for (at, change) in changes.iter().enumerate() {
        let entry = &catalogue.entries[change.entry];
        let path = entry.language_path(target.tag);
        let slot = edited.entry(path.clone()).or_insert_with(|| Edited {
            path: path.clone(),
            scope: entry.scope.clone(),
            root: entry.scope == exchange::JADPO,
            text: String::new(),
            changes: Vec::new(),
        });
        slot.changes.push(at);
    }
    for edited in edited.values_mut() {
        let members: Vec<(String, Wording)> = edited
            .changes
            .iter()
            .map(|&at| {
                (
                    catalogue.entries[changes[at].entry].member.clone(),
                    changes[at].wording.clone(),
                )
            })
            .collect();
        let existing = exchange::file_for(&loaded.db, loaded.project, &edited.scope, target.tag)
            .and_then(|file| texts.get(&file.path).map(|text| (file, text.as_str())));
        let written = exchange::write_members(existing, edited.root, &members);
        edited.text = jadpo_fmt::format_file(&written).unwrap_or(written);
    }

    // Does the project still check? The same checks the language files get.
    let baseline = jadpo_check::check(&loaded.db, loaded.project);
    for edited in edited.values() {
        loaded.set_text(&edited.path, edited.text.clone());
    }
    let after = jadpo_check::check(&loaded.db, loaded.project);
    let touched: Vec<&str> = edited.keys().map(String::as_str).collect();
    let mut remaining = baseline;
    let mut found: Vec<Diagnostic> = Vec::new();
    for diagnostic in after {
        if let Some(at) = remaining.iter().position(|seen| same(seen, &diagnostic)) {
            remaining.remove(at);
            continue;
        }
        let in_touched = diagnostic
            .primary
            .as_ref()
            .is_some_and(|label| touched.contains(&label.span.path.as_str()));
        if in_touched || diagnostic.rule.id.starts_with("translation.") {
            found.push(diagnostic);
        }
    }
    if !found.is_empty() {
        let anchored = found
            .into_iter()
            .map(|diagnostic| {
                anchor(
                    loaded, &catalogue, &target, &edited, &changes, &parsed, &shown, &text,
                    diagnostic,
                )
            })
            .collect();
        return fail(loaded, anchored);
    }

    // The record.
    let mut records: BTreeMap<String, Vec<sources::Entry>> = BTreeMap::new();
    for change in &changes {
        let entry = &catalogue.entries[change.entry];
        let scope = entry.record_scope().to_string();
        let slot = records.entry(scope.clone()).or_insert_with(|| {
            sources::standings(
                &loaded.db,
                loaded.project,
                jadpo_check::lookups::Name::new(&loaded.db, scope),
            )
            .record
            .clone()
        });
        let from = if change.stale {
            // Translated from other wording than today's: a hash that is
            // not today's keeps it stale until it is translated again.
            sources::hash(&format!("older\u{1f}{}", exchange::default_hash(entry)))
        } else {
            exchange::default_hash(entry)
        };
        let line = sources::Entry {
            locale: target.tag.to_string(),
            member: entry.record_member(),
            from,
            wording: sources::hash(&change.wording.canonical()),
            accepted: false,
        };
        slot.retain(|seen| !(seen.locale == line.locale && seen.member == line.member));
        slot.push(line);
    }
    let mut writes: Vec<(String, String)> = edited
        .values()
        .map(|edited| (edited.path.clone(), edited.text.clone()))
        .collect();
    for (scope, entries) in &records {
        writes.push((sources::record_path(scope), sources::render(entries)));
    }
    let originals: Vec<(PathBuf, Option<String>)> = writes
        .iter()
        .map(|(path, _)| {
            let full = loaded.root.join(path);
            let before = std::fs::read_to_string(&full).ok();
            (full, before)
        })
        .collect();
    for (at, (path, text)) in writes.iter().enumerate() {
        let full = loaded.root.join(path);
        let result = full
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&full, text));
        if let Err(error) = result {
            // Put back what was written, so a failed import changes nothing.
            for (full, before) in originals.iter().take(at) {
                let _ = match before {
                    Some(before) => std::fs::write(full, before),
                    None => std::fs::remove_file(full),
                };
            }
            return Reply::failed(
                Diagnostic::new(
                    &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                    format!("`{path}` couldn't be written"),
                )
                .reason(format!("Writing it failed: {error}. Nothing was changed."))
                .next_for(
                    "Check the folder can be written, then run the command again",
                    jadpo_diag::Decider::Human,
                ),
            );
        }
    }

    // What to say.
    let mut screen = Screen::new();
    let mut rows_data = Vec::new();
    let (mut added, mut reworded) = (0usize, 0usize);
    for edited in edited.values() {
        let mut rows = Vec::new();
        for &at in &edited.changes {
            let change = &changes[at];
            let entry = &catalogue.entries[change.entry];
            for (variant, kind, _) in &change.units {
                let label = match variant {
                    Some(variant) => format!("{}.{variant}", entry.member),
                    None => entry.member.clone(),
                };
                let word = match kind {
                    Kind::Added => {
                        added += 1;
                        "added"
                    }
                    Kind::Reworded => {
                        reworded += 1;
                        "reworded"
                    }
                };
                rows_data.push(Json::object([
                    ("path", Json::from(edited.path.as_str())),
                    ("unit", Json::from(entry.id(variant.as_deref()))),
                    ("change", Json::from(word)),
                ]));
                rows.push((label, word.to_string()));
            }
        }
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Wrote {}", edited.path),
            rows,
        });
    }
    let mut record_paths = Vec::new();
    for scope in records.keys() {
        let path = sources::record_path(scope);
        screen.push(Block::Status {
            outcome: Outcome::Success,
            text: format!("Wrote {path}"),
            rows: Vec::new(),
        });
        record_paths.push(path);
    }
    let mut facts = Vec::new();
    for (n, word) in [
        (added, "added"),
        (reworded, "reworded"),
        (unchanged, "unchanged"),
    ] {
        if n > 0 {
            facts.push(Fact::plain(format!("{n} {word}")));
        }
    }
    if skipped > 0 {
        facts.push(Fact::plain(format!("{skipped} not translated yet")));
    }
    if stale_units > 0 {
        facts.push(Fact::problem(format!("{stale_units} stale")));
    }
    screen.push(Block::summary(
        Outcome::Success,
        format!("Imported {}", target.tag),
        facts,
    ));
    if stale_units > 0 {
        screen.push(Block::hint(
            "jadpo translations missing",
            "lists the translations made from older wording",
        ));
    } else {
        screen.push(Block::hint("jadpo check", "confirms the imported wording"));
    }
    Reply::screen(
        screen,
        import_data(
            &target,
            &rows_data,
            &record_paths,
            added,
            reworded,
            unchanged,
            skipped,
            stale_units,
        ),
    )
}

#[allow(clippy::too_many_arguments)]
fn import_data(
    target: &exchange::Target,
    units: &[Json],
    records: &[String],
    added: usize,
    reworded: usize,
    unchanged: usize,
    skipped: usize,
    stale: usize,
) -> Json {
    Json::object([
        ("locale", Json::from(target.tag)),
        ("units", Json::array(units.to_vec())),
        ("records", Json::array(records.iter().map(String::as_str))),
        ("added", added.into()),
        ("reworded", reworded.into()),
        ("unchanged", unchanged.into()),
        ("notTranslated", skipped.into()),
        ("stale", stale.into()),
    ])
}

/// A language file as the import would leave it.
struct Edited {
    path: String,
    scope: String,
    root: bool,
    text: String,
    /// The changes made in it.
    changes: Vec<usize>,
}

fn same(a: &Diagnostic, b: &Diagnostic) -> bool {
    a.rule.id == b.rule.id
        && a.message == b.message
        && a.primary.as_ref().map(|label| &label.span.path)
            == b.primary.as_ref().map(|label| &label.span.path)
}

fn unknown_unit(
    shown: &str,
    text: &str,
    unit: &InUnit,
    catalogue: &Catalogue,
    target: &exchange::Target,
    expected: &BTreeMap<String, (usize, Option<String>)>,
) -> Diagnostic {
    let at = unit.start.min(text.len());
    let end = unit.end.clamp(at + 1, text.len().max(at + 1));
    let near = nearest(&unit.id, expected.keys().map(String::as_str));
    // A unit of a message that exists, in a form the language does not use.
    let message = catalogue.entries.iter().find(|entry| {
        unit.id == entry.id(None)
            || unit
                .id
                .strip_prefix(&format!("{}.", entry.id(None)))
                .is_some()
    });
    let reason = match message {
        Some(entry) if !matches!(entry.shape, Shape::Plain) => {
            let listed: Vec<String> = entry
                .variants(target.ordinal)
                .into_iter()
                .flatten()
                .map(|variant| format!("`{variant}`"))
                .collect();
            format!(
                "`{}` is worded in {} with one unit for each of {}, and `{}` is none of them.{}",
                entry.id(None),
                target.tag,
                and_list(&listed),
                unit.id,
                near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
            )
        }
        _ => format!(
            "A unit is named `<domain>.<message>`, with `.<value>` or `.<plural form>` for a message whose wording is chosen by one, as `jadpo translations export` writes them.{}",
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        ),
    };
    Diagnostic::new(
        &rule::TRANSLATION_EXCHANGE_UNIT_UNKNOWN,
        format!("`{}` isn't a unit of wording in this project", unit.id),
    )
    .at(Span::new(shown, at as u32, end as u32), "no such unit")
    .reason(reason)
    .next(match near {
        Some(near) => format!("Rename the unit to `{near}` in the file, or export again with `jadpo translations export {}`", target.tag),
        None => format!("Remove the unit, or export again with `jadpo translations export {}` so the file has the project's units", target.tag),
    })
}

/// Moves a problem found in a language file the import would write to the
/// unit of the imported file that caused it: the file written is not one
/// the person holds.
#[allow(clippy::too_many_arguments)]
fn anchor(
    loaded: &Loaded,
    catalogue: &Catalogue,
    target: &exchange::Target,
    edited: &BTreeMap<String, Edited>,
    changes: &[Change],
    parsed: &InFile,
    shown: &str,
    text: &str,
    mut diagnostic: Diagnostic,
) -> Diagnostic {
    let Some(primary) = diagnostic.primary.clone() else {
        return diagnostic;
    };
    let Some(file) = edited.get(&primary.span.path) else {
        return diagnostic;
    };
    let language = exchange::file_for(&loaded.db, loaded.project, &file.scope, target.tag);
    // The change, and the unit inside it, the problem is on.
    let mut chosen: Option<(usize, usize)> = None;
    for &at in &file.changes {
        let change = &changes[at];
        let entry = &catalogue.entries[change.entry];
        let member = language.and_then(|language| exchange::member_of(language, entry));
        let Some(member) = member else { continue };
        if primary.span.start >= member.span.start && primary.span.start <= member.span.end {
            let variant = match &member.body {
                jadpo_check::translations::Body::Variants(variants) => variants
                    .iter()
                    .find(|variant| {
                        primary.span.start >= variant.key_span.start
                            && variant
                                .written
                                .as_ref()
                                .is_some_and(|written| primary.span.start <= written.span.end)
                    })
                    .map(|variant| variant.name.clone()),
                _ => None,
            };
            let unit = change
                .units
                .iter()
                .find(|(seen, _, _)| seen.as_deref() == variant.as_deref())
                .or_else(|| change.units.first())
                .map(|(_, _, unit)| *unit);
            chosen = unit.map(|unit| (at, unit));
            break;
        }
    }
    let unit_at = chosen.map(|(_, unit)| unit).or_else(|| {
        file.changes
            .first()
            .and_then(|&at| changes[at].units.first().map(|(_, _, unit)| *unit))
    });
    if let Some(unit_at) = unit_at {
        let unit = &parsed.units[unit_at];
        let start = unit.start.min(text.len());
        let end = unit.end.clamp(start + 1, text.len().max(start + 1));
        // A wording problem points inside the wording when the file has it
        // in one piece.
        diagnostic.primary = Some(jadpo_diag::Label {
            span: Span::new(shown, start as u32, end as u32),
            text: primary.text,
        });
    }
    diagnostic
        .secondary
        .retain(|label| !edited.contains_key(&label.span.path));
    if let Some(next) = diagnostic.next.as_mut() {
        next.edits.clear();
    }
    diagnostic
}
