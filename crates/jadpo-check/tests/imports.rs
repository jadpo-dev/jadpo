//! Imports (area 1, "Imports"): every name a file uses that it doesn't
//! declare comes in on a `use` line, built-ins never do, a test file sees
//! its subject's declarations, and each mistake is reported once with the
//! edit that fixes it.

use crate::support;

use jadpo_db::{Database, Layout};
use support::{APP, problems, project};

const PENCE: (&str, &str) = (
    "shared/types/pence.jadpo",
    "type Pence = Int {\n    min: 0\n}\n",
);

const PERCENT: (&str, &str) = (
    "shared/types/percent.jadpo",
    "type Percent = Int {\n    min: 0\n    max: 100\n}\n",
);

const TOTAL: &str = "domains/sales/actions/total.jadpo";

/// An action in `sales` whose file starts with `head` and takes `params`.
fn total(head: &str, params: &str) -> String {
    format!("{head}action total({params}) -> Unit {{}}\n")
}

/// The edits of the one problem in `files`, applied to `path`'s text.
fn fixed(files: &[(&str, &str)], path: &str) -> String {
    let db = Database::default();
    let project = project(&db, Layout::Project, files);
    let problems = jadpo_check::check(&db, project);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    let mut text = files
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, text)| text.to_string())
        .expect("the file");
    let mut edits: Vec<(u32, u32, String)> = problems[0]
        .next
        .iter()
        .flat_map(|next| &next.edits)
        .filter(|edit| edit.span.path == path)
        .map(|edit| (edit.span.start, edit.span.end, edit.replacement.clone()))
        .collect();
    edits.sort_by_key(|a| std::cmp::Reverse((a.0, a.1)));
    for (start, end, replacement) in edits {
        text.replace_range(start as usize..end as usize, &replacement);
    }
    text
}

#[test]
fn a_project_with_its_imports_checks_cleanly() {
    let files = [
        APP,
        PENCE,
        (
            "domains/sales/types/basket.jadpo",
            "use shared/types { Pence }\n\ntype Basket = Object {\n    total: Pence\n}\n",
        ),
        (
            "shared/libraries/pricing.jadpo",
            "use shared/types { Pence }\n\nlibrary pricing {\n    function doubled(amount: Pence) -> Pence {\n        return Pence(amount * 2)\n    }\n}\n",
        ),
        (
            "domains/menu/actions/price_item.jadpo",
            "use shared/types { Pence }\n\npublic action price_item() -> Pence {\n    return 250\n}\n",
        ),
        (
            "domains/sales/actions/price_basket.jadpo",
            "use shared/types { Pence }\nuse shared/libraries { pricing }\nuse sales/types { Basket }\nuse menu\n\naction price_basket(basket: Basket) -> Pence {\n    var each: Pence = menu.price_item()\n    return pricing.doubled(each)\n}\n",
        ),
        (
            "domains/sales/actions/checkout.jadpo",
            "use shared/types { Pence }\nuse sales/types { Basket }\nuse sales/actions { price_basket }\n\naction checkout(basket: Basket) -> Pence {\n    return price_basket(basket)\n}\n",
        ),
        // A test sees its subject's declarations without importing them.
        (
            "domains/sales/actions/price_basket.test.jadpo",
            "use shared/types { Pence }\nuse sales/types { Basket }\n\ntest \"a basket costs twice an item\" {\n    var total: Pence = price_basket(Basket {\n        total: 0\n    })\n    assert total == 500\n}\n",
        ),
    ];
    assert_eq!(problems(Layout::Project, &files), Vec::<String>::new());
}

#[test]
fn a_name_used_without_its_import_is_missing_and_the_edit_adds_it() {
    let text = total("", "basket: Pence");
    let files = [APP, PENCE, (TOTAL, text.as_str())];
    assert_eq!(
        problems(Layout::Project, &files),
        [format!("{TOTAL} import.missing: `Pence` isn't imported")]
    );
    assert_eq!(
        fixed(&files, TOTAL),
        total("use shared/types { Pence }\n\n", "basket: Pence")
    );
}

#[test]
fn a_missing_name_joins_its_folders_line() {
    let text = total(
        "use shared/types { Pence }\n\n",
        "basket: Pence, off: Percent",
    );
    let files = [APP, PENCE, PERCENT, (TOTAL, text.as_str())];
    assert_eq!(
        fixed(&files, TOTAL),
        total(
            "use shared/types { Pence, Percent }\n\n",
            "basket: Pence, off: Percent"
        )
    );
}

