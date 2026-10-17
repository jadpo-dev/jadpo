//! The shape of the tree: where comments land (the sink's one rule), what
//! the typed AST reads back, and templates.

use jadpo_syntax::ast::{self, AstNode, Decl, SourceFile};
use jadpo_syntax::{SyntaxKind, parse_file, parse_template};

fn file(text: &str) -> SourceFile {
    let parse = parse_file(text);
    assert!(parse.errors().is_empty(), "{:#?}", parse.errors());
    SourceFile::cast(parse.syntax()).expect("a source file")
}

#[test]
fn documentation_comments_are_a_declarations_first_children() {
    let source = file(
        "// a note\n\n/// Rounds to the penny.\n/// Ties go to even.\npublic function half_even(amount: Price) -> Price {\n    return amount\n}\n",
    );
    let decl = source.decls().next().expect("a declaration");
    let docs: Vec<String> = ast::doc_comments(decl.syntax())
        .iter()
        .map(|token| token.text().to_string())
        .collect();
    assert_eq!(docs, ["/// Rounds to the penny.", "/// Ties go to even."]);
    let Decl::FunctionDecl(function) = decl else {
        panic!("a function")
    };
    assert_eq!(
        function.name().expect("name").syntax().text().to_string(),
        "half_even"
    );
    assert!(function.visibility().is_some());
    assert_eq!(function.param_list().expect("params").params().count(), 1);
}

#[test]
fn a_trailing_comment_sits_beside_the_last_token_on_its_line() {
    let parse = parse_file("type Pence = Int {\n    min: 0  // never negative\n}\n");
    let comment = parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::COMMENT)
        .expect("the comment");
    let parent = comment.parent().expect("a parent");
    assert_eq!(
        parent.kind(),
        SyntaxKind::LITERAL,
        "with `0`, inside its node"
    );
}

#[test]
fn a_comment_on_its_own_line_leads_the_next_member() {
    let parse =
        parse_file("type Pence = Int {\n    min: 0\n    // the largest price\n    max: 100\n}\n");
    let comment = parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == SyntaxKind::COMMENT)
        .expect("the comment");
    let member = comment.parent().expect("a parent");
    assert_eq!(member.kind(), SyntaxKind::MEMBER);
    assert!(member.text().to_string().ends_with("max: 100"));
}

#[test]
fn templates_hold_jadpo_inside_double_braces() {
    let text = "Hello {{ display_name }},\n{{ if (due_at != none) }}Due {{ temporal.format(due_at) }}.{{ else if (late) }}Late.{{ else }}No date.{{ end }}\n{{ for todo in todos }}- {{ todo.title }}\n{{ end }}{{ include button(label: t.open_app, url: app_url) }}\n";
    let parse = parse_template(text);
    assert!(parse.errors().is_empty(), "{:#?}", parse.errors());
    assert_eq!(parse.syntax().to_string(), text);
    let kinds: Vec<SyntaxKind> = parse.syntax().descendants().map(|n| n.kind()).collect();
    for kind in [
        SyntaxKind::TEMPLATE_VALUE,
        SyntaxKind::TEMPLATE_IF,
        SyntaxKind::TEMPLATE_ELSE,
        SyntaxKind::TEMPLATE_FOR,
        SyntaxKind::TEMPLATE_INCLUDE,
        SyntaxKind::TEMPLATE_END_TAG,
    ] {
        assert!(kinds.contains(&kind), "{kind:?}");
    }
}

#[test]
fn an_unclosed_template_block_is_reported_where_it_opens() {
    let parse = parse_template("{{ if (a) }}text");
    let error = &parse.errors()[0];
    assert_eq!(error.message, "This `{{ if }}` has no `{{ end }}`");
    assert_eq!(error.start, 0);
    let parse = parse_template("text {{ end }}");
    assert_eq!(
        parse.errors()[0].message,
        "`{{ end }}` has no matching `{{ if }}` or `{{ for }}`"
    );
}

#[test]
fn a_field_access_reads_the_name_after_the_dot() {
    let source = file("function f(a: Thing) -> Thing {\n    return a.b.c\n}\n");
    let fields: Vec<String> = source
        .syntax()
        .descendants()
        .filter_map(ast::FieldExpr::cast)
        .map(|field| {
            let left = field.expr().expect("left").syntax().to_string();
            let name = field.name_ref().expect("name").syntax().to_string();
            format!("{left} . {name}")
        })
        .collect();
    assert_eq!(fields, ["a.b . c", "a . b"]);
}

#[test]
fn use_lines_name_a_folder_and_what_they_bring_in() {
    let source = file("use shared/types { Pence, Percent }\nuse menu\n\ntype A = Int {}\n");
    let lines: Vec<(String, Vec<String>)> = source
        .use_decls()
        .map(|line| {
            let path = line
                .use_path()
                .map(|path| path.syntax().text().to_string())
                .unwrap_or_default();
            let names = line
                .use_names()
                .into_iter()
                .flat_map(|names| names.name_refs())
                .map(|name| name.syntax().text().to_string())
                .collect();
            (path, names)
        })
        .collect();
    assert_eq!(
        lines,
        [
            (
                "shared/types".to_string(),
                vec!["Pence".to_string(), "Percent".to_string()]
            ),
            ("menu".to_string(), Vec::new()),
        ]
    );
    // The declarations are counted without the `use` lines.
    assert_eq!(source.decls().count(), 1);
}

