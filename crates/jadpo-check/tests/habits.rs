//! Habits from other languages (decision register, 2026-10-15) as the
//! whole check sees them. A habit whose Jadpo form only renames or drops
//! tokens parses as that form, so checking goes on: the project gives the
//! habit's one error and exactly the problems it gives with the habit's
//! edit made. Any other syntax error still comes alone, no problem is
//! reported on a line with a syntax error, and the habits the checker
//! finds, `string` for `Text` or `null` for `none`, are one error each
//! whose edit checks cleanly.

use std::path::{Path, PathBuf};

use jadpo_diag::Diagnostic;

/// Declarations every sample uses, and a helper with an unused variable,
/// so a sample that is checked always gives that problem.
const BASE: &str = r#"type Count = Int {
    min: 0
}

type Name = Text {}

type Flag = Bool {}

type Status = Enum {
    open
    done
}

type Pair = Object {
    left: Count
    right: Count
}

type Missing = Failure {
    kind: FailureKind.not_found
}

function found(n: Count) fails Missing -> Count {
    if (n == 0) {
        reject Missing
    }
    return n
}

function spare_helper(n: Count) -> Count {
    var spare: Count = n
    return n
}
"#;

fn project(name: &str, source: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("habits-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("folder");
    std::fs::write(root.join("main.jadpo"), format!("{BASE}\n{source}")).expect("write");
    root
}

fn check(name: &str, source: &str) -> Vec<Diagnostic> {
    let loaded = jadpo_db::load(&project(name, source));
    jadpo_check::check(&loaded.db, loaded.project)
}

/// The source with every diagnostic's edits made, offsets shifted past the
/// base declarations.
fn fixed(source: &str, diagnostics: &[Diagnostic]) -> String {
    let base = BASE.len() as u32 + 1;
    let mut edits: Vec<(u32, u32, String)> = diagnostics
        .iter()
        .filter_map(|d| d.next.as_ref())
        .flat_map(|next| next.edits.iter())
        .map(|edit| {
            (
                edit.span.start - base,
                edit.span.end - base,
                edit.replacement.clone(),
            )
        })
        .collect();
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut out = source.to_string();
    for (start, end, text) in edits {
        out.replace_range(start as usize..end as usize, &text);
    }
    out
}

fn shown(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

/// Clean habits: each sample gives its habit's error, then exactly what
/// the source gives with the habit's edit made, which is more than nothing.
const CLEAN: &[(&str, &str, &str)] = &[
    (
        "let",
        "syntax.variable_keyword",
        "function f(n: Count) -> Count {\n    let m: Count = n\n    return m\n}\n",
    ),
    (
        "const",
        "syntax.variable_keyword",
        "function f(n: Count) -> Count {\n    const m: Count = n\n    return m\n}\n",
    ),
    (
        "val",
        "syntax.variable_keyword",
        "function f(n: Count) -> Count {\n    val m: Count = n\n    return m\n}\n",
    ),
    (
        "let-mut",
        "syntax.variable_keyword",
        "function f(n: Count) -> Count {\n    let mut m: Count = n\n    m = Count(m + 1)\n    return m\n}\n",
    ),
    (
        "fn",
        "syntax.function_keyword",
        "fn f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "def",
        "syntax.function_keyword",
        "def f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "colon-result",
        "syntax.return_type_arrow",
        "function f(n: Count): Count {\n    return n\n}\n",
    ),
    (
        "strict-equal",
        "syntax.equality_operator",
        "function f(a: Count, b: Count) -> Flag {\n    return Flag(a === b)\n}\n",
    ),
    (
        "strict-not-equal",
        "syntax.equality_operator",
        "function f(a: Count, b: Count) -> Flag {\n    return Flag(a !== b)\n}\n",
    ),
    (
        "sql-not-equal",
        "syntax.equality_operator",
        "function f(a: Count, b: Count) -> Flag {\n    return Flag(a <> b)\n}\n",
    ),
    (
        "single-equal",
        "syntax.equality_operator",
        "function f(a: Count, b: Count) -> Flag {\n    if (a = b) {\n        return true\n    }\n    return false\n}\n",
    ),
    (
        "is-null",
        "syntax.equality_operator",
        "function f(a: Count?) -> Flag {\n    return Flag(a IS NULL)\n}\n",
    ),
    (
        "is-not-none",
        "syntax.equality_operator",
        "function f(a: Count?) -> Flag {\n    return Flag(a is not None)\n}\n",
    ),
    (
        "and",
        "syntax.logical_operator",
        "function f(a: Flag, b: Flag) -> Flag {\n    return Flag(a && b)\n}\n",
    ),
    (
        "or",
        "syntax.logical_operator",
        "function f(a: Flag, b: Flag) -> Flag {\n    return Flag(a||b)\n}\n",
    ),
    (
        "not",
        "syntax.logical_operator",
        "function f(a: Flag) -> Flag {\n    return Flag(!a)\n}\n",
    ),
    (
        "capital-and",
        "syntax.logical_operator",
        "function f(a: Flag, b: Flag) -> Flag {\n    return Flag(a AND b)\n}\n",
    ),
    (
        "capital-not",
        "syntax.logical_operator",
        "function f(a: Flag) -> Flag {\n    return Flag(NOT a)\n}\n",
    ),
    (
        "semicolon",
        "syntax.semicolon",
        "function f(n: Count) -> Count {\n    return n;\n}\n",
    ),
    (
        "hash",
        "syntax.comment_style",
        "# Gives back what it's given.\nfunction f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "block-comment",
        "syntax.comment_style",
        "/* Gives back what it's given. */\nfunction f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "doc-comment",
        "syntax.comment_style",
        "/**\n * Gives back what it's given.\n */\nfunction f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "export",
        "syntax.export_keyword",
        "export function f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "pub",
        "syntax.export_keyword",
        "pub fn f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "async",
        "syntax.async_await",
        "async function f(n: Count) -> Count {\n    return n\n}\n",
    ),
    (
        "await",
        "syntax.async_await",
        "function f(n: Count) -> Count {\n    return await spare_helper(n)\n}\n",
    ),
    (
        "new",
        "syntax.new_keyword",
        "function f(n: Count) -> Name {\n    return new Name(\"x\")\n}\n",
    ),
    (
        "switch",
        "syntax.switch_statement",
        "function f(s: Status) -> Count {\n    return switch (s) {\n        Status.open -> 1\n        Status.done -> 2\n    }\n}\n",
    ),
    (
        "case",
        "syntax.switch_statement",
        "function f(s: Status) -> Count {\n    return match s {\n        case Status.open -> 1\n        Status.done -> 2\n    }\n}\n",
    ),
    (
        "default",
        "syntax.switch_statement",
        "function f(n: Count) -> Count {\n    return match n {\n        0 -> 1\n        default: 2\n    }\n}\n",
    ),
    (
        "when-else",
        "syntax.switch_statement",
        "function f(n: Count) -> Count {\n    return match n {\n        0 -> 1\n        else -> 2\n    }\n}\n",
    ),
    (
        "try-call",
        "syntax.try_catch",
        "function f(n: Count) fails Missing -> Count {\n    var m: Count = try found(n)\n    return m\n}\n",
    ),
    (
        "throw",
        "syntax.unsupported_throw",
        "function f(n: Count) fails Missing -> Count {\n    if (n == 1) {\n        throw Missing\n    }\n    return n\n}\n",
    ),
    (
        "raise",
        "syntax.unsupported_throw",
        "function f(n: Count) fails Missing -> Count {\n    if (n == 1) {\n        raise Missing\n    }\n    return n\n}\n",
    ),
    (
        "fat-arrow-arm",
        "syntax.match_arrow",
        "function f(s: Status) -> Count {\n    return match s {\n        Status.open => 1\n        Status.done -> 2\n    }\n}\n",
    ),
    (
        "arrow-function",
        "syntax.inline_function_arrow",
        "function f(pairs: List<Pair>) -> List<Count> {\n    return list.map(pairs, pair => pair.left)\n}\n",
    ),
    (
        "bracketed-arrow",
        "syntax.inline_function_arrow",
        "function f(pairs: List<Pair>) -> List<Count> {\n    return list.map(pairs, (pair) => pair.left)\n}\n",
    ),
    (
        "lambda",
        "syntax.inline_function_arrow",
        "function f(pairs: List<Pair>) -> List<Count> {\n    return list.map(pairs, lambda pair: pair.left)\n}\n",
    ),
    (
        "member-equals",
        "syntax.colon_required",
        "function f(n: Count) -> Pair {\n    return Pair {\n        left = n\n        right: n\n    }\n}\n",
    ),
    (
        "member-comma",
        "syntax.member_comma",
        "function f(n: Count) -> Pair {\n    return Pair {\n        left: n,\n        right: n\n    }\n}\n",
    ),
    (
        "for-of",
        "syntax.for_loop",
        "function f(pairs: List<Pair>) -> Count {\n    var mut total: Count = 0\n    for (const pair of pairs) {\n        total = Count(total + pair.left)\n    }\n    return total\n}\n",
    ),
    (
        "foreach",
        "syntax.for_loop",
        "function f(pairs: List<Pair>) -> Count {\n    var mut total: Count = 0\n    foreach (var pair in pairs) {\n        total = Count(total + pair.left)\n    }\n    return total\n}\n",
    ),
];

#[test]
fn a_clean_habit_is_checked_as_its_jadpo_form() {
    let mut failures = Vec::new();
    for (name, rule, source) in CLEAN {
        let found = check(name, source);
        let (syntax, rest): (Vec<Diagnostic>, Vec<Diagnostic>) = found
            .iter()
            .cloned()
            .partition(|d| d.rule.id.starts_with("syntax."));
        if shown(&syntax)
            != [format!(
                "{rule}: {}",
                syntax.first().map_or("", |d| &d.message)
            )]
        {
            failures.push(format!(
                "{name}: expected one {rule}, got {:?}",
                shown(&found)
            ));
            continue;
        }
        // A line shows one habit at a time: `pub fn` takes two rounds.
        let mut edited = fixed(source, &syntax);
        let mut expected = check(&format!("{name}-fixed"), &edited);
        for round in 0..3 {
            let left: Vec<Diagnostic> = expected
                .iter()
                .filter(|d| d.rule.id.starts_with("syntax."))
                .cloned()
                .collect();
            if left.is_empty() {
                break;
            }
            edited = fixed(&edited, &left);
            expected = check(&format!("{name}-fixed-{round}"), &edited);
        }
        if shown(&expected).iter().any(|d| d.starts_with("syntax.")) {
            failures.push(format!(
                "{name}: the edit leaves {:?}:\n{edited}",
                shown(&expected)
            ));
            continue;
        }
        if shown(&rest) != shown(&expected) || expected.is_empty() {
            failures.push(format!(
                "{name}: with the habit {:?}, with its edit {:?}",
                shown(&rest),
                shown(&expected)
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn any_other_syntax_error_comes_alone() {
    // `Tag[]` doesn't parse as `List<Tag>`, so nothing else is checked.
    let found = check(
        "unclean",
        "function f(pairs: Pair[]) -> Count {\n    return 0\n}\n",
    );
    assert_eq!(
        shown(&found),
        ["syntax.collection_type: Jadpo writes `List<Pair>`, not `Pair[]`"]
    );
}

#[test]
fn nothing_else_is_reported_on_a_line_with_a_syntax_error() {
    // `m` is never used, but its line has the `let` error.
    let found = check(
        "same-line",
        "function f(n: Count) -> Count {\n    let m: Count = n\n    return n\n}\n",
    );
    assert_eq!(
        shown(&found),
        [
            "semantic.unused_variable: `spare` is never used",
            "syntax.variable_keyword: Jadpo writes `var`, not `let`",
        ]
    );
}

/// The habits the checker finds: another language's name for a type or a
/// value, when the project declares no such name.
const CHECKED: &[(&str, &str, &str)] = &[
    (
        "string",
        "type.foreign_type",
        "function f(n: Count) -> Name {\n    var t: string = \"x\"\n    return Name(t)\n}\n",
    ),
    (
        "str-field",
        "type.foreign_type",
        "type Label = Object {\n    text: str\n}\n",
    ),
    (
        "vec",
        "type.foreign_type",
        "function f(pairs: Vec<Pair>) -> Count {\n    return 0\n}\n",
    ),
    (
        "option",
        "type.foreign_type",
        "function f(n: Option<Count>) -> Count {\n    return 0\n}\n",
    ),
    (
        "optional-field",
        "type.foreign_type",
        "type Label = Object {\n    count: Optional<Count>\n}\n",
    ),
    (
        "null",
        "type.foreign_value",
        "function f(n: Count) -> Count? {\n    return null\n}\n",
    ),
    (
        "none",
        "type.foreign_value",
        "function f(n: Count) -> Count? {\n    return None\n}\n",
    ),
    (
        "true",
        "type.foreign_value",
        "function f(n: Count) -> Flag {\n    return True\n}\n",
    ),
];

#[test]
fn the_checker_finds_names_from_other_languages() {
    let mut failures = Vec::new();
    for (name, rule, source) in CHECKED {
        let found = check(name, source);
        let habits: Vec<&Diagnostic> = found.iter().filter(|d| d.rule.id == *rule).collect();
        let others: Vec<String> = shown(&found)
            .into_iter()
            .filter(|d| !d.starts_with(rule) && !d.contains("`spare`"))
            .collect();
        if habits.len() != 1 || !others.is_empty() {
            failures.push(format!(
                "{name}: expected one {rule}, got {:?}",
                shown(&found)
            ));
            continue;
        }
        let edited = fixed(source, &found);
        let after: Vec<String> = shown(&check(&format!("{name}-fixed"), &edited))
            .into_iter()
            .filter(|d| {
                !d.contains("`spare`") && !d.contains("never used") && !d.contains("isn't used")
            })
            .collect();
        if !after.is_empty() {
            failures.push(format!("{name}: the edit leaves {after:?}:\n{edited}"));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn a_declared_name_is_the_projects_own() {
    let found = check(
        "declared",
        "type String = Text {}\n\nfunction f(n: Count) -> String {\n    return \"x\"\n}\n",
    );
    assert_eq!(
        shown(&found),
        ["semantic.unused_variable: `spare` is never used"]
    );
}
