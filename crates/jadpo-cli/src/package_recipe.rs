//! `jadpo package recipe <package> <recipe> --into <domain>` (area 11,
//! "Recipes"; area 14; chunk 7.6): copies one of a package's recipes, a
//! complete wiring example in its `recipes/<recipe>/` laid out as a
//! domain is, into one of the app's domains, where it becomes the app's
//! own code.
//!
//! It copies all or nothing: a file the domain already has is never
//! written over, and naming one refuses the copy. What it wrote is
//! listed, then the footer and the one command that shows what to fit to
//! the app:
//!
//! ```text
//! ✓ Copied recipe track_parcels into domains/shipping
//!   domains/shipping/actions/track_parcel.jadpo
//!   domains/shipping/schedules/parcels_nightly.jadpo
//!
//! ✓ recipe copied  AcmeCouriers.v1 · track_parcels · 2 files
//!
//! → jadpo check  shows what to fit to the app
//! ```

use std::path::{Path, PathBuf};

use jadpo_check::surface;
use jadpo_diag::nearest;
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_needed;
use crate::package::{and_list, arguments, plural, write_failed};
use crate::run::{Call, Reply, quoted};

/// The folder a package keeps its recipes in.
const RECIPES: &str = "recipes";

/// The folder the app's domains are in.
const DOMAINS: &str = "domains";

