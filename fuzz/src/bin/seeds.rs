//! Writes each fuzz target's seed corpus under the folder it is given
//! (`fuzz/corpus` by default): for `parse` and `check`, every `.jadpo`
//! file of the teaching examples and the golden todo, the parser's broken
//! files and each `jadpo` block of the error pages; for `check`, each
//! teaching example whole too; for `request`, each route of each app
//! `request` serves. Run it after `cargo fuzz build`, from its build
//! (docs/security.md, "Fuzzing").

use std::path::{Path, PathBuf};

use jadpo_fuzz::{check, request, root};

// This build is instrumented for coverage, whose callbacks libFuzzer
// defines, and libFuzzer needs a target: one that does nothing. Its own
// `main` is left out, since this one is defined.
libfuzzer_sys::fuzz_target!(|_data: &[u8]| {});

fn main() {
    let out = std::env::args_os()
        .nth(1)
        .map_or_else(|| root().join("fuzz/corpus"), PathBuf::from);
    let root = root();
    let mut sources = Vec::new();
    for folder in [
        "examples",
        "benchmarks/golden-todo",
        "crates/jadpo-syntax/tests/broken",
    ] {
        walk(&root.join(folder), &mut |path| {
            if path.extension().is_some_and(|e| e == "jadpo") {
                sources.push(std::fs::read_to_string(path).expect("readable"));
            }
        });
    }
    walk(&root.join("docs/errors"), &mut |path| {
        if path.extension().is_some_and(|e| e == "md") {
            let text = std::fs::read_to_string(path).expect("readable");
            sources.extend(blocks(&text));
        }
    });
    write(&out.join("parse"), &sources);
    let mut projects = sources.clone();
    for entry in std::fs::read_dir(root.join("examples")).expect("the examples") {
        let folder = entry.expect("an example").path();
        if !folder.join("app.jadpo").is_file() {
            continue;
        }
        let mut files = Vec::new();
        walk(&folder, &mut |path| {
            let relative = path
                .strip_prefix(&folder)
                .expect("inside")
                .to_string_lossy();
            if let Ok(text) = std::fs::read_to_string(path)
                && !relative.ends_with(".md")
            {
                files.push((relative.replace('\\', "/"), text));
            }
        });
        files.sort();
        projects.push(check::project_text(&files));
    }
    write(&out.join("check"), &projects);
    let programs = request::programs();
    let requests: Vec<Vec<u8>> = programs
        .iter()
        .enumerate()
        .flat_map(|(app, program)| request::seeds(app as u8, program))
        .collect();
    write(&out.join("request"), &requests);
    println!(
        "wrote {} parse, {} check and {} request seeds under {}",
        sources.len(),
        projects.len(),
        requests.len(),
        out.display()
    );
}

/// Every file under `folder`, hidden ones and `build/` left out.
fn walk(folder: &Path, visit: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with('.') || name == "build" || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

/// The ```` ```jadpo ```` blocks of a Markdown page.
fn blocks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut block: Option<String> = None;
    for line in text.lines() {
        match &mut block {
            None if line.trim_start().starts_with("```jadpo") => block = Some(String::new()),
            Some(_) if line.trim_start().starts_with("```") => out.extend(block.take()),
            Some(lines) => {
                lines.push_str(line);
                lines.push('\n');
            }
            None => {}
        }
    }
    out
}

fn write<T: AsRef<[u8]>>(folder: &Path, seeds: &[T]) {
    std::fs::create_dir_all(folder).expect("a corpus folder");
    for (n, seed) in seeds.iter().enumerate() {
        std::fs::write(folder.join(format!("seed-{n:05}")), seed).expect("a seed");
    }
}
