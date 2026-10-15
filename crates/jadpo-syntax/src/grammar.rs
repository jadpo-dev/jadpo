//! The recursive-descent grammar (`jadpo.ungram` written as code).
//!
//! Recovery happens at declaration and member boundaries: a mistake inside a
//! member or statement skips to the end of its line, and a mistake between
//! declarations skips to the next line that starts one. Each mistake is
//! reported once, with the reason and the next step written for the person
//! or agent who has to fix it.

use jadpo_diag::rule;

use crate::SyntaxKind::{self, *};
use crate::error::{SyntaxError, TextEdit};
use crate::lexer::HTTP_METHODS;
use crate::parser::{CompletedMarker, Parser};

/// Declarations that exist once per project and carry no name (area 1).
pub(crate) const SINGLETONS: &[&str] = &[
    "acceptance",
    "application",
    "authentication",
    "config",
    "flags",
    "locales",
    "migration",
    "policy",
    "principal",
];

// --- files ------------------------------------------------------------------

pub(crate) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_eof() {
        if at_use(p) {
            use_decl(p);
            continue;
        }
        if at_decl(p) {
            let before = p.events.len();
            decl(p);
            if p.events.len() == before {
                p.bump_error();
            }
            continue;
        }
        expected_declaration(p);
        recover_to_declaration(p);
    }
    m.complete(p, SOURCE_FILE);
}

pub(crate) fn snippet(p: &mut Parser<'_>) {
    let m = p.start();
    while !p.at_eof() {
        let before = p.position();
        if at_use(p) {
            use_decl(p);
        } else if at_decl(p) {
            decl(p);
        } else if at_route_entry(p) {
            route_entry(p);
        } else {
            block_item(p);
        }
        if p.position() == before {
            p.bump_error();
        }
    }
    m.complete(p, SNIPPET);
}

pub(crate) fn template(p: &mut Parser<'_>) {
    let m = p.start();
    template_body(p);
    while !p.at_eof() {
        // A stray `{{ else }}` or `{{ end }}` at the top level.
        let (start, end) = p.range();
        let tag = template_tag_text(p);
        p.push_error(
            SyntaxError::new(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                format!("`{{{{ {tag} }}}}` has no matching `{{{{ if }}}}` or `{{{{ for }}}}`"),
                start,
                end,
            )
            .label("nothing to close here")
            .reason(
                "`{{ else }}` and `{{ end }}` belong to an `{{ if }}` or `{{ for }}` above them.",
            )
            .next(format!(
                "Remove `{{{{ {tag} }}}}`, or add the `{{{{ if }}}}` it belongs to"
            )),
        );
        let e = p.start();
        skip_template_tag(p);
        e.complete(p, ERROR);
        template_body(p);
    }
    m.complete(p, TEMPLATE);
}

fn expected_declaration(p: &mut Parser<'_>) {
    let (start, end) = p.range();
    let text = p.current_text();
    let mut error = SyntaxError::new(
        &rule::SYNTAX_EXPECTED_DECLARATION,
        format!("`{text}` doesn't start a declaration"),
        start,
        end,
    )
    .label("expected a declaration")
    .reason(
        "A file holds declarations, such as `type`, `function`, `action`, `query` and `entity`, \
         each starting a line.",
    );
    if p.at(IDENT) && p.nth(1) == EQ {
        error = error
            .next(format!(
                "Start the declaration with `type`: `type {text} = ...`"
            ))
            .edit(TextEdit::insert(start, "type "));
    } else if p.at(IDENT) && p.nth(1) == L_BRACE {
        let words = SINGLETONS
            .iter()
            .chain(["entity", "library", "routes", "schedule", "subscriber"].iter());
        match crate::nearest(text, words.copied()) {
            Some(near) => {
                error = error
                    .reason(format!(
                        "A file holds declarations, such as `type`, `entity` and `{near}`; \
                         `{text}` isn't one. Did you mean `{near}`?"
                    ))
                    .next(format!("Use `{near}` instead of `{text}`"))
                    .edit(TextEdit::replace(start, end, near));
            }
            None => {
                error = error
                    .next("Start the line with a declaration keyword such as `type` or `function`")
            }
        }
    } else if p.at(DOT3) {
        error = error
            .message_override("`...` stands for code left out of an example")
            .label("not Jadpo code")
            .reason("Documentation examples use `...` for code they leave out; a source file has to say what it means.")
            .next("Replace `...` with the code it stands for");
    } else {
        error =
            error.next("Start the line with a declaration keyword such as `type` or `function`");
    }
    p.push_error(error);
}

/// Skips to the next line that starts a declaration.
fn recover_to_declaration(p: &mut Parser<'_>) {
    let m = p.start();
    loop {
        skip_balanced(p);
        if p.at_eof() || (p.on_new_line() && (at_decl(p) || at_use(p))) {
            break;
        }
    }
    m.complete(p, ERROR);
}

/// Consumes one token, or a whole bracketed group.
fn skip_balanced(p: &mut Parser<'_>) {
    let close = match p.current() {
        L_BRACE => R_BRACE,
        L_PAREN => R_PAREN,
        L_BRACK => R_BRACK,
        _ => {
            p.bump_any();
            return;
        }
    };
    let open = p.current();
    p.bump_any();
    let mut depth = 1u32;
    while !p.at_eof() {
        let kind = p.current();
        p.bump_any();
        if kind == open {
            depth += 1;
        } else if kind == close {
            depth -= 1;
            if depth == 0 {
                return;
            }
        }
    }
}

/// Skips the rest of the line (keeping brackets balanced), stopping before
/// a `}` that closes the enclosing block.
fn skip_line(p: &mut Parser<'_>) {
    if p.at_eof() || p.at(R_BRACE) || p.on_new_line() {
        return;
    }
    let m = p.start();
    loop {
        skip_balanced(p);
        if p.at_eof() || p.at(R_BRACE) || p.on_new_line() {
            break;
        }
    }
    m.complete(p, ERROR);
}

// --- imports -------------------------------------------------------------------

/// `use` starting a line, followed on the same line by a folder or domain:
/// `use shared/types { Pence }`, `use menu` (area 1, Imports). Anywhere
/// else `use` is an ordinary word, which naming reserves.
pub(crate) fn at_use(p: &Parser<'_>) -> bool {
    p.at_word("use")
        && p.on_new_line()
        && (p.nth(1) == IDENT || p.nth(1).is_keyword())
        && !p.nth_on_new_line(1)
}

fn use_decl(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump_remap(USE_KW);
    let path = p.start();
    use_segment(p);
    while p.at(SLASH) && !p.on_new_line() {
        p.bump(SLASH);
        if (p.at(IDENT) || p.current().is_keyword()) && !p.on_new_line() {
            use_segment(p);
        } else {
            let found = p.found();
            let error = p
                .error_here(&rule::SYNTAX_EXPECTED_NAME, "Expected a folder after `/`")
                .label("expected a folder")
                .reason(format!(
                    "A `use` line names a domain's or `shared/`'s folder, such as `shared/types`; found {found}."
                ))
                .next("Name the folder after `/`, or remove `/`");
            p.push_error(error);
            break;
        }
    }
    path.complete(p, USE_PATH);
    if p.at(L_BRACE) && !p.on_new_line() {
        use_names(p);
    }
    if !p.at_eof() && !p.on_new_line() {
        let found = p.found();
        let error = p
            .error_here(&rule::SYNTAX_UNEXPECTED_TOKEN, "Expected the end of the `use` line")
            .label("expected the end of the line")
            .reason(format!(
                "A `use` line is `use <folder> {{ Names }}` or `use <domain>`, alone on its line; found {found}."
            ))
            .next("Put the names in braces after the folder, and anything else on its own line");
        p.push_error(error);
        skip_line(p);
    }
    m.complete(p, USE_DECL);
}

/// One name in a `use` path. A folder may be a word a declaration starts
/// with (`routes`), so keywords are read as names here.
fn use_segment(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump_remap(IDENT);
    m.complete(p, NAME_REF);
}

/// `{ Pence, Percent }`: the names a `use` line brings in, separated by
/// commas, on one line or across several.
fn use_names(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.range();
    p.bump(L_BRACE);
    p.with_newlines(true, |p| {
        while !p.at(R_BRACE) && !p.at_eof() {
            if p.at(IDENT) {
                let name = p.start();
                p.bump(IDENT);
                name.complete(p, NAME_REF);
            } else {
                let found = p.found();
                let error = p
                    .error_here(&rule::SYNTAX_EXPECTED_NAME, "Expected a name to import")
                    .label("expected a name")
                    .reason(format!(
                        "The braces of a `use` line hold the names it brings in, separated by commas; found {found}."
                    ))
                    .next("Write the names the file takes from the folder, separated by commas");
                p.push_error(error);
                if p.at(L_BRACE) || at_decl(p) || at_use(p) {
                    return;
                }
                p.bump_error();
                continue;
            }
            if p.eat(COMMA) || p.at(R_BRACE) {
                continue;
            }
            if p.at(IDENT) {
                let error = p
                    .error_here(&rule::SYNTAX_UNEXPECTED_TOKEN, "Expected `,` between names")
                    .label("expected `,` before this name")
                    .reason("The names a `use` line brings in are separated by commas.")
                    .next("Add `,` between the names")
                    .edit(p.insert_after_prev(","));
                p.push_error(error);
                continue;
            }
            if !p.at_eof() && !at_decl(p) && !at_use(p) {
                let found = p.found();
                let error = p
                    .error_here(&rule::SYNTAX_UNEXPECTED_TOKEN, "Expected `,` or `}`")
                    .label("expected `,` or `}`")
                    .reason(format!(
                        "The names a `use` line brings in are separated by commas and end with `}}`; found {found}."
                    ))
                    .next("Separate the names with commas and close the braces");
                p.push_error(error);
                p.bump_error();
            } else {
                break;
            }
        }
    });
    if !p.eat(R_BRACE) {
        let (start, end) = p.range();
        let error = SyntaxError::new(
            &rule::SYNTAX_UNEXPECTED_TOKEN,
            "This list of names isn't closed",
            start,
            end,
        )
        .label("expected `}`")
        .also(open.0, open.1, "opened here")
        .reason("The names a `use` line brings in end with `}`.")
        .next("Add `}` after the last name")
        .edit(p.insert_after_prev(" }"));
        p.push_error(error);
    }
    m.complete(p, USE_NAMES);
}