pub fn recipe(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("package recipe");
    };
    let usage = "Run `jadpo package recipe <package> <recipe> --into <domain>`";
    let (Some(wanted), Some(name)) = (call.arguments.get(1), call.arguments.get(2)) else {
        return Reply::failed(arguments(
            "`jadpo package recipe` needs a package and one of its recipes".to_string(),
            "It copies one of a package's recipes into a domain of the app.".to_string(),
            usage.to_string(),
        ));
    };
    if call.arguments.len() > 3 {
        return Reply::failed(arguments(
            format!(
                "`jadpo package recipe` takes one recipe, not `{}`",
                call.arguments[3]
            ),
            "It copies one recipe at a time.".to_string(),
            usage.to_string(),
        ));
    }
    let loaded = jadpo_db::load_whole(&root);
    let (db, project) = (&loaded.db, loaded.project);
    let homes = surface::homes(db, project);
    let all: Vec<&jadpo_db::PackageHome> = homes
        .values()
        .filter(|home| *wanted == home.name || *wanted == home.code || *wanted == home.versioned())
        .collect();
    if all.len() > 1 {
        let held: Vec<String> = all.iter().map(|home| home.versioned()).collect();
        return Reply::failed(arguments(
            format!("`{wanted}` names {} packages", all.len()),
            format!(
                "The project holds {} side by side, so `jadpo package recipe` needs the version too.",
                and_list(&quoted(&held))
            ),
            format!("Use `jadpo package recipe {} {name}`", held[0]),
        ));
    }
    let Some(home) = all.first().copied() else {
        let folders: Vec<String> = homes.values().map(|home| home.name.clone()).collect();
        let holds = if folders.is_empty() {
            "The project holds no package: none in `packages/` or `shared/providers/`.".to_string()
        } else {
            format!("The project holds {}.", and_list(&quoted(&folders)))
        };
        return Reply::failed(match nearest(wanted, folders.iter().map(String::as_str)) {
            Some(near) => arguments(
                format!("`{wanted}` isn't a package in this project"),
                format!("{holds} Did you mean `{near}`?"),
                format!("Use `jadpo package recipe {near} {name}`"),
            ),
            None => arguments(
                format!("`{wanted}` isn't a package in this project"),
                holds,
                "Install one with `jadpo package add`".to_string(),
            ),
        });
    };
    let package = home.versioned();
    let recipes_folder = root.join(&home.folder).join(RECIPES);
    let recipes = folders_in(&recipes_folder);
    if !recipes.contains(name) {
        let has = if recipes.is_empty() {
            format!("`{package}` ships no recipes.")
        } else {
            format!("`{package}` ships {}.", and_list(&quoted(&recipes)))
        };
        return Reply::failed(match nearest(name, recipes.iter().map(String::as_str)) {
            Some(near) => arguments(
                format!("`{package}` has no recipe `{name}`"),
                format!("{has} Did you mean `{near}`?"),
                format!(
                    "Use `jadpo package recipe {} {near}{}`",
                    home.name,
                    call.value("--into")
                        .map(|domain| format!(" --into {domain}"))
                        .unwrap_or_default()
                ),
            ),
            None => arguments(
                format!("`{package}` has no recipe `{name}`"),
                has,
                format!(
                    "Run `jadpo package show {}` to see what it ships",
                    home.name
                ),
            ),
        });
    }
    let domains: Vec<String> = jadpo_db::symbols(db, project)
        .domains
        .iter()
        .cloned()
        .collect();
    let Some(domain) = call.value("--into") else {
        return Reply::failed(arguments(
            "`jadpo package recipe` needs `--into <domain>`".to_string(),
            "A recipe becomes one of the app's domains' own code, so the command names which."
                .to_string(),
            format!(
                "Run `jadpo package recipe {} {name} --into <domain>`",
                home.name
            ),
        ));
    };
    if !domains.iter().any(|known| known == domain) {
        let holds = if domains.is_empty() {
            "The app has no domain yet.".to_string()
        } else {
            format!("The app's domains are {}.", and_list(&quoted(&domains)))
        };
        return Reply::failed(match nearest(domain, domains.iter().map(String::as_str)) {
            Some(near) => arguments(
                format!("`{domain}` isn't a domain of the app"),
                format!("{holds} Did you mean `{near}`?"),
                format!("Use `--into {near}`"),
            ),
            None => arguments(
                format!("`{domain}` isn't a domain of the app"),
                holds,
                format!(
                    "Create `{}/{domain}/` first, or name one of the app's domains",
                    DOMAINS
                ),
            ),
        });
    }
    let from = recipes_folder.join(name);
    let mut files: Vec<String> = Vec::new();
    files_in(&from, "", &mut files);
    files.sort();
    let into = format!("{}/{domain}", DOMAINS);
    let targets: Vec<String> = files.iter().map(|file| format!("{into}/{file}")).collect();
    let taken: Vec<String> = targets
        .iter()
        .filter(|target| root.join(target).exists())
        .cloned()
        .collect();
    if !taken.is_empty() {
        return Reply::failed(arguments(
            format!(
                "{} already {}",
                and_list(&quoted(&taken)),
                if taken.len() == 1 { "exists" } else { "exist" }
            ),
            "A recipe becomes the app's own code, so copying it never writes over a file the app has; nothing was copied.".to_string(),
            format!(
                "Move or rename {}, or copy the recipe into another domain",
                if taken.len() == 1 { "that file" } else { "those files" }
            ),
        ));
    }
    for (file, target) in files.iter().zip(&targets) {
        let path = root.join(target);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::copy(from.join(file), &path).map(|_| ()));
        if let Err(error) = written {
            return write_failed("package recipe", &format!("`{target}`"), &error.to_string());
        }
    }
    let screen = Screen::new()
        .with(Block::Status {
            outcome: Outcome::Success,
            text: format!("Copied recipe {name} into {into}"),
            rows: targets
                .iter()
                .map(|target| (target.clone(), String::new()))
                .collect(),
        })
        .with(Block::summary(
            Outcome::Success,
            "recipe copied",
            [
                Fact::plain(package.clone()),
                Fact::plain(name.clone()),
                Fact::plain(plural(targets.len(), "file", "files")),
            ],
        ))
        .with(Block::hint("jadpo check", "shows what to fit to the app"));
    Reply::screen(
        screen,
        Json::object([
            ("package", Json::from(package.as_str())),
            ("recipe", Json::from(name.as_str())),
            ("domain", Json::from(domain)),
            (
                "files",
                Json::array(targets.iter().map(|target| Json::from(target.as_str()))),
            ),
        ]),
    )
}

