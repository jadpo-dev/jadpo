//! A project loaded from files in memory (docs/compiler-runtime.md §2.2,
//! "Loading"), as a base commit read from git is, gives the same graph and
//! the same problems as the project read from disk: every rule that makes
//! files a project is in `jadpo_db::assemble`, which reads nothing.

use std::path::{Path, PathBuf};

use jadpo_check::check;
use jadpo_check::graph::graph;
use jadpo_db::{FileSet, Loaded, load, load_files};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `folder`, whatever it is, as a git tree would list
/// it: the rules, not the reader, decide what the project holds.
fn every_file(folder: &Path) -> FileSet {
    let mut files = FileSet::default();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("listed").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let shown = path
                .strip_prefix(folder)
                .expect("inside")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(
                shown,
                std::fs::read_to_string(&path).map_err(|error| error.to_string()),
            );
        }
    }
    files
}

fn problems(loaded: &Loaded) -> String {
    format!("{:#?}", check(&loaded.db, loaded.project))
}

#[test]
fn every_example_loads_from_memory_to_the_graph_it_loads_to_from_disk() {
    let mut projects: Vec<PathBuf> = std::fs::read_dir(root().join("examples"))
        .expect("examples/ exists")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    projects.push(root().join("benchmarks/golden-todo"));
    projects.sort();
    for project in projects {
        let from_disk = load(&project);
        let in_memory = load_files(project.clone(), &every_file(&project));
        let shown = project.display();
        assert_eq!(
            problems(&in_memory),
            problems(&from_disk),
            "{shown}: the problems differ"
        );
        assert!(
            *graph(&in_memory.db, in_memory.project) == *graph(&from_disk.db, from_disk.project),
            "{shown}: the graphs differ"
        );
        assert_eq!(in_memory.unreadable, from_disk.unreadable, "{shown}");
        assert_eq!(in_memory.links, from_disk.links, "{shown}");
    }
}
