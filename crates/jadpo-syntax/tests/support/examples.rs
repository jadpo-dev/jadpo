//! Every Jadpo example in the documentation and `examples/`, for the parse,
//! fuzz and formatter tests. Shared by `jadpo-syntax` and `jadpo-fmt`
//! through `#[path]`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// One ```` ```jadpo ```` block, or one `.jadpo` file.
#[derive(Debug, Clone)]
pub struct Example {
    /// Repository-relative file and 1-based line of the fence.
    pub place: String,
    pub attrs: Vec<String>,
    pub source: String,
    /// Inside a `::: today` box: the archived v0 compiler's syntax.
    pub today: bool,
    /// On one of the eighteen area pages.
    pub area_page: bool,
}

impl Example {
    pub fn fragment(&self) -> bool {
        self.attrs.iter().any(|attr| attr == "fragment")
    }

    /// The decided syntax: a fragment outside a `today` box on an area page,
    /// or a `.jadpo` file under `examples/`.
    pub fn decided(&self) -> bool {
        self.area_page && self.fragment() && !self.today
    }
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            markdown_files(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("md" | "jadpo")
        ) {
            out.push(path);
        }
    }
}

pub fn all() -> Vec<Example> {
    let root = root();
    let mut files = Vec::new();
    markdown_files(&root.join("docs/language"), &mut files);
    markdown_files(&root.join("examples"), &mut files);
    let mut examples = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("readable");
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let area_page = path.parent().is_some_and(|p| p.ends_with("docs/language"))
            && file_name.len() > 3
            && file_name.as_bytes()[..2].iter().all(u8::is_ascii_digit);
        if path.extension().and_then(|e| e.to_str()) == Some("jadpo") {
            examples.push(Example {
                place: relative,
                attrs: vec!["file".into()],
                source: text,
                today: false,
                area_page: false,
            });
            continue;
        }
        let lines: Vec<&str> = text.lines().collect();
        let mut today = false;
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if line.trim() == "::: today" {
                today = true;
            } else if line.trim() == ":::" {
                today = false;
            }
            if let Some(info) = line.strip_prefix("```") {
                let mut j = i + 1;
                while j < lines.len() && !lines[j].starts_with("```") {
                    j += 1;
                }
                let mut words = info.split_whitespace();
                if words.next() == Some("jadpo") {
                    let source = lines[i + 1..j.min(lines.len())].join("\n") + "\n";
                    examples.push(Example {
                        place: format!("{relative}:{}", i + 1),
                        attrs: words.map(str::to_string).collect(),
                        source,
                        today,
                        area_page,
                    });
                }
                i = j;
            }
            i += 1;
        }
    }
    examples
}

/// The specification corpus as projects (docs/compiler-runtime.md §2.7):
/// each language page's `check` projects, named `<page> <project>`, and
/// each teaching example in `examples/`, named `examples <project>`, with
/// their files by project-relative path.
pub fn projects() -> Vec<(String, Vec<(String, String)>)> {
    use std::collections::BTreeMap;
    let mut projects: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    for example in all() {
        if !example.area_page {
            continue;
        }
        let page = example
            .place
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string();
        let n = counters.entry(page.clone()).or_default();
        *n += 1;
        if !example.attrs.iter().any(|attr| attr == "check") {
            continue;
        }
        let attr = |name: &str| {
            example
                .attrs
                .iter()
                .find_map(|attr| attr.strip_prefix(&format!("{name}=")))
                .map(str::to_string)
        };
        let project = attr("project").unwrap_or_else(|| format!("example-{n}"));
        let file = attr("file").unwrap_or_else(|| "main.jadpo".to_string());
        if !file.ends_with(".jadpo") {
            continue;
        }
        // A later block of the same file replaces it, as the page's
        // checker writes them (a project shown before and after a change).
        let files = projects.entry(format!("{page} {project}")).or_default();
        files.retain(|(path, _)| path != &file);
        files.push((file, example.source));
    }
    // The teaching examples' projects (examples/<project>/), which the
    // spec corpus holds too (tools/examples.py).
    for example in all() {
        let Some(rest) = example.place.strip_prefix("examples/") else {
            continue;
        };
        let Some((project, file)) = rest.split_once('/') else {
            continue;
        };
        if !file.ends_with(".jadpo") || project == "golden-todo" {
            continue;
        }
        projects
            .entry(format!("examples {project}"))
            .or_default()
            .push((file.to_string(), example.source));
    }
    projects.into_iter().collect()
}