#[test]
fn use_is_reserved_where_something_is_named() {
    let parse = jadpo_syntax::parse_file("function use() -> Unit {}\n");
    assert_eq!(
        parse.errors().first().map(|e| e.message.as_str()),
        Some("`use` is a reserved word")
    );
}

#[test]
fn decline_lines_are_the_first_children_of_the_declaration_below() {
    let source = file(
        "/// Rounds for the till.\ndecline library.similar_exists: \"Rounds to 5p\"\ndecline constant.similar_exists: \"Another reason\"\npublic function till_round(price: Pence) -> Pence {\n    return price\n}\n",
    );
    let decl = source.decls().next().expect("a declaration");
    assert_eq!(decl.syntax().kind(), SyntaxKind::FUNCTION_DECL);
    let declines: Vec<(String, String)> = ast::declines(decl.syntax())
        .map(|decline| {
            (
                decline.decline_rule().expect("rule").id(),
                decline.reason().expect("reason").text().to_string(),
            )
        })
        .collect();
    assert_eq!(
        declines,
        [
            (
                "library.similar_exists".to_string(),
                "\"Rounds to 5p\"".to_string()
            ),
            (
                "constant.similar_exists".to_string(),
                "\"Another reason\"".to_string()
            ),
        ]
    );
    let docs: Vec<String> = ast::doc_comments(decl.syntax())
        .iter()
        .map(|token| token.text().to_string())
        .collect();
    assert_eq!(docs, ["/// Rounds for the till."]);
    let Decl::FunctionDecl(function) = decl else {
        panic!("a function")
    };
    assert_eq!(
        function.name().expect("name").syntax().text().to_string(),
        "till_round"
    );
}

#[test]
fn a_decline_line_above_a_member_or_a_library_function_belongs_to_it() {
    let source = file(
        "type Box = Object {\n    decline personal.unread: \"Kept for the regulator\"\n    width: Int\n}\n\nlibrary rounding {\n    decline library.similar_exists: \"Rounds down\"\n    function capped(amount: Int) -> Int {\n        return amount\n    }\n}\n",
    );
    let owners: Vec<SyntaxKind> = source
        .syntax()
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::DECLINE)
        .filter_map(|node| node.parent())
        .map(|parent| parent.kind())
        .collect();
    assert_eq!(owners, [SyntaxKind::MEMBER, SyntaxKind::FUNCTION_DECL]);
}

/// The keyword sets the editor offers (area 14, "Completion") are the ones
/// the parser starts declarations, statements and expressions with.
#[test]
fn the_keyword_sets_start_what_they_say() {
    let first = |text: &str| {
        parse_file(text)
            .syntax()
            .children()
            .next()
            .map(|node| node.kind())
    };
    for kind in jadpo_syntax::DECLARATION_KEYWORDS {
        let word = kind.text().expect("a keyword");
        let found = first(&format!("{word} thing {{}}\n"));
        assert!(
            found.is_some_and(|kind| kind != SyntaxKind::ERROR),
            "`{word}` starts {found:?}"
        );
    }
    for word in jadpo_syntax::CONTEXTUAL_DECLARATIONS {
        let text = if *word == "test" {
            "test \"it works\" {}\n".to_string()
        } else {
            format!("{word} thing {{}}\n")
        };
        let found = first(&text);
        assert!(
            found.is_some_and(|kind| kind != SyntaxKind::ERROR),
            "`{word}` starts {found:?}"
        );
    }
    let in_body = |line: &str| {
        let parse = parse_file(&format!("action a() -> Int {{\n    {line}\n}}\n"));
        let block = parse
            .syntax()
            .descendants()
            .find(|node| node.kind() == SyntaxKind::BLOCK)
            .expect("a block");
        block.children().next().expect("a statement")
    };
    for kind in jadpo_syntax::STATEMENT_KEYWORDS {
        let word = kind.text().expect("a keyword");
        let statement = in_body(&format!("{word} x"));
        assert!(
            !matches!(statement.kind(), SyntaxKind::EXPR_STMT | SyntaxKind::ERROR),
            "`{word}` starts {:?}",
            statement.kind()
        );
    }
    for kind in jadpo_syntax::EXPRESSION_KEYWORDS {
        let word = kind.text().expect("a keyword");
        let statement = in_body(&format!("return {word} x"));
        let value = statement.children().next().map(|node| node.kind());
        assert!(
            value.is_some_and(|kind| kind != SyntaxKind::ERROR),
            "`{word}` starts {value:?}"
        );
    }
}

#[test]
fn an_inline_function_reads_back_one_or_two_parameters() {
    let source = file(
        "function f(a: Thing) -> Thing {\n    return map.filter(list.map(a, item -> item), (key, value) -> key)\n}\n",
    );
    let names: Vec<Vec<String>> = source
        .syntax()
        .descendants()
        .filter_map(ast::LambdaExpr::cast)
        .map(|lambda| {
            lambda
                .names()
                .map(|name| name.syntax().text().to_string())
                .collect()
        })
        .collect();
    assert_eq!(names, [vec!["item"], vec!["key", "value"]]);
    // A parenthesised expression is still one: only names followed by `->`
    // are parameters.
    let source = file("function f(a: Thing) -> Thing {\n    return g((a), b)\n}\n");
    assert_eq!(
        source
            .syntax()
            .descendants()
            .filter_map(ast::LambdaExpr::cast)
            .count(),
        0
    );
}
