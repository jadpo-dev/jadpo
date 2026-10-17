//! The corpus of broken files in `tests/broken/` (decision register,
//! "Errors for habits from other languages", 2026-10-15): habits from
//! other languages and common slips, each broken line marked with the one
//! error it gives as `// error: rule.id`. A syntax error never brings a
//! second on its line, and no line without a mark gives one. The edits
//! make the files parse, round by round as `jadpo fmt` would apply them,
//! except where a mark says `(no edit)`. Every habit the lexer and parser
//! find is in the corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jadpo_syntax::habits::{self, Detected, HABITS, Habit};
use jadpo_syntax::{SyntaxError, parse_file};

fn corpus() -> Vec<(PathBuf, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/broken");
    let mut files: Vec<(PathBuf, String)> = std::fs::read_dir(&dir)
        .expect("tests/broken")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "jadpo"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("read");
            (path, text)
        })
        .collect();
    files.sort();
    assert!(files.len() >= 8, "the corpus is in tests/broken/");
    files
}

/// Each marked line, by number: its rule and whether it has an edit.
fn marks(text: &str) -> BTreeMap<usize, (String, bool)> {
    let mut out = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if let Some((_, mark)) = line.rsplit_once("// error: ") {
            let mark = mark.trim();
            let (rule, editable) = match mark.strip_suffix("(no edit)") {
                Some(rule) => (rule.trim(), false),
                None => (mark, true),
            };
            out.insert(number, (rule.to_string(), editable));
        }
    }
    out
}

fn line_of(text: &str, offset: u32) -> usize {
    text[..offset as usize].matches('\n').count()
}

