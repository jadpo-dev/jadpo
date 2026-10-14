//! The layout the formatter owns (areas 1 and 14): indentation, spacing,
//! blank lines (one between declarations, none doubled), one member per
//! line, whole signatures on one line until they are too long, and every
//! comment kept where it was written.

use jadpo_fmt::{format_file, format_snippet};

fn fmt(text: &str) -> String {
    let out = format_file(text).unwrap_or_else(|errors| panic!("{errors:#?}"));
    assert_eq!(
        format_file(&out).expect("formatted output parses"),
        out,
        "formatting is idempotent"
    );
    out
}

#[test]
fn one_member_per_line_and_one_blank_line_between_declarations() {
    assert_eq!(
        fmt("type Flags = Object { done: Bool pinned: Bool }\ntype Count = Int {\n    min: 0\n}\n"),
        "type Flags = Object {\n    done: Bool\n    pinned: Bool\n}\n\ntype Count = Int {\n    min: 0\n}\n"
    );
}

#[test]
fn blank_lines_are_never_doubled_and_never_open_or_close_a_body() {
    assert_eq!(
        fmt(
            "\n\n\nentity Project {\n\n    id: Uuid\n\n\n\n    name: ProjectName\n\n    unique: {\n        name_per_owner: [owner_id, name]\n    }\n\n}\n\n\n\ntype A = B {}\n\n"
        ),
        "entity Project {\n    id: Uuid\n\n    name: ProjectName\n\n    unique: {\n        name_per_owner: [owner_id, name]\n    }\n}\n\ntype A = B {}\n"
    );
}

#[test]
fn empty_bodies_are_two_braces() {
    assert_eq!(
        fmt("type OrderId = Uuid {  }\n"),
        "type OrderId = Uuid {}\n"
    );
    assert_eq!(fmt("action a() -> Unit {\n}\n"), "action a() -> Unit {}\n");
}

#[test]
fn a_signature_stays_on_one_line_until_it_is_too_long() {
    assert_eq!(
        fmt("function f(x: T)\n    fails A\n    -> R\n{\n    return x\n}\n"),
        "function f(x: T) fails A -> R {\n    return x\n}\n"
    );
    assert_eq!(
        fmt(
            "action reschedule_todo(todo_id: Todo.id, due_at: DueAt) fails TodoNotFound, DueDateInPast -> TodoView {\n    return x\n}\n"
        ),
        "action reschedule_todo(\n    todo_id: Todo.id,\n    due_at: DueAt\n) fails TodoNotFound, DueDateInPast -> TodoView {\n    return x\n}\n"
    );
}

#[test]
fn spacing_is_normalised() {
    assert_eq!(
        fmt(
            "function  f ( a:Pence,b : Pence )->Pence{\n  var  total:Pence=Pence(a+b*  2)\n  return total\n}\n"
        ),
        "function f(a: Pence, b: Pence) -> Pence {\n    var total: Pence = Pence(a + b * 2)\n    return total\n}\n"
    );
    assert_eq!(
        fmt(
            "type Basket = Output {\n    lines: List< BasketLine >\n    tags: Map<Tag,Weight>?\n}\n"
        ),
        "type Basket = Output {\n    lines: List<BasketLine>\n    tags: Map<Tag, Weight>?\n}\n"
    );
}

#[test]
fn calls_never_get_a_space_before_their_brackets() {
    assert_eq!(
        fmt(
            "function f(x: T) -> T {\n    return match lookup(x) {\n        success (value) -> temporal.format (value)\n        Missing -> propagate\n    }\n}\n"
        ),
        "function f(x: T) -> T {\n    return match lookup(x) {\n        success(value) -> temporal.format(value)\n        Missing -> propagate\n    }\n}\n"
    );
}

#[test]
fn conditions_and_else_branches() {
    assert_eq!(
        fmt(
            "function f(a: Flag) -> Size {\n    if (a) { return \"x\" }\n    else if (not a) {\n        return \"y\"\n    }\n    else {\n    }\n    return \"z\"\n}\n"
        ),
        "function f(a: Flag) -> Size {\n    if (a) {\n        return \"x\"\n    } else if (not a) {\n        return \"y\"\n    } else {}\n    return \"z\"\n}\n"
    );
}

#[test]
fn long_calls_put_one_argument_per_line() {
    assert_eq!(
        fmt(
            "function f(d: Day) -> Instant {\n    return temporal.resolve(date: d, at: \"09:30\", zone: Zone.europe_london, overlap: LocalOverlap.reject, gap: LocalGap.reject,)\n}\n"
        ),
        "function f(d: Day) -> Instant {\n    return temporal.resolve(\n        date: d,\n        at: \"09:30\",\n        zone: Zone.europe_london,\n        overlap: LocalOverlap.reject,\n        gap: LocalGap.reject\n    )\n}\n"
    );
    assert_eq!(
        fmt("function f(a: A) -> A {\n    return g(\n        a,\n        a,\n    )\n}\n"),
        "function f(a: A) -> A {\n    return g(a, a)\n}\n"
    );
}

