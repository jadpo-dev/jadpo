//! Area 3, logic: the page's examples, run.

mod support;

use jadpo_interp::{FaultKind, MAX_DEPTH, Outcome, Value, on_large_stack};
use support::{decimal, fault, int, list, main, program, text, value, where_fault};

const NUMBERS: &str =
    "type Pence = Int {\n    min: 0\n}\n\ntype Quantity = Int {\n    min: 1\n    max: 1000\n}\n";

#[test]
fn a_function_computes_its_result() {
    let program = program(&[
        ("numbers.jadpo", NUMBERS),
        (
            "main.jadpo",
            "/// Total for one order line before discounts.\nfunction line_total(price: Pence, quantity: Quantity) -> Pence {\n    var total: Pence = Pence(price * quantity)\n    return total\n}\n",
        ),
    ]);
    assert_eq!(
        value(&program, "line_total", vec![int(125), int(4)]),
        int(500)
    );
}

#[test]
fn a_variable_that_can_change_is_pointed_at_new_values() {
    let program = main(
        "type Choice = Text {}\n\nfunction choose(initial: Choice, replacement: Choice) -> Choice {\n    var mut selected: Choice = initial\n    selected = replacement\n    return selected\n}\n",
    );
    assert_eq!(
        value(&program, "choose", vec![text("a"), text("b")]),
        text("b")
    );
}

#[test]
fn an_else_if_chain_takes_the_first_condition_that_holds() {
    let program = main(
        "type Count = Int {}\n\ntype Size = Text {}\n\nfunction size(count: Count, maximum: Count) -> Size {\n    if (count > maximum) {\n        return \"large\"\n    } else if (count > 10) {\n        return \"medium\"\n    } else if (count > 5) {\n        return \"some\"\n    }\n    return \"small\"\n}\n",
    );
    for (count, expected) in [(200, "large"), (11, "medium"), (6, "some"), (1, "small")] {
        assert_eq!(
            value(&program, "size", vec![int(count), int(100)]),
            text(expected)
        );
    }
}

