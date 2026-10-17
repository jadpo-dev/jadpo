//! Area 3, logic: its decided examples check cleanly, each of its errors is
//! caught on its own mistake, one mistake gives one error, and a body edit
//! re-checks only its own file's bodies.

use crate::support;

use jadpo_db::{Database, Layout};
use salsa::Setter;
use support::rules;

fn check(source: &str) -> Vec<String> {
    rules(Layout::Loose, &[("main.jadpo", source)])
}

const NAMES: &str = "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n\ntype Label = Text {}\n\ntype Flag = Bool {}\n\ntype DeliveryState = Enum {\n    pending\n    sent\n    failed\n}\n\ntype ReceiptId = Uuid {}\n\ntype PaymentOutcome = Enum {\n    pending\n    paid: {\n        receipt_id: ReceiptId\n    }\n    declined\n}\n\n";

fn with_names(source: &str) -> Vec<String> {
    check(&format!("{NAMES}{source}"))
}

const CLEAN: &[String] = &[];

#[test]
fn functions_variables_and_conditions_check() {
    assert_eq!(
        with_names(
            "function line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)\n    return total\n}\n\nfunction with_delivery(price: Pence, quantity: Quantity, delivery: Pence) -> Pence {\n    var subtotal: Pence = line_total(price, quantity)\n    var mut total: Pence = subtotal\n    total = Pence(total + delivery)\n    return total\n}\n\nfunction size(total: Pence, limit: Pence) -> Label {\n    if (total > limit) {\n        return \"large\"\n    } else if (total > 10) {\n        return \"medium\"\n    } else {\n        return \"small\"\n    }\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn match_binds_variant_data_and_nullable_values() {
    assert_eq!(
        with_names(
            "type Listing = Object {\n    name: Label\n    subtitle: Label?\n}\n\nfunction receipt(outcome: PaymentOutcome) -> ReceiptId? {\n    return match outcome {\n        PaymentOutcome.pending -> none\n        PaymentOutcome.paid(paid) -> paid.receipt_id\n        PaymentOutcome.declined -> none\n    }\n}\n\nfunction headline(listing: Listing) -> Label {\n    return match listing.subtitle {\n        some(subtitle) -> subtitle\n        none -> listing.name\n    }\n}\n\nfunction outcome_label(outcome: PaymentOutcome) -> Label {\n    return match outcome {\n        PaymentOutcome.pending -> \"pending\"\n        PaymentOutcome.paid -> \"paid\"\n        PaymentOutcome.declined -> \"declined\"\n    }\n}\n\nfunction shout(label: Label) -> Label {\n    match label {\n        \"\" -> {\n            return \"unnamed\"\n        }\n        _ -> {\n            return label\n        }\n    }\n}\n\nfunction answer(flag: Flag) -> Label {\n    return match flag {\n        true -> \"yes\"\n        false -> \"no\"\n    }\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn unit_calls_attempt_and_bare_return_stand_alone() {
    assert_eq!(
        with_names(
            "action notify(state: DeliveryState) -> Unit {\n}\n\naction follow_up(state: DeliveryState) -> Unit {\n    match state {\n        DeliveryState.pending -> notify(state)\n        DeliveryState.sent -> {}\n        DeliveryState.failed -> {\n            notify(state)\n        }\n    }\n}\n\naction archive(state: DeliveryState, done: Flag) -> Unit {\n    if (done) {\n        return\n    }\n    notify(state)\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn loops_inline_functions_and_recursive_functions_check() {
    assert_eq!(
        with_names(
            "type OrderLine = Object {\n    subtotal: Pence\n}\n\ntype Comment = Object {\n    replies: List<Comment>\n}\n\ntype Replies = Int {\n    min: 0\n}\n\nfunction order_total(lines: List<OrderLine>) -> Pence {\n    var mut total: Pence = 0\n    for line in lines {\n        total = Pence(total + line.subtotal)\n    }\n    return total\n}\n\nfunction reply_count(comment: Comment) -> Replies {\n    var mut count: Replies = 0\n    for reply in comment.replies {\n        count = Replies(count + 1 + reply_count(reply))\n    }\n    return count\n}\n\nfunction subtotals(lines: List<OrderLine>, floor: Pence) -> List<Pence> {\n    var large: List<OrderLine> = collection.filter(lines, line -> line.subtotal > floor)\n    return collection.map(large, line -> line.subtotal)\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn inline_functions_take_the_list_and_the_expected_result() {
    let todos = "type TodoTitle = Text {\n    max_length: 80\n}\n\ntype TodoStatus = Enum {\n    open\n    done\n}\n\nentity Todo {\n    id: Uuid\n    title: TodoTitle\n    status: TodoStatus\n}\n\naction notify_owner(todo_id: Todo.id) -> Unit {}\n\n";
    // `todo.title` is a `Todo.title`; the list it builds is a
    // `List<TodoTitle>` because that is what the variable requires.
    assert_eq!(
        check(&format!(
            "{todos}function open_titles(todos: List<Todo>) -> List<TodoTitle> {{\n    var open: List<Todo> = collection.filter(todos, todo -> todo.status == TodoStatus.open)\n    var titles: List<TodoTitle> = collection.map(open, todo -> todo.title)\n    return titles\n}}\n\naction rename(todo: Todo, title: TodoTitle) -> Todo {{\n    var fresh: TodoTitle = title\n    return Todo.update(todo) {{\n        set: {{\n            title: fresh\n        }}\n    }}\n}}\n"
        )),
        // `fresh` is read in the update's block.
        Vec::<&str>::new()
    );
    assert_eq!(
        check(&format!(
            "{todos}function open_ids(todos: List<Todo>) -> List<Todo> {{\n    return collection.filter(todos, todo -> notify_owner(todo.id) == notify_owner(todo.id))\n}}\n"
        ))
        .first()
        .map(String::as_str),
        Some("effect.function_calls_action"),
        "an inline function is pure"
    );
}

#[test]
fn division_needs_a_divisor_that_excludes_zero() {
    assert_eq!(
        with_names(
            "function per_item(total: Pence, quantity: Quantity) -> Pence {\n    var boxes: Int = integer.divide(total, quantity, rounding: Rounding.up)\n    return Pence(boxes + total / Pence(2) - total % 3)\n}\n"
        ),
        ["type.divisor_zero"],
        "`Pence` allows zero"
    );
    assert_eq!(
        with_names(
            "type People = Int {\n    min: 1\n}\n\ntype Portion = Int {\n    min: 0\n}\n\nfunction share(total: People, people: People) -> Portion {\n    return Portion(total / people)\n}\n"
        ),
        CLEAN
    );
}

#[test]
fn each_mistake_gives_its_rule() {
    let cases: &[(&str, &str)] = &[
        (
            "action save(label: Label) -> Label {\n    return label\n}\n\nfunction pretend(label: Label) -> Label {\n    return save(label)\n}\n",
            "effect.function_calls_action",
        ),
        (
            "entity Customer {\n    id: Uuid\n}\n\nfunction find(id: Customer.id) -> Customer {\n    return attempt Customer.get(id)\n}\n",
            "effect.function_persistence",
        ),
        (
            "action again(label: Label) -> Unit {\n    again(label)\n}\n",
            "effect.recursive_action",
        ),
        (
            "function f(price: Pence) -> Pence {\n    if (price > 10) {\n        return price\n    }\n}\n",
            "semantic.missing_return",
        ),
        (
            "function f(price: Pence) -> Pence {\n    return\n}\n",
            "semantic.missing_return",
        ),
        (
            "function f(price: Pence) -> Pence {\n    return price\n    return 0\n}\n",
            "semantic.unreachable_code",
        ),
        (
            "function g(price: Pence) -> Pence {\n    return price\n}\n\naction f(price: Pence) -> Unit {\n    g(price)\n}\n",
            "semantic.unused_result",
        ),
        (
            "function f(price: Pence) -> Pence {\n    var spare: Pence = price\n    return price\n}\n",
            "semantic.unused_variable",
        ),
        (
            "function f(outcome: PaymentOutcome) -> Label {\n    return match outcome {\n        PaymentOutcome.pending -> \"pending\"\n        PaymentOutcome.paid(paid) -> \"paid\"\n        PaymentOutcome.declined -> \"declined\"\n    }\n}\n",
            "semantic.unused_variable",
        ),
        (
            "function f(price: Pence) -> Pence {\n    var price: Pence = 1\n    return price\n}\n",
            "semantic.duplicate_declaration",
        ),
        (
            "function f(a: Pence, b: Pence) -> Pence {\n    return a\n}\n\nfunction g(price: Pence) -> Pence {\n    return f(price)\n}\n",
            "type.argument_count",
        ),
        (
            "function f(price: Pence, label: Label) -> Pence {\n    return Pence(price + label)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "function f(price: Pence, quantity: Quantity) -> Pence {\n    return Pence(price + quantity)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "type Price = Decimal {\n    scale: 2\n}\n\nfunction f(price: Price, quantity: Quantity) -> Price {\n    return Price(price % quantity)\n}\n",
            "type.arithmetic_operand",
        ),
        (
            "function f(price: Pence) -> Pence {\n    var fixed: Pence = price\n    fixed = 1\n    return fixed\n}\n",
            "type.assign_immutable",
        ),
        (
            "function f(price: Pence) -> Pence {\n    price = 1\n    return price\n}\n",
            "type.assign_immutable",
        ),
        (
            "function f(price: Pence) -> Pence {\n    total = price\n    return price\n}\n",
            "type.assign_unknown",
        ),
        (
            "function f(price: Pence) -> Pence {\n    return Pence(price / 0)\n}\n",
            "type.divisor_zero",
        ),
        (
            "function f(price: Pence, quantity: Quantity) -> Flag {\n    return Flag(price == quantity)\n}\n",
            "type.incomparable",
        ),
        (
            "function f(price: Pence) -> Flag {\n    return Flag(price == none)\n}\n",
            "type.incomparable",
        ),
        (
            "function g(price: Pence) -> Pence {\n    return price\n}\n\nfunction f(price: Pence) -> Pence {\n    return g(item -> price)\n}\n",
            "type.inline_function_placement",
        ),
        (
            "function f(price: Pence) -> Flag {\n    return Flag(price and price)\n}\n",
            "type.logical_operand",
        ),
        (
            "function f(outcome: PaymentOutcome) -> Label {\n    return match outcome {\n        PaymentOutcome.pending -> \"pending\"\n        PaymentOutcome.paid -> {\n            var x: Label = \"paid\"\n            assert x == \"paid\"\n        }\n        PaymentOutcome.declined -> \"declined\"\n    }\n}\n",
            "type.match_arm_value",
        ),
        (
            "function f(state: DeliveryState) -> Label {\n    return match state {\n        DeliveryState.pending -> \"a\"\n        DeliveryState.pending -> \"b\"\n        DeliveryState.sent -> \"c\"\n        DeliveryState.failed -> \"d\"\n    }\n}\n",
            "type.match_duplicate_pattern",
        ),
        (
            "function f(state: DeliveryState) -> Label {\n    return match state {\n        DeliveryState.pending -> \"a\"\n        DeliveryState.sent -> \"b\"\n    }\n}\n",
            "type.match_non_exhaustive",
        ),
        (
            "function f(flag: Flag) -> Label {\n    return match flag {\n        true -> \"yes\"\n    }\n}\n",
            "type.match_non_exhaustive",
        ),
        (
            "function f(label: Label?) -> Label {\n    return match label {\n        some(text) -> text\n    }\n}\n",
            "type.match_non_exhaustive",
        ),
        (
            "function f(state: DeliveryState) -> Label {\n    return match state {\n        true -> \"yes\"\n        DeliveryState.pending -> \"a\"\n        DeliveryState.sent -> \"b\"\n        DeliveryState.failed -> \"c\"\n    }\n}\n",
            "type.match_pattern_type",
        ),
        (
            "function f(label: Label) -> Label {\n    return match label {\n        some(text) -> text\n        _ -> label\n    }\n}\n",
            "type.match_some_non_optional",
        ),
        (
            "function f(outcome: PaymentOutcome) -> Label {\n    return match outcome {\n        PaymentOutcome.pending(waiting) -> \"a\"\n        PaymentOutcome.paid -> \"b\"\n        PaymentOutcome.declined -> \"c\"\n    }\n}\n",
            "type.match_unknown_binding",
        ),
        (
            "function f(state: DeliveryState) -> Label {\n    return match state {\n        DeliveryState.pending -> \"a\"\n        DeliveryState.sant -> \"b\"\n        DeliveryState.failed -> \"c\"\n    }\n}\n",
            "type.match_unknown_variant",
        ),
        (
            "function f(label: Label) -> Label {\n    return match label {\n        _ -> label\n        \"\" -> \"unnamed\"\n    }\n}\n",
            "type.match_unreachable_pattern",
        ),
        (
            "function f(state: DeliveryState) -> Label {\n    return match state {\n        DeliveryState.pending -> \"a\"\n        _ -> \"b\"\n    }\n}\n",
            "type.match_wildcard_closed",
        ),
        (
            "function f(label: Label) -> Label {\n    return match label {\n        \"\" -> \"unnamed\"\n    }\n}\n",
            "type.match_wildcard_required",
        ),
        (
            "function f(lines: List<Pence>) -> Pence {\n    return Pence(collection.count(list: lines))\n}\n",
            "type.named_argument_unknown",
        ),
        (
            "function g(a: Pence) -> Pence {\n    return a\n}\n\nfunction f(price: Pence) -> Pence {\n    return g(a: price)\n}\n",
            "type.named_argument_unsupported",
        ),
        (
            "function f(label: Label, other: Label) -> Flag {\n    return Flag(label < other)\n}\n",
            "type.ordering_operand",
        ),
        (
            "function f(price: Pence?, other: Pence) -> Flag {\n    return Flag(price < other)\n}\n",
            "type.ordering_operand",
        ),
        (
            "function f(label: Label) -> Flag {\n    return Flag(not label)\n}\n",
            "type.unary_operand",
        ),
        (
            "function f(label: Label) -> Label {\n    return lable\n}\n",
            "type.unknown_value",
        ),
        (
            "function g(label: Label) -> Label {\n    return label\n}\n\nfunction f(label: Label) -> Label {\n    var h: Label = g\n    return h\n}\n",
            "type.not_a_value",
        ),
        (
            "function f(price: Pence) -> Label {\n    if (price) {\n        return \"yes\"\n    }\n    return \"no\"\n}\n",
            "type.mismatch",
        ),
        (
            "function f(price: Pence) -> Pence {\n    for item in price {\n        return item\n    }\n    return price\n}\n",
            "type.mismatch",
        ),
    ];
    for (source, rule) in cases {
        assert_eq!(with_names(source), [*rule], "{source}");
    }
}

#[test]
fn one_mistake_gives_one_error() {
    // A misspelt name: the variable it meant is not also unused.
    assert_eq!(
        with_names(
            "function f(price: Pence) -> Pence {\n    var total: Pence = price\n    return totl\n}\n"
        ),
        ["type.unknown_value"]
    );
    // Code after `return`: once, not again as an unused variable.
    assert_eq!(
        with_names(
            "function f(price: Pence) -> Pence {\n    return price\n    var spare: Pence = price\n    var other: Pence = spare\n}\n"
        ),
        ["semantic.unreachable_code"]
    );
    // A pattern of the wrong kind: not also a match that misses cases.
    assert_eq!(
        with_names(
            "function f(state: DeliveryState) -> Label {\n    return match state {\n        true -> \"yes\"\n        _ -> \"no\"\n    }\n}\n"
        ),
        ["type.match_pattern_type"]
    );
    // A match missing a case: every arm returns, so the function doesn't
    // also end without a result.
    assert_eq!(
        with_names(
            "function f(state: DeliveryState) -> Label {\n    match state {\n        DeliveryState.pending -> {\n            return \"a\"\n        }\n        DeliveryState.sent -> {\n            return \"b\"\n        }\n    }\n}\n"
        ),
        ["type.match_non_exhaustive"]
    );
    // A name declared twice is not also unused.
    assert_eq!(
        with_names(
            "function f(price: Pence) -> Pence {\n    var price: Pence = 1\n    return 1\n}\n"
        ),
        ["semantic.duplicate_declaration"]
    );
    // An unknown type: everything built from it checks quietly, operators
    // included.
    assert_eq!(
        with_names(
            "function f(price: Pence, odd: Missing) -> Flag {\n    var mixed: Pence = Pence(price + odd.amount)\n    return Flag(odd and mixed > odd)\n}\n"
        ),
        ["semantic.unknown_name"]
    );
}

#[test]
fn checking_is_deterministic() {
    let source = format!(
        "{NAMES}function f(price: Pence, label: Label) -> Pence {{\n    var spare: Pence = price\n    return Pence(price + label)\n}}\n"
    );
    let first = check(&source);
    assert_eq!(first, check(&source));
    assert_eq!(
        first,
        ["semantic.unused_variable", "type.arithmetic_operand"]
    );
}

const A: &str =
    "type Count = Int {\n    min: 0\n}\n\naction tally(x: Count) -> Count {\n    return x\n}\n";
const B: &str = "action total(x: Count) -> Count {\n    return tally(x)\n}\n";

#[test]
fn editing_a_body_rechecks_only_that_files_bodies() {
    let mut db = Database::recording();
    let project = support::project(&db, Layout::Loose, &[("a.jadpo", A), ("b.jadpo", B)]);
    let a = project.files(&db)[0];
    assert!(jadpo_check::check(&db, project).is_empty());
    let cold = db.take_executions();
    for query in [
        "typed_body",
        "action_calls",
        "action_cycles",
        "recursion_problems",
    ] {
        assert!(
            cold.iter().any(|key| key.starts_with(&format!("{query}("))),
            "{query} runs: {cold:?}"
        );
    }
    a.set_text(&mut db).to(A.replace(
        "    return x\n",
        "    var same: Count = x\n    return same\n",
    ));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(count("typed_body"), 1, "only a's body: {log:?}");
    assert_eq!(count("action_calls"), 1, "only a's calls: {log:?}");
    assert_eq!(
        count("action_cycles"),
        0,
        "a still calls nothing, so the cycles are reused: {log:?}"
    );
    assert_eq!(count("recursion_problems"), 0, "{log:?}");

    // The control: a new call between actions finds the cycles again.
    a.set_text(&mut db)
        .to(A.replace("    return x\n", "    return total(x)\n"));
    jadpo_check::check(&db, project);
    let log = db.take_executions();
    assert_eq!(
        log.iter()
            .filter(|key| key.starts_with("action_cycles("))
            .count(),
        1,
        "{log:?}"
    );
}

#[test]
fn a_new_call_that_closes_a_cycle_is_reported_where_it_is() {
    let mut db = Database::recording();
    let project = support::project(&db, Layout::Loose, &[("a.jadpo", A), ("b.jadpo", B)]);
    let a = project.files(&db)[0];
    assert!(jadpo_check::check(&db, project).is_empty());
    a.set_text(&mut db)
        .to(A.replace("    return x\n", "    return total(x)\n"));
    let problems = jadpo_check::check(&db, project);
    let found: Vec<(&str, &str)> = problems
        .iter()
        .map(|d| {
            (
                d.primary.as_ref().map_or("", |l| l.span.path.as_str()),
                d.rule.id,
            )
        })
        .collect();
    assert_eq!(found, [("b.jadpo", "effect.recursive_action")]);
}
