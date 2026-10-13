//! The derived files `jadpo check` writes to `build/` (areas 14 and 18):
//! the manifest and the views of it that areas 1 to 4 can fill. Each is
//! canonical, so the same source always writes the same bytes, and a file
//! whose bytes would not change is left alone.
//!
//! Written for a project only, never for loose files, and also when the
//! check finds errors, so what an agent looks up is never stale (area 14,
//! question 3); the manifest's `errors` says the graph may be incomplete.
//! The other views come with their areas: the schema, query and
//! permissions reports in `build/report/` (5, 6, 9), OpenAPI (7) and the
//! monitoring rules (16).

use std::path::Path;

use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::manifest::{Stamp, manifest};
use jadpo_graph::{Graph, catalogue};

use crate::help::VERSION;

/// Writes every derived file; the paths written, relative to the root.
pub fn write(root: &Path, graph: &Graph, errors: usize) -> Result<Vec<String>, Box<Diagnostic>> {
    let stamp = Stamp {
        compiler: VERSION.to_string(),
        errors,
    };
    let libraries = catalogue::libraries(graph);
    let failures = catalogue::failures(graph);
    let files = [
        ("build/manifest.json", manifest(graph, &stamp).to_document()),
        ("build/catalogue/libraries.md", libraries.markdown),
        (
            "build/catalogue/libraries.json",
            libraries.json.to_document(),
        ),
        ("build/catalogue/failures.md", failures.markdown),
        ("build/catalogue/failures.json", failures.json.to_document()),
    ];
    let mut written = Vec::new();
    for (path, text) in files {
        put(root, path, &text)?;
        written.push(path.to_string());
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
