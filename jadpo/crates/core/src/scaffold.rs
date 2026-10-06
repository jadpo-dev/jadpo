use jadpo_diagnostics::Diagnostic;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaffoldFile {
    pub relative_path: &'static str,
    pub contents: String,
}

pub fn create_project(destination: &Path) -> Result<Vec<ScaffoldFile>, Diagnostic> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Diagnostic::error("JADPO_SCAFFOLD_NAME_INVALID"))?;

    if destination.exists() {
        if !destination.is_dir() {
            return Err(Diagnostic::error("JADPO_SCAFFOLD_DESTINATION_EXISTS"));
        }
        let mut entries = fs::read_dir(destination)
            .map_err(|_error| Diagnostic::error("JADPO_SCAFFOLD_READ_FAILED"))?;
        if entries
            .next()
            .transpose()
            .map_err(|_error| Diagnostic::error("JADPO_SCAFFOLD_READ_FAILED"))?
            .is_some()
        {
            return Err(Diagnostic::error("JADPO_SCAFFOLD_DESTINATION_NOT_EMPTY"));
        }
    }

    let files = scaffold_files(name);
    for file in &files {
        let path = destination.join(file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| write_error(parent, error))?;
        }
        fs::write(&path, &file.contents).map_err(|error| write_error(&path, error))?;
    }
    Ok(files)
}

fn scaffold_files(name: &str) -> Vec<ScaffoldFile> {
    vec![
        ScaffoldFile {
            relative_path: ".app/project.json",
            contents: format!(
                "{{\"schema_version\":1,\"scaffold_version\":1,\"jadpo_version\":\"{}\",\"project_name\":{},\"recipe\":{{\"command\":\"new\",\"capabilities\":[]}}}}\n",
                env!("CARGO_PKG_VERSION"),
                json_string(name)
            ),
        },
        ScaffoldFile {
            relative_path: ".gitignore",
            contents: "build/\n.local/\n.env\n.env.local\n".to_owned(),
        },
        ScaffoldFile {
            relative_path: "README.md",
            contents: format!(
                "# {name}\n\nGenerated with Jadpo scaffold version 1.\n\n```text\njadpo check .\njadpo artifacts .\njadpo build .\n```\n"
            ),
        },
        ScaffoldFile {
            relative_path: "app.jadpo",
            contents: concat!(
                "type Health = Object {\n",
                "    ready: Bool\n",
                "}\n\n",
                "route GET /health {\n",
                "    auth: none\n",
                "    output: Health\n",
                "    action: {\n",
                "        return Health {\n",
                "            ready: true\n",
                "        }\n",
                "    }\n",
                "}\n"
            )
            .to_owned(),
        },
        ScaffoldFile {
            relative_path: "tests/README.md",
            contents: "# Tests\n\nAuthored acceptance tests belong here.\n".to_owned(),
        },
    ]
}

fn write_error(_path: &Path, _error: std::io::Error) -> Diagnostic {
    Diagnostic::error("JADPO_SCAFFOLD_WRITE_FAILED")
}

fn json_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::create_project;
    use crate::{analyze_project, derive_artifacts};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn creates_a_valid_static_base() {
        let root = temporary_root("valid");
        let project = root.join("example");
        let files = create_project(&project).expect("project should be created");
        let analyzed = analyze_project(&project).expect("scaffold should be analyzable");

        assert_eq!(files.len(), 5);
        assert!(analyzed.syntax.diagnostics().next().is_none());
        assert!(analyzed.semantics.diagnostics.is_empty());
        assert!(analyzed.typing.diagnostics.is_empty());
        assert!(analyzed.failures.diagnostics.is_empty());
        assert_eq!(derive_artifacts(&project, &analyzed).len(), 16);
        let application = files
            .iter()
            .find(|file| file.relative_path == "app.jadpo")
            .expect("scaffold should include an application source");
        assert!(application.contents.contains("    action: {"));
        assert!(!application.contents.contains("    run:"));
        assert!(files.iter().all(|file| !matches!(
            file.relative_path,
            "package.json" | "bun.lock" | "bun.lockb"
        )));
        fs::remove_dir_all(root).expect("temporary root should be removable");
    }

    #[test]
    fn identical_names_produce_identical_scaffolds() {
        let root = temporary_root("deterministic");
        let first = root.join("one/example");
        let second = root.join("two/example");
        let first_files = create_project(&first).expect("first project should be created");
        let second_files = create_project(&second).expect("second project should be created");

        for file in first_files {
            let matching = second_files
                .iter()
                .find(|candidate| candidate.relative_path == file.relative_path)
                .expect("same file should exist in second scaffold");
            assert_eq!(file.contents, matching.contents, "{}", file.relative_path);
            assert_eq!(
                fs::read(first.join(file.relative_path)).expect("first file should be readable"),
                fs::read(second.join(file.relative_path)).expect("second file should be readable")
            );
        }
        fs::remove_dir_all(root).expect("temporary root should be removable");
    }

    #[test]
    fn refuses_to_overwrite_a_non_empty_directory() {
        let root = temporary_root("collision");
        let project = root.join("example");
        fs::create_dir_all(&project).expect("project should be created");
        fs::write(project.join("owned.txt"), "keep me\n").expect("owned file should be written");

        let diagnostic = create_project(&project).expect_err("scaffold must refuse overwrite");
        assert_eq!(diagnostic.code, "JADPO_SCAFFOLD_DESTINATION_NOT_EMPTY");
        assert_eq!(
            fs::read_to_string(project.join("owned.txt")).expect("owned file should remain"),
            "keep me\n"
        );
        fs::remove_dir_all(root).expect("temporary root should be removable");
    }

    fn temporary_root(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("jadpo-scaffold-{}-{suffix}", std::process::id()))
    }
}