// --- declarations -------------------------------------------------------------

fn contextual_decl(p: &Parser<'_>, n: usize) -> Option<SyntaxKind> {
    if p.nth(n) != IDENT {
        return None;
    }
    let word = p.nth_text(n);
    let next = p.nth(n + 1);
    let kind = match word {
        "test" if next == STRING => TEST_DECL,
        "seed" if next == L_BRACE || (next == IDENT && p.nth(n + 2) == L_BRACE) => SEED_DECL,
        "package" if next == IDENT && !p.nth_on_new_line(n + 1) => PACKAGE_DECL,
        "resource" if next == IDENT && p.nth(n + 2) == L_BRACE => RESOURCE_DECL,
        "provide" if next == IDENT && p.nth(n + 2) == L_BRACE => PROVIDE_DECL,
        _ if next == L_BRACE && SINGLETONS.contains(&word) => SINGLETON_DECL,
        _ => return None,
    };
    Some(kind)
}

/// Whether a block that hasn't seen its `}` ends here, because a new
/// declaration starts: a declaration keyword on a new line, or a word that
/// only sometimes starts one (`config {`, `test "..."`) written at the
/// start of a line. Indented, such a word is a member of the block.
pub(crate) fn ends_block(p: &Parser<'_>) -> bool {
    if p.on_new_line() && p.at_column_zero() && at_use(p) {
        return true;
    }
    if !p.on_new_line() || !at_decl(p) {
        return false;
    }
    // `type: Name` is a member that took a reserved word, not a declaration.
    if p.current().is_keyword() && p.nth(1) == COLON {
        return false;
    }
    let n = usize::from(p.at(PUBLIC_KW));
    p.nth(n) != IDENT || p.at_column_zero()
}

pub(crate) fn at_decl(p: &Parser<'_>) -> bool {
    let n = usize::from(p.at(PUBLIC_KW));
    match p.nth(n) {
        TYPE_KW | FUNCTION_KW | ACTION_KW | QUERY_KW | FIXTURE_KW | ENTITY_KW | LIBRARY_KW
        | ROUTES_KW | SCHEDULE_KW | SUBSCRIBER_KW => true,
        IDENT => contextual_decl(p, n).is_some() || n == 1,
        _ => n == 1,
    }
}

fn decl(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(PUBLIC_KW) {
        let v = p.start();
        p.bump(PUBLIC_KW);
        v.complete(p, VISIBILITY);
    }
    match p.current() {
        TYPE_KW => type_decl(p, m),
        FUNCTION_KW => callable(p, m, FUNCTION_DECL, false),
        ACTION_KW => callable(p, m, ACTION_DECL, false),
        QUERY_KW => callable(p, m, QUERY_DECL, true),
        FIXTURE_KW => {
            p.bump(FIXTURE_KW);
            name(p, "this fixture");
            if p.at(L_PAREN) {
                param_list(p);
            }
            if p.at(THIN_ARROW) {
                ret_type(p);
            }
            body_start(p, false);
            m.complete(p, FIXTURE_DECL);
        }
        ENTITY_KW => keyword_named(p, m, ENTITY_DECL, "this entity"),
        LIBRARY_KW => keyword_named(p, m, LIBRARY_DECL, "this library"),
        ROUTES_KW => keyword_named(p, m, ROUTES_DECL, "this route group"),
        SCHEDULE_KW => keyword_named(p, m, SCHEDULE_DECL, "this schedule"),
        SUBSCRIBER_KW => keyword_named(p, m, SUBSCRIBER_DECL, "this subscriber"),
        IDENT => match contextual_decl(p, 0) {
            Some(TEST_DECL) => {
                p.bump_remap(TEST_KW);
                p.bump(STRING);
                body_start(p, false);
                m.complete(p, TEST_DECL);
            }
            Some(SEED_DECL) => {
                p.bump_remap(SEED_KW);
                if p.at(IDENT) {
                    name(p, "this seed");
                }
                body_start(p, false);
                m.complete(p, SEED_DECL);
            }
            Some(PACKAGE_DECL) => {
                p.bump_remap(PACKAGE_KW);
                path(p);
                body_start(p, true);
                m.complete(p, PACKAGE_DECL);
            }
            Some(kind @ (RESOURCE_DECL | PROVIDE_DECL)) => {
                p.bump_remap(if kind == RESOURCE_DECL {
                    RESOURCE_KW
                } else {
                    PROVIDE_KW
                });
                name(p, "this declaration");
                body_start(p, true);
                m.complete(p, kind);
            }
            Some(_) => {
                p.bump(IDENT);
                member_list(p, Members::Declaration);
                m.complete(p, SINGLETON_DECL);
            }
            None => {
                after_public(p);
                m.complete(p, ERROR);
            }
        },
        _ => {
            after_public(p);
            m.complete(p, ERROR);
        }
    }
}

/// `public` followed by something that isn't a declaration.
fn after_public(p: &mut Parser<'_>) {
    let found = p.found();
    let error = p
        .error_here(
            &rule::SYNTAX_EXPECTED_DECLARATION,
            "`public` has no declaration after it",
        )
        .label("expected a declaration")
        .reason(format!(
            "`public` marks a declaration another domain may use; found {found} instead."
        ))
        .next("Write the declaration after `public`, or remove `public`");
    p.push_error(error);
    skip_line(p);
}

fn keyword_named(p: &mut Parser<'_>, m: crate::parser::Marker, kind: SyntaxKind, what: &str) {
    p.bump_any();
    name(p, what);
    body_start(p, true);
    m.complete(p, kind);
}

/// The `{` that opens a declaration's body: members or statements.
fn body_start(p: &mut Parser<'_>, members: bool) {
    if p.at(L_BRACE) {
        if members {
            member_list(p, Members::Declaration);
        } else {
            block(p);
        }
        return;
    }
    let found = p.found();
    let (start, _) = p.range();
    let error = SyntaxError::new(
        &rule::SYNTAX_UNEXPECTED_TOKEN,
        "Expected `{` to start the body",
        start,
        p.range().1,
    )
    .label("expected `{`")
    .reason(format!("A declaration's body is in braces; found {found}."))
    .next("Add `{` and the body, ending with `}`");
    p.push_error(error);
    skip_line(p);
}

fn type_decl(p: &mut Parser<'_>, m: crate::parser::Marker) {
    p.bump(TYPE_KW);
    let type_name = p.current_text().to_string();
    name(p, "this type");
    if !p.eat(EQ) && !p.at(L_BRACE) {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `=` after the type's name",
            )
            .label("expected `=`")
            .reason(format!(
                "Every type is written `type Name = Base {{ ... }}`; found {found}."
            ))
            .next(format!(
                "Write `type {type_name} = ` followed by its base and block"
            ));
        p.push_error(error);
        skip_line(p);
        m.complete(p, TYPE_DECL);
        return;
    }
    let mut base = None;
    let base_start = p.range().0;
    if p.at(L_BRACE) {
        let error = p
            .error_here(
                &rule::SYNTAX_TYPE_PARENT_REQUIRED,
                format!("`{type_name}` needs a base type"),
            )
            .label("expected a base before `{`")
            .reason(
                "Every type is written `type Name = Base { ... }`; the base says what it is \
                 built from, such as `Object`, `Enum`, `Input` or `Text`.",
            )
            .next(format!(
                "Add the base before `{{`, for example `type {type_name} = Object {{`"
            ));
        p.push_error(error);
    } else {
        base = ty(p, "the base type");
    }
    if p.at(L_BRACE) {
        member_list(p, Members::Declaration);
    } else if base.is_some() {
        let at = p.prev_end();
        let base_text = p.text()[base_start as usize..at as usize].to_string();
        let error = SyntaxError::new(
            &rule::SYNTAX_TYPE_BLOCK_REQUIRED,
            format!("`{type_name}` needs a block"),
            base_start,
            at,
        )
        .label("expected `{}` after the base")
        .reason(
            "Every type is written `type Name = Base { ... }`. Without a block it reads like \
             an alias, and Jadpo has no aliases: every type is a new type.",
        )
        .next(format!("Add `{{}}` after `{base_text}`"))
        .edit(TextEdit::insert(at, " {}"));
        p.push_error(error);
        skip_line(p);
    }
    m.complete(p, TYPE_DECL);
}

fn callable(p: &mut Parser<'_>, m: crate::parser::Marker, kind: SyntaxKind, members: bool) {
    let keyword = p.current_text().to_string();
    p.bump_any();
    name(p, &format!("this {keyword}"));
    if p.at(L_PAREN) {
        param_list(p);
    } else {
        let found = p.found();
        let error = p
            .error_here(&rule::SYNTAX_UNEXPECTED_TOKEN, "Expected `(` and the parameters")
            .label("expected `(`")
            .reason(format!(
                "A {keyword} lists its parameters in brackets, even when there are none: `()`; found {found}."
            ))
            .next("Add the parameters in brackets after the name")
            .edit(p.insert_after_prev("()"));
        p.push_error(error);
    }
    if p.at(FAILS_KW) {
        fails_clause(p);
    }
    let arrow = p.range().0;
    if p.at(THIN_ARROW) {
        ret_type(p);
    }
    if p.at(FAILS_KW) {
        let before = p.prev_end();
        let start = p.range().0;
        fails_clause(p);
        let end = p.prev_end();
        let clause = p.text()[start as usize..end as usize].to_string();
        let error = SyntaxError::new(
            &rule::SYNTAX_FAILS_AFTER_RESULT,
            "`fails` goes before `->`",
            start,
            end,
        )
        .label("after the result type")
        .reason("A signature says what can fail before what it returns: `fails A, B -> Result`.")
        .next(format!("Move `{clause}` before `->`"))
        .edit(TextEdit::replace(before, end, ""))
        .edit(TextEdit::insert(arrow, format!("{clause} ")));
        p.push_error(error);
    }
    body_start(p, members);
    m.complete(p, kind);
}

