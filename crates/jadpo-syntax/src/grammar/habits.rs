//! Habits from other languages, as the parser meets them (`crate::habits`
//! holds the catalogue). Each is one error with its edit, and parsing
//! carries on as if the Jadpo form had been written: a word Jadpo spells
//! differently is read as Jadpo's word, and a word Jadpo doesn't have is
//! kept in the tree as `SKIPPED`, read as nothing. When that is all the
//! Jadpo form changes, the error is clean and checking goes on.

use super::*;
use crate::habits::{self as h, Habit};

/// Records the error for `habit` at `start..end`.
pub(super) fn report(
    p: &mut Parser<'_>,
    habit: &'static Habit,
    (written, jadpo): (&str, &str),
    (start, end): (u32, u32),
    edits: Vec<TextEdit>,
    clean: bool,
) {
    let mut error = SyntaxError::habit(habit, written, jadpo, start, end);
    for edit in edits {
        error = error.edit(edit);
    }
    if clean {
        error = error.clean_recovery();
    }
    p.push_error(error);
}

/// Records the error for `habit` at the current token, which Jadpo spells
/// `jadpo`, with the edit that writes it, and reads the token as `kind`.
fn rename(p: &mut Parser<'_>, habit: &'static Habit, jadpo: &str, kind: SyntaxKind) {
    let (start, end) = p.range();
    let written = p.current_text().to_string();
    report(
        p,
        habit,
        (&written, jadpo),
        (start, end),
        vec![TextEdit::replace(start, end, jadpo)],
        true,
    );
    p.bump_remap(kind);
}

/// Records the error for `habit` at the current token, which Jadpo
/// doesn't have, with the edit that removes it and the spaces after it,
/// and reads it as nothing.
fn drop_word(p: &mut Parser<'_>, habit: &'static Habit) {
    let (start, end) = p.range();
    let written = p.current_text().to_string();
    let after = p.nth_range(1).0.max(end);
    let to = if p.nth_on_new_line(1) { end } else { after };
    report(
        p,
        habit,
        (&written, ""),
        (start, end),
        vec![TextEdit::replace(start, to, "")],
        true,
    );
    p.skip_token();
}

/// The text of the source from `start` to `end`.
fn source(p: &Parser<'_>, start: u32, end: u32) -> String {
    p.text()
        .get(start as usize..end as usize)
        .unwrap_or("")
        .to_string()
}

