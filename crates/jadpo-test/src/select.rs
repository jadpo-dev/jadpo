//! What to run: the folder to load and the path, line, name, kind and last
//! failures to filter by, each with the error that says why nothing
//! matched; and the order, which a seed decides and reproduces.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use jadpo_db::Loaded;
use jadpo_diag::{Diagnostic, nearest, rule};

use crate::{Options, Ran, Selection, Test, TestKind};

/// Where to load and what the path argument names.
#[derive(Clone, Debug)]
pub(crate) struct Target {
    /// The folder to load: the project around it, or loose files.
    pub folder: PathBuf,
    /// The file or folder named, absolute.
    pub path: Option<PathBuf>,
    pub line: Option<u32>,
}

/// The path filter, relative to the root.
pub(crate) struct Filter {
    pub path: Option<String>,
    pub line: Option<u32>,
}

impl Target {
    pub fn filter(&self, loaded: &Loaded) -> Filter {
        let path = self
            .path
            .as_deref()
            .and_then(|path| loaded.relative(path))
            .filter(|path| !path.is_empty());
        Filter {
            path,
            line: self.line,
        }
    }
}

fn arguments(message: String, reason: &str, next: &str) -> Ran {
    Ran::Stopped(Box::new(
        Diagnostic::new(&rule::CLI_TEST_ARGUMENTS, message)
            .reason(reason)
            .next(next),
    ))
}

/// Splits `written` into a path that exists and an optional line,
/// trying the folder the command runs in, then the project's root, so a
/// rerun command's project-relative path works from any folder in it.
fn locate(written: &str, cwd: &Path) -> Option<(PathBuf, Option<u32>, String)> {
    let mut bases = vec![cwd.to_path_buf()];
    if let Some(root) = jadpo_db::find_root(cwd) {
        bases.push(root);
    }
    for base in &bases {
        if base.join(written).exists() {
            return Some((base.join(written), None, written.to_string()));
        }
        if let Some((path, line)) = written.rsplit_once(':')
            && !line.is_empty()
            && line.chars().all(|c| c.is_ascii_digit())
            && base.join(path).exists()
        {
            return Some((base.join(path), line.parse().ok(), path.to_string()));
        }
    }
    None
}

pub(crate) fn resolve(options: &Options) -> Result<Target, Ran> {
    let Some(written) = &options.target else {
        return match jadpo_db::find_root(&options.cwd) {
            Some(root) => Ok(Target {
                folder: root,
                path: None,
                line: None,
            }),
            None => Err(Ran::NoProject),
        };
    };
    let Some((path, line, as_written)) = locate(written, &options.cwd) else {
        return Err(arguments(
            format!("`{written}` doesn't exist"),
            "There is no file or folder at this path.",
            "Give a `.test.jadpo` file, a file and a line as `<file>:<line>`, or a folder",
        ));
    };
    if path.is_file() && path.extension().and_then(|e| e.to_str()) != Some("jadpo") {
        return Err(arguments(
            format!("`{as_written}` isn't a Jadpo file"),
            "`jadpo test` runs the tests in files whose names end in `.jadpo`.",
            "Give a `.test.jadpo` file, or a folder to run every test in it",
        ));
    }
    if line.is_some() && path.is_dir() {
        return Err(arguments(
            format!("`{written}` names a line in a folder"),
            "A line picks the test at that line of a file.",
            "Give a file and a line as `<file>:<line>`, or the folder alone",
        ));
    }
    if line == Some(0) {
        return Err(arguments(
            format!("`{written}` isn't a line"),
            "Lines are counted from 1.",
            "Give a line inside the test, counting from 1",
        ));
    }
    let folder = if path.is_dir() {
        path.clone()
    } else {
        path.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    Ok(Target {
        folder,
        path: Some(path),
        line,
    })
}

fn no_tests(message: String, reason: String, next: String) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(&rule::TEST_NO_TESTS, message)
            .reason(reason)
            .next(next),
    )
}