#[test]
fn a_call_into_another_domain_needs_the_domain() {
    let files = [
        APP,
        PENCE,
        (
            "domains/menu/actions/price_item.jadpo",
            "use shared/types { Pence }\n\npublic action price_item() -> Pence {\n    return 250\n}\n",
        ),
        (
            "domains/sales/actions/sell.jadpo",
            "use shared/types { Pence }\n\naction sell() -> Pence {\n    return menu.price_item()\n}\n",
        ),
    ];
    assert_eq!(
        problems(Layout::Project, &files),
        ["domains/sales/actions/sell.jadpo import.missing: `menu` isn't imported"]
    );
    assert_eq!(
        fixed(&files, "domains/sales/actions/sell.jadpo"),
        "use shared/types { Pence }\nuse menu\n\naction sell() -> Pence {\n    return menu.price_item()\n}\n"
    );
}

#[test]
fn the_first_use_line_goes_below_the_files_opening_comment() {
    let text = total(
        "// What a customer pays.\n\n/// The total.\n",
        "basket: Pence",
    );
    let files = [APP, PENCE, (TOTAL, text.as_str())];
    assert_eq!(
        fixed(&files, TOTAL),
        total(
            "// What a customer pays.\n\nuse shared/types { Pence }\n\n/// The total.\n",
            "basket: Pence"
        )
    );
}

#[test]
fn an_unused_import_is_reported_and_the_edit_removes_it() {
    let text = total("use shared/types { Pence, Percent }\n\n", "basket: Pence");
    let files = [APP, PENCE, PERCENT, (TOTAL, text.as_str())];
    let used = (
        "domains/sales/actions/discount.jadpo",
        "use shared/types { Percent }\n\naction discount(off: Percent) -> Unit {}\n",
    );
    let files = [files.as_slice(), &[used]].concat();
    assert_eq!(
        problems(Layout::Project, &files),
        [format!(
            "{TOTAL} import.unused: `Percent` is imported but not used"
        )]
    );
    assert_eq!(
        fixed(&files, TOTAL),
        total("use shared/types { Pence }\n\n", "basket: Pence")
    );
    // A domain with no call through it.
    let text = total("use shared/types { Pence }\nuse menu\n\n", "basket: Pence");
    let files = [
        APP,
        PENCE,
        (
            "domains/menu/actions/noop.jadpo",
            "action noop() -> Unit {}\n",
        ),
        (TOTAL, text.as_str()),
    ];
    assert_eq!(
        problems(Layout::Project, &files),
        [format!(
            "{TOTAL} import.unused: `menu` is imported but not used"
        )]
    );
    assert_eq!(
        fixed(&files, TOTAL),
        total("use shared/types { Pence }\n\n", "basket: Pence")
    );
}

#[test]
fn an_import_from_the_wrong_folder_is_unknown_and_the_edit_moves_it() {
    let text = total("use shared/failures { Pence }\n\n", "basket: Pence");
    let files = [APP, PENCE, (TOTAL, text.as_str())];
    assert_eq!(
        problems(Layout::Project, &files),
        [format!(
            "{TOTAL} import.unknown: `Pence` isn't in `shared/failures`"
        )]
    );
    assert_eq!(
        fixed(&files, TOTAL),
        total("use shared/types { Pence }\n\n", "basket: Pence")
    );
}

#[test]
fn unknown_folders_domains_and_names() {
    let line = |head: &str| {
        let text = total(&format!("{head}\n\n"), "basket: Pence");
        let files = [APP, PENCE, (TOTAL, text.as_str())];
        problems(Layout::Project, &files)
    };
    assert_eq!(
        line("use shared/type { Pence }")[0],
        format!("{TOTAL} import.unknown: `shared/type` isn't a folder of `shared/`")
    );
    assert_eq!(
        line("use sales/helpers { Pence }")[0],
        format!("{TOTAL} import.unknown: `sales/helpers` isn't a folder")
    );
    assert_eq!(
        line("use sails/types { Pence }")[0],
        format!("{TOTAL} import.unknown: `sails` isn't a domain")
    );
    assert_eq!(
        line("use shared/types { Pence, Pense }"),
        [format!("{TOTAL} import.unknown: `Pense` isn't declared")]
    );
}

