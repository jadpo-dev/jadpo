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