/// `a pure`, `an action`.
fn with_a(word: &str) -> String {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        format!("an {word}")
    } else {
        format!("a {word}")
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The tests `selection` asks for, or why there are none.
pub(crate) fn choose(
    tests: &[Test],
    selection: &Selection,
    last: Option<&BTreeSet<String>>,
    project: bool,
) -> Result<Vec<Test>, Box<Diagnostic>> {
    if tests.is_empty() {
        let whose = if project {
            "This project has no tests"
        } else {
            "These files have no tests"
        };
        return Err(no_tests(
            whose.to_string(),
            "No file holds a `test` declaration, so there is nothing to run.".to_string(),
            "Add a test beside the code it tests, in a file named `<name>.test.jadpo`".to_string(),
        ));
    }
    let all = count(tests.len(), "test", "tests");
    let mut chosen: Vec<Test> = tests.to_vec();
    if let Some(path) = &selection.path {
        let inside = format!("{path}/");
        chosen.retain(|test| test.file == *path || test.file.starts_with(&inside));
        if chosen.is_empty() {
            let mut files: Vec<&str> = tests.iter().map(|test| test.file.as_str()).collect();
            files.dedup();
            let shown: Vec<String> = files
                .iter()
                .take(3)
                .map(|file| format!("`{file}`"))
                .collect();
            let more = match files.len() {
                n if n > 3 => format!(" and {} more", count(n - 3, "file", "files")),
                _ => String::new(),
            };
            return Err(no_tests(
                format!("`{path}` has no tests"),
                format!(
                    "The project's {all} are in {}{more}.",
                    crate::screen::either_and(&shown)
                ),
                "Give a file or folder that holds tests, or leave out the path to run every test"
                    .to_string(),
            ));
        }
    }
    if let Some(line) = selection.line {
        let in_file = chosen.clone();
        chosen.retain(|test| test.line <= line && line <= test.end_line);
        if chosen.is_empty() {
            let path = selection.path.as_deref().unwrap_or_default();
            let starts: Vec<String> = in_file.iter().map(|test| test.line.to_string()).collect();
            let first = in_file.first().map_or(1, |test| test.line);
            return Err(no_tests(
                format!("No test at `{path}:{line}`"),
                format!(
                    "The tests in this file start at {} {}.",
                    if starts.len() == 1 { "line" } else { "lines" },
                    crate::screen::either_and(&starts)
                ),
                format!("Give a line inside a test, such as `jadpo test {path}:{first}`"),
            ));
        }
    }
    if let Some(name) = &selection.name {
        let wanted = name.to_lowercase();
        let before = chosen.clone();
        chosen.retain(|test| test.name.to_lowercase().contains(&wanted));
        if chosen.is_empty() {
            let near = nearest(name, before.iter().map(|test| test.name.as_str()));
            let (reason, next) = match near {
                Some(near) => (
                    format!(
                        "None of the {} contains this text. Did you mean `{near}`?",
                        count(before.len(), "test's name", "tests' names")
                    ),
                    format!("Use `--name \"{near}\"`"),
                ),
                None => (
                    format!(
                        "None of the {} contains this text.",
                        count(before.len(), "test's name", "tests' names")
                    ),
                    "Check the spelling, or leave out `--name` to run every test".to_string(),
                ),
            };
            return Err(no_tests(
                format!("No test's name contains `{name}`"),
                reason,
                next,
            ));
        }
    }
    if let Some(kind) = selection.kind {
        let before = chosen.len();
        chosen.retain(|test| test.kind == kind);
        if chosen.is_empty() {
            return Err(no_tests(
                format!("No {} tests to run", kind.word()),
                format!(
                    "Of the {} asked for, none is {} test: a test's kind comes from what it calls.",
                    count(before, "test", "tests"),
                    with_a(kind.word())
                ),
                "Leave out `--kind` to run every kind".to_string(),
            ));
        }
    }
    if let Some(last) = last {
        chosen.retain(|test| last.contains(&test.key));
        if chosen.is_empty() {
            // Only filters can leave them out: when none of them exists
            // any more, there is nothing to rerun (`Ran::NothingToRerun`).
            let given: Vec<&str> = [
                selection.path.as_ref().map(|_| "path"),
                selection.name.as_ref().map(|_| "name"),
                selection.kind.map(|_| "kind"),
            ]
            .into_iter()
            .flatten()
            .collect();
            let given: Vec<String> = given.iter().map(|word| word.to_string()).collect();
            return Err(no_tests(
                "None of the tests asked for failed last time".to_string(),
                format!(
                    "The {} that failed last time {} left out by the {} given.",
                    count(last.len(), "test", "tests"),
                    if last.len() == 1 { "is" } else { "are" },
                    crate::screen::either_and(&given)
                ),
                "Leave out `--failed` to run the tests asked for, or leave out the other filters to rerun every failure".to_string(),
            ));
        }
    }
    Ok(chosen)
}

/// `--kind route` and `--kind schedule`, which come with later areas.
pub(crate) fn kind_not_built(kind: TestKind) -> Diagnostic {
    let (kinds, reason) = match kind {
        TestKind::Route => (
            "Route tests",
            "A route test sends a request through a route (area 7), and this toolchain runs pure and action tests so far.",
        ),
        _ => (
            "Schedule tests",
            "A schedule test runs a schedule once (area 12), and this toolchain runs pure and action tests so far.",
        ),
    };
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!("{kinds} aren't built yet"),
    )
    .reason(reason)
    .next("Run `jadpo test --kind pure` or `jadpo test --kind action`")
    .help_at("13-testing.html#kinds-of-test")
}

/// A seed for a run that wasn't given one: short enough to type.
pub(crate) fn new_seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let mixed = splitmix(u64::from(nanos) ^ u64::from(std::process::id()));
    mixed % 99_999 + 1
}

fn splitmix(state: u64) -> u64 {
    let mut z = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The tests in the order `seed` gives: a Fisher-Yates shuffle of file and
/// line order, so the same tests and seed always run in the same order.
pub(crate) fn shuffle(mut tests: Vec<Test>, seed: u64) -> Vec<Test> {
    let mut state = seed;
    for i in (1..tests.len()).rev() {
        state = splitmix(state);
        let j = (state % (i as u64 + 1)) as usize;
        tests.swap(i, j);
    }
    tests
}
