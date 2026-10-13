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