/// The folders in `folder`, by name, sorted, hidden ones left out.
fn folders_in(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.'))
        .collect();
    out.sort();
    out
}

/// Every file under `folder`, as a path from it with `/`, hidden ones
/// left out.
fn files_in(folder: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if entry.path().is_dir() {
            files_in(&entry.path(), &relative, out);
        } else {
            out.push(relative);
        }
    }
}

// --- recipes compiled in the package's `sample/` app (chunk 7.6b) ----------

/// The folder a package keeps the app its recipes compile in.
const SAMPLE: &str = "sample";

/// How the package's recipes compiled in its `sample/` app, for the
/// publishing bar's `recipes` row (area 11, "What publishing requires",
/// item 4): whether each did, the row's detail, the problems found, each
/// at its place in the package's folder, and the texts they are shown
/// over.
pub(crate) struct InSample {
    pub passed: bool,
    pub detail: String,
    pub problems: Vec<jadpo_diag::Diagnostic>,
    pub sources: std::collections::BTreeMap<String, String>,
}

/// Compiles each recipe of the package in `folder`, `compiled` there,
/// copied on its own into the one domain of the package's `sample/` app
/// as `jadpo package recipe` copies it, with the package placed as the
/// sample's own integration, as `jadpo package check` places it. A
/// recipe's migrations and records are the app's to write once copied,
/// so they aren't asked for. `None` when the package ships no recipes.
pub(crate) fn in_sample(folder: &Path, compiled: &crate::package::Compiled) -> Option<InSample> {
    let recipes = folders_in(&folder.join(RECIPES));
    if recipes.is_empty() {
        return None;
    }
    let sample = folder.join(SAMPLE);
    let domains = folders_in(&sample.join(DOMAINS));
    let domain = match (
        sample.join(jadpo_db::PROJECT_FILE).is_file(),
        domains.as_slice(),
    ) {
        (true, [only]) => only.clone(),
        (false, _) => return Some(sample_missing(compiled, "no sample/ app", None)),
        (true, _) => {
            return Some(sample_missing(
                compiled,
                &format!("sample/ has {}", plural(domains.len(), "domain", "domains")),
                Some(domains.len()),
            ));
        }
    };
    let mut sample_files = Vec::new();
    files_in(&sample, "", &mut sample_files);
    let mut package_files = Vec::new();
    files_in(folder, "", &mut package_files);
    package_files.retain(|file| {
        !file.starts_with(&format!("{RECIPES}/"))
            && !file.starts_with(&format!("{SAMPLE}/"))
            && !file.starts_with("build/")
    });
    let mut out = InSample {
        passed: true,
        detail: String::new(),
        problems: Vec::new(),
        sources: std::collections::BTreeMap::new(),
    };
    let mut failing: Vec<String> = Vec::new();
    for recipe in &recipes {
        let mut recipe_files = Vec::new();
        files_in(&folder.join(RECIPES).join(recipe), "", &mut recipe_files);
        let into = format!("{DOMAINS}/{domain}");
        let Ok(scratch) = crate::package::Scratch::new() else {
            failing.push(recipe.clone());
            continue;
        };
        let copied = (|| -> std::io::Result<()> {
            let copy = |from: PathBuf, to: PathBuf| -> std::io::Result<()> {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(from, to).map(|_| ())
            };
            for file in &sample_files {
                copy(sample.join(file), scratch.0.join(file))?;
            }
            for file in &package_files {
                copy(
                    folder.join(file),
                    scratch.0.join(&compiled.mount).join(file),
                )?;
            }
            for file in &recipe_files {
                copy(
                    folder.join(RECIPES).join(recipe).join(file),
                    scratch.0.join(&into).join(file),
                )?;
            }
            Ok(())
        })();
        if copied.is_err() {
            failing.push(recipe.clone());
            continue;
        }
        let loaded = jadpo_db::load(&scratch.0);
        // Each place in the scratch app, as the package's folder names it.
        let mount = format!("{}/", compiled.mount);
        let recipe_at = format!("{into}/");
        let place = |path: &str| -> String {
            if let Some(rest) = path.strip_prefix(&mount) {
                rest.to_string()
            } else if let Some(rest) = path.strip_prefix(&recipe_at)
                && recipe_files.iter().any(|file| file == rest)
            {
                format!("{RECIPES}/{recipe}/{rest}")
            } else {
                format!("{SAMPLE}/{path}")
            }
        };
        let problems: Vec<jadpo_diag::Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
            .into_iter()
            .filter(|d| {
                d.rule.id != jadpo_diag::rule::PACKAGE_RESERVED_NAME.id
                    && !d.rule.id.starts_with("migration.")
                    && !d.rule.id.starts_with("record.")
            })
            .map(|mut d| {
                if let Some(label) = &mut d.primary {
                    label.span.path = place(&label.span.path);
                }
                for label in &mut d.secondary {
                    label.span.path = place(&label.span.path);
                }
                d
            })
            .collect();
        if problems.is_empty() {
            continue;
        }
        failing.push(recipe.clone());
        for (path, text) in jadpo_db::source_texts(&loaded.db, loaded.project) {
            out.sources.insert(place(&path), text);
        }
        // A problem of the sample's own, found with each recipe, is shown
        // once.
        for problem in problems {
            let seen = out.problems.iter().any(|known| {
                known.rule.id == problem.rule.id
                    && known.message == problem.message
                    && known.primary.as_ref().map(|label| &label.span)
                        == problem.primary.as_ref().map(|label| &label.span)
            });
            if !seen {
                out.problems.push(problem);
            }
        }
    }
    out.passed = failing.is_empty();
    out.detail = match failing.as_slice() {
        [] => format!(
            "{} in sample/",
            plural(recipes.len(), "recipe compiles", "recipes compile")
        ),
        [one] if recipes.len() == 1 => format!("{one} doesn't compile"),
        _ => format!("{} of {} don't compile", failing.len(), recipes.len()),
    };
    Some(out)
}