/// The spaces that start the line `at` is on.
fn indent_at(p: &Parser<'_>, at: u32) -> String {
    let text = p.text();
    let at = (at as usize).min(text.len());
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    text[line..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// Where the line `at` is on starts, and where the next one does.
fn line_bounds(p: &Parser<'_>, at: u32) -> (u32, u32) {
    let text = p.text();
    let at = (at as usize).min(text.len());
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i + 1);
    (start as u32, end as u32)
}

/// The end of the code on the line `at` is on, before any `//` comment
/// and trailing spaces or `;`.
fn code_end(p: &Parser<'_>, at: u32) -> u32 {
    let text = p.text();
    let (_, next) = line_bounds(p, at);
    let line = &text[at as usize..next as usize];
    let code = line.split("//").next().unwrap_or("");
    let trimmed = code.trim_end_matches(|c: char| c.is_whitespace() || c == ';');
    at + trimmed.len() as u32
}

/// A `match` on `subject` with two arms, as an edit writes it at `at`.
fn match_text(p: &Parser<'_>, at: u32, subject: &str, arms: [(&str, &str); 2]) -> String {
    let indent = indent_at(p, at);
    let mut out = format!("match {subject} {{");
    for (pattern, result) in arms {
        out.push_str(&format!("\n{indent}    {pattern} -> {result}"));
    }
    out.push_str(&format!("\n{indent}}}"));
    out
}

/// A `match` the next step describes on one line: `` `match a` with the
/// arms `x -> y` and `z -> w` ``.
fn arms_summary(subject: &str, arms: [&str; 2]) -> String {
    format!(
        "`match {subject}` with the arms `{}` and `{}`",
        arms[0], arms[1]
    )
}

/// The name an arm binds a value that may be `none` to: its field's name
/// (`listing.subtitle` binds `subtitle`), or `value`.
fn binding_for(subject: &str) -> String {
    match subject.rsplit_once('.') {
        Some((_, field))
            if !field.is_empty()
                && field
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') =>
        {
            field.to_string()
        }
        _ => "value".to_string(),
    }
}

// --- declarations ---------------------------------------------------------------

/// How many words before a declaration another language puts there and
/// Jadpo doesn't have: `export`, `export default`, `pub`, `pub(crate)`,
/// `async`, all on the declaration's line.
pub(super) fn decl_prefix_len(p: &Parser<'_>) -> usize {
    let mut n = 0;
    loop {
        if p.nth(n) != IDENT || (n > 0 && p.nth_on_new_line(n)) {
            return n;
        }
        match p.nth_text(n) {
            "export" | "async" => n += 1,
            "default" if n > 0 && p.nth_text(n - 1) == "export" => n += 1,
            "pub" => {
                n += 1;
                if p.nth(n) == L_PAREN && p.nth(n + 1) == IDENT && p.nth(n + 2) == R_PAREN {
                    n += 3;
                }
            }
            _ => return n,
        }
        if p.nth_on_new_line(n) {
            return 0;
        }
    }
}

/// Reports and reads past the words [`decl_prefix_len`] counts: one error
/// a word, of which a line shows the first.
pub(super) fn decl_prefix(p: &mut Parser<'_>) {
    let n = decl_prefix_len(p);
    let mut done = 0;
    while done < n {
        match p.current_text() {
            "async" => drop_word(p, &h::ASYNC),
            "pub" if p.nth(1) == L_PAREN => {
                let (start, _) = p.range();
                let (_, close) = p.nth_range(3);
                let to = p.nth_range(4).0;
                report(
                    p,
                    &h::EXPORT,
                    (&source(p, start, close), ""),
                    (start, close),
                    vec![TextEdit::replace(start, to, "")],
                    true,
                );
                for _ in 0..4 {
                    p.skip_token();
                }
                done += 3;
            }
            "export" if p.nth_word(1, "default") => {
                let (start, _) = p.range();
                let (_, end) = p.nth_range(1);
                let to = p.nth_range(2).0;
                report(
                    p,
                    &h::EXPORT,
                    ("export default", ""),
                    (start, end),
                    vec![TextEdit::replace(start, to, "")],
                    true,
                );
                p.skip_token();
                p.skip_token();
                done += 1;
            }
            _ => drop_word(p, &h::EXPORT),
        }
        done += 1;
    }
}

/// `const name` at the start of a declaration, `n` tokens ahead: a fixed
/// value at the top of a file, which Jadpo writes `constant` (area 3).
pub(super) fn const_keyword_at(p: &Parser<'_>, n: usize) -> bool {
    p.nth(n) == IDENT
        && p.nth_text(n) == "const"
        && !p.nth_on_new_line(n + 1)
        && (p.nth(n + 1) == IDENT || p.nth(n + 1) == MUT_KW)
}

/// `const name: T = v` at the top of a file: reported, leaving `const` for
/// the constant's parser to read as `constant`.
pub(super) fn const_keyword(p: &mut Parser<'_>) {
    rename_only(p, &h::CONST_DECL, "constant");
}

/// The end of the code on the line `at` is on, before any comment.
pub(super) fn line_code_end(p: &Parser<'_>, at: u32) -> u32 {
    code_end(p, at)
}

/// `fn name(`, `func name(`, `def name(` or `fun name(`, `n` tokens ahead.
pub(super) fn function_keyword_at(p: &Parser<'_>, n: usize) -> Option<&'static Habit> {
    if p.nth(n) != IDENT
        || p.nth(n + 1) != IDENT
        || p.nth_on_new_line(n + 1)
        || !matches!(p.nth(n + 2), L_PAREN | L_ANGLE)
    {
        return None;
    }
    h::function_keyword(p.nth_text(n))
}

/// `struct Name {`, `interface Name {` or `enum Name {`, `n` tokens ahead:
/// the base Jadpo writes after `=`.
pub(super) fn type_keyword_at(p: &Parser<'_>, n: usize) -> Option<&'static str> {
    if p.nth(n) != IDENT || p.nth(n + 1) != IDENT || p.nth_on_new_line(n + 1) {
        return None;
    }
    let opens = match p.nth(n + 2) {
        L_BRACE | L_ANGLE => true,
        IDENT => matches!(p.nth_text(n + 2), "extends" | "implements"),
        _ => false,
    };
    if !opens {
        return None;
    }
    match p.nth_text(n) {
        "struct" | "interface" => Some("Object"),
        "enum" => Some("Enum"),
        _ => None,
    }
}

/// `fn name(...)`: reported, and parsed as the function it is.
pub(super) fn function_keyword(p: &mut Parser<'_>, m: crate::parser::Marker) {
    if let Some(habit) = function_keyword_at(p, 0) {
        rename_only(p, habit, "function");
    }
    callable(p, m, FUNCTION_DECL, false);
}

/// Reports the current word as `habit` with the edit that writes `jadpo`,
/// leaving the token for the caller to read.
fn rename_only(p: &mut Parser<'_>, habit: &'static Habit, jadpo: &str) {
    let (start, end) = p.range();
    let written = p.current_text().to_string();
    report(
        p,
        habit,
        (&written, jadpo),
        (start, end),
        vec![TextEdit::replace(start, end, jadpo)],
        true,
    );
}

/// `struct Point { ... }`: reported, and parsed as the type it is.
pub(super) fn type_keyword(p: &mut Parser<'_>, m: crate::parser::Marker) {
    let base = type_keyword_at(p, 0).unwrap_or("Object");
    let (start, _) = p.range();
    let word = p.current_text().to_string();
    let type_name = p.nth_text(1).to_string();
    let (_, name_end) = p.nth_range(1);
    let written = format!("{word} {type_name}");
    let jadpo = format!("type {type_name} = {base}");
    report(
        p,
        &h::STRUCT,
        (&written, &jadpo),
        (start, name_end),
        vec![TextEdit::replace(start, name_end, jadpo.clone())],
        false,
    );
    p.bump_remap(TYPE_KW);
    name(p, "this type");
    if p.at(L_BRACE) {
        member_list(p, Members::Declaration);
    } else {
        skip_line(p);
    }
    m.complete(p, TYPE_DECL);
}

/// Go's `type Point struct {`, after the name: reported, and read as
/// `type Point = Object {`.
pub(super) fn go_struct(p: &mut Parser<'_>, type_name: &str) -> bool {
    let base = match p.current_text() {
        "struct" | "interface" if p.at(IDENT) && p.nth(1) == L_BRACE && !p.on_new_line() => {
            "Object"
        }
        _ => return false,
    };
    let (start, end) = p.range();
    let word = p.current_text().to_string();
    report(
        p,
        &h::STRUCT,
        (
            &format!("type {type_name} {word}"),
            &format!("type {type_name} = {base}"),
        ),
        (start, end),
        vec![TextEdit::replace(start, end, format!("= {base}"))],
        false,
    );
    p.skip_token();
    true
}

// --- imports ------------------------------------------------------------------

/// A line in another language's import form: `import { A } from "x"`,
/// `import a.b.C`, `from a.b import C`.
pub(super) fn at_import(p: &Parser<'_>) -> bool {
    if !p.on_new_line() || p.nth_on_new_line(1) {
        return false;
    }
    if p.at_word("import") {
        return matches!(p.nth(1), L_BRACE | IDENT | STRING | STAR | L_PAREN);
    }
    if p.at_word("from") && p.nth(1) == IDENT {
        let mut i = 2;
        while !p.nth_on_new_line(i) && p.nth(i) != EOF {
            if p.nth_word(i, "import") {
                return true;
            }
            i += 1;
        }
    }
    false
}

/// The import line: one error, whose edit writes it as a `use` line when
/// its folder and names are clear, or removes it for `jadpo fmt` to add
/// the `use` lines the file needs. The line is skipped.
pub(super) fn import(p: &mut Parser<'_>) {
    let (start, _) = p.range();
    let m = p.start();
    // Go's `import (` block runs to its `)`.
    if p.at_word("import") && p.nth(1) == L_PAREN {
        p.bump_any();
    }
    loop {
        skip_balanced(p);
        if p.at_eof() || p.on_new_line() {
            break;
        }
    }
    m.complete(p, ERROR);
    let end = p.prev_end();
    let written = source(p, start, end);
    let (line_start, _) = line_bounds(p, start);
    let (_, line_next) = line_bounds(p, end.saturating_sub(1));
    let (edit, next) = match use_line(&written) {
        Some(line) => (
            TextEdit::replace(start, code_end(p, start).max(start), line.clone()),
            format!("Write `{line}`"),
        ),
        None => (
            TextEdit::replace(line_start, line_next, ""),
            "Remove this line: `jadpo fmt` adds the `use` lines a file needs".to_string(),
        ),
    };
    let word_end = start + if written.starts_with("from") { 4 } else { 6 };
    let mut error = SyntaxError::habit(&h::IMPORT, &written, &next, start, word_end);
    error.next = next;
    p.push_error(error.edit(edit));
}

/// The `use` line an import means, when its folder and names are clear.
fn use_line(written: &str) -> Option<String> {
    let line = written.trim().trim_end_matches(';').trim();
    let (path, names): (String, Vec<String>) = if let Some(rest) = line.strip_prefix("from ") {
        // Python: `from a.b import C, D as E`.
        let (module, names) = rest.split_once(" import ")?;
        let names = names
            .trim_matches(|c| c == '(' || c == ')' || c == ' ')
            .split(',')
            .map(|n| n.split(" as ").next().unwrap_or("").trim().to_string())
            .collect();
        (module.trim().replace('.', "/"), names)
    } else {
        let rest = line.strip_prefix("import ")?.trim();
        if let Some((names, from)) = rest.split_once(" from ") {
            // JavaScript: `import { A, B } from "./types"`, `import A from "x"`.
            let names = names.trim();
            let names: Vec<String> =
                if let Some(inner) = names.strip_prefix('{').and_then(|n| n.strip_suffix('}')) {
                    inner
                        .split(',')
                        .map(|n| {
                            let n = n.trim();
                            let n = n.strip_prefix("type ").unwrap_or(n);
                            n.split(" as ").next().unwrap_or("").trim().to_string()
                        })
                        .filter(|n| !n.is_empty())
                        .collect()
                } else if names.contains('*') || names.contains(',') {
                    return None;
                } else {
                    vec![names.to_string()]
                };
            let from = from
                .trim()
                .trim_matches(|c| c == '"' || c == '\'' || c == '`');
            (folder(from)?, names)
        } else {
            // Java and Kotlin: `import a.b.C`.
            let (module, last) = rest.rsplit_once('.')?;
            if !last.starts_with(|c: char| c.is_ascii_uppercase()) {
                return None;
            }
            (module.replace('.', "/"), vec![last.to_string()])
        }
    };
    let valid_name =
        |n: &String| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let valid_path = !path.is_empty()
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment.starts_with(|c: char| c.is_ascii_lowercase())
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
    if names.is_empty() || !names.iter().all(valid_name) || !valid_path {
        return None;
    }
    Some(format!("use {path} {{ {} }}", names.join(", ")))
}

/// A JavaScript module path as a folder: `./shared/types.ts` is
/// `shared/types`.
fn folder(path: &str) -> Option<String> {
    let mut path = path;
    loop {
        let before = path;
        for prefix in ["./", "../", "@/", "~/"] {
            path = path.strip_prefix(prefix).unwrap_or(path);
        }
        if path == before {
            break;
        }
    }
    for extension in [".jadpo", ".ts", ".tsx", ".js", ".mjs"] {
        path = path.strip_suffix(extension).unwrap_or(path);
    }
    (!path.is_empty()).then(|| path.to_string())
}

/// Rust's `use a::b::{C, D};`: after the first segment, at `::`. One error
/// whose edit writes the line as `use a/b { C, D }`; the path is read on.
pub(super) fn path_colons(p: &mut Parser<'_>, path_start: u32) {
    let (start, _) = p.range();
    let (_, end) = p.nth_range(1);
    let line_end = code_end(p, path_start);
    let written = source(p, path_start, line_end);
    let mut jadpo = written.replace("::{", " {").replace("::", "/");
    if let Some((path, names)) = jadpo.split_once('{') {
        let names = names.trim_end_matches('}').trim();
        jadpo = format!("{} {{ {names} }}", path.trim_end());
    } else if let Some((path, last)) = jadpo.rsplit_once('/')
        && last.starts_with(|c: char| c.is_ascii_uppercase())
    {
        jadpo = format!("{path} {{ {last} }}");
    }
    let next = format!("use {jadpo}");
    report(
        p,
        &h::PATH_COLONS,
        ("::", &next),
        (start, end),
        vec![TextEdit::replace(path_start, line_end, jadpo)],
        false,
    );
}

// --- statements ---------------------------------------------------------------

/// `let x: T = v`, `let mut x`, `const x`, `val x`: one error, then the
/// `var` statement it is.
pub(super) fn at_variable_keyword(p: &Parser<'_>) -> bool {
    p.at(IDENT)
        && h::variable_keyword(p.current_text()).is_some()
        && !p.nth_on_new_line(1)
        && (p.nth(1) == IDENT || p.nth(1).is_keyword())
}

pub(super) fn variable_keyword(p: &mut Parser<'_>) {
    let word = p.current_text();
    let habit = if word == "let" && p.nth_text(1) == "mut" {
        &h::LET_MUT
    } else {
        h::variable_keyword(word).unwrap_or(&h::LET)
    };
    let (start, end) = p.range();
    let (written, jadpo) = if std::ptr::eq(habit, &h::LET_MUT) {
        ("let mut", "var mut")
    } else {
        (word, "var")
    };
    report(
        p,
        habit,
        (written, jadpo),
        (start, end),
        vec![TextEdit::replace(start, end, "var")],
        true,
    );
    var_stmt(p);
}

/// `throw X` or `raise X`: one error, read as `reject X`. `new` after it
/// goes, and so do empty brackets: `throw new NotFound()` is `reject
/// NotFound`.
pub(super) fn throw(p: &mut Parser<'_>) {
    let (start, end) = p.range();
    let word = p.current_text().to_string();
    let mut edits = vec![TextEdit::replace(start, end, "reject")];
    // `reject` needs the failure it raises; `throw` with nothing after it
    // has none to keep.
    let mut clean = p.nth(1) != R_BRACE && p.nth(1) != EOF;
    let new_at = 1;
    if p.nth_word(new_at, "new") && !p.nth_on_new_line(new_at) {
        let (new_start, _) = p.nth_range(new_at);
        let (next_start, _) = p.nth_range(new_at + 1);
        edits.push(TextEdit::replace(new_start, next_start, ""));
    }
    let name_at = if p.nth_word(1, "new") { 2 } else { 1 };
    if p.nth(name_at) == IDENT && p.nth(name_at + 1) == L_PAREN && p.nth(name_at + 2) == R_PAREN {
        let (open, _) = p.nth_range(name_at + 1);
        let (_, close) = p.nth_range(name_at + 2);
        edits.push(TextEdit::replace(open, close, ""));
        clean = false;
    }
    report(p, &h::THROW, (&word, "reject"), (start, end), edits, clean);
    let m = p.start();
    let r = p.start();
    p.bump_remap(REJECT_KW);
    if p.at_word("new") && !p.on_new_line() {
        p.skip_token();
    }
    if !(p.at_eof() || p.at(R_BRACE) || p.on_new_line()) {
        expr(p);
    }
    r.complete(p, REJECT_EXPR);
    m.complete(p, EXPR_STMT);
}

/// `try {` or Python's `try:`: one error, without an edit, since what to
/// do with each failure is the author's to decide. The block's statements
/// are read as the enclosing block's, and `catch`, `except` and `finally`
/// after it are one error each, with their blocks read the same way.
pub(super) fn at_try_block(p: &Parser<'_>) -> bool {
    p.at_word("try") && !p.nth_on_new_line(1) && matches!(p.nth(1), L_BRACE | COLON)
}

pub(super) fn try_block(p: &mut Parser<'_>) {
    let (start, end) = p.range();
    report(
        p,
        &h::TRY_BLOCK,
        ("try", ""),
        (start, end),
        Vec::new(),
        false,
    );
    p.skip_token();
    if p.at(COLON) {
        skip_line(p);
        return;
    }
    inline_block(p);
    while (p.at_word("catch") || p.at_word("except") || p.at_word("finally")) && !p.at_eof() {
        let (start, end) = p.range();
        let word = p.current_text().to_string();
        report(
            p,
            &h::TRY_BLOCK,
            (&word, ""),
            (start, end),
            Vec::new(),
            false,
        );
        let m = p.start();
        p.skip_token();
        while !p.at(L_BRACE) && !p.at_eof() && !p.on_new_line() {
            skip_balanced(p);
        }
        m.complete(p, ERROR);
        if p.at(L_BRACE) {
            inline_block(p);
        } else {
            break;
        }
    }
}

/// A `{ ... }` whose statements belong to the enclosing block: its braces
/// are read as nothing.
fn inline_block(p: &mut Parser<'_>) {
    if !p.at(L_BRACE) {
        return;
    }
    p.skip_token();
    while !p.at(R_BRACE) && !p.at_eof() {
        if ends_block(p) {
            return;
        }
        let before = p.position();
        block_item(p);
        if p.position() == before {
            p.bump_error();
        }
    }
    if p.at(R_BRACE) {
        p.skip_token();
    }
}

/// Swift's `try call()`: one error, then the `attempt` it is.
pub(super) fn at_try_call(p: &Parser<'_>) -> bool {
    p.at_word("try") && !p.nth_on_new_line(1) && matches!(p.nth(1), IDENT | L_PAREN)
}

pub(super) fn try_call(p: &mut Parser<'_>) {
    rename_only(p, &h::TRY_CALL, "attempt");
}

/// `count++`, `count--`, `count += n`: after the statement's first value,
/// at the operator. One error whose edit writes `count = count + n`.
pub(super) fn at_increment(p: &Parser<'_>) -> bool {
    !p.on_new_line()
        && matches!(p.current(), PLUS | MINUS | STAR | SLASH)
        && p.nth_adjacent(1)
        && (p.nth(1) == EQ || (p.nth(1) == p.current() && matches!(p.current(), PLUS | MINUS)))
}

pub(super) fn increment(p: &mut Parser<'_>, lhs_start: u32) {
    let target = source(p, lhs_start, p.prev_end());
    let (op_start, _) = p.range();
    let op = match p.current() {
        PLUS => "+",
        MINUS => "-",
        STAR => "*",
        _ => "/",
    };
    let compound = p.nth(1) == EQ;
    let e = p.start();
    p.bump_any();
    p.bump_any();
    e.complete(p, ERROR);
    let op_end = p.prev_end();
    let (amount, end) = if compound {
        let value_start = p.range().0;
        expr(p);
        let value_end = p.prev_end().max(value_start);
        let value = source(p, value_start, value_end);
        let simple = !value.contains([' ', '+', '-', '*', '/', '%']);
        (if simple { value } else { format!("({value})") }, value_end)
    } else {
        ("1".to_string(), op_end)
    };
    let written = source(p, lhs_start, end);
    let jadpo = format!("{target} = {target} {op} {amount}");
    report(
        p,
        &h::INCREMENT,
        (&written, &jadpo),
        (op_start, op_end),
        vec![TextEdit::replace(lhs_start, end, jadpo.clone())],
        false,
    );
}

/// An assertion in another test framework's form, starting a statement:
/// `assert_eq!(a, b)`, `assertEquals(a, b)`, `self.assertEqual(a, b)`,
/// `expect(a).toBe(b)`, `assert.equal(a, b)`. One error whose edit writes
/// `assert ...`; the statement is skipped.
pub(super) fn assertion(p: &mut Parser<'_>) -> bool {
    let Some((jadpo, end)) = assertion_form(p) else {
        return false;
    };
    let (start, _) = p.range();
    let written = source(p, start, end);
    // The lexer's `!` in `assert_eq!(` is part of this one.
    let mut error = SyntaxError::habit(&h::ASSERTION, &written, &jadpo, start, end)
        .edit(TextEdit::replace(start, end, jadpo.clone()));
    error.message = h::ASSERTION.message(&short(&written), &short(&jadpo));
    p.replace_errors(start, end, error);
    skip_statement(p);
    true
}

fn short(text: &str) -> String {
    if text.chars().count() <= 40 {
        text.to_string()
    } else {
        let cut: String = text.chars().take(37).collect();
        format!("{cut}...")
    }
}

/// The Jadpo assertion an assertion statement means, and where it ends.
fn assertion_form(p: &Parser<'_>) -> Option<(String, u32)> {
    let text = p.text();
    let word = p.current_text();
    // `assert_eq!(`, `assert!(`: a macro call in Rust.
    let macro_call = matches!(p.current(), IDENT | ASSERT_KW)
        && p.nth(1) == ERROR
        && p.nth_text(1) == "!"
        && p.nth_adjacent(1)
        && p.nth(2) == L_PAREN;
    if macro_call {
        let (open, _) = p.nth_range(2);
        let (args, end) = split_args(text, open)?;
        let form = match (word, args.as_slice()) {
            ("assert_eq", [a, b, ..]) => format!("assert {a} == {b}"),
            ("assert_ne", [a, b, ..]) => format!("assert {a} != {b}"),
            ("assert", [c, ..]) => format!("assert {c}"),
            _ => return None,
        };
        return Some((form, end));
    }
    // `assert.equal(a, b)`: Node's assert module.
    if p.at(ASSERT_KW) && p.nth(1) == DOT && p.nth_adjacent(1) && p.nth(3) == L_PAREN {
        let (open, _) = p.nth_range(3);
        let (args, end) = split_args(text, open)?;
        let form = match (p.nth_text(2), args.as_slice()) {
            ("equal" | "strictEqual" | "deepEqual" | "deepStrictEqual", [a, b, ..]) => {
                format!("assert {a} == {b}")
            }
            ("notEqual" | "notStrictEqual" | "notDeepEqual", [a, b, ..]) => {
                format!("assert {a} != {b}")
            }
            ("ok", [c, ..]) => format!("assert {c}"),
            _ => return None,
        };
        return Some((form, end));
    }
    // `expect(a).toBe(b)`: Jest and Vitest.
    if p.at_word("expect") && p.nth(1) == L_PAREN {
        let (open, _) = p.nth_range(1);
        let (args, after) = split_args(text, open)?;
        let rest = &text[after as usize..];
        let negated = rest.starts_with(".not.");
        let rest = rest.strip_prefix(".not").unwrap_or(rest);
        let method = rest.strip_prefix('.')?;
        let name_len = method
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(method.len());
        let (name, after_name) = method.split_at(name_len);
        if !matches!(name, "toBe" | "toEqual" | "toStrictEqual") || !after_name.starts_with('(') {
            return None;
        }
        let open2 = (text.len() - after_name.len()) as u32;
        let (expected, end) = split_args(text, open2)?;
        let (Some(a), Some(b)) = (args.first(), expected.first()) else {
            return None;
        };
        let op = if negated { "!=" } else { "==" };
        return Some((format!("assert {a} {op} {b}"), end));
    }
    // `assertEquals(...)`, `self.assertEqual(...)`, `Assertions.assertEquals(...)`.
    let (call, open_at) =
        if p.at(IDENT) && p.nth(1) == DOT && p.nth(2) == IDENT && p.nth(3) == L_PAREN {
            (p.nth_text(2), 3)
        } else if p.at(IDENT) && p.nth(1) == L_PAREN {
            (word, 1)
        } else {
            return None;
        };
    let (open, _) = p.nth_range(open_at);
    let form = |args: &[String]| -> Option<String> {
        let form = match (call, args) {
            // JUnit puts the expected value first.
            ("assertEquals", [expected, actual, ..]) => format!("assert {actual} == {expected}"),
            ("assertNotEquals", [expected, actual, ..]) => {
                format!("assert {actual} != {expected}")
            }
            ("assertEqual", [a, b, ..]) => format!("assert {a} == {b}"),
            ("assertNotEqual", [a, b, ..]) => format!("assert {a} != {b}"),
            ("assertTrue", [c, ..]) => format!("assert {c}"),
            ("assertFalse", [c, ..]) => format!("assert not {c}"),
            _ => return None,
        };
        Some(form)
    };
    let (args, end) = split_args(text, open)?;
    Some((form(&args)?, end))
}

/// The arguments of the call whose `(` is at `open`, split at top-level
/// commas, and the end of its `)`.
fn split_args(text: &str, open: u32) -> Option<(Vec<String>, u32)> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut args = Vec::new();
    let mut from = open as usize + 1;
    let mut i = open as usize;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_string {
            if b == b'\\' {
                i += 1;
            } else if b == q {
                in_string = None;
            }
        } else {
            match b {
                b'"' | b'\'' | b'`' => in_string = Some(b),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let last = text[from..i].trim();
                        if !last.is_empty() {
                            args.push(last.to_string());
                        }
                        return Some((args, i as u32 + 1));
                    }
                }
                b',' if depth == 1 => {
                    args.push(text[from..i].trim().to_string());
                    from = i + 1;
                }
                b'\n' if depth == 0 => return None,
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// `for (const item of items) {`, `for (item in items)`, `for (Tag item :
/// items)`, `foreach (var item in items)`: after `for` or `foreach`, at
/// `(`. One error whose edit writes `for item in items`, and the loop is
/// read as that. A counting loop, `for (i = 0; i < n; i++)`, has no Jadpo
/// form to write: one error, and its brackets are skipped.
pub(super) fn for_brackets(p: &mut Parser<'_>, for_start: u32) {
    let open = p.range();
    let shape = for_shape(p);
    let Some((name_at, op_at, close_at)) = shape else {
        report(
            p,
            &h::FOR_COUNTING,
            ("for (;;)", ""),
            open,
            Vec::new(),
            false,
        );
        let e = p.start();
        skip_balanced(p);
        e.complete(p, ERROR);
        return;
    };
    let item = p.nth_text(name_at).to_string();
    let (list_start, _) = p.nth_range(op_at + 1);
    let (close, close_end) = p.nth_range(close_at);
    let list = source(p, list_start, close).trim().to_string();
    let written = source(p, for_start, close_end);
    let jadpo = format!("for {item} in {list}");
    report(
        p,
        &h::FOR_OF,
        (&written, &jadpo),
        (open.0, close_end),
        vec![TextEdit::replace(for_start, close_end, jadpo.clone())],
        true,
    );
    // `(`, and any `const`, `let`, `var` or type before the name.
    for _ in 0..name_at {
        p.skip_token();
    }
    name(p, "the loop's variable");
    p.bump_remap(IN_KW);
    p.with_newlines(true, |p| p.with_records(false, |p| expr(p)));
    if p.at(R_PAREN) {
        p.skip_token();
    }
}

/// Where the name, the `of`/`in`/`:` and the `)` are in a `for (...)`
/// that walks a list.
fn for_shape(p: &Parser<'_>) -> Option<(usize, usize, usize)> {
    let mut i = 1;
    // `const`, `let`, `var`, `val`, `final`.
    if p.nth(i) == VAR_KW
        || (p.nth(i) == IDENT && matches!(p.nth_text(i), "const" | "let" | "val" | "final"))
    {
        i += 1;
    }
    // Java's or C#'s type before the name: `Tag item`.
    if p.nth(i) == IDENT && p.nth(i + 1) == IDENT && !matches!(p.nth_text(i + 1), "of" | "in") {
        i += 1;
    }
    if p.nth(i) != IDENT {
        return None;
    }
    let name_at = i;
    let op_at = i + 1;
    let op = p.nth(op_at);
    if !(op == IN_KW || op == COLON || (op == IDENT && p.nth_text(op_at) == "of")) {
        return None;
    }
    let mut depth = 0;
    let mut j = op_at + 1;
    loop {
        match p.nth(j) {
            L_PAREN | L_BRACK => depth += 1,
            R_BRACK => depth -= 1,
            R_PAREN if depth == 0 => return (j > op_at + 1).then_some((name_at, op_at, j)),
            R_PAREN => depth -= 1,
            EOF | L_BRACE => return None,
            _ => {}
        }
        j += 1;
    }
}

/// `} elif (c) {`: one error whose edit writes `else if`, and the branch
/// is read as the `else if` it is.
pub(super) fn at_else_if(p: &Parser<'_>) -> bool {
    p.at(IDENT) && h::is_else_if(p.current_text()) && !p.nth_on_new_line(1)
}

pub(super) fn else_if(p: &mut Parser<'_>) {
    let branch = p.start();
    let (start, end) = p.range();
    let word = p.current_text().to_string();
    report(
        p,
        &h::ELIF,
        (&word, "else if"),
        (start, end),
        vec![TextEdit::replace(start, end, "else if")],
        false,
    );
    p.bump_remap(ELSE_KW);
    condition(p);
    if_block(p);
    branch.complete(p, ELSE_IF);
}

// --- expressions ----------------------------------------------------------------

/// `switch (x) {`, `switch x {` or Kotlin's `when (x) {`, with the `{` on
/// the same line, so a function called `switch` is never mistaken.
pub(super) fn at_switch(p: &Parser<'_>) -> bool {
    let kotlin = p.at_word("when");
    if !(p.at_word("switch") || kotlin) || p.nth_on_new_line(1) || p.nth(1) == L_BRACE {
        return false;
    }
    if kotlin && p.nth(1) != L_PAREN {
        return false;
    }
    let mut i = 1;
    let mut depth = 0;
    while !p.nth_on_new_line(i) && p.nth(i) != EOF {
        match p.nth(i) {
            L_PAREN | L_BRACK => depth += 1,
            R_PAREN | R_BRACK => depth -= 1,
            L_BRACE if depth == 0 => return i > 1 && p.nth_on_new_line(i + 1),
            _ => {}
        }
        i += 1;
    }
    false
}

pub(super) fn switch(p: &mut Parser<'_>) {
    rename_only(p, &h::SWITCH, "match");
}

/// `await x`: one error, and `x` is read alone.
pub(super) fn at_await(p: &Parser<'_>) -> bool {
    p.at_word("await") && !p.nth_on_new_line(1) && starts_operand(p, 1)
}

pub(super) fn await_word(p: &mut Parser<'_>) {
    drop_word(p, &h::AWAIT);
}

/// `new Type(...)`: one error, and `Type(...)` is read alone.
pub(super) fn at_new(p: &Parser<'_>) -> bool {
    p.at_word("new")
        && !p.nth_on_new_line(1)
        && p.nth(1) == IDENT
        && p.nth_text(1).starts_with(|c: char| c.is_ascii_uppercase())
}

pub(super) fn new_word(p: &mut Parser<'_>) {
    drop_word(p, &h::NEW);
}

/// SQL's `NOT x`: one error, read as `not x`.
pub(super) fn at_capital_not(p: &Parser<'_>) -> bool {
    p.at_word("NOT") && !p.nth_on_new_line(1) && starts_operand(p, 1)
}

pub(super) fn capital_not(p: &mut Parser<'_>) {
    rename(p, &h::CAPITAL_LOGIC, "not", NOT_KW);
}

/// Whether the token `n` ahead can start an operand.
fn starts_operand(p: &Parser<'_>, n: usize) -> bool {
    matches!(
        p.nth(n),
        IDENT
            | INT_NUMBER
            | DECIMAL_NUMBER
            | UNIT_NUMBER
            | STRING
            | TRUE_KW
            | FALSE_KW
            | NONE_KW
            | L_PAREN
            | L_BRACK
            | NOT_KW
            | MINUS
    )
}

/// An infix habit at the current token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InfixHabit {
    /// SQL's `AND` and `OR`, read as `and` and `or`.
    Capital(SyntaxKind),
    /// `=` comparing, read as `==`.
    SingleEquals,
    /// `IS NULL`, `is not None`, read as `== none` and `!= none`.
    Is,
    /// `a ?? b` or Kotlin's `a ?: b`.
    Coalesce,
    /// `c ? a : b`.
    Ternary,
}

/// The infix habit here, with its binding power, if there is one.
pub(super) fn infix_habit(p: &Parser<'_>) -> Option<(u8, InfixHabit)> {
    match p.current() {
        IDENT if p.at_word("OR") => Some((1, InfixHabit::Capital(OR_KW))),
        IDENT if p.at_word("AND") => Some((2, InfixHabit::Capital(AND_KW))),
        EQ if !p.assigning && !p.nth_on_new_line(1) => Some((3, InfixHabit::SingleEquals)),
        IDENT if (p.at_word("is") || p.at_word("IS")) && null_test_len(p).is_some() => {
            Some((3, InfixHabit::Is))
        }
        QUESTION if p.nth_adjacent(1) && matches!(p.nth(1), QUESTION | COLON) => {
            Some((1, InfixHabit::Coalesce))
        }
        QUESTION if ternary_ahead(p) => Some((1, InfixHabit::Ternary)),
        _ => None,
    }
}

/// How many tokens `IS NULL`, `IS NOT NULL`, `is None` or `is not None`
/// takes.
fn null_test_len(p: &Parser<'_>) -> Option<usize> {
    let mut n = 1;
    if (p.nth_word(n, "not") || p.nth(n) == NOT_KW || p.nth_word(n, "NOT")) && !p.nth_on_new_line(n)
    {
        n += 1;
    }
    let null = (p.nth(n) == IDENT && h::is_null_word(p.nth_text(n))) || p.nth(n) == NONE_KW;
    (null && !p.nth_on_new_line(n)).then_some(n + 1)
}

/// A `?` followed on its line by a value and then a `:`: a ternary.
pub(super) fn ternary_ahead(p: &Parser<'_>) -> bool {
    if p.nth_on_new_line(1) || !starts_operand(p, 1) {
        return false;
    }
    let mut i = 1;
    let mut depth = 0;
    while !p.nth_on_new_line(i) && p.nth(i) != EOF {
        match p.nth(i) {
            L_PAREN | L_BRACK | L_BRACE => depth += 1,
            R_PAREN | R_BRACK | R_BRACE if depth == 0 => return false,
            R_PAREN | R_BRACK | R_BRACE => depth -= 1,
            COLON if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// Parses the habit at the current token after `lhs`, which starts at
/// `start`; the binary expression it makes.
pub(super) fn infix(
    p: &mut Parser<'_>,
    lhs: CompletedMarker,
    start: u32,
    power: u8,
    habit: InfixHabit,
) -> CompletedMarker {
    match habit {
        InfixHabit::Capital(kind) => {
            let jadpo = if kind == AND_KW { "and" } else { "or" };
            let m = lhs.precede(p);
            rename(p, &h::CAPITAL_LOGIC, jadpo, kind);
            expr_bp(p, power);
            m.complete(p, BIN_EXPR)
        }
        InfixHabit::SingleEquals => {
            let m = lhs.precede(p);
            rename(p, &h::SINGLE_EQUAL, "==", EQ2);
            expr_bp(p, power);
            m.complete(p, BIN_EXPR)
        }
        InfixHabit::Is => {
            let len = null_test_len(p).unwrap_or(2);
            let (is_start, _) = p.range();
            let (_, is_end) = p.nth_range(len - 1);
            let written = source(p, is_start, is_end);
            let negated = len == 3;
            let jadpo = if negated { "!= none" } else { "== none" };
            let habit = if p.at_word("IS") {
                &h::SQL_IS_NULL
            } else {
                &h::PYTHON_IS_NONE
            };
            report(
                p,
                habit,
                (&written, jadpo),
                (is_start, is_end),
                vec![TextEdit::replace(is_start, is_end, jadpo)],
                true,
            );
            let m = lhs.precede(p);
            p.bump_remap(if negated { NEQ } else { EQ2 });
            if negated {
                p.skip_token();
            }
            let literal = p.start();
            p.bump_remap(NONE_KW);
            literal.complete(p, LITERAL);
            m.complete(p, BIN_EXPR)
        }
        InfixHabit::Coalesce => {
            let subject = source(p, start, p.prev_end());
            let (op_start, _) = p.range();
            let (_, op_end) = p.nth_range(1);
            let written = source(p, op_start, op_end);
            let m = lhs.precede(p);
            p.skip_token();
            p.skip_token();
            let rhs_start = p.range().0;
            expr_bp(p, power);
            let end = p.prev_end().max(rhs_start);
            let fallback = source(p, rhs_start, end);
            let bind = binding_for(&subject);
            let jadpo = match_text(
                p,
                start,
                &subject,
                [(&format!("some({bind})"), &bind), ("none", &fallback)],
            );
            let summary = arms_summary(
                &subject,
                [
                    &format!("some({bind}) -> {bind}"),
                    &format!("none -> {fallback}"),
                ],
            );
            report(
                p,
                &h::NULL_COALESCING,
                (&written, &summary),
                (op_start, op_end),
                vec![TextEdit::replace(start, end, jadpo.clone())],
                false,
            );
            m.complete(p, BIN_EXPR)
        }
        InfixHabit::Ternary => {
            let subject = source(p, start, p.prev_end());
            let (op_start, op_end) = p.range();
            let m = lhs.precede(p);
            p.skip_token();
            let a_start = p.range().0;
            expr_bp(p, power);
            let a = source(p, a_start, p.prev_end().max(a_start));
            if p.at(COLON) {
                p.skip_token();
            }
            let b_start = p.range().0;
            expr_bp(p, 0);
            let end = p.prev_end().max(b_start);
            let b = source(p, b_start, end);
            // A condition reads best in brackets, as an `if`'s is.
            let subject = if subject.contains(' ') {
                format!("({subject})")
            } else {
                subject
            };
            let jadpo = match_text(p, start, &subject, [("true", &a), ("false", &b)]);
            let summary = arms_summary(
                &subject,
                [&format!("true -> {a}"), &format!("false -> {b}")],
            );
            report(
                p,
                &h::TERNARY,
                ("? :", &summary),
                (op_start, op_end),
                vec![TextEdit::replace(start, end, jadpo.clone())],
                false,
            );
            m.complete(p, BIN_EXPR)
        }
    }
}

/// Postfix habits, after a value that starts at `start`: `?.`, Rust's `?`
/// after a call, `Tag[]`, `list[Tag]` and `Tag | null`. True when one was
/// read.
pub(super) fn postfix_habit(
    p: &mut Parser<'_>,
    lhs: &mut CompletedMarker,
    start: u32,
    chain: &mut Option<u32>,
) -> bool {
    match p.current() {
        // `?.`, or `? .` with spaces between, which would format to `?.`.
        QUESTION if p.nth(1) == DOT && !p.nth_on_new_line(1) => {
            chain.get_or_insert(p.range().0);
            p.skip_token();
            true
        }
        QUESTION
            if lhs.kind() == CALL_EXPR
                && (p.nth_on_new_line(1)
                    || matches!(p.nth(1), R_PAREN | COMMA | R_BRACE | EOF)) =>
        {
            let (q, q_end) = p.range();
            let call = source(p, start, q);
            let whole = p.stmt_value_start == Some(start);
            let jadpo = format!("attempt {call}");
            let edits = if whole {
                vec![
                    TextEdit::insert(start, "attempt "),
                    TextEdit::replace(q, q_end, ""),
                ]
            } else {
                Vec::new()
            };
            let mut error = SyntaxError::habit(&h::QUESTION_MARK, "?", &jadpo, q, q_end);
            if !whole {
                error.next =
                    "Move the call to a line of its own and write `attempt` before it".into();
            }
            for edit in edits {
                error = error.edit(edit);
            }
            p.push_error(error);
            p.skip_token();
            true
        }
        L_BRACK
            if p.nth_adjacent(0) && matches!(lhs.kind(), NAME_REF | FIELD_EXPR | GENERIC_EXPR) =>
        {
            if p.nth(1) == R_BRACK {
                array_suffix(p, start);
                true
            } else if let Some(jadpo) = python_collection(&source(p, start, p.prev_end())) {
                python_brackets(p, start, jadpo);
                true
            } else {
                false
            }
        }
        ERROR
            if p.current_text() == "|"
                && matches!(lhs.kind(), NAME_REF | FIELD_EXPR | GENERIC_EXPR)
                && p.nth(1) == IDENT
                && h::is_null_word(p.nth_text(1))
                && !p.nth_on_new_line(1) =>
        {
            null_union(p, start);
            true
        }
        _ => false,
    }
}

/// After a postfix chain from `start` with a `?.` at `chain`: the error,
/// whose edit writes the `match` it means.
pub(super) fn optional_chaining(p: &mut Parser<'_>, start: u32, chain: u32) {
    let end = p.prev_end();
    let subject = source(p, start, chain).trim_end().to_string();
    let after = source(p, chain + 1, end);
    let rest = after.trim_start().to_string();
    let dot = chain + 1 + (after.len() - rest.len()) as u32;
    let bind = binding_for(&subject);
    let jadpo = match_text(
        p,
        start,
        &subject,
        [
            (&format!("some({bind})"), &format!("{bind}{rest}")),
            ("none", "none"),
        ],
    );
    let summary = arms_summary(
        &subject,
        [&format!("some({bind}) -> {bind}{rest}"), "none -> none"],
    );
    report(
        p,
        &h::OPTIONAL_CHAINING,
        ("?.", &summary),
        (chain, dot + 1),
        vec![TextEdit::replace(start, end, jadpo.clone())],
        false,
    );
}

/// `Tag[]` (or `Tag[][]`) after a type starting at `start`: the error,
/// whose edit writes `List<Tag>`; the brackets are read as nothing.
pub(super) fn array_suffix(p: &mut Parser<'_>, start: u32) {
    let element = source(p, start, p.prev_end());
    let (open, _) = p.range();
    let mut jadpo = element.clone();
    while p.at(L_BRACK) && p.nth(1) == R_BRACK && p.nth_adjacent(0) {
        p.skip_token();
        p.skip_token();
        jadpo = format!("List<{jadpo}>");
    }
    let end = p.prev_end();
    let written = source(p, start, end);
    report(
        p,
        &h::ARRAY_TYPE,
        (&written, &jadpo),
        (open, end),
        vec![TextEdit::replace(start, end, jadpo.clone())],
        false,
    );
}

/// Go's `[]Tag`, at `[`: the error, whose edit writes `List<Tag>`. The
/// brackets are read as nothing and the type after them as the type.
pub(super) fn at_go_slice(p: &Parser<'_>) -> bool {
    p.at(L_BRACK) && p.nth(1) == R_BRACK && p.nth(2) == IDENT && p.nth_adjacent(2)
}

pub(super) fn go_slice(p: &mut Parser<'_>) -> (u32, String) {
    let (start, _) = p.range();
    let (_, close) = p.nth_range(1);
    let (element_start, mut element_end) = p.nth_range(2);
    let mut i = 3;
    while p.nth(i) == DOT && p.nth(i + 1) == IDENT {
        element_end = p.nth_range(i + 1).1;
        i += 2;
    }
    let element = source(p, element_start, element_end);
    let jadpo = format!("List<{element}>");
    let written = source(p, start, element_end);
    report(
        p,
        &h::ARRAY_TYPE,
        (&written, &jadpo),
        (start, close),
        vec![TextEdit::replace(start, element_end, jadpo.clone())],
        false,
    );
    p.skip_token();
    p.skip_token();
    (start, jadpo)
}

/// Python's collection type with its arguments in square brackets, as
/// Jadpo writes its name: `list[Tag]` is `List`, `Optional[Tag]` is `?`.
pub(super) fn python_collection(name: &str) -> Option<&'static str> {
    match name {
        "list" | "List" => Some("List"),
        "dict" | "Dict" => Some("Map"),
        "set" | "Set" => Some("Set"),
        "Optional" => Some("?"),
        _ => None,
    }
}

/// `list[Tag]` after the name that starts at `start`, at `[`: the error,
/// whose edit writes `List<Tag>` (or `Tag?` for `Optional[Tag]`).
pub(super) fn python_brackets(p: &mut Parser<'_>, start: u32, jadpo_name: &str) {
    let (open, _) = p.range();
    let e = p.start();
    skip_balanced(p);
    e.complete(p, ERROR);
    let end = p.prev_end();
    let written = source(p, start, end);
    let inner = source(p, open + 1, end.saturating_sub(1))
        .trim()
        .to_string();
    let jadpo = if jadpo_name == "?" {
        format!("{inner}?")
    } else {
        format!("{jadpo_name}<{inner}>")
    };
    let habit = if jadpo_name == "?" {
        &h::NULL_UNION
    } else {
        &h::ARRAY_TYPE
    };
    report(
        p,
        habit,
        (&written, &jadpo),
        (open, end),
        vec![TextEdit::replace(start, end, jadpo.clone())],
        false,
    );
}

/// `Tag | null`, `Tag | undefined` or `Tag | None` after the type that
/// starts at `start`, at `|`: the error, whose edit writes `Tag?`.
pub(super) fn null_union(p: &mut Parser<'_>, start: u32) {
    let ty = source(p, start, p.prev_end());
    let (bar, _) = p.range();
    let (_, end) = p.nth_range(1);
    let written = source(p, start, end);
    let jadpo = format!("{ty}?");
    let error = SyntaxError::habit(&h::NULL_UNION, &written, &jadpo, bar, end)
        .edit(TextEdit::replace(start, end, jadpo.clone()));
    p.replace_errors(bar, end, error);
    p.skip_token();
    p.skip_token();
}

/// An inline function in another language's form, starting an argument:
/// `x => e`, `(x) => e`, `lambda x: e` or `|x| e`. True when one was read
/// as the `x -> e` it is.
pub(super) fn inline_function(p: &mut Parser<'_>) -> bool {
    let (start, _) = p.range();
    // (tokens before the name, the arrow's offset from the name, clean)
    let shape = if p.at(IDENT) && p.nth(1) == FAT_ARROW {
        (0, 1, true)
    } else if p.at(L_PAREN) && p.nth(1) == IDENT && p.nth(2) == R_PAREN && p.nth(3) == FAT_ARROW {
        (1, 2, true)
    } else if p.at_word("lambda") && p.nth(1) == IDENT && p.nth(2) == COLON {
        (1, 1, true)
    } else if p.at(ERROR)
        && p.current_text() == "|"
        && p.nth(1) == IDENT
        && p.nth(2) == ERROR
        && p.nth_text(2) == "|"
    {
        (1, 1, false)
    } else {
        return false;
    };
    let (before, arrow_offset, clean) = shape;
    let item = p.nth_text(before).to_string();
    let arrow_at = before + arrow_offset;
    let (_, arrow_end) = p.nth_range(arrow_at);
    let after = p.nth_range(arrow_at + 1).0;
    let written_head = source(p, start, arrow_end);
    let body_start = if clean { after } else { arrow_end };
    let body_end = expression_end(p, body_start);
    let body = source(p, body_start, body_end).trim().to_string();
    let written = format!("{written_head} {body}");
    let jadpo = format!("{item} -> {body}");
    let mut error = SyntaxError::habit(
        &h::ARROW_FUNCTION,
        &short(&written),
        &short(&jadpo),
        start,
        arrow_end,
    )
    .edit(TextEdit::replace(
        start,
        if clean { after } else { arrow_end + 1 },
        format!("{item} -> "),
    ));
    if clean {
        error = error.clean_recovery();
    }
    error.next = h::ARROW_FUNCTION.next_for(&written, &short(&jadpo));
    p.replace_errors(start, arrow_end, error);
    let lambda = p.start();
    for _ in 0..before {
        p.skip_token();
    }
    name(p, "the function's parameter");
    if p.at(R_PAREN) || (p.at(ERROR) && p.current_text() == "|") {
        p.skip_token();
    }
    if matches!(p.current(), FAT_ARROW | COLON) {
        p.bump_remap(THIN_ARROW);
    }
    expr(p);
    lambda.complete(p, LAMBDA_EXPR);
    true
}

/// Where the argument starting at `from` ends: the next top-level `,` or
/// the call's `)`.
fn expression_end(p: &Parser<'_>, from: u32) -> u32 {
    let text = p.text();
    let mut depth = 0i32;
    for (i, c) in text[from as usize..].char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return from + i as u32,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return from + i as u32,
            '\n' if depth == 0 => return from + i as u32,
            _ => {}
        }
    }
    text.len() as u32
}

/// `name = value` as an argument: one error, read as `name: value`.
pub(super) fn at_named_equals(p: &Parser<'_>) -> bool {
    p.at(IDENT) && p.nth(1) == EQ && !p.nth_on_new_line(1)
}

pub(super) fn named_equals(p: &mut Parser<'_>) {
    let (start, end) = p.nth_range(1);
    let key_end = p.range().1;
    let clean = colon_reads_the_same_after(p);
    report(
        p,
        &h::NAMED_EQUALS,
        ("=", ":"),
        (start, end),
        vec![TextEdit::replace(key_end, end, ":")],
        clean,
    );
}

/// Whether the `=` at `p.nth(1)` can become `:` without changing how the
/// rest of the line reads. The lexer reads a `/` after `:` as a route path
/// (`/_length` one segment) but after `=` as a division, so with a `/`
/// next the edit is still offered but not as one that makes the file
/// right.
fn colon_reads_the_same_after(p: &Parser<'_>) -> bool {
    p.nth(2) != SLASH
}

// --- members --------------------------------------------------------------------

/// `name = value` as a member: one error, read as `name: value`.
pub(super) fn member_equals(p: &mut Parser<'_>, context: Members) {
    let key = p.current_text().to_string();
    let (key_start, key_end) = p.range();
    let (eq_start, eq_end) = p.nth_range(1);
    report(
        p,
        &h::MEMBER_EQUALS,
        (&format!("{key} ="), &format!("{key}:")),
        (eq_start, eq_end),
        vec![TextEdit::replace(key_end, eq_end, ":")],
        colon_reads_the_same_after(p),
    );
    let _ = key_start;
    let m = p.start();
    member_key(p);
    p.bump_remap(COLON);
    member_value(p, context);
    m.complete(p, MEMBER);
    member_end(p);
}

/// A `,` after a member: one error, read as nothing. With another member
/// after it on the line, a bare one moves to its own line, since only
/// `name: value` members may share a line.
pub(super) fn member_comma(p: &mut Parser<'_>, bare: bool) {
    let (start, end) = p.range();
    let more = !p.nth_on_new_line(1) && !matches!(p.nth(1), R_BRACE | EOF);
    let edit = if bare && more {
        let next = p.nth_range(1).0;
        let (line_start, _) = line_bounds(p, start);
        let opened_here = p.text()[line_start as usize..start as usize].contains('{');
        let mut indent = indent_at(p, start);
        if opened_here {
            indent.push_str("    ");
        }
        TextEdit::replace(start, next, format!("\n{indent}"))
    } else if more {
        // `a: b,c: d` keeps a space where the comma was, so the two
        // members don't run together.
        let joined = p.nth_range(1).0 == end;
        TextEdit::replace(start, end, if joined { " " } else { "" })
    } else {
        let before = p.text()[..start as usize]
            .trim_end_matches([' ', '\t'])
            .len() as u32;
        TextEdit::replace(before, end, "")
    };
    report(
        p,
        &h::MEMBER_COMMA,
        (",", ""),
        (start, end),
        vec![edit],
        true,
    );
    p.skip_token();
}

// --- match arms -----------------------------------------------------------------

/// An arm in a `switch`'s form: `case P:`, `case P ->`, `default:`, or
/// Kotlin's `else ->`. True when one was read.
pub(super) fn switch_arm(p: &mut Parser<'_>) -> bool {
    let case = p.at_word("case") && !p.nth_on_new_line(1);
    let default = (p.at_word("default") || p.at(ELSE_KW))
        && matches!(p.nth(1), COLON | THIN_ARROW | FAT_ARROW)
        && !p.nth_on_new_line(1);
    if !case && !default {
        return false;
    }
    let m = p.start();
    let (arm_start, word_end) = p.range();
    let mut edits = Vec::new();
    let pattern_text = if case {
        p.skip_token();
        let pattern_start = p.range().0;
        pattern(p);
        source(p, pattern_start, p.prev_end())
    } else {
        let w = p.start();
        p.bump_remap(UNDERSCORE);
        w.complete(p, WILDCARD_PAT);
        "_".to_string()
    };
    let (arrow_start, arrow_end) = p.range();
    let arrow = p.current();
    if !matches!(arrow, COLON | THIN_ARROW | FAT_ARROW) {
        report(
            p,
            &h::CASE,
            ("case", &format!("{pattern_text} -> ...")),
            (arm_start, word_end),
            Vec::new(),
            false,
        );
        skip_line(p);
        m.complete(p, MATCH_ARM);
        return true;
    }
    p.bump_remap(THIN_ARROW);
    let habit = if case { &h::CASE } else { &h::DEFAULT };
    let written_word = source(p, arm_start, arrow_end);
    if p.on_new_line() || p.at_eof() {
        // A block of statements on the lines after, to the next arm: the
        // arm's block, with `break` gone.
        let indent = indent_at(p, arm_start);
        edits.push(TextEdit::replace(
            arm_start,
            arrow_end,
            format!("{pattern_text} -> {{"),
        ));
        let block = p.start();
        while !p.at_eof() && !p.at(R_BRACE) && !at_arm_start(p) {
            if p.at_word("break") && p.nth_on_new_line(1) {
                let (line_start, line_next) = line_bounds(p, p.range().0);
                p.remove_errors(line_start, line_next);
                edits.push(TextEdit::replace(line_start, line_next, ""));
                p.skip_token();
                continue;
            }
            if ends_block(p) {
                break;
            }
            let before = p.position();
            block_item(p);
            if p.position() == before {
                p.bump_error();
            }
        }
        block.complete(p, BLOCK);
        let (next_line, _) = line_bounds(p, p.range().0);
        edits.push(TextEdit::insert(next_line, format!("{indent}}}\n")));
        report(
            p,
            habit,
            (&written_word, &format!("{pattern_text} -> {{ ... }}")),
            (arm_start, if case { word_end } else { arrow_end }),
            edits,
            false,
        );
        m.complete(p, MATCH_ARM);
        return true;
    }
    // The result on the same line: a value, or a statement made a block.
    let statement = matches!(p.current(), RETURN_KW | VAR_KW | IF_KW | FOR_KW | EMIT_KW);
    let result_start = p.range().0;
    let clean = !statement;
    if statement {
        let block = p.start();
        block_item(p);
        block.complete(p, BLOCK);
    } else if p.at(L_BRACE) {
        block(p);
    } else {
        expr(p);
    }
    let result_end = p.prev_end().max(result_start);
    let result = source(p, result_start, result_end);
    let jadpo = if statement {
        format!("{pattern_text} -> {{ {result} }}")
    } else {
        format!("{pattern_text} -> {result}")
    };
    edits.push(TextEdit::replace(arm_start, result_end, jadpo.clone()));
    let _ = (arrow_start, arrow);
    report(
        p,
        habit,
        (&written_word, &jadpo),
        (arm_start, if case { word_end } else { arrow_end }),
        edits,
        clean,
    );
    m.complete(p, MATCH_ARM);
    true
}

/// A line that starts another arm: `case`, `default`, or a Jadpo arm,
/// whose `->` stands outside any brackets.
fn at_arm_start(p: &Parser<'_>) -> bool {
    if !p.on_new_line() {
        return false;
    }
    if (p.at_word("case") && !p.nth_on_new_line(1))
        || ((p.at_word("default") || p.at(ELSE_KW))
            && matches!(p.nth(1), COLON | THIN_ARROW | FAT_ARROW))
    {
        return true;
    }
    let mut depth = 0;
    let mut i = 0;
    while (i == 0 || !p.nth_on_new_line(i)) && p.nth(i) != EOF {
        match p.nth(i) {
            L_PAREN | L_BRACK | L_BRACE => depth += 1,
            R_PAREN | R_BRACK | R_BRACE => depth -= 1,
            THIN_ARROW | FAT_ARROW if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}
