//! The derived files `jadpo check` writes to `build/` (areas 14 and 18):
//! the manifest and the views of it that areas 1 to 4 can fill. Each is
//! canonical, so the same source always writes the same bytes, and a file
//! whose bytes would not change is left alone.
//!
//! Written for a project only, never for loose files, and also when the
//! check finds errors, so what an agent looks up is never stale (area 14,
//! question 3); the manifest's `errors` says the graph may be incomplete.
//! The other views come with their areas: the schema, query and
//! permissions reports in `build/report/` (5, 6, 9) and the monitoring
//! rules (16). Since chunk 4.1 each API's OpenAPI document is written to
//! `build/openapi/<api>.json` once its routes read cleanly (area 7), and
//! since chunk 4.4 the permissions report to `build/report/permissions.md`
//! and `.json` once policy reads cleanly (area 9). Since chunk 7.1b each
//! of the app's own integrations' surfaces to
//! `build/providers/<name>/<version>/surface.jadpo_record` (area 11, "The
//! surface"): an installed package carries its own, committed. Since
//! chunk 7.1c the network report to `build/report/network.md` and `.json`
//! once the connections read cleanly (area 11), and since chunk 7.2e2
//! the provider report to `build/report/providers.md` and `.json`.

use std::path::Path;

use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::manifest::{Stamp, manifest};
use jadpo_graph::{Graph, catalogue};

use crate::help::VERSION;

/// Each API's OpenAPI document, then the text its clients show for the
/// failures it answers with, one file for each supported locale
/// ([`crate::failure_text`]), all written to `build/openapi/`.
pub fn openapi(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    title: &str,
) -> Vec<(String, jadpo_graph::json::Value)> {
    let mut files = jadpo_check::routes::openapi::documents(db, project, title);
    // Only once the routes read cleanly, as the documents themselves are.
    if !files.is_empty() {
        files.extend(crate::failure_text::documents(db, project));
    }
    files
}

/// The surface of each of the app's own integrations in
/// `shared/providers/`, by the path it is written to in `build/`.
pub fn surfaces(db: &dyn jadpo_db::Db, project: jadpo_db::Project) -> Vec<(String, String)> {
    jadpo_check::surface::homes(db, project)
        .values()
        .filter(|home| !home.installed)
        .filter_map(|home| {
            let text = jadpo_check::surface::text(db, project, &home.folder, None)?;
            Some((
                format!(
                    "build/{}/{}/{}/{}",
                    jadpo_db::PROVIDERS,
                    home.name,
                    home.version,
                    jadpo_db::record::SURFACE_FILE
                ),
                text,
            ))
        })
        .collect()
}

/// The network report (area 11, question 9; chunk 7.1c), once the
/// connections read cleanly: `build/report/network.md` and its `.json`
/// twin, every host the app can reach by the connection that reaches it.
pub fn network(db: &dyn jadpo_db::Db, project: jadpo_db::Project) -> Vec<(String, String)> {
    if !jadpo_check::connections::connection_problems(db, project).is_empty()
        || !jadpo_check::config::connection_problems(db, project).is_empty()
    {
        return Vec::new();
    }
    let report = jadpo_check::connections::report(db, project);
    vec![
        ("build/report/network.md".to_string(), report.markdown),
        (
            "build/report/network.json".to_string(),
            report.json.to_document(),
        ),
    ]
}

/// The provider report (area 11, layers 4 and 5; chunk 7.2e2), once the
/// connections read cleanly and the app has an integration of its own:
/// `build/report/providers.md` and its `.json` twin, each call confirmed
/// or not and each imported type it reaches reviewed or not, for `jadpo
/// approval`.
pub fn providers(db: &dyn jadpo_db::Db, project: jadpo_db::Project) -> Vec<(String, String)> {
    if !jadpo_check::connections::connection_problems(db, project).is_empty() {
        return Vec::new();
    }
    let Some((markdown, json)) = jadpo_check::review::report(db, project) else {
        return Vec::new();
    };
    vec![
        ("build/report/providers.md".to_string(), markdown),
        (
            "build/report/providers.json".to_string(),
            json.to_document(),
        ),
    ]
}

/// What the manifest says of the check that wrote it (area 18): the
/// compiler, how many errors it found, the commit the project is at and
/// whether its tree was modified, and each deployable's target and build
/// id ([`crate::stamp`]; chunk 8.1o2).
pub fn stamp(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    root: &Path,
    graph: &Graph,
    errors: usize,
) -> Stamp {
    let lock = std::fs::read(root.join(jadpo_db::LOCK)).unwrap_or_default();
    let digest = graph.digest();
    Stamp {
        compiler: VERSION.to_string(),
        errors,
        source: Some(crate::stamp::commit(root)),
        deployables: crate::stamp::deployables(
            &jadpo_check::deployables::deployables(db, project).list,
            &digest,
            VERSION,
            &lock,
        ),
    }
}

/// Writes every derived file; the paths written, relative to the root.
pub fn write(
    root: &Path,
    graph: &Graph,
    stamp: &Stamp,
    openapi: &[(String, jadpo_graph::json::Value)],
    permissions: Option<&jadpo_check::policy::report::Report>,
    surfaces: &[(String, String)],
) -> Result<Vec<String>, Box<Diagnostic>> {
    let libraries = catalogue::libraries(graph);
    let failures = catalogue::failures(graph);
    let mut files = vec![
        (
            "build/manifest.json".to_string(),
            manifest(graph, stamp).to_document(),
        ),
        (
            "build/catalogue/libraries.md".to_string(),
            libraries.markdown,
        ),
        (
            "build/catalogue/libraries.json".to_string(),
            libraries.json.to_document(),
        ),
        ("build/catalogue/failures.md".to_string(), failures.markdown),
        (
            "build/catalogue/failures.json".to_string(),
            failures.json.to_document(),
        ),
    ];
    // One OpenAPI document per API (area 7), once its routes read cleanly.
    for (api, document) in openapi {
        files.push((format!("build/openapi/{api}.json"), document.to_document()));
    }
    // Who may do what to every entity (area 9), once policy reads cleanly.
    if let Some(report) = permissions {
        files.push((
            "build/report/permissions.md".to_string(),
            report.markdown.clone(),
        ));
        files.push((
            "build/report/permissions.json".to_string(),
            report.json.to_document(),
        ));
    }
    // The app's own integrations' surfaces and the network report (area
    // 11).
    files.extend(surfaces.iter().cloned());
    let mut written = Vec::new();
    for (path, text) in files {
        put(root, &path, &text)?;
        written.push(path);
    }
    Ok(written)
}

/// Writes `text` to `path` under `root` through a temporary file, so a
/// reader never sees half of it, unless it already holds those bytes.
fn put(root: &Path, path: &str, text: &str) -> Result<(), Box<Diagnostic>> {
    let target = root.join(path);
    if std::fs::read(&target).is_ok_and(|bytes| bytes == text.as_bytes()) {
        return Ok(());
    }
    let failed = |error: std::io::Error| {
        Box::new(
            Diagnostic::new(
                &rule::TOOLCHAIN_ARTIFACT_WRITE_FAILED,
                format!("`{path}` couldn't be written"),
            )
            .reason(format!("Writing it failed: {error}."))
            .next("Check that `build/` can be written, then run `jadpo check` again"),
        )
    };
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder).map_err(failed)?;
    }
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    let temporary = target.with_file_name(name);
    std::fs::write(&temporary, text).map_err(failed)?;
    std::fs::rename(&temporary, &target).map_err(failed)
}