#[test]
fn operators_work_on_the_representation() {
    let program = main(
        "type Score = Int {}\n\ntype Ratio = Decimal {\n    scale: 2\n}\n\ntype Phrase = Text {}\n\ntype Flag = Bool {}\n\ntype Divisor = Int {\n    min: 1\n}\n\nfunction total(left: Score, right: Score) -> Score {\n    return Score(left + right * 2 - left % 3)\n}\n\nfunction label(prefix: Phrase, suffix: Phrase) -> Phrase {\n    return Phrase(prefix + \" \" + suffix)\n}\n\nfunction allowed(left: Score, right: Score, disabled: Flag) -> Flag {\n    return Flag(not disabled and (left < right or left == right))\n}\n\nfunction flip(n: Score) -> Score {\n    return Score(-n)\n}\n\nfunction halves(n: Divisor, d: Divisor) -> Score {\n    return Score(n / d)\n}\n\nfunction scaled(r: Ratio, n: Score) -> Ratio {\n    return Ratio(r * n)\n}\n",
    );
    assert_eq!(value(&program, "total", vec![int(7), int(5)]), int(16));
    assert_eq!(
        value(&program, "label", vec![text("Buy"), text("milk")]),
        text("Buy milk")
    );
    assert_eq!(
        value(
            &program,
            "allowed",
            vec![int(1), int(2), Value::Bool(false)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "allowed",
            vec![int(3), int(2), Value::Bool(false)]
        ),
        Value::Bool(false)
    );
    assert_eq!(value(&program, "flip", vec![int(4)]), int(-4));
    // Question 7: `Int` division drops the remainder.
    assert_eq!(value(&program, "halves", vec![int(10), int(3)]), int(3));
    // `*` may mix a `Decimal` with an `Int`.
    assert_eq!(
        value(&program, "scaled", vec![decimal("1.25"), int(3)]).to_string(),
        "3.75"
    );
    // Arithmetic is checked: overflow is a fault at the operator's line.
    let overflow = fault(&program, "flip", vec![int(i64::MIN)]);
    assert_eq!(
        where_fault(&program, &overflow),
        (FaultKind::IntOverflow, vec!["main.jadpo:28".to_string()])
    );
}

#[test]
fn and_and_or_stop_as_soon_as_the_answer_is_known() {
    let program = main(
        "type Score = Int {}\n\ntype Flag = Bool {}\n\nfunction guarded(skip: Flag, n: Score) -> Flag {\n    return Flag(skip or n * n > 0)\n}\n",
    );
    assert_eq!(
        value(&program, "guarded", vec![Value::Bool(true), int(i64::MAX)]),
        Value::Bool(true),
        "the right side never runs"
    );
    let overflow = fault(&program, "guarded", vec![Value::Bool(false), int(i64::MAX)]);
    assert_eq!(overflow.kind, FaultKind::IntOverflow);
}

#[test]
fn division_rounds_where_it_is_stored_and_by_name_otherwise() {
    // Question 7.
    let program = main(
        "type Amount = Decimal {\n    scale: 2\n    min: 0.01\n}\n\ntype Ratio = Decimal {\n    scale: 2\n}\n\ntype ItemCount = Int {\n    min: 0\n}\n\ntype BoxSize = Int {\n    min: 1\n}\n\ntype BoxCount = Int {}\n\nfunction ratio(a: Amount, b: Amount) -> Ratio {\n    return Ratio(a / b)\n}\n\nfunction boxes(items: ItemCount, box_size: BoxSize) -> BoxCount {\n    var boxes: BoxCount = BoxCount(integer.divide(items, box_size, rounding: Rounding.up))  // box_size can't be zero\n    return boxes\n}\n",
    );
    let ratio =
        |a: &str, b: &str| value(&program, "ratio", vec![decimal(a), decimal(b)]).to_string();
    assert_eq!(ratio("10.00", "4.00"), "2.50");
    assert_eq!(ratio("10.00", "3.00"), "3.33");
    assert_eq!(ratio("0.05", "2.00"), "0.02", "0.025, half to even");
    assert_eq!(ratio("0.15", "2.00"), "0.08", "0.075, half to even");
    assert_eq!(value(&program, "boxes", vec![int(25), int(10)]), int(3));
    assert_eq!(value(&program, "boxes", vec![int(20), int(10)]), int(2));
}

#[test]
fn match_chooses_between_the_cases_of_a_value() {
    let program = program(&[
        (
            "shapes.jadpo",
            "type ReceiptId = Uuid {}\n\ntype PaymentOutcome = Enum {\n    pending\n    paid: {\n        receipt_id: ReceiptId\n    }\n    declined\n}\n\ntype ProductName = Text {\n    min_length: 1\n    max_length: 120\n}\n\ntype Listing = Object {\n    name: ProductName\n    subtitle: ProductName?\n}\n\ntype TodoStatus = Enum {\n    open\n    done\n}\n\nentity Todo {\n    id: Uuid\n    status: TodoStatus\n}\n\naction notify_owner(todo_id: Todo.id) -> Unit {}\n",
        ),
        (
            "main.jadpo",
            "function receipt(outcome: PaymentOutcome) -> ReceiptId? {\n    return match outcome {\n        PaymentOutcome.pending -> none\n        PaymentOutcome.paid(paid) -> paid.receipt_id\n        PaymentOutcome.declined -> none\n    }\n}\n\nfunction headline(listing: Listing) -> ProductName {\n    return match listing.subtitle {\n        some(subtitle) -> subtitle\n        none -> listing.name\n    }\n}\n\naction follow_up(todo: Todo) -> Unit {\n    match todo.status {\n        TodoStatus.open -> {\n            notify_owner(todo.id)\n        }\n        TodoStatus.done -> {}\n    }\n}\n",
        ),
    ]);
    let id = Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e");
    let paid = program
        .variant("PaymentOutcome", "paid", vec![("receipt_id", id.clone())])
        .expect("paid");
    assert_eq!(value(&program, "receipt", vec![paid]), id);
    let declined = program
        .variant("PaymentOutcome", "declined", vec![])
        .expect("declined");
    assert_eq!(value(&program, "receipt", vec![declined]), Value::None);
    for status in ["open", "done"] {
        let todo = program
            .object(
                "Todo",
                vec![
                    ("id", id.clone()),
                    (
                        "status",
                        program
                            .variant("TodoStatus", status, vec![])
                            .expect("status"),
                    ),
                ],
            )
            .expect("Todo");
        assert_eq!(value(&program, "follow_up", vec![todo]), Value::Unit);
    }
}

#[test]
fn match_on_text_and_numbers_falls_back_to_the_wildcard() {
    let program = main(
        "type ProductName = Text {}\n\ntype Rank = Int {}\n\nfunction shout(name: ProductName) -> ProductName {\n    return match name {\n        \"\" -> \"unnamed\"\n        _ -> name\n    }\n}\n\nfunction medal(rank: Rank) -> ProductName {\n    return match rank {\n        1 -> \"gold\"\n        2 -> \"silver\"\n        _ -> \"none\"\n    }\n}\n",
    );
    assert_eq!(value(&program, "shout", vec![text("")]), text("unnamed"));
    assert_eq!(value(&program, "shout", vec![text("Lamp")]), text("Lamp"));
    assert_eq!(value(&program, "medal", vec![int(2)]), text("silver"));
    assert_eq!(value(&program, "medal", vec![int(7)]), text("none"));
}

const LOOKUP: &str = "type LookupCode = Text {}\n\ntype LookupResult = Text {}\n\ntype LookupMissing = Failure {\n    kind: FailureKind.not_found\n}\n\ntype LookupRefused = Failure {\n    kind: FailureKind.rejected\n}\n\ntype LookupRejected = Failure {\n    kind: FailureKind.rejected\n}\n\nfunction lookup(key: LookupCode) fails LookupMissing, LookupRefused -> LookupResult {\n    if (key == \"missing\") {\n        reject LookupMissing\n    }\n    if (key == \"refused\") {\n        reject LookupRefused\n    }\n    return \"found\"\n}\n";

#[test]
fn match_on_a_calls_outcome_recovers_maps_or_passes_on() {
    let program = program(&[
        ("lookup.jadpo", LOOKUP),
        (
            "main.jadpo",
            "function lookup_or_default(key: LookupCode) fails LookupRejected -> LookupResult {\n    return match lookup(key) {\n        success(value) -> value\n        LookupMissing -> \"not found\"\n        LookupRefused -> reject LookupRejected\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        value(&program, "lookup_or_default", vec![text("x")]),
        text("found")
    );
    assert_eq!(
        value(&program, "lookup_or_default", vec![text("missing")]),
        text("not found")
    );
    let rejected = support::failure(&program, "lookup_or_default", vec![text("refused")]);
    assert_eq!(rejected.name, "LookupRejected");
    assert_eq!(
        rejected.cause.as_ref().map(|cause| cause.name.as_str()),
        Some("LookupRefused"),
        "the mapped failure is kept as its cause (area 4, question 8)"
    );
}

#[test]
fn attempt_passes_a_failure_on_and_an_infallible_value_through() {
    let program = program(&[
        ("lookup.jadpo", LOOKUP),
        (
            "main.jadpo",
            "function looked_up(key: LookupCode) fails LookupMissing, LookupRefused -> LookupResult {\n    var found: LookupResult = attempt lookup(key)\n    return found\n}\n",
        ),
    ]);
    assert_eq!(value(&program, "looked_up", vec![text("x")]), text("found"));
    let missing = support::failure(&program, "looked_up", vec![text("missing")]);
    assert_eq!(missing.name, "LookupMissing");
    let lines: Vec<u32> = missing
        .frames
        .iter()
        .filter_map(|frame| program.places().place(*frame))
        .map(|place| place.line)
        .collect();
    assert_eq!(
        lines,
        [19, 2],
        "raised in lookup, through looked_up's attempt"
    );
}

#[test]
fn a_loop_runs_its_body_once_per_item() {
    let program = program(&[
        (
            "lines.jadpo",
            "type Pence = Int {\n    min: 0\n}\n\ntype OrderLine = Object {\n    subtotal: Pence\n}\n",
        ),
        (
            "main.jadpo",
            "function order_total(lines: List<OrderLine>) -> Pence {\n    var mut total: Pence = 0\n    for line in lines {\n        total = Pence(total + line.subtotal)\n    }\n    return total\n}\n",
        ),
    ]);
    let line = |n: i64| {
        program
            .object("OrderLine", vec![("subtotal", int(n))])
            .expect("OrderLine")
    };
    assert_eq!(
        value(
            &program,
            "order_total",
            vec![list(vec![line(100), line(250)])]
        ),
        int(350)
    );
    assert_eq!(value(&program, "order_total", vec![list(vec![])]), int(0));
}

#[test]
fn a_return_inside_a_loop_leaves_the_callable() {
    let program = main(
        "type Score = Int {}\n\nfunction first_above(scores: List<Score>, limit: Score) -> Score {\n    for score in scores {\n        if (score > limit) {\n            return score\n        }\n    }\n    return limit\n}\n",
    );
    assert_eq!(
        value(
            &program,
            "first_above",
            vec![list(vec![int(1), int(9), int(12)]), int(5)]
        ),
        int(9)
    );
}

#[test]
fn inline_functions_are_passed_to_collection_operations() {
    let program = main(
        "type TodoTitle = Text {\n    max_length: 200\n}\n\ntype TodoStatus = Enum {\n    open\n    done\n}\n\ntype Todo = Object {\n    title: TodoTitle\n    status: TodoStatus\n}\n\nfunction open_titles(todos: List<Todo>) -> List<TodoTitle> {\n    var open: List<Todo> = collection.filter(todos, todo -> todo.status == TodoStatus.open)\n    var titles: List<TodoTitle> = collection.map(open, todo -> todo.title)\n    return titles\n}\n",
    );
    let todo = |title: &str, status: &str| {
        program
            .object(
                "Todo",
                vec![
                    ("title", text(title)),
                    (
                        "status",
                        program
                            .variant("TodoStatus", status, vec![])
                            .expect("status"),
                    ),
                ],
            )
            .expect("Todo")
    };
    let todos = list(vec![
        todo("Buy milk", "open"),
        todo("Post letter", "done"),
        todo("Call Ada", "open"),
    ]);
    assert_eq!(
        value(&program, "open_titles", vec![todos]),
        list(vec![text("Buy milk"), text("Call Ada")])
    );
}

#[test]
fn recursion_walks_a_recursive_type() {
    let program = program(&[
        (
            "comments.jadpo",
            "type CommentBody = Text {\n    max_length: 2000\n}\n\ntype Comment = Object {\n    body: CommentBody\n    replies: List<Comment>\n}\n\ntype ReplyTotal = Int {\n    min: 0\n}\n",
        ),
        (
            "main.jadpo",
            "function reply_count(comment: Comment) -> ReplyTotal {\n    var mut count: ReplyTotal = 0\n    for reply in comment.replies {\n        count = ReplyTotal(count + 1 + reply_count(reply))\n    }\n    return count\n}\n",
        ),
    ]);
    let comment = |replies: Vec<Value>| {
        program
            .object(
                "Comment",
                vec![("body", text("hi")), ("replies", list(replies))],
            )
            .expect("Comment")
    };
    let thread = comment(vec![
        comment(vec![comment(vec![]), comment(vec![])]),
        comment(vec![]),
    ]);
    assert_eq!(value(&program, "reply_count", vec![thread]), int(4));
}

const COUNTDOWN: &str = "type Count = Int {\n    min: 0\n}\n\ntype CountBelowZero = Failure {\n    kind: FailureKind.invalid_value\n}\n\nfunction countdown(n: Count) fails CountBelowZero -> Count {\n    if (n == 0) {\n        return n\n    }\n    var next: Count = attempt Count(n - 1) {\n        min: CountBelowZero\n    }\n    return attempt countdown(next)\n}\n";

#[test]
fn recursion_past_the_depth_limit_is_a_fault_not_a_crash() {
    // Question 14.
    let (shallow, deep) = on_large_stack(|| {
        let program = main(COUNTDOWN);
        let shallow = value(&program, "countdown", vec![int(10)]);
        let deep = match program.call("countdown", vec![int(5000)]) {
            Ok(Outcome::Fault(fault)) => {
                let (kind, frames) = where_fault(&program, &fault);
                (kind, frames.len(), frames.first().cloned())
            }
            other => panic!("{other:?}"),
        };
        (shallow.to_string(), deep)
    })
    .expect("a thread");
    assert_eq!(shallow, "0");
    assert_eq!(
        deep,
        (
            FaultKind::RecursionLimit,
            MAX_DEPTH,
            Some("main.jadpo:16".to_string())
        )
    );
}

#[test]
fn a_parameter_that_accepts_several_types_takes_an_enum() {
    let program = program(&[
        (
            "parties.jadpo",
            "entity Customer {\n    id: Uuid\n}\n\nentity Supplier {\n    id: Uuid\n}\n\ntype PartyTitle = Text {\n    max_length: 80\n}\n\nfunction customer_label(id: Customer.id) -> PartyTitle {\n    return \"customer\"\n}\n\nfunction supplier_label(id: Supplier.id) -> PartyTitle {\n    return \"supplier\"\n}\n",
        ),
        (
            "main.jadpo",
            "type PartyId = Enum {\n    customer: {\n        id: Customer.id\n    }\n    supplier: {\n        id: Supplier.id\n    }\n}\n\nfunction party_label(party: PartyId) -> PartyTitle {\n    return match party {\n        PartyId.customer(customer) -> customer_label(customer.id)\n        PartyId.supplier(supplier) -> supplier_label(supplier.id)\n    }\n}\n",
        ),
    ]);
    let id = Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e");
    let supplier = program
        .variant("PartyId", "supplier", vec![("id", id)])
        .expect("supplier");
    assert_eq!(
        value(&program, "party_label", vec![supplier]),
        text("supplier")
    );
}

#[test]
fn a_library_function_is_called_through_its_library() {
    let program = program(&[
        (
            "pricing.jadpo",
            "type Price = Decimal {\n    scale: 2\n    min: 0.00\n}\n\nlibrary pricing {\n    function doubled(amount: Price) -> Price {\n        return Price(amount * 2)\n    }\n}\n",
        ),
        (
            "main.jadpo",
            "function total(subtotal: Price) -> Price {\n    var total: Price = pricing.doubled(subtotal)\n    return total\n}\n",
        ),
    ]);
    assert_eq!(
        value(&program, "total", vec![decimal("12.34")]).to_string(),
        "24.68"
    );
}

#[test]
fn a_set_literal_holds_each_value_once_in_order() {
    let program = main(
        "type Tag = Text {\n    min_length: 1\n}\n\ntype Tags = Object {\n    tags: Set<Tag>\n}\n\nfunction tags() -> Tags {\n    return Tags {\n        tags: [\"home\", \"urgent\"]\n    }\n}\n",
    );
    let tags = value(&program, "tags", vec![]);
    let expected = program
        .object(
            "Tags",
            vec![("tags", Value::set(vec![text("urgent"), text("home")]))],
        )
        .expect("Tags");
    assert_eq!(tags, expected);
}

#[test]
fn an_inline_function_reads_the_variables_around_it() {
    let program = main(
        "type Score = Int {}\n\nfunction above(scores: List<Score>, limit: Score) -> List<Score> {\n    var kept: List<Score> = collection.filter(scores, score -> score > limit)\n    return kept\n}\n\nfunction squares(scores: List<Score>) -> List<Score> {\n    var squared: List<Score> = collection.map(scores, score -> Score(score * score))\n    return squared\n}\n",
    );
    assert_eq!(
        value(
            &program,
            "above",
            vec![list(vec![int(1), int(7), int(3), int(9)]), int(4)]
        ),
        list(vec![int(7), int(9)])
    );
    let overflow = fault(&program, "squares", vec![list(vec![int(2), int(i64::MAX)])]);
    assert_eq!(
        where_fault(&program, &overflow),
        (FaultKind::IntOverflow, vec!["main.jadpo:9".to_string()]),
        "a fault inside an inline function stops the operation at its line"
    );
}

#[test]
fn a_return_inside_a_match_arm_leaves_the_callable() {
    let program = main(
        "type TodoStatus = Enum {\n    open\n    done\n}\n\ntype Label = Text {}\n\nfunction describe(status: TodoStatus) -> Label {\n    match status {\n        TodoStatus.open -> {\n            return \"to do\"\n        }\n        TodoStatus.done -> {}\n    }\n    return \"finished\"\n}\n",
    );
    let status = |name: &str| program.variant("TodoStatus", name, vec![]).expect("status");
    assert_eq!(
        value(&program, "describe", vec![status("open")]),
        text("to do")
    );
    assert_eq!(
        value(&program, "describe", vec![status("done")]),
        text("finished")
    );
}

#[test]
fn a_decimal_past_38_digits_is_a_fault() {
    let program = main(
        "type Amount = Decimal {\n    scale: 2\n}\n\nfunction squared(a: Amount) -> Amount {\n    return Amount(a * a)\n}\n",
    );
    assert_eq!(
        value(&program, "squared", vec![decimal("1.50")]).to_string(),
        "2.25"
    );
    let big = decimal("12345678901234567890.00");
    let overflow = fault(&program, "squared", vec![big]);
    assert_eq!(
        where_fault(&program, &overflow),
        (FaultKind::DecimalOverflow, vec!["main.jadpo:6".to_string()])
    );
}
