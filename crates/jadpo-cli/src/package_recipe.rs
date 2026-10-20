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