#[test]
fn short_lists_stay_on_one_line() {
    assert_eq!(
        fmt(
            "type TodoView = Output {\n    pick: [\n        Todo.id,\n        Todo.title\n    ]\n}\n"
        ),
        "type TodoView = Output {\n    pick: [Todo.id, Todo.title]\n}\n"
    );
}

#[test]
fn records_and_operation_blocks_hold_one_member_per_line() {
    assert_eq!(
        fmt(
            "action a(t: Todo) -> Todo {\n    var done: Todo = Todo.update(t) { set: { status: TodoStatus.done } }\n    return done\n}\n"
        ),
        "action a(t: Todo) -> Todo {\n    var done: Todo = Todo.update(t) {\n        set: {\n            status: TodoStatus.done\n        }\n    }\n    return done\n}\n"
    );
}

#[test]
fn an_attempts_mappings_are_one_per_line() {
    assert_eq!(
        fmt(
            "function f(n: Count) fails Low, High -> CakeCount {\n    var c: CakeCount = attempt CakeCount(n)   { min: Low max: High }\n    return c\n}\n"
        ),
        "function f(n: Count) fails Low, High -> CakeCount {\n    var c: CakeCount = attempt CakeCount(n) {\n        min: Low\n        max: High\n    }\n    return c\n}\n"
    );
    // An arm's failure with fields nests one level further.
    assert_eq!(
        fmt(
            "function f(n: Count) fails High -> CakeCount {\n    return attempt CakeCount(n) {\n        invalid: High { asked: n }\n    }\n}\n"
        ),
        "function f(n: Count) fails High -> CakeCount {\n    return attempt CakeCount(n) {\n        invalid: High {\n            asked: n\n        }\n    }\n}\n"
    );
}

#[test]
fn routes_keep_their_paths_whole() {
    assert_eq!(
        fmt(
            "routes Todos {\n    prefix: /todos\n    GET /{todo_id:Todo.id}/complete {\n        output: TodoView\n        run: complete_todo(todo_id)\n    }\n}\n"
        ),
        "routes Todos {\n    prefix: /todos\n    GET /{todo_id: Todo.id}/complete {\n        output: TodoView\n        run: complete_todo(todo_id)\n    }\n}\n"
    );
}

#[test]
fn comments_stay_where_they_were_written() {
    let text = "// Prices are whole pence.\n\n/// A price.\ntype Pence = Int {\n    min: 0    // never negative\n\n    // the largest a price can be\n    max: 100000\n    // nothing after max\n}\n// end of file\n";
    assert_eq!(
        fmt(text),
        "// Prices are whole pence.\n\n/// A price.\ntype Pence = Int {\n    min: 0  // never negative\n\n    // the largest a price can be\n    max: 100000\n    // nothing after max\n}\n// end of file\n"
    );
}

#[test]
fn trailing_comments_never_break_a_line_that_fits() {
    assert_eq!(
        fmt("function f(a: A) -> A {\n    return g(a, a) // why\n}\n"),
        "function f(a: A) -> A {\n    return g(a, a)  // why\n}\n"
    );
    assert_eq!(
        fmt("function f(a: A) -> A {\n    return g(a, // first\n        a)\n}\n"),
        "function f(a: A) -> A {\n    return g(\n        a,  // first\n        a\n    )\n}\n"
    );
}

#[test]
fn match_arms_are_one_per_line() {
    assert_eq!(
        fmt(
            "function f(o: Outcome) -> Label {\n    return match o { Outcome.pending->\"p\"\n  Outcome.paid(paid)   ->   paid.label }\n}\n"
        ),
        "function f(o: Outcome) -> Label {\n    return match o {\n        Outcome.pending -> \"p\"\n        Outcome.paid(paid) -> paid.label\n    }\n}\n"
    );
}

#[test]
fn a_file_with_syntax_errors_is_not_formatted() {
    let errors = format_file("type Count = Int {\n    min: 0 $\n}\n").expect_err("refused");
    assert_eq!(errors[0].rule.id, "syntax.unexpected_character");
}

#[test]
fn snippets_keep_their_elisions() {
    assert_eq!(
        format_snippet("entity Todo {\n    ...\n    due_at: Instant?\n}\n").expect("formats"),
        "entity Todo {\n    ...\n    due_at: Instant?\n}\n"
    );
}

#[test]
fn an_empty_file_stays_empty() {
    assert_eq!(fmt(""), "");
    assert_eq!(fmt("\n\n"), "");
}

#[test]
fn comparisons_keep_a_space_on_both_sides() {
    let source = "function f(a: Count, b: Count) -> Flag {\n    return Flag(a < b or a > b or a<=b or (a) >= 1)\n}\n";
    assert_eq!(
        fmt(source),
        "function f(a: Count, b: Count) -> Flag {\n    return Flag(a < b or a > b or a <= b or (a) >= 1)\n}\n"
    );
    assert_eq!(
        fmt("type Tags = Object {\n    tags: List<Tag>\n}\n"),
        "type Tags = Object {\n    tags: List<Tag>\n}\n",
        "type arguments stay tight"
    );
}