fn param_list(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.range();
    p.bump(L_PAREN);
    p.with_newlines(true, |p| {
        while !p.at(R_PAREN) && !p.at_eof() {
            if at_signature_end(p) {
                unclosed_params(p, open);
                return;
            }
            if p.at(IDENT) || p.current().is_keyword() {
                param(p);
            } else {
                let found = p.found();
                let error = p
                    .error_here(&rule::SYNTAX_EXPECTED_NAME, "Expected a parameter name")
                    .label("expected a name")
                    .reason(format!("Each parameter is `name: Type`; found {found}."))
                    .next("Write the parameter as `name: Type`");
                p.push_error(error);
                p.bump_error();
            }
            if p.eat(COMMA) {
                continue;
            }
            if p.at(R_PAREN) {
                break;
            }
            if at_signature_end(p) {
                unclosed_params(p, open);
                return;
            }
            let found = p.found();
            let error = p
                .error_here(
                    &rule::SYNTAX_UNEXPECTED_TOKEN,
                    "Expected `,` or `)` after this parameter",
                )
                .label("expected `,` or `)`")
                .reason(format!(
                    "Parameters are separated by commas; found {found}."
                ))
                .next("Add `,` between the parameters");
            p.push_error(error);
            if !(p.at(IDENT) && p.on_new_line()) {
                p.bump_error();
            }
        }
        if !p.eat(R_PAREN) {
            unclosed_params(p, open);
        }
    });
    m.complete(p, PARAM_LIST);
}

fn at_signature_end(p: &Parser<'_>) -> bool {
    matches!(p.current(), THIN_ARROW | L_BRACE | FAILS_KW)
}

fn unclosed_params(p: &mut Parser<'_>, open: (u32, u32)) {
    let found = p.found();
    let (start, end) = p.range();
    let after = p.text()[..p.prev_end() as usize]
        .rsplit(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_string();
    let error = SyntaxError::new(
        &rule::SYNTAX_UNEXPECTED_TOKEN,
        "This parameter list isn't closed",
        start,
        end,
    )
    .label(if p.at_eof() {
        "expected `)`".to_string()
    } else {
        format!("expected `)` before {found}")
    })
    .also(open.0, open.1, "opened here")
    .reason("The parameters end with `)`, before `fails`, `->` or the body.")
    .next(if after.is_empty() {
        "Add `)` after the last parameter".to_string()
    } else {
        format!("Add `)` after `{after}`")
    })
    .edit(p.insert_after_prev(")"));
    p.push_error(error);
}

fn param(p: &mut Parser<'_>) {
    let m = p.start();
    let param_name = p.current_text().to_string();
    name(p, "this parameter");
    if p.eat(COLON) {
        ty(p, "the parameter's type");
    } else {
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                format!("`{param_name}` needs a type"),
            )
            .label("expected `:` and a type")
            .reason("Every parameter has a type: `name: Type`.")
            .next(format!("Write `{param_name}: Type`"));
        p.push_error(error);
    }
    m.complete(p, PARAM);
}

fn fails_clause(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(FAILS_KW);
    loop {
        ty(p, "a failure");
        if !p.eat(COMMA) {
            break;
        }
    }
    m.complete(p, FAILS_CLAUSE);
}

fn ret_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(THIN_ARROW);
    ty(p, "the result type");
    m.complete(p, RET_TYPE);
}

/// A declared name. Reserved words are reported and kept, so the rest
/// still parses.
/// Declaration words the lexer reads as names, because a setting may use
/// them as its key (`package: Stripe.v2024_06_20`, `test: "..."`): they
/// are reserved wherever something is named (area 1, question 7).
pub const RESERVED_NAMES: &[&str] = &["package", "provide", "resource", "seed", "test", "use"];

fn name(p: &mut Parser<'_>, what: &str) {
    match p.current() {
        IDENT if RESERVED_NAMES.contains(&p.current_text()) => {
            reserved_word(p);
            let m = p.start();
            p.bump(IDENT);
            m.complete(p, NAME);
        }
        IDENT => {
            let m = p.start();
            p.bump(IDENT);
            m.complete(p, NAME);
        }
        kind if kind.is_keyword() => {
            reserved_word(p);
            let m = p.start();
            p.bump_remap(IDENT);
            m.complete(p, NAME);
        }
        _ => {
            let found = p.found();
            let error = p
                .error_here(
                    &rule::SYNTAX_EXPECTED_NAME,
                    format!("Expected a name for {what}"),
                )
                .label("expected a name")
                .reason(format!(
                    "Names are letters, digits and `_`, starting with a letter; found {found}."
                ))
                .next(format!("Give {what} a name"));
            p.push_error(error);
        }
    }
}

fn reserved_word(p: &mut Parser<'_>) {
    let word = p.current_text().to_string();
    let error = p
        .error_here(
            &rule::SYNTAX_EXPECTED_NAME,
            format!("`{word}` is a reserved word"),
        )
        .label("can't be a name")
        .reason(
            "Control-flow and declaration words are reserved, so they can't name a \
             declaration, field, parameter or variable.",
        )
        .next(format!("Choose another name than `{word}`"));
    p.push_error(error);
}

fn name_ref(p: &mut Parser<'_>) -> bool {
    match p.current() {
        IDENT => {
            let m = p.start();
            p.bump(IDENT);
            m.complete(p, NAME_REF);
            true
        }
        _ => false,
    }
}

/// A name after `.`: any word, keywords included (`Access.public`).
fn field_name(p: &mut Parser<'_>) -> bool {
    let kind = p.current();
    if kind == IDENT || kind.is_keyword() {
        let m = p.start();
        p.bump_remap(IDENT);
        m.complete(p, NAME_REF);
        true
    } else {
        let found = p.found();
        let error = p
            .error_here(&rule::SYNTAX_EXPECTED_NAME, "Expected a name after `.`")
            .label("expected a name")
            .reason(format!(
                "A `.` is followed by the name of a field, operation or member; found {found}."
            ))
            .next("Write the name after `.`, or remove the `.`");
        p.push_error(error);
        false
    }
}

fn path(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    if !name_ref(p) {
        let found = p.found();
        let error = p
            .error_here(&rule::SYNTAX_EXPECTED_NAME, "Expected a name")
            .label("expected a name")
            .reason(format!("Found {found} where a name belongs."))
            .next("Write the name here");
        p.push_error(error);
    }
    while p.at(DOT) && !p.line_break_ends() {
        p.bump(DOT);
        if !field_name(p) {
            break;
        }
    }
    m.complete(p, PATH)
}

// --- types --------------------------------------------------------------------

fn ty(p: &mut Parser<'_>, what: &str) -> Option<CompletedMarker> {
    if !p.at(IDENT) {
        let found = p.found();
        if p.current().is_keyword() {
            reserved_word(p);
        } else {
            let error = p
                .error_here(&rule::SYNTAX_UNEXPECTED_TOKEN, format!("Expected {what}"))
                .label("expected a type")
                .reason(format!(
                    "A type is a name such as `Count`, `Order.id` or `List<Tag>`; found {found}."
                ))
                .next(format!("Write {what} here"));
            p.push_error(error);
            return None;
        }
    }
    let m = p.start();
    if p.current().is_keyword() {
        let path = p.start();
        let n = p.start();
        p.bump_remap(IDENT);
        n.complete(p, NAME_REF);
        path.complete(p, PATH);
    } else {
        path(p);
    }
    if p.at(L_ANGLE) && !p.line_break_ends() {
        generic_arg_list(p);
    }
    let mut done = m.complete(p, PATH_TYPE);
    let mut levels = 0;
    while p.at(QUESTION) && !p.on_new_line() && p.enter() {
        levels += 1;
        let m = done.precede(p);
        p.bump(QUESTION);
        done = m.complete(p, NULLABLE_TYPE);
    }
    for _ in 0..levels {
        p.leave();
    }
    Some(done)
}

fn generic_arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(L_ANGLE);
    if !p.enter() {
        m.complete(p, GENERIC_ARG_LIST);
        return;
    }
    p.with_newlines(true, |p| {
        while !p.at(R_ANGLE) && !p.at_eof() {
            if ty(p, "a type").is_none() {
                break;
            }
            if !p.eat(COMMA) {
                break;
            }
        }
        if !p.eat(R_ANGLE) {
            let found = p.found();
            let error = p
                .error_here(
                    &rule::SYNTAX_UNEXPECTED_TOKEN,
                    "Expected `>` to close the type's arguments",
                )
                .label("expected `>`")
                .reason(format!(
                    "`<` opened a list of types, as in `List<Tag>`; found {found}."
                ))
                .next("Add `>` after the last type");
            p.push_error(error);
        }
    });
    p.leave();
    m.complete(p, GENERIC_ARG_LIST);
}

/// Whether `<` starts a list of types (`List<Tag>`) rather than comparing.
fn generic_ahead(p: &Parser<'_>) -> bool {
    fn ty_ahead(p: &Parser<'_>, i: &mut usize, depth: u32) -> bool {
        if depth > 16 || p.nth(*i) != IDENT {
            return false;
        }
        *i += 1;
        while p.nth(*i) == DOT && p.nth(*i + 1) == IDENT {
            *i += 2;
        }
        if p.nth(*i) == L_ANGLE {
            *i += 1;
            loop {
                if !ty_ahead(p, i, depth + 1) {
                    return false;
                }
                if p.nth(*i) == COMMA {
                    *i += 1;
                    continue;
                }
                break;
            }
            if p.nth(*i) != R_ANGLE {
                return false;
            }
            *i += 1;
        }
        while p.nth(*i) == QUESTION {
            *i += 1;
        }
        true
    }
    let mut i = 1;
    loop {
        if !ty_ahead(p, &mut i, 0) {
            return false;
        }
        if p.nth(i) == COMMA {
            i += 1;
            continue;
        }
        break;
    }
    p.nth(i) == R_ANGLE
}

