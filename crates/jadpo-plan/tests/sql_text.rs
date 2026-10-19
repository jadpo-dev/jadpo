//! All SQL text is written in `jadpo-sql` (docs/compiler-runtime.md §2.1;
//! chunk 6.3b): the plan decides what each statement does and where each
//! value comes from, and `jadpo-sql` writes it, so the SQL has one author.
//! This holds `jadpo-plan`'s own source to it: no string literal outside a
//! comment or a test holds a word or a sign only SQL text would.

use std::path::{Path, PathBuf};

/// What only SQL text holds.
const SQL: &[&str] = &[
    "SELECT",
    "INSERT",
    "UPDATE ",
    "DELETE",
    "WHERE",
    " FROM ",
    " AND ",
    " OR ",
    "NULL",
    "COLLATE",
    "CASE ",
    "EXISTS",
    "::",
    "SET LOCAL",
    "ST_",
    "coalesce(",
    "sum(",
    "count(",
    "NULLIF",
    "ARRAY[",
    "round(",
    "<=>",
    "<->",
    "<#>",
    "ORDER BY",
    "LIMIT",
    "$1",
    "$2",
    "= ANY",
    "IS NOT",
    "halfvec",
];

fn sources(folder: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).expect("the source folder") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The string literals of `code`, a line of Rust without its comment: each
/// `"..."` with its escapes, and each raw `r"..."` or `r#"..."#`.
fn literals(code: &str) -> Vec<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let raw = chars[i] == 'r' && matches!(chars.get(i + 1), Some('"' | '#'));
        if raw {
            let mut hashes = 0;
            let mut j = i + 1;
            while chars.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if chars.get(j) == Some(&'"') {
                let close: String = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let rest: String = chars[j + 1..].iter().collect();
                let end = rest.find(&close).unwrap_or(rest.len());
                out.push(rest[..end].to_string());
                i = j + 1 + rest[..end].chars().count() + close.len();
                continue;
            }
        }
        if chars[i] == '\'' {
            // A character literal, `'"'` among them.
            if let Some(end) = chars[i + 1..].iter().take(4).position(|c| *c == '\'') {
                i += end + 2;
                continue;
            }
        }
        if chars[i] == '"' {
            let mut text = String::new();
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '"' {
                if chars[j] == '\\' {
                    j += 1;
                }
                if let Some(c) = chars.get(j) {
                    text.push(*c);
                }
                j += 1;
            }
            out.push(text);
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

#[test]
fn the_plan_writes_no_sql_text() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    files.sort();
    assert!(files.len() > 10, "the plan's sources: {files:?}");
    let mut found = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("a source");
        // Tests write what they expect, SQL included.
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for (n, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for literal in literals(line) {
                if let Some(word) = SQL.iter().find(|word| literal.contains(*word)) {
                    found.push(format!(
                        "{}:{}: `{word}` in \"{literal}\"",
                        file.display(),
                        n + 1
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "SQL text belongs in `jadpo-sql`:\n{}",
        found.join("\n")
    );
    // The scan sees SQL where it is.
    assert_eq!(
        literals(r#"let sql = format!("{} IS NOT NULL", ident(c)); let q = '"';"#),
        ["{} IS NOT NULL"]
    );
}