/// `publish.sample_missing`: the package's recipes have no sample app
/// with one domain to compile in; `domains` is how many it has, when it
/// has an app.
fn sample_missing(
    compiled: &crate::package::Compiled,
    detail: &str,
    domains: Option<usize>,
) -> InSample {
    let package = &compiled.package;
    let (message, reason) = match domains {
        None => (
            format!("`{package}`'s recipes have no sample app to compile in"),
            format!(
                "Each recipe is compiled before it is published, copied into the one domain of the app in the package's `{SAMPLE}/` folder as `jadpo package recipe` copies it into an app's, so a recipe that no longer fits the package is caught there; this package has no `{SAMPLE}/{}`.",
                jadpo_db::PROJECT_FILE
            ),
        ),
        Some(count) => (
            format!("`{package}`'s sample app has no one domain for its recipes"),
            format!(
                "Each recipe is compiled before it is published, copied into the one domain of the app in the package's `{SAMPLE}/` folder as `jadpo package recipe` copies it into an app's; `{SAMPLE}/{DOMAINS}/` holds {}.",
                plural(count, "domain", "domains")
            ),
        ),
    };
    InSample {
        passed: false,
        detail: detail.to_string(),
        problems: vec![
            jadpo_diag::Diagnostic::new(&jadpo_diag::rule::PUBLISH_SAMPLE_MISSING, message)
                .reason(reason)
                .next(format!(
                    "Write a small app in `{SAMPLE}/`, its `{}` connecting to `{package}` and one domain under `{SAMPLE}/{DOMAINS}/` that the connection lists",
                    jadpo_db::PROJECT_FILE
                )),
        ],
        sources: std::collections::BTreeMap::new(),
    }
}