// --- members ------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Members {
    /// A declaration's body: bare names are enum variants.
    Declaration,
    /// The block after a value (`TodoView { ... }`): every member has a value.
    Record,
}

fn member_list(p: &mut Parser<'_>, context: Members) -> CompletedMarker {
    let m = p.start();
    let open = p.range();
    p.bump(L_BRACE);
    if !p.enter() {
        return m.complete(p, MEMBER_LIST);
    }
    p.with_newlines(false, |p| {
        p.with_records(true, |p| {
            while !p.at(R_BRACE) && !p.at_eof() {
                if ends_block(p) && !p.at(FUNCTION_KW) {
                    break;
                }
                let before = p.position();
                member_item(p, context);
                if p.position() == before {
                    p.bump_error();
                }
            }
        })
    });
    p.leave();
    close_brace(p, open);
    m.complete(p, MEMBER_LIST)
}

fn close_brace(p: &mut Parser<'_>, open: (u32, u32)) {
    if p.eat(R_BRACE) {
        return;
    }
    let (start, end) = open;
    let found = p.found();
    let error = SyntaxError::new(
        &rule::SYNTAX_UNEXPECTED_TOKEN,
        "This `{` isn't closed",
        start,
        end,
    )
    .label("no matching `}`")
    .reason(format!(
        "Every `{{` is closed by a `}}`; found {found} before this one was."
    ))
    .next("Add `}` where the block ends");
    p.push_error(error);
}

fn at_route_entry(p: &Parser<'_>) -> bool {
    p.at(IDENT) && HTTP_METHODS.contains(&p.current_text()) && p.nth(1) == SLASH
}

/// The number of tokens in a member key (`name`, `Todo.priority`) when one
/// followed by `:` starts here.
fn member_key_len(p: &Parser<'_>) -> Option<usize> {
    if p.current() != IDENT {
        return None;
    }
    let mut i = 1;
    while p.nth(i) == DOT && (p.nth(i + 1) == IDENT || p.nth(i + 1).is_keyword()) {
        i += 2;
    }
    (p.nth(i) == COLON).then_some(i)
}

fn member_item(p: &mut Parser<'_>, context: Members) {
    match p.current() {
        DOT3 => elision(p),
        _ if at_decl(p) && !(p.current().is_keyword() && p.nth(1) == COLON) => decl(p),
        _ if at_route_entry(p) => route_entry(p),
        IDENT => {
            if member_key_len(p).is_some() {
                member(p, context);
            } else if p.nth_on_new_line(1) || p.nth(1) == R_BRACE || p.nth(1) == EOF {
                bare_member(p, context);
            } else {
                member_without_colon(p, context);
            }
        }
        kind if kind.is_keyword() && p.nth(1) == COLON => {
            reserved_word(p);
            member(p, context);
        }
        COLON => {
            let error = p
                .error_here(&rule::SYNTAX_EXPECTED_NAME, "This member has no name")
                .label("expected a name before `:`")
                .reason("Every member inside braces is `name: value`.")
                .next("Add the member's name before `:`");
            p.push_error(error);
            let m = p.start();
            p.bump(COLON);
            member_value(p, context);
            m.complete(p, MEMBER);
        }
        _ => {
            let found = p.found();
            let error = p
                .error_here(
                    &rule::SYNTAX_UNEXPECTED_TOKEN,
                    format!("Expected a member, found {found}"),
                )
                .label("expected `name: value`")
                .reason("Inside braces, each line holds one member, written `name: value`.")
                .next("Write this line as `name: value`");
            p.push_error(error);
            skip_line_or_token(p);
        }
    }
}

fn skip_line_or_token(p: &mut Parser<'_>) {
    let m = p.start();
    loop {
        skip_balanced(p);
        if p.at_eof() || p.at(R_BRACE) || p.on_new_line() {
            break;
        }
    }
    m.complete(p, ERROR);
}

fn elision(p: &mut Parser<'_>) {
    if !p.snippet {
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "`...` stands for code left out of an example",
            )
            .label("not Jadpo code")
            .reason("Documentation examples use `...` for code they leave out; a source file has to say what it means.")
            .next("Replace `...` with the code it stands for");
        p.push_error(error);
    }
    let m = p.start();
    p.bump(DOT3);
    m.complete(p, ELISION);
}

fn member_key(p: &mut Parser<'_>) {
    let k = p.start();
    let n = p.start();
    p.bump_remap(IDENT);
    n.complete(p, NAME);
    while p.at(DOT) {
        p.bump(DOT);
        let n = p.start();
        p.bump_remap(IDENT);
        n.complete(p, NAME);
    }
    k.complete(p, MEMBER_KEY);
}

fn member(p: &mut Parser<'_>, context: Members) {
    let m = p.start();
    member_key(p);
    p.bump(COLON);
    member_value(p, context);
    m.complete(p, MEMBER);
    member_end(p);
}

fn member_value(p: &mut Parser<'_>, context: Members) {
    if p.at(L_BRACE) {
        member_list(p, context);
        return;
    }
    if p.on_new_line() || p.at(R_BRACE) || p.at_eof() {
        let key = member_key_before_colon(p);
        let (start, end) = p.nth_range_before();
        let error = SyntaxError::new(
            &rule::SYNTAX_EXPECTED_EXPRESSION,
            format!("`{key}:` has no value"),
            start,
            end,
        )
        .label("expected a value after `:`")
        .reason("Every member inside braces is `name: value`, with the value on the same line.")
        .next(format!("Write the value after `{key}:`"));
        p.push_error(error);
        return;
    }
    expr(p);
}

/// The key text of the member being parsed, for messages.
fn member_key_before_colon(p: &Parser<'_>) -> String {
    let end = p.prev_end() as usize;
    let text = &p.text()[..end.saturating_sub(1)];
    let start = text
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |i| i + 1);
    text[start..].to_string()
}

/// After a member: a new line, `}`, or another member on the same line
/// (which the formatter moves to its own line).
fn member_end(p: &mut Parser<'_>) {
    if p.at_eof() || p.at(R_BRACE) || p.on_new_line() || p.at(DOT3) {
        return;
    }
    if p.at(IDENT) && (member_key_len(p).is_some() || p.nth(1) == R_BRACE) {
        return;
    }
    let found = p.found();
    let error = p
        .error_here(&rule::SYNTAX_UNEXPECTED_TOKEN, "Expected the end of the line")
        .label("unexpected here")
        .reason(format!(
            "A member's value is one thing, such as a name, a call or a block; found {found} after it."
        ))
        .next("Remove it, or start a new member on its own line");
    p.push_error(error);
    skip_line(p);
}

fn bare_member(p: &mut Parser<'_>, context: Members) {
    if context == Members::Record {
        let word = p.current_text().to_string();
        let (start, end) = p.range();
        let error = SyntaxError::new(
            &rule::SYNTAX_COLON_REQUIRED,
            format!("`{word}` needs a value"),
            start,
            end,
        )
        .label("expected `name: value`")
        .reason("Every field in braces is written `name: value`; Jadpo has no shorthand that repeats the name.")
        .next(format!("Write `{word}: {word}`"))
        .edit(TextEdit::replace(start, end, format!("{word}: {word}")));
        p.push_error(error);
    }
    let m = p.start();
    let n = p.start();
    p.bump(IDENT);
    n.complete(p, NAME);
    m.complete(p, BARE_MEMBER);
}

/// `format email`, `kind NotFound`, `paid { ... }`: a member missing `:`.
fn member_without_colon(p: &mut Parser<'_>, context: Members) {
    let word = p.current_text().to_string();
    let (start, end) = p.range();
    let line_end = p.text()[end as usize..]
        .find('\n')
        .map_or(p.text().len(), |i| end as usize + i);
    let value = p.text()[end as usize..line_end]
        .split("//")
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let error = SyntaxError::new(
        &rule::SYNTAX_COLON_REQUIRED,
        format!("`{word}` needs a colon before its value"),
        start,
        end,
    )
    .label(format!("expected `:` after `{word}`"))
    .reason(
        "Every member inside braces is `name: value`, with a colon even when the value is a block.",
    )
    .next(if p.nth(1) == L_BRACE {
        format!("Write `{word}: {{`")
    } else {
        format!("Write `{word}: {value}`")
    })
    .edit(TextEdit::insert(end, ":"));
    p.push_error(error);
    let m = p.start();
    let k = p.start();
    let n = p.start();
    p.bump(IDENT);
    n.complete(p, NAME);
    k.complete(p, MEMBER_KEY);
    if p.at(L_BRACE) {
        member_list(p, context);
    } else if can_start_expr(p) {
        expr(p);
    }
    m.complete(p, MEMBER);
    skip_line(p);
}

fn route_entry(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(IDENT);
    route_path(p);
    body_start(p, true);
    m.complete(p, ROUTE_ENTRY);
}

fn route_path(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let mut first = true;
    while p.at(SLASH) && (first || p.nth_adjacent(0)) {
        first = false;
        let s = p.start();
        p.bump(SLASH);
        if p.at(PATH_SEGMENT) {
            p.bump(PATH_SEGMENT);
        }
        if p.at(L_BRACE) && p.nth_adjacent(0) {
            path_param(p);
        }
        s.complete(p, ROUTE_SEGMENT);
    }
    m.complete(p, ROUTE_PATH)
}

fn path_param(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.range();
    p.bump(L_BRACE);
    p.with_newlines(true, |p| {
        name(p, "this placeholder");
        if p.eat(COLON) {
            ty(p, "the placeholder's type");
        }
        if !p.eat(R_BRACE) {
            let found = p.found();
            let error = SyntaxError::new(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "This placeholder isn't closed",
                open.0,
                open.1,
            )
            .label("no matching `}`")
            .reason(format!(
                "A path placeholder is `{{name}}` or `{{name: Type}}`; found {found}."
            ))
            .next("Add `}` after the placeholder");
            p.push_error(error);
        }
    });
    m.complete(p, PATH_PARAM);
}