#[test]
fn every_broken_line_gives_one_error_and_no_other_line_gives_any() {
    let mut failures = Vec::new();
    for (path, text) in corpus() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let expected = marks(&text);
        let parse = parse_file(&text);
        let mut found: BTreeMap<usize, Vec<&SyntaxError>> = BTreeMap::new();
        for error in parse.errors() {
            found
                .entry(line_of(&text, error.start))
                .or_default()
                .push(error);
        }
        for (line, errors) in &found {
            let rules: Vec<&str> = errors.iter().map(|e| e.rule.id).collect();
            match expected.get(line) {
                Some((rule, _)) if rules == [rule.as_str()] => {}
                Some((rule, _)) => failures.push(format!(
                    "{name}:{}: expected one {rule}, got {rules:?}: {:?}",
                    line + 1,
                    errors.iter().map(|e| &e.message).collect::<Vec<_>>()
                )),
                None => failures.push(format!(
                    "{name}:{}: an unmarked line gives {rules:?}: {:?}",
                    line + 1,
                    errors.iter().map(|e| &e.message).collect::<Vec<_>>()
                )),
            }
        }
        for (line, (rule, _)) in &expected {
            if !found.contains_key(line) {
                failures.push(format!("{name}:{}: expected {rule}, got nothing", line + 1));
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Applies the edits of every error that doesn't overlap an earlier one.
fn apply(text: &str, errors: &[SyntaxError]) -> String {
    let mut edits: Vec<_> = errors.iter().flat_map(|e| e.edits.clone()).collect();
    edits.sort_by_key(|edit| (edit.start, edit.end));
    let mut kept: Vec<jadpo_syntax::TextEdit> = Vec::new();
    for edit in edits {
        if kept.last().is_none_or(|last| last.end <= edit.start) {
            kept.push(edit);
        }
    }
    let mut out = text.to_string();
    for edit in kept.iter().rev() {
        out.replace_range(edit.start as usize..edit.end as usize, &edit.text);
    }
    out
}

#[test]
fn the_edits_make_the_corpus_parse() {
    let mut failures = Vec::new();
    for (path, text) in corpus() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let without_edit: Vec<String> = marks(&text)
            .into_values()
            .filter(|(_, editable)| !editable)
            .map(|(rule, _)| rule)
            .collect();
        let mut current = text.clone();
        for _ in 0..8 {
            let parse = parse_file(&current);
            if parse.errors().iter().all(|e| e.edits.is_empty()) {
                break;
            }
            current = apply(&current, parse.errors());
        }
        let left: Vec<&str> = parse_file(&current)
            .errors()
            .iter()
            .map(|e| e.rule.id)
            .collect();
        let mut allowed = without_edit.clone();
        let mut unexpected = Vec::new();
        for rule in &left {
            match allowed.iter().position(|r| r == rule) {
                Some(i) => {
                    allowed.remove(i);
                }
                None => unexpected.push(*rule),
            }
        }
        if !unexpected.is_empty() {
            failures.push(format!(
                "{name}: after the edits, still {unexpected:?}:\n{current}\n{:#?}",
                parse_file(&current)
                    .errors()
                    .iter()
                    .map(|e| (&e.message, line_of(&current, e.start) + 1))
                    .collect::<Vec<_>>()
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// What each habit the lexer and parser find looks like in the corpus: a
/// line marked with the habit's rule that contains this text.
const SAMPLES: &[(&Habit, &str)] = &[
    (&habits::STRICT_EQUAL, "==="),
    (&habits::STRICT_NOT_EQUAL, "!=="),
    (&habits::SQL_NOT_EQUAL, "<>"),
    (&habits::SINGLE_EQUAL, "owner_id = owner"),
    (&habits::SQL_IS_NULL, "IS NULL"),
    (&habits::PYTHON_IS_NONE, "is None"),
    (&habits::SYMBOL_AND, "&&"),
    (&habits::SYMBOL_OR, "||"),
    (&habits::SYMBOL_NOT, "(!"),
    (&habits::CAPITAL_LOGIC, "NOT "),
    (&habits::TERNARY, " ? "),
    (&habits::INCREMENT, "++"),
    (&habits::SEMICOLON, ";"),
    (&habits::LET, "let count"),
    (&habits::LET_MUT, "let mut"),
    (&habits::CONST, "const count"),
    (&habits::CONST_DECL, "const free_delivery_from"),
    (&habits::VAL, "val "),
    (&habits::FN, "fn zero"),
    (&habits::FUNC, "func "),
    (&habits::DEF, "def "),
    (&habits::FUN, "fun "),
    (&habits::COLON_RESULT, "): Pence"),
    (&habits::ASYNC, "async "),
    (&habits::AWAIT, "await "),
    (&habits::ELIF, "elif"),
    (&habits::FOR_OF, "for (const"),
    (&habits::FOR_COUNTING, "for (var i"),
    (&habits::SWITCH, "switch ("),
    (&habits::CASE, "case "),
    (&habits::DEFAULT, "default"),
    (&habits::TRY_CALL, "try load"),
    (&habits::TRY_BLOCK, "try {"),
    (&habits::QUESTION_MARK, ")?"),
    (&habits::THROW, "throw "),
    (&habits::FAT_ARROW_ARM, "=> 1"),
    (&habits::ARROW_FUNCTION, "item =>"),
    (&habits::NAMED_EQUALS, "reverse = true"),
    (&habits::MEMBER_EQUALS, "x = 1"),
    (&habits::MEMBER_COMMA, "price: Pence,"),
    (&habits::ASSERTION, "assert_eq!"),
    (&habits::HASH_COMMENT, "# A comment"),
    (&habits::BLOCK_COMMENT, "/*"),
    (&habits::IMPORT, "import {"),
    (&habits::PATH_COLONS, "::"),
    (&habits::EXPORT, "export "),
    (&habits::SINGLE_QUOTES, "'"),
    (&habits::NEW, "new Thing"),
    (&habits::STRUCT, "struct Line"),
    (&habits::ARRAY_TYPE, "Line[]"),
    (&habits::NULL_UNION, "| null"),
    (&habits::NULL_COALESCING, "??"),
    (&habits::OPTIONAL_CHAINING, "?."),
];

#[test]
fn every_habit_the_parser_finds_is_in_the_corpus() {
    let files = corpus();
    let mut missing = Vec::new();
    for habit in HABITS {
        if habit.detected == Detected::Checker {
            continue;
        }
        let Some((_, sample)) = SAMPLES.iter().find(|(h, _)| std::ptr::eq(*h, *habit)) else {
            missing.push(format!("{} has no sample in SAMPLES", habit.written));
            continue;
        };
        let covered = files.iter().any(|(_, text)| {
            text.lines().any(|line| {
                line.contains(sample)
                    && line
                        .rsplit_once("// error: ")
                        .is_some_and(|(_, mark)| mark.starts_with(habit.rule.id))
            })
        });
        if !covered {
            missing.push(format!(
                "{} ({}): no line marked {} contains {sample:?}",
                habit.written, habit.rule.id, habit.rule.id
            ));
        }
    }
    assert!(missing.is_empty(), "\n{}", missing.join("\n"));
}