#[test]
fn built_ins_own_declarations_and_the_own_domain_need_no_import() {
    let text = "use shared/types { Pence, List }\nuse sales/actions { total }\nuse sales\n\naction total(basket: Pence, items: List<Pence>) -> Unit {}\n";
    let files = [APP, PENCE, (TOTAL, text)];
    assert_eq!(
        problems(Layout::Project, &files),
        [
            format!("{TOTAL} import.unneeded: `List` is built in"),
            format!("{TOTAL} import.unneeded: `total` is declared in this file"),
            format!("{TOTAL} import.unneeded: `sales` is this file's own domain"),
        ]
    );
}

#[test]
fn a_test_needs_no_import_for_its_subjects_declarations() {
    let files = [
        APP,
        PENCE,
        (
            "domains/sales/actions/price.jadpo",
            "use shared/types { Pence }\n\nfunction loaf_price() -> Pence {\n    return 250\n}\n\naction price() -> Pence {\n    return loaf_price()\n}\n",
        ),
        (
            "domains/sales/actions/price.test.jadpo",
            "use sales/actions { loaf_price }\n\ntest \"a loaf\" {\n    assert loaf_price() == 250\n    assert price() == 250\n}\n",
        ),
    ];
    assert_eq!(
        problems(Layout::Project, &files),
        [
            "domains/sales/actions/price.test.jadpo import.unneeded: `loaf_price` is declared in this test's subject"
        ]
    );
}

#[test]
fn a_name_or_folder_imported_twice() {
    let text = total(
        "use shared/types { Pence }\nuse shared/types { Percent }\n\n",
        "basket: Pence, off: Percent",
    );
    let files = [APP, PENCE, PERCENT, (TOTAL, text.as_str())];
    assert_eq!(
        problems(Layout::Project, &files),
        [format!(
            "{TOTAL} import.duplicate: `shared/types` has two `use` lines"
        )]
    );
    assert_eq!(
        fixed(&files, TOTAL),
        total(
            "use shared/types { Pence, Percent }\n\n",
            "basket: Pence, off: Percent"
        )
    );
    let text = total("use shared/types { Pence, Pence }\n\n", "basket: Pence");
    let files = [APP, PENCE, (TOTAL, text.as_str())];
    assert_eq!(
        problems(Layout::Project, &files),
        [format!(
            "{TOTAL} import.duplicate: `Pence` is imported twice"
        )]
    );
    assert_eq!(
        fixed(&files, TOTAL),
        total("use shared/types { Pence }\n\n", "basket: Pence")
    );
}

#[test]
fn a_private_name_from_another_domain_is_visibilitys_to_report() {
    let files = [
        APP,
        (
            "domains/menu/types/dish.jadpo",
            "type Dish = Object {\n    name: Text\n}\n",
        ),
        (
            "domains/menu/actions/cook.jadpo",
            "use menu/types { Dish }\n\naction cook(dish: Dish) -> Unit {}\n",
        ),
        (
            "domains/sales/actions/order.jadpo",
            "action order(dish: Dish) -> Unit {}\n",
        ),
    ];
    assert_eq!(
        problems(Layout::Project, &files),
        [
            "domains/sales/actions/order.jadpo visibility.private_cross_domain: `Dish` is private to domain `menu`"
        ]
    );
}

#[test]
fn loose_files_need_no_imports_and_have_no_folders() {
    let files = [
        ("a.jadpo", "type Pence = Int {\n    min: 0\n}\n"),
        ("b.jadpo", "type Basket = Object {\n    total: Pence\n}\n"),
    ];
    assert_eq!(problems(Layout::Loose, &files), Vec::<String>::new());
    let files = [
        ("a.jadpo", "type Pence = Int {\n    min: 0\n}\n"),
        (
            "b.jadpo",
            "use shared/types { Pence }\n\ntype Basket = Object {\n    total: Pence\n}\n",
        ),
    ];
    assert_eq!(
        problems(Layout::Loose, &files),
        ["b.jadpo import.unknown: `shared/types` isn't a folder here"]
    );
}