// --- statements -----------------------------------------------------------------

fn block(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let open = p.range();
    p.bump(L_BRACE);
    if !p.enter() {
        return m.complete(p, BLOCK);
    }
    p.with_newlines(false, |p| {
        p.with_records(true, |p| {
            while !p.at(R_BRACE) && !p.at_eof() {
                if ends_block(p) {
                    break;
                }
                let before = p.position();
                block_item(p);
                if p.position() == before {
                    p.bump_error();
                }
            }
        })
    });
    p.leave();
    close_brace(p, open);
    m.complete(p, BLOCK)
}

fn block_item(p: &mut Parser<'_>) {
    match p.current() {
        VAR_KW => var_stmt(p),
        RETURN_KW => return_stmt(p),
        IF_KW => if_stmt(p),
        FOR_KW => for_stmt(p),
        EMIT_KW => keyword_stmt(p, EMIT_KW, EMIT_STMT),
        ASSERT_KW => assert_stmt(p),
        DOT3 if p.nth_on_new_line(1) || p.nth(1) == R_BRACE => {
            elision(p);
            return;
        }
        ELSE_KW => {
            let error = p
                .error_here(&rule::SYNTAX_EXPECTED_STATEMENT, "`else` without an `if`")
                .label("no `if` before this")
                .reason("`else` follows the `}` that closes an `if` block.")
                .next("Put `else` right after the `if` block's `}`, or remove it");
            p.push_error(error);
            skip_line_or_token(p);
            return;
        }
        IDENT if member_key_len(p).is_some() => {
            member(p, Members::Declaration);
            return;
        }
        IDENT if p.at_word("while") || p.at_word("loop") => {
            let word = p.current_text().to_string();
            let error = p
                .error_here(&rule::SYNTAX_EXPECTED_STATEMENT, format!("Jadpo has no `{word}` loop"))
                .label("not a Jadpo statement")
                .reason("The only loop is `for item in items`, which always ends after its list; stop early with `return`.")
                .next("Loop over a list with `for`, or narrow it first with `collection.filter`");
            p.push_error(error);
            skip_statement(p);
            return;
        }
        IDENT if (p.at_word("let") || p.at_word("const")) && p.nth(1) == IDENT => {
            let word = p.current_text().to_string();
            let (start, end) = p.range();
            let error = SyntaxError::new(
                &rule::SYNTAX_EXPECTED_STATEMENT,
                format!("Jadpo declares variables with `var`, not `{word}`"),
                start,
                end,
            )
            .label("not a Jadpo statement")
            .reason("A variable is `var name: Type = value`, or `var mut` when it can change.")
            .next(format!("Use `var` instead of `{word}`"))
            .edit(TextEdit::replace(start, end, "var"));
            p.push_error(error);
            skip_statement(p);
            return;
        }
        IDENT if p.at_word("throw") && !p.nth_on_new_line(1) => {
            let error = p
                .error_here(&rule::SYNTAX_UNSUPPORTED_THROW, "Jadpo has no `throw`")
                .label("not a Jadpo statement")
                .reason("A callable raises one of its declared failures with `reject`, and lists it in `fails`.")
                .next("Use `reject` with a declared failure, as in `reject NoteRejected`");
            p.push_error(error);
            skip_statement(p);
            return;
        }
        _ => expr_stmt(p),
    }
    statement_end(p);
}

fn skip_statement(p: &mut Parser<'_>) {
    let m = p.start();
    loop {
        skip_balanced(p);
        if p.at_eof() || p.at(R_BRACE) || p.on_new_line() {
            break;
        }
    }
    m.complete(p, ERROR);
}

fn statement_end(p: &mut Parser<'_>) {
    if p.at_eof() || p.at(R_BRACE) || p.on_new_line() {
        return;
    }
    let found = p.found();
    let error = p
        .error_here(
            &rule::SYNTAX_UNEXPECTED_TOKEN,
            "Expected the end of the line",
        )
        .label("unexpected here")
        .reason(format!(
            "Each statement is on a line of its own, with no `;`; found {found} after one."
        ))
        .next("Remove it, or move it to a line of its own");
    p.push_error(error);
    skip_line(p);
}

fn var_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(VAR_KW);
    if p.at_word("mut") {
        p.bump_remap(MUT_KW);
    } else {
        p.eat(MUT_KW);
    }
    let var_name = p.current_text().to_string();
    let name_end = p.range().1;
    name(p, "this variable");
    let typed = p.eat(COLON);
    if typed {
        ty(p, "the variable's type");
    }
    if !typed {
        missing_variable_type(p, &var_name, name_end);
    }
    if p.at(EQ) && (p.nth_on_new_line(1) || matches!(p.nth(1), R_BRACE | EOF)) {
        let (start, end) = p.range();
        let error = SyntaxError::new(
            &rule::SYNTAX_EXPECTED_EXPRESSION,
            format!("`{var_name}` has no value"),
            start,
            end,
        )
        .label("expected a value after `=`")
        .reason(
            "A variable is given its value on the line that declares it: `var name: Type = value`.",
        )
        .next(format!("Write `{var_name}`'s value after `=`"));
        p.push_error(error);
        p.bump(EQ);
    } else if p.eat(EQ) {
        stmt_value(p);
    } else {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                format!("`{var_name}` needs a value"),
            )
            .label("expected `=` and a value")
            .reason(format!(
                "A variable is given its value where it is declared: `var name: Type = value`; found {found}."
            ))
            .next(format!("Write `= value` after `{var_name}`'s type"));
        p.push_error(error);
    }
    m.complete(p, VAR_STMT);
}

/// `var price = Price(0.0)`: every variable states its type (area 2).
fn missing_variable_type(p: &mut Parser<'_>, var_name: &str, name_end: u32) {
    let mut error = SyntaxError::new(
        &rule::SYNTAX_VARIABLE_TYPE_REQUIRED,
        format!("`{var_name}` needs a type"),
        name_end.saturating_sub(var_name.len() as u32),
        name_end,
    )
    .label("expected `: Type` after the name")
    .reason("Every variable states its type, so a reader sees it where the value is introduced.");
    // `= Price(0.0)`: the type is the constructor's, and the literal takes it.
    let constructor = p.at(EQ)
        && p.nth(1) == IDENT
        && p.nth_text(1).starts_with(|c: char| c.is_ascii_uppercase())
        && p.nth(2) == L_PAREN;
    let literal_at = if p.nth(3) == MINUS { 4 } else { 3 };
    let literal = matches!(
        p.nth(literal_at),
        INT_NUMBER | DECIMAL_NUMBER | STRING | UNIT_NUMBER | TRUE_KW | FALSE_KW
    ) && p.nth(literal_at + 1) == R_PAREN
        && (p.nth_on_new_line(literal_at + 2) || p.nth(literal_at + 2) == R_BRACE);
    if constructor {
        let ty = p.nth_text(1).to_string();
        if literal {
            let (call_start, _) = p.nth_range(1);
            let (_, call_end) = p.nth_range(literal_at + 1);
            let (lit_start, _) = p.nth_range(3);
            let (_, lit_end) = p.nth_range(literal_at);
            let value = p.text()[lit_start as usize..lit_end as usize].to_string();
            error = error
                .next(format!("Write `var {var_name}: {ty} = {value}`"))
                .edit(TextEdit::insert(name_end, format!(": {ty}")))
                .edit(TextEdit::replace(call_start, call_end, value));
        } else {
            error = error
                .next(format!("Write `var {var_name}: {ty} = ...`"))
                .edit(TextEdit::insert(name_end, format!(": {ty}")));
        }
    } else {
        error = error.next(format!(
            "Add the type after the name: `var {var_name}: Type = ...`"
        ));
    }
    p.push_error(error);
}

fn stmt_value(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    if p.at(ATTEMPT_KW) {
        Some(attempt_expr(p))
    } else {
        expr(p)
    }
}

fn attempt_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    let (attempt_start, _) = p.range();
    p.bump(ATTEMPT_KW);
    p.attempting(expr);
    if p.at(L_BRACE) && !p.on_new_line() {
        member_list(p, Members::Record);
    } else if p.at(IDENT) && p.nth(1) == COLON && !p.on_new_line() {
        trailing_mappings(p, attempt_start);
    }
    m.complete(p, ATTEMPT_EXPR)
}

/// `attempt Count(n) invalid: TooBig`: the mappings written after the call
/// instead of in its block (area 4, "Construction failures by rule"). They
/// are kept as the block's members, so nothing else reports them, and the
/// edit moves them into a block, one per line.
fn trailing_mappings(p: &mut Parser<'_>, attempt_start: u32) {
    let before = p.prev_end();
    let (start, _) = p.range();
    let list = p.start();
    let mut written = Vec::new();
    while p.at(IDENT) && p.nth(1) == COLON && !p.on_new_line() {
        let (from, _) = p.range();
        let mapping = p.start();
        member_key(p);
        p.bump(COLON);
        if can_start_expr(p) && !p.on_new_line() {
            p.with_records(true, expr);
        }
        mapping.complete(p, MEMBER);
        written.push(p.text()[from as usize..p.prev_end() as usize].to_string());
    }
    list.complete(p, MEMBER_LIST);
    let end = p.prev_end();
    let line_start = p.text()[..attempt_start as usize]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let indent: String = p.text()[line_start..]
        .chars()
        .take_while(|c| *c == ' ')
        .collect();
    let mut block = String::from(" {");
    for mapping in &written {
        block.push_str(&format!("\n{indent}    {mapping}"));
    }
    block.push_str(&format!("\n{indent}}}"));
    let first = written.first().cloned().unwrap_or_default();
    let error = SyntaxError::new(
        &rule::SYNTAX_MAPPING_BLOCK_REQUIRED,
        "An `attempt`'s mappings go in a block after the call",
        start,
        end,
    )
    .label("after the call, not in a block")
    .reason("An `attempt` maps its failures in a block, one per line, the same form writes and integration calls use; a construction's keys are the rules that can break, with `invalid:` covering the rest.")
    .next(if written.len() == 1 {
        format!("Move `{first}` into a block after the call: `{{ {first} }}`")
    } else {
        "Move the mappings into a block after the call, one per line".to_string()
    })
    .edit(TextEdit::replace(before, end, block));
    p.push_error(error);
}

fn return_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(RETURN_KW);
    if !(p.at_eof() || p.at(R_BRACE) || p.on_new_line()) {
        stmt_value(p);
    }
    m.complete(p, RETURN_STMT);
}

fn if_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(IF_KW);
    condition(p);
    if_block(p);
    // `else if` branches are siblings in one list, however many there are:
    // a chain never nests the tree.
    while p.at(ELSE_KW) && p.nth(1) == IF_KW {
        let branch = p.start();
        p.bump(ELSE_KW);
        p.bump(IF_KW);
        condition(p);
        if_block(p);
        branch.complete(p, ELSE_IF);
    }
    if p.at(ELSE_KW) {
        let e = p.start();
        p.bump(ELSE_KW);
        if p.at(L_BRACE) {
            block(p);
        } else {
            let found = p.found();
            let error = p
                .error_here(
                    &rule::SYNTAX_UNEXPECTED_TOKEN,
                    "Expected `{` or `if` after `else`",
                )
                .label("expected `{` or `if`")
                .reason(format!(
                    "`else` is followed by a block or another `if`; found {found}."
                ))
                .next("Add the `else` block in braces");
            p.push_error(error);
        }
        e.complete(p, ELSE_BRANCH);
    }
    m.complete(p, IF_STMT);
}

/// The block after an `if` or `else if` condition.
fn if_block(p: &mut Parser<'_>) {
    if p.at(L_BRACE) {
        block(p);
    } else {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `{` after the condition",
            )
            .label("expected `{`")
            .reason(format!("An `if` runs a block in braces; found {found}."))
            .next("Add `{` after the condition, and `}` after the block");
        p.push_error(error);
        skip_line(p);
    }
}

fn condition(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(L_PAREN) {
        let open = p.range();
        p.bump(L_PAREN);
        p.with_newlines(true, |p| {
            expr(p);
            if !p.eat(R_PAREN) {
                close_bracket_error(p, open, ")");
            }
        });
        if !p.at(L_BRACE) && infix_power(p).is_some() && !p.on_new_line() {
            let error = SyntaxError::new(
                &rule::SYNTAX_CONDITION_BRACKETS,
                "The whole condition goes in brackets",
                open.0,
                p.range().1,
            )
            .label("only part of the condition is bracketed")
            .reason("Conditions are always in brackets, `if (total > limit) {`, so the block can't be mistaken for part of the condition.")
            .next("Put brackets around the whole condition");
            p.push_error(error);
            while !p.at(L_BRACE) && !p.at_eof() && !p.on_new_line() {
                p.bump_error();
            }
        }
        m.complete(p, CONDITION);
        return;
    }
    let (start, _) = p.range();
    let checkpoint = p.error_count();
    p.with_records(false, |p| expr(p));
    let end = p.prev_end();
    if end <= start {
        // No condition at all: the expression's own error says so.
        m.complete(p, CONDITION);
        return;
    }
    let text = p.text()[start as usize..end as usize].to_string();
    let error = SyntaxError::new(
        &rule::SYNTAX_CONDITION_BRACKETS,
        "This condition needs brackets",
        start,
        end,
    )
    .label("expected `(` before the condition")
    .reason("Conditions are always in brackets, `if (total > limit) {`, so the block can't be mistaken for part of the condition.")
    .next(format!("Write `if ({text}) {{`"))
    .edit(TextEdit::insert(start, "("))
    .edit(TextEdit::insert(end, ")"));
    p.truncate_errors(checkpoint);
    p.push_error(error);
    m.complete(p, CONDITION);
}

fn close_bracket_error(p: &mut Parser<'_>, open: (u32, u32), close: &str) {
    let found = p.found();
    let error = SyntaxError::new(
        &rule::SYNTAX_UNEXPECTED_TOKEN,
        format!("Expected `{close}` to close this bracket"),
        open.0,
        open.1,
    )
    .label(format!("no matching `{close}`"))
    .reason(format!("Found {found} before the bracket was closed."))
    .next(format!("Add `{close}` where the bracket ends"));
    p.push_error(error);
}

fn for_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(FOR_KW);
    name(p, "the loop's variable");
    if !p.eat(IN_KW) {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `in` after the loop's variable",
            )
            .label("expected `in`")
            .reason(format!(
                "A loop is `for item in items {{ }}`; found {found}."
            ))
            .next("Write `in` and the list to loop over");
        p.push_error(error);
    }
    p.with_records(false, |p| expr(p));
    if p.at(L_BRACE) {
        block(p);
    } else {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `{` after the list",
            )
            .label("expected `{`")
            .reason(format!("A loop runs a block in braces; found {found}."))
            .next("Add the loop's block in braces");
        p.push_error(error);
    }
    m.complete(p, FOR_STMT);
}

fn keyword_stmt(p: &mut Parser<'_>, keyword: SyntaxKind, kind: SyntaxKind) {
    let m = p.start();
    p.bump(keyword);
    expr(p);
    m.complete(p, kind);
}

fn assert_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(ASSERT_KW);
    expr(p);
    if p.at(FAILS_KW) {
        let f = p.start();
        p.bump(FAILS_KW);
        ty(p, "the failure");
        f.complete(p, ASSERT_FAILS);
    }
    m.complete(p, ASSERT_STMT);
}

fn expr_stmt(p: &mut Parser<'_>) {
    let m = p.start();
    stmt_value(p);
    if p.at(EQ) && !p.on_new_line() {
        p.bump(EQ);
        stmt_value(p);
        m.complete(p, ASSIGN_STMT);
    } else {
        m.complete(p, EXPR_STMT);
    }
}

// --- expressions -----------------------------------------------------------------

fn can_start_expr(p: &Parser<'_>) -> bool {
    matches!(
        p.current(),
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
            | MATCH_KW
            | REJECT_KW
            | PROPAGATE_KW
            | ATTEMPT_KW
            | NOT_KW
            | MINUS
            | SLASH
            | DOT3
    )
}

pub(crate) fn expr(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    expr_bp(p, 0)
}

/// Binding power of the operator here, loosest first (area 3).
fn infix_power(p: &Parser<'_>) -> Option<(u8, SyntaxKind)> {
    let power = match p.current() {
        OR_KW => (1, OR_KW),
        AND_KW => (2, AND_KW),
        EQ2 | NEQ | IN_KW => (3, p.current()),
        IDENT if p.at_word("matches") => (3, MATCHES_KW),
        L_ANGLE | LTEQ | R_ANGLE | GTEQ => (4, p.current()),
        PLUS | MINUS => (5, p.current()),
        STAR | SLASH | PERCENT => (6, p.current()),
        // `&&` and `||` were reported by the lexer; read them as `and`/`or`.
        ERROR if p.current_text() == "||" => (1, ERROR),
        ERROR if p.current_text() == "&&" => (2, ERROR),
        _ => return None,
    };
    Some(power)
}

fn expr_bp(p: &mut Parser<'_>, min: u8) -> Option<CompletedMarker> {
    if !p.enter() {
        return None;
    }
    let result = expr_bp_inner(p, min);
    p.leave();
    result
}

fn expr_bp_inner(p: &mut Parser<'_>, min: u8) -> Option<CompletedMarker> {
    let mut lhs = unary(p)?;
    // Each operator nests the tree one level deeper.
    let mut levels = 0;
    while let Some((power, kind)) = infix_power(p) {
        if power <= min || p.line_break_ends() || !p.enter() {
            break;
        }
        levels += 1;
        let m = lhs.precede(p);
        p.bump_remap(kind);
        expr_bp(p, power);
        lhs = m.complete(p, BIN_EXPR);
    }
    for _ in 0..levels {
        p.leave();
    }
    Some(lhs)
}

fn unary(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    match p.current() {
        NOT_KW | MINUS => {
            let m = p.start();
            p.bump_any();
            expr_bp(p, 7);
            Some(m.complete(p, PREFIX_EXPR))
        }
        ERROR if p.current_text() == "!" => {
            let m = p.start();
            p.bump_any();
            expr_bp(p, 7);
            Some(m.complete(p, PREFIX_EXPR))
        }
        ATTEMPT_KW => {
            let error = p
                .error_here(
                    &rule::SYNTAX_ATTEMPT_PLACEMENT,
                    "`attempt` can only start a statement",
                )
                .label("inside an expression")
                .reason(
                    "Every place a callable can stop early is at the start of a line: a `var`, \
                     an assignment, a `return`, or a statement of its own.",
                )
                .next("Move the call into its own `var` statement with `attempt`, then use the variable");
            p.push_error(error);
            let done = attempt_expr(p);
            Some(postfix(p, done))
        }
        _ => {
            let done = primary(p)?;
            Some(postfix(p, done))
        }
    }
}

fn primary(p: &mut Parser<'_>) -> Option<CompletedMarker> {
    let m = p.start();
    let kind = match p.current() {
        INT_NUMBER | DECIMAL_NUMBER | UNIT_NUMBER | STRING | TRUE_KW | FALSE_KW | NONE_KW => {
            p.bump_any();
            LITERAL
        }
        IDENT => {
            p.bump(IDENT);
            NAME_REF
        }
        L_PAREN => {
            let open = p.range();
            p.bump(L_PAREN);
            p.with_newlines(true, |p| {
                p.with_records(true, |p| {
                    expr(p);
                    if !p.eat(R_PAREN) {
                        close_bracket_error(p, open, ")");
                    }
                })
            });
            PAREN_EXPR
        }
        L_BRACK => {
            list(p);
            LIST_EXPR
        }
        MATCH_KW => {
            match_expr(p);
            MATCH_EXPR
        }
        REJECT_KW => {
            p.bump(REJECT_KW);
            expr(p);
            REJECT_EXPR
        }
        PROPAGATE_KW => {
            p.bump(PROPAGATE_KW);
            PROPAGATE_EXPR
        }
        SLASH => {
            route_path(p);
            ROUTE_PATH_EXPR
        }
        DOT3 => {
            if !p.snippet {
                m.abandon(p);
                elision(p);
                return None;
            }
            p.bump(DOT3);
            ELISION_EXPR
        }
        _ => {
            m.abandon(p);
            expected_expression(p);
            return None;
        }
    };
    Some(m.complete(p, kind))
}