#[test]
fn an_entitys_provided_failure_comes_from_the_entitys_folder() {
    let files = [
        APP,
        (
            "domains/planning/entities/todo.jadpo",
            "entity Todo {\n    id: Uuid\n    title: Text\n}\n",
        ),
        (
            "domains/planning/actions/rename_todo.jadpo",
            "use planning/entities { Todo, TodoNotFound }\n\naction rename_todo(todo_id: Todo.id) fails TodoNotFound -> Todo {\n    var todo: Todo = attempt Todo.get(todo_id)\n    return todo\n}\n",
        ),
    ];
    let found = problems(Layout::Project, &files);
    assert!(!found.iter().any(|p| p.contains("import.")), "{found:#?}");
}

/// The one problem's edits, each file's text after them, by path; a new
/// file starts empty.
fn applied(files: &[(&str, &str)]) -> Vec<(String, String)> {
    let db = Database::default();
    let project = project(&db, Layout::Project, files);
    let problems = jadpo_check::check(&db, project);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    let mut texts: std::collections::BTreeMap<String, String> = files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    let mut edits: Vec<(String, u32, u32, String)> = problems[0]
        .next
        .iter()
        .flat_map(|next| &next.edits)
        .map(|edit| {
            (
                edit.span.path.clone(),
                edit.span.start,
                edit.span.end,
                edit.replacement.clone(),
            )
        })
        .collect();
    edits.sort_by_key(|a| std::cmp::Reverse((a.1, a.2)));
    let mut changed = std::collections::BTreeSet::new();
    for (path, start, end, replacement) in edits {
        let text = texts.entry(path.clone()).or_default();
        text.replace_range(start as usize..end as usize, &replacement);
        changed.insert(path);
    }
    changed
        .into_iter()
        .map(|path| {
            let text = texts[&path].clone();
            (path, text)
        })
        .collect()
}

#[test]
fn a_type_another_file_uses_moves_to_a_file_of_its_own_in_one_step() {
    let files = [
        APP,
        (
            "domains/planning/actions/postpone_all.jadpo",
            "use planning/actions { SnoozeDays }\n\naction postpone_all(days: SnoozeDays) -> Unit {}\n",
        ),
        (
            "domains/planning/actions/snooze_todo.jadpo",
            "// Snoozing a todo.\n\n/// Days to snooze for.\ntype SnoozeDays = Int {\n    min: 1\n    max: 30\n}\n\naction snooze_todo(days: SnoozeDays) -> Unit {}\n",
        ),
    ];
    assert_eq!(
        applied(&files),
        [
            (
                "domains/planning/actions/snooze_todo.jadpo".to_string(),
                "// Snoozing a todo.\n\nuse planning/types { SnoozeDays }\n\naction snooze_todo(days: SnoozeDays) -> Unit {}\n".to_string()
            ),
            (
                "domains/planning/types/snooze_days.jadpo".to_string(),
                "/// Days to snooze for.\ntype SnoozeDays = Int {\n    min: 1\n    max: 30\n}\n".to_string()
            ),
        ]
    );
}

#[test]
fn a_second_type_in_a_types_file_moves_out_once_another_file_uses_it() {
    let files = [
        APP,
        (
            "domains/sales/types/basket.jadpo",
            "type Basket = Object {\n    code: DiscountCode\n}\n\ntype DiscountCode = Text {\n    max_length: 12\n}\n",
        ),
        (
            "domains/sales/actions/apply_code.jadpo",
            "use sales/types { Basket, DiscountCode }\n\naction apply_code(basket: Basket, code: DiscountCode) -> Unit {}\n",
        ),
    ];
    let found = problems(Layout::Project, &files);
    assert_eq!(
        found,
        [
            "domains/sales/types/basket.jadpo layout.promotion_required: `DiscountCode` is used beyond this file"
        ]
    );
    assert_eq!(
        applied(&files),
        [
            (
                "domains/sales/types/basket.jadpo".to_string(),
                "use sales/types { DiscountCode }\n\ntype Basket = Object {\n    code: DiscountCode\n}\n".to_string()
            ),
            (
                "domains/sales/types/discount_code.jadpo".to_string(),
                "type DiscountCode = Text {\n    max_length: 12\n}\n".to_string()
            ),
        ]
    );
    // Used only by its own file, it stays.
    let files = [
        APP,
        files[1],
        (
            "domains/sales/actions/apply_code.jadpo",
            "use sales/types { Basket }\n\naction apply_code(basket: Basket) -> Unit {}\n",
        ),
    ];
    assert_eq!(problems(Layout::Project, &files), Vec::<String>::new());
}