fn expected_expression(p: &mut Parser<'_>) {
    let found = p.found();
    let (start, end) = if p.at_eof() || p.on_new_line() || p.at(R_BRACE) {
        p.nth_range_before()
    } else {
        p.range()
    };
    let error = SyntaxError::new(&rule::SYNTAX_EXPECTED_EXPRESSION, "Expected a value here", start, end)
        .label(if p.at_eof() || p.on_new_line() {
            "expected a value after this".to_string()
        } else {
            "expected a value".to_string()
        })
        .reason(format!(
            "A value is a name, a literal such as `1` or `\"text\"`, a call or a bracketed expression; found {found}."
        ))
        .next("Write the value here");
    p.push_error(error);
    if !(p.at_eof()
        || p.on_new_line()
        || matches!(p.current(), R_BRACE | R_PAREN | R_BRACK | COMMA | L_BRACE))
    {
        p.bump_error();
    }
}

fn postfix(p: &mut Parser<'_>, lhs: CompletedMarker) -> CompletedMarker {
    // Each suffix nests the tree one level deeper.
    let mut levels = 0;
    let done = postfix_levels(p, lhs, &mut levels);
    for _ in 0..levels {
        p.leave();
    }
    done
}

fn postfix_levels(
    p: &mut Parser<'_>,
    mut lhs: CompletedMarker,
    levels: &mut u32,
) -> CompletedMarker {
    loop {
        let suffix = matches!(p.current(), DOT | L_PAREN | L_BRACE | L_ANGLE | QUESTION);
        if suffix && !p.enter() {
            return lhs;
        }
        if suffix {
            *levels += 1;
        }
        lhs = match p.current() {
            DOT if !p.line_break_ends() => {
                let m = lhs.precede(p);
                p.bump(DOT);
                field_name(p);
                m.complete(p, FIELD_EXPR)
            }
            L_PAREN if !p.on_new_line() => {
                let m = lhs.precede(p);
                arg_list(p);
                m.complete(p, CALL_EXPR)
            }
            // `attempt Count(n) { max: TooBig }`: the block is the
            // attempt's mappings.
            L_BRACE if p.attempted && lhs.kind() == CALL_EXPR => return lhs,
            L_BRACE if p.records_allowed && !p.on_new_line() => {
                let m = lhs.precede(p);
                member_list(p, Members::Record);
                m.complete(p, RECORD_EXPR)
            }
            L_ANGLE if matches!(lhs.kind(), NAME_REF | FIELD_EXPR) && generic_ahead(p) => {
                let m = lhs.precede(p);
                generic_arg_list(p);
                m.complete(p, GENERIC_EXPR)
            }
            QUESTION if !p.on_new_line() => {
                let m = lhs.precede(p);
                p.bump(QUESTION);
                m.complete(p, NULLABLE_EXPR)
            }
            _ => return lhs,
        };
    }
}

fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.range();
    p.bump(L_PAREN);
    if !p.enter() {
        m.complete(p, ARG_LIST);
        return;
    }
    p.with_newlines(true, |p| {
        p.with_records(true, |p| {
            let mut named: Vec<(String, (u32, u32))> = Vec::new();
            while !p.at(R_PAREN) && !p.at_eof() {
                if matches!(p.current(), R_BRACE | R_BRACK) {
                    break;
                }
                let before = p.position();
                arg(p, &mut named);
                if p.position() == before {
                    p.bump_error();
                }
                if p.eat(COMMA) {
                    continue;
                }
                if p.at(R_PAREN) || p.at_eof() {
                    break;
                }
                let found = p.found();
                let error = p
                    .error_here(
                        &rule::SYNTAX_UNEXPECTED_TOKEN,
                        "Expected `,` or `)` after this argument",
                    )
                    .label("expected `,` or `)`")
                    .reason(format!("Arguments are separated by commas; found {found}."))
                    .next("Add `,` between the arguments, or `)` after the last one");
                p.push_error(error);
                if !(p.on_new_line() && can_start_expr(p)) {
                    while !p.at_eof() && !matches!(p.current(), COMMA | R_PAREN | R_BRACE) {
                        p.bump_error();
                    }
                    p.eat(COMMA);
                }
            }
        })
    });
    p.leave();
    if !p.eat(R_PAREN) {
        close_bracket_error(p, open, ")");
    }
    m.complete(p, ARG_LIST);
}

fn arg(p: &mut Parser<'_>, named: &mut Vec<(String, (u32, u32))>) {
    let m = p.start();
    if p.at(IDENT) && p.nth(1) == COLON {
        let word = p.current_text().to_string();
        let range = p.range();
        if let Some((_, first)) = named.iter().find(|(name, _)| *name == word) {
            let error = SyntaxError::new(
                &rule::SYNTAX_NAMED_ARGUMENT_DUPLICATE,
                format!("`{word}:` is given twice"),
                range.0,
                range.1,
            )
            .label("given again here")
            .also(first.0, first.1, "first given here")
            .reason("Each named argument is given once.")
            .next(format!("Remove one of the `{word}:` arguments"));
            p.push_error(error);
        }
        named.push((word, range));
        name(p, "this argument");
        p.bump(COLON);
        expr(p);
    } else if p.at(IDENT) && p.nth(1) == THIN_ARROW {
        let lambda = p.start();
        name(p, "the function's parameter");
        p.bump(THIN_ARROW);
        expr(p);
        lambda.complete(p, LAMBDA_EXPR);
    } else {
        if let Some((first, first_range)) = named.first() {
            let (start, _) = p.range();
            let checkpoint = p.position();
            let first = first.clone();
            let first_range = *first_range;
            expr(p);
            let end = p.prev_end();
            if p.position() != checkpoint {
                let error = SyntaxError::new(
                    &rule::SYNTAX_POSITIONAL_AFTER_NAMED,
                    "A positional argument follows a named one",
                    start,
                    end,
                )
                .label("not named")
                .also(first_range.0, first_range.1, "named from here")
                .reason("Once an argument is named, every argument after it is named too.")
                .next(format!("Name this argument, or move it before `{first}:`"));
                p.push_error(error);
            }
        } else {
            expr(p);
        }
    }
    m.complete(p, ARG);
}

fn list(p: &mut Parser<'_>) {
    let open = p.range();
    p.bump(L_BRACK);
    if !p.enter() {
        return;
    }
    p.with_newlines(true, |p| {
        p.with_records(true, |p| {
            while !p.at(R_BRACK) && !p.at_eof() {
                if matches!(p.current(), R_BRACE | R_PAREN) {
                    break;
                }
                let before = p.position();
                if let Some(item) = expr(p)
                    && (p.at_word("asc") || p.at_word("desc"))
                {
                    let m = item.precede(p);
                    let kind = if p.at_word("asc") { ASC_KW } else { DESC_KW };
                    p.bump_remap(kind);
                    m.complete(p, ORDERED_EXPR);
                }
                if p.position() == before {
                    p.bump_error();
                }
                if p.eat(COMMA) {
                    continue;
                }
                if p.at(R_BRACK) || p.at_eof() {
                    break;
                }
                let found = p.found();
                let error = p
                    .error_here(
                        &rule::SYNTAX_UNEXPECTED_TOKEN,
                        "Expected `,` or `]` after this item",
                    )
                    .label("expected `,` or `]`")
                    .reason(format!(
                        "List items are separated by commas; found {found}."
                    ))
                    .next("Add `,` between the items, or `]` after the last one");
                p.push_error(error);
                if !(p.on_new_line() && can_start_expr(p)) {
                    while !p.at_eof() && !matches!(p.current(), COMMA | R_BRACK | R_BRACE) {
                        p.bump_error();
                    }
                    p.eat(COMMA);
                }
            }
        })
    });
    p.leave();
    if !p.eat(R_BRACK) {
        close_bracket_error(p, open, "]");
    }
}

fn match_expr(p: &mut Parser<'_>) {
    p.bump(MATCH_KW);
    p.with_records(false, |p| expr(p));
    if !p.at(L_BRACE) {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `{` and the match's arms",
            )
            .label("expected `{`")
            .reason(format!(
                "A `match` lists its arms in braces, one per line; found {found}."
            ))
            .next("Add the arms in braces after the value being matched");
        p.push_error(error);
        return;
    }
    let arms = p.start();
    let open = p.range();
    p.bump(L_BRACE);
    if !p.enter() {
        arms.complete(p, MATCH_ARM_LIST);
        return;
    }
    p.with_newlines(false, |p| {
        p.with_records(true, |p| {
            while !p.at(R_BRACE) && !p.at_eof() {
                if ends_block(p) {
                    break;
                }
                let before = p.position();
                match_arm(p);
                if p.position() == before {
                    p.bump_error();
                }
                statement_end(p);
            }
        })
    });
    p.leave();
    close_brace(p, open);
    arms.complete(p, MATCH_ARM_LIST);
}

fn match_arm(p: &mut Parser<'_>) {
    let m = p.start();
    pattern(p);
    if p.at(FAT_ARROW) {
        let (start, end) = p.range();
        let error = SyntaxError::new(&rule::SYNTAX_MATCH_ARROW, "Match arms use `->`", start, end)
            .label("expected `->`")
            .reason("`=>` was removed: an arm uses the same arrow as a result type.")
            .next("Use `->` instead of `=>`")
            .edit(TextEdit::replace(start, end, "->"));
        p.push_error(error);
        p.bump(FAT_ARROW);
    } else if !p.eat(THIN_ARROW) {
        let found = p.found();
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `->` after the pattern",
            )
            .label("expected `->`")
            .reason(format!("An arm is `pattern -> result`; found {found}."))
            .next("Write `->` and the arm's result after the pattern");
        p.push_error(error);
        skip_line(p);
        m.complete(p, MATCH_ARM);
        return;
    }
    if p.at(L_BRACE) {
        block(p);
    } else if p.on_new_line() || p.at(R_BRACE) || p.at_eof() {
        let (start, end) = p.nth_range_before();
        let error = SyntaxError::new(&rule::SYNTAX_EXPECTED_EXPRESSION, "This arm has no result", start, end)
            .label("expected a result after `->`")
            .reason("An arm's result is a value, `reject`, `propagate` or a block, on the same line as its pattern.")
            .next("Write the arm's result after `->`");
        p.push_error(error);
    } else {
        expr(p);
    }
    m.complete(p, MATCH_ARM);
}

fn pattern(p: &mut Parser<'_>) {
    match p.current() {
        UNDERSCORE => {
            let m = p.start();
            p.bump(UNDERSCORE);
            m.complete(p, WILDCARD_PAT);
        }
        MINUS | INT_NUMBER | DECIMAL_NUMBER | UNIT_NUMBER | STRING | TRUE_KW | FALSE_KW
        | NONE_KW => {
            let m = p.start();
            p.eat(MINUS);
            let l = p.start();
            if matches!(
                p.current(),
                INT_NUMBER | DECIMAL_NUMBER | UNIT_NUMBER | STRING | TRUE_KW | FALSE_KW | NONE_KW
            ) {
                p.bump_any();
            }
            l.complete(p, LITERAL);
            m.complete(p, LITERAL_PAT);
        }
        IDENT if p.at_word("failure") && p.nth(1) == IDENT && !p.nth_on_new_line(1) => {
            let (start, _) = p.range();
            let (next, _) = p.nth_range(1);
            let failure = p.nth_text(1).to_string();
            let error = SyntaxError::new(
                &rule::SYNTAX_MATCH_PATTERN,
                "Failure arms have no `failure` prefix",
                start,
                start + 7,
            )
            .label("not part of a pattern")
            .reason("Failure names are types of their own, so an arm names the failure alone.")
            .next(format!("Write `{failure} -> ...`"))
            .edit(TextEdit::replace(start, next, ""));
            p.push_error(error);
            p.bump_error();
            case_pattern(p);
        }
        IDENT => case_pattern(p),
        _ => {
            let found = p.found();
            let error = p
                .error_here(&rule::SYNTAX_MATCH_PATTERN, "Expected a pattern")
                .label("expected a pattern")
                .reason(format!(
                    "An arm starts with a case such as `Status.open`, `some(x)`, a literal or `_`; found {found}."
                ))
                .next("Start the arm with the case it handles");
            p.push_error(error);
            if !matches!(p.current(), THIN_ARROW | FAT_ARROW | R_BRACE) && !p.at_eof() {
                p.bump_error();
            }
        }
    }
}

fn case_pattern(p: &mut Parser<'_>) {
    let m = p.start();
    let start = p.range().0;
    path(p);
    if p.at(L_PAREN) && !p.on_new_line() {
        let b = p.start();
        p.bump(L_PAREN);
        name(p, "the case's data");
        if !p.eat(R_PAREN) {
            let found = p.found();
            let error = p
                .error_here(&rule::SYNTAX_MATCH_PATTERN, "Expected `)` after the name")
                .label("expected `)`")
                .reason(format!(
                    "A case names its data with one name in brackets, as in `some(x)`; found {found}."
                ))
                .next("Close the brackets after the name");
            p.push_error(error);
        }
        b.complete(p, PAT_BINDING);
    } else if p.at(L_BRACE) && !p.on_new_line() {
        let case_end = p.prev_end();
        let case = p.text()[start as usize..case_end as usize].to_string();
        let variant = case.rsplit('.').next().unwrap_or("value").to_string();
        let (brace_start, _) = p.range();
        let e = p.start();
        skip_balanced(p);
        e.complete(p, ERROR);
        let brace_end = p.prev_end();
        let error = SyntaxError::new(
            &rule::SYNTAX_MATCH_PATTERN,
            "A match arm names a case's data in brackets",
            brace_start,
            brace_end,
        )
        .label("expected `(name)`")
        .reason(format!(
            "An arm binds a case's data to one name, `{case}({variant})`, and reads its fields from it, as in `{variant}.field`."
        ))
        .next(format!("Write `{case}({variant})`"))
        .edit(TextEdit::replace(case_end, brace_end, format!("({variant})")));
        p.push_error(error);
    }
    m.complete(p, CASE_PAT);
}

// --- templates -------------------------------------------------------------------

fn template_tag_text(p: &Parser<'_>) -> String {
    let mut i = 1;
    let mut words = Vec::new();
    while !matches!(p.nth(i), R_DBRACE | EOF) && i < 6 {
        words.push(p.nth_text(i).to_string());
        i += 1;
    }
    words.join(" ")
}

fn skip_template_tag(p: &mut Parser<'_>) {
    p.bump_any();
    while !p.at(R_DBRACE) && !p.at_eof() {
        p.bump_any();
    }
    p.eat(R_DBRACE);
}

fn at_tag(p: &Parser<'_>, word: SyntaxKind) -> bool {
    p.at(L_DBRACE) && p.nth(1) == word
}

fn at_tag_word(p: &Parser<'_>, word: &str) -> bool {
    p.at(L_DBRACE) && p.nth_word(1, word)
}

/// Parts up to the end of the input, or to an `{{ else }}` or `{{ end }}`
/// when inside a block. Past the depth limit, the rest of the template is
/// skipped without recursing.
fn template_body(p: &mut Parser<'_>) {
    let m = p.start();
    if !p.enter() {
        while !p.at_eof() {
            p.bump_error();
        }
        m.complete(p, TEMPLATE_BODY);
        return;
    }
    while !p.at_eof() {
        if at_tag(p, ELSE_KW) || at_tag_word(p, "end") {
            break;
        }
        match p.current() {
            TEMPLATE_TEXT => {
                let t = p.start();
                p.bump(TEMPLATE_TEXT);
                t.complete(p, TEMPLATE_LITERAL);
            }
            L_DBRACE if p.nth(1) == IF_KW => template_if(p),
            L_DBRACE if p.nth(1) == FOR_KW => template_for(p),
            L_DBRACE if p.nth_word(1, "include") => {
                let t = p.start();
                p.bump(L_DBRACE);
                p.bump_remap(INCLUDE_KW);
                expr(p);
                close_template_tag(p);
                t.complete(p, TEMPLATE_INCLUDE);
            }
            L_DBRACE => {
                let t = p.start();
                p.bump(L_DBRACE);
                expr(p);
                close_template_tag(p);
                t.complete(p, TEMPLATE_VALUE);
            }
            _ => p.bump_error(),
        }
    }
    p.leave();
    m.complete(p, TEMPLATE_BODY);
}

fn close_template_tag(p: &mut Parser<'_>) {
    if p.eat(R_DBRACE) {
        return;
    }
    let found = p.found();
    let error = p
        .error_here(
            &rule::SYNTAX_UNEXPECTED_TOKEN,
            "Expected `}}` to close this tag",
        )
        .label("expected `}}`")
        .reason(format!(
            "A template tag holds one value or keyword, then `}}}}`; found {found}."
        ))
        .next("Add `}}` at the end of the tag");
    p.push_error(error);
    while !p.at(R_DBRACE) && !p.at_eof() && !p.at(TEMPLATE_TEXT) {
        p.bump_error();
    }
    p.eat(R_DBRACE);
}

fn template_if(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.range();
    let tag = p.start();
    p.bump(L_DBRACE);
    p.bump(IF_KW);
    condition(p);
    close_template_tag(p);
    tag.complete(p, TEMPLATE_IF_TAG);
    template_body(p);
    while at_tag(p, ELSE_KW) {
        let branch = p.start();
        let tag = p.start();
        p.bump(L_DBRACE);
        p.bump(ELSE_KW);
        if p.eat(IF_KW) {
            condition(p);
        }
        close_template_tag(p);
        tag.complete(p, TEMPLATE_ELSE_TAG);
        template_body(p);
        branch.complete(p, TEMPLATE_ELSE);
    }
    end_tag(p, open, "if");
    m.complete(p, TEMPLATE_IF);
}

fn template_for(p: &mut Parser<'_>) {
    let m = p.start();
    let open = p.range();
    let tag = p.start();
    p.bump(L_DBRACE);
    p.bump(FOR_KW);
    name(p, "the loop's variable");
    if !p.eat(IN_KW) {
        let error = p
            .error_here(
                &rule::SYNTAX_UNEXPECTED_TOKEN,
                "Expected `in` after the loop's variable",
            )
            .label("expected `in`")
            .reason("A template loop is `{{ for item in items }}`.")
            .next("Write `in` and the list to loop over");
        p.push_error(error);
    }
    expr(p);
    close_template_tag(p);
    tag.complete(p, TEMPLATE_FOR_TAG);
    template_body(p);
    end_tag(p, open, "for");
    m.complete(p, TEMPLATE_FOR);
}

fn end_tag(p: &mut Parser<'_>, open: (u32, u32), what: &str) {
    if at_tag_word(p, "end") {
        let tag = p.start();
        p.bump(L_DBRACE);
        p.bump_remap(END_KW);
        close_template_tag(p);
        tag.complete(p, TEMPLATE_END_TAG);
        return;
    }
    let error = SyntaxError::new(
        &rule::SYNTAX_UNEXPECTED_TOKEN,
        format!("This `{{{{ {what} }}}}` has no `{{{{ end }}}}`"),
        open.0,
        open.1,
    )
    .label("never closed")
    .reason(format!(
        "Every `{{{{ {what} }}}}` block ends with `{{{{ end }}}}`."
    ))
    .next("Add `{{ end }}` where the block ends");
    p.push_error(error);
}

impl SyntaxError {
    fn message_override(mut self, message: &str) -> SyntaxError {
        self.message = message.to_string();
        self
    }
}
