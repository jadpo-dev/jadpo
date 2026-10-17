//! Making a constant a setting (area 10, "Making a constant a setting",
//! chunk 5.3f): the code `jadpo config move` and the editor's "Make this
//! a setting" share. It adds a `config` field of the constant's name and
//! type with its value as `default:` and its description, deletes the
//! constant, rewrites every read to `config.<name>`, and refuses, editing
//! nothing, a type a setting can't hold, a name a setting has, and a read
//! in a query, an update by `id`, a backfill or another constant's value.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_check::setting_move::{self, Mark, Moved};
use jadpo_db::{Database, Layout, Loaded};

use crate::support::{self, APP};

fn loaded(layout: Layout, files: &[(&str, &str)]) -> Loaded {
    let db = Database::default();
    let project = support::project(&db, layout, files);
    Loaded {
        db,
        project,
        root: PathBuf::from("/nowhere"),
        unreadable: Vec::new(),
    }
}

fn texts(loaded: &Loaded) -> BTreeMap<String, String> {
    jadpo_db::source_texts(&loaded.db, loaded.project)
}

fn errors(loaded: &Loaded) -> Vec<String> {
    jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

/// Moves `written` in `loaded`, which must succeed.
fn moved(loaded: &mut Loaded, written: &str) -> Moved {
    let id = setting_move::find(&loaded.db, loaded.project, written).expect("a constant");
    let plan = setting_move::plan(&loaded.db, loaded.project, &id).expect("a plan");
    setting_move::apply(loaded, &plan).expect("moved")
}

/// The refusals of moving `written`, as `rule: message`.
fn refused(loaded: &Loaded, written: &str) -> Vec<String> {
    let id = setting_move::find(&loaded.db, loaded.project, written).expect("a constant");
    setting_move::plan(&loaded.db, loaded.project, &id)
        .expect_err("refused")
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect()
}

/// The page's example: a library constant in a domain, read from an
/// action.
const ACCOUNTS: &[(&str, &str)] = &[
    APP,
    (
        "domains/accounts/types/login_attempts.jadpo",
        "type LoginAttempts = Int {\n    min: 1\n    max: 100\n}\n",
    ),
    (
        "domains/accounts/types/account_locked.jadpo",
        "type AccountLocked = Bool {}\n",
    ),
    (
        "domains/accounts/libraries/security.jadpo",
        "use accounts/types { LoginAttempts }\n\nlibrary security {\n    /// Failed sign-ins before an account is locked.\n    constant max_login_attempts: LoginAttempts = 5\n}\n",
    ),
    (
        "domains/accounts/actions/sign_in.jadpo",
        "use accounts/types { AccountLocked, LoginAttempts }\nuse accounts/libraries { security }\n\naction sign_in(failed_attempts: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failed_attempts >= security.max_login_attempts)\n}\n",
    ),
];

#[test]
fn the_pages_example_becomes_a_setting() {
    let mut loaded = loaded(Layout::Project, ACCOUNTS);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    let moved = moved(&mut loaded, "security.max_login_attempts");
    let texts = texts(&loaded);
    assert_eq!(
        texts.get("config.jadpo").map(String::as_str),
        Some(
            "use accounts/types { LoginAttempts }\n\nconfig {\n    /// Failed sign-ins before an account is locked.\n    max_login_attempts: LoginAttempts {\n        default: 5\n    }\n}\n"
        )
    );
    assert_eq!(
        texts
            .get("domains/accounts/actions/sign_in.jadpo")
            .map(String::as_str),
        Some(
            "use accounts/types { AccountLocked, LoginAttempts }\n\naction sign_in(failed_attempts: LoginAttempts) -> AccountLocked {\n    return AccountLocked(failed_attempts >= config.max_login_attempts)\n}\n"
        )
    );
    // The library held nothing else, so it and its file are gone.
    assert!(!texts.contains_key("domains/accounts/libraries/security.jadpo"));
    assert_eq!(errors(&loaded), Vec::<String>::new());
    type Rows = Vec<(Mark, String, String)>;
    let shown: Vec<(String, Rows)> = moved
        .files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.rows
                    .iter()
                    .map(|row| (row.mark, row.subject.clone(), row.note.clone()))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            (
                "config.jadpo".to_string(),
                vec![(
                    Mark::Added,
                    "max_login_attempts".to_string(),
                    "setting, default 5".to_string()
                ),]
            ),
            (
                "domains/accounts/libraries/security.jadpo".to_string(),
                vec![
                    (
                        Mark::Removed,
                        "max_login_attempts".to_string(),
                        "constant".to_string()
                    ),
                    (
                        Mark::Removed,
                        "security".to_string(),
                        "library, nothing left in it".to_string()
                    ),
                    (
                        Mark::Removed,
                        "security.jadpo".to_string(),
                        "file, nothing left in it".to_string()
                    ),
                ]
            ),
            (
                "domains/accounts/actions/sign_in.jadpo".to_string(),
                vec![(
                    Mark::Changed,
                    "line 5".to_string(),
                    "reads config.max_login_attempts".to_string()
                ),]
            ),
        ]
    );
}

#[test]
fn the_constant_is_named_as_code_reads_it_or_after_its_domain() {
    let loaded = loaded(Layout::Project, ACCOUNTS);
    let short = setting_move::find(&loaded.db, loaded.project, "security.max_login_attempts");
    let long = setting_move::find(
        &loaded.db,
        loaded.project,
        "accounts.security.max_login_attempts",
    );
    assert!(short.is_ok());
    assert_eq!(short.ok(), long.ok());
    let wrong = setting_move::find(&loaded.db, loaded.project, "security.max_login_attempt")
        .expect_err("no such constant");
    assert_eq!(wrong.rule.id, "cli.config_arguments");
    assert_eq!(
        wrong.message,
        "`security.max_login_attempt` isn't a constant"
    );
    assert_eq!(
        wrong.next.as_ref().map(|next| next.text.as_str()),
        Some("Use `security.max_login_attempts` instead of `security.max_login_attempt`")
    );
}

#[test]
fn the_editor_finds_the_constant_on_its_declaration_or_a_read() {
    let loaded = loaded(Layout::Project, ACCOUNTS);
    let id = setting_move::find(&loaded.db, loaded.project, "security.max_login_attempts").ok();
    let file = |path: &str| loaded.file(path).expect("file");
    let library = ACCOUNTS[3].1;
    let declared = library.find("max_login_attempts").expect("declared") as u32 + 3;
    assert_eq!(
        setting_move::at(&loaded.db, loaded.project, file(ACCOUNTS[3].0), declared),
        id
    );
    let action = ACCOUNTS[4].1;
    let read = action.find("max_login_attempts").expect("read") as u32 + 3;
    assert_eq!(
        setting_move::at(&loaded.db, loaded.project, file(ACCOUNTS[4].0), read),
        id
    );
    let elsewhere = action.find("failed_attempts").expect("parameter") as u32;
    assert_eq!(
        setting_move::at(&loaded.db, loaded.project, file(ACCOUNTS[4].0), elsewhere),
        None
    );
}

const PENCE: &str = "type Pence = Int {\n    min: 0\n}\n\n";

#[test]
fn a_constant_beside_its_use_joins_the_settings_in_name_order() {
    let text = format!(
        "{PENCE}config {{\n    business_name: Pence {{\n        default: 1\n    }}\n    tax_band: Pence {{\n        default: 2\n    }}\n}}\n\n/// Orders from this many pence travel free.\nconstant free_delivery_from: Pence = 5000\n\nfunction delivery_fee(subtotal: Pence) -> Pence {{\n    if (subtotal >= free_delivery_from) {{\n        return 0\n    }}\n    return Pence(config.business_name + config.tax_band)\n}}\n"
    );
    let mut loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    moved(&mut loaded, "free_delivery_from");
    assert_eq!(
        texts(&loaded).get("main.jadpo").map(String::as_str),
        Some(
            format!(
                "{PENCE}config {{\n    business_name: Pence {{\n        default: 1\n    }}\n    /// Orders from this many pence travel free.\n    free_delivery_from: Pence {{\n        default: 5000\n    }}\n    tax_band: Pence {{\n        default: 2\n    }}\n}}\n\nfunction delivery_fee(subtotal: Pence) -> Pence {{\n    if (subtotal >= config.free_delivery_from) {{\n        return 0\n    }}\n    return Pence(config.business_name + config.tax_band)\n}}\n"
            )
            .as_str()
        )
    );
    assert_eq!(errors(&loaded), Vec::<String>::new());
}

#[test]
fn a_worked_out_value_is_written_as_the_compiler_worked_it_out() {
    let text = format!(
        "{PENCE}constant small_order_fee: Pence = 250\nconstant large_order_fee: Pence = Pence(small_order_fee * 2)\n\nfunction fees() -> Pence {{\n    return Pence(small_order_fee + large_order_fee)\n}}\n"
    );
    let mut loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    moved(&mut loaded, "large_order_fee");
    let texts = texts(&loaded);
    assert_eq!(
        texts.get("config.jadpo").map(String::as_str),
        Some("config {\n    large_order_fee: Pence {\n        default: 500\n    }\n}\n")
    );
    assert_eq!(
        texts.get("main.jadpo").map(String::as_str),
        Some(
            format!(
                "{PENCE}constant small_order_fee: Pence = 250\n\nfunction fees() -> Pence {{\n    return Pence(small_order_fee + config.large_order_fee)\n}}\n"
            )
            .as_str()
        )
    );
    assert_eq!(errors(&loaded), Vec::<String>::new());
}

#[test]
fn a_type_a_setting_cant_hold_is_refused() {
    let text = format!(
        "{PENCE}constant fees: List<Pence> = [250, 500]\n\nfunction first_fee() -> Pence {{\n    for fee in fees {{\n        return fee\n    }}\n    return 0\n}}\n"
    );
    let loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert_eq!(
        refused(&loaded, "fees"),
        ["configuration.move_type_unsupported: `fees` can't be a setting"]
    );
}

#[test]
fn a_name_a_setting_has_is_refused() {
    let text = format!(
        "{PENCE}config {{\n    page_size: Pence {{\n        default: 20\n    }}\n}}\n\nconstant page_size: Pence = 25\n\nfunction pages() -> Pence {{\n    return Pence(page_size + config.page_size)\n}}\n"
    );
    let loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert_eq!(
        refused(&loaded, "page_size"),
        ["configuration.move_name_taken: A setting is already called `page_size`"]
    );
}

#[test]
fn a_read_in_a_query_is_refused() {
    let text = "type PageCount = Int {\n    min: 1\n    max: 5000\n}\n\nentity Manual {\n    id: Uuid\n    page_count: PageCount\n}\n\nconstant longest_manual: PageCount = 900\n\nquery long_manuals() -> List<Manual> {\n    from: Manual\n    where: Manual.page_count > longest_manual\n    order: [Manual.page_count asc]\n    limit: 20\n}\n";
    let loaded = loaded(Layout::Loose, &[("main.jadpo", text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert_eq!(
        refused(&loaded, "longest_manual"),
        ["configuration.move_read_unsupported: `longest_manual` is read in a query"]
    );
}

#[test]
fn a_read_in_an_update_by_id_is_refused() {
    let text = "type Count = Int {\n    min: 0\n}\n\nentity Counter {\n    id: Uuid\n    hits: Count\n}\n\nconstant step: Count = 1\n\naction hit(counter_id: Counter.id) fails CounterNotFound -> Counter {\n    return attempt Counter.update(counter_id) {\n        set: {\n            hits: Count(Counter.hits + step)\n        }\n    }\n}\n";
    let loaded = loaded(Layout::Loose, &[("main.jadpo", text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert_eq!(
        refused(&loaded, "step"),
        ["configuration.move_read_unsupported: `step` is read in an update by `id`"]
    );
}

#[test]
fn a_read_in_another_constants_value_is_refused() {
    let text = format!(
        "{PENCE}constant small_order_fee: Pence = 250\nconstant large_order_fee: Pence = Pence(small_order_fee * 2)\n\nfunction fees() -> Pence {{\n    return Pence(small_order_fee + large_order_fee)\n}}\n"
    );
    let loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    let before = texts(&loaded);
    assert_eq!(
        refused(&loaded, "small_order_fee"),
        ["configuration.move_read_unsupported: `small_order_fee` is read in a constant's value"]
    );
    assert_eq!(texts(&loaded), before);
}

#[test]
fn every_refusal_is_given_at_once() {
    let text = format!(
        "{PENCE}config {{\n    fees: Pence {{\n        default: 20\n    }}\n}}\n\nconstant fees: List<Pence> = [250, 500]\nconstant first: List<Pence> = fees\n\nfunction first_fee() -> Pence {{\n    for fee in first {{\n        return Pence(fee + config.fees)\n    }}\n    return 0\n}}\n"
    );
    let loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert_eq!(
        refused(&loaded, "fees"),
        [
            "configuration.move_type_unsupported: `fees` can't be a setting",
            "configuration.move_name_taken: A setting is already called `fees`",
            "configuration.move_read_unsupported: `fees` is read in a constant's value",
        ]
    );
}

#[test]
fn a_default_naming_the_constant_makes_the_constant_that_setting() {
    let text = format!(
        "{PENCE}constant largest_page: Pence = 100\n\nconfig {{\n    page_size: Pence {{\n        default: largest_page\n    }}\n}}\n\nfunction page() -> Pence {{\n    return Pence(config.page_size + largest_page)\n}}\n"
    );
    let mut loaded = loaded(Layout::Loose, &[("main.jadpo", &text)]);
    let problems = jadpo_check::check(&loaded.db, loaded.project);
    assert_eq!(problems.len(), 1);
    assert_eq!(
        problems[0].next.as_ref().map(|next| next.text.as_str()),
        Some(
            "Run `jadpo config move largest_page`, which makes the constant this setting, or write a literal value after `default:`"
        )
    );
    let id = setting_move::find(&loaded.db, loaded.project, "largest_page").expect("found");
    assert!(setting_move::answered(
        &loaded.db,
        loaded.project,
        &id,
        &problems[0]
    ));
    let moved = moved(&mut loaded, "largest_page");
    assert_eq!(
        texts(&loaded).get("main.jadpo").map(String::as_str),
        Some(
            format!(
                "{PENCE}config {{\n    page_size: Pence {{\n        default: 100\n    }}\n}}\n\nfunction page() -> Pence {{\n    return Pence(config.page_size + config.page_size)\n}}\n"
            )
            .as_str()
        )
    );
    assert_eq!(errors(&loaded), Vec::<String>::new());
    assert_eq!(moved.files[0].rows[0].note, "default 100, was largest_page");
}

#[test]
fn a_move_whose_result_would_not_check_is_undone() {
    // `small_order_fee` is read only by `large_order_fee`, so moving
    // `large_order_fee` would leave it unused beside the action.
    let files: &[(&str, &str)] = &[
        APP,
        (
            "domains/shop/types/pence.jadpo",
            "type Pence = Int {\n    min: 0\n}\n",
        ),
        (
            "domains/shop/actions/fee.jadpo",
            "use shop/types { Pence }\n\naction fee() -> Pence {\n    return large_order_fee\n}\n\nconstant small_order_fee: Pence = 250\nconstant large_order_fee: Pence = Pence(small_order_fee * 2)\n",
        ),
    ];
    let mut loaded = loaded(Layout::Project, files);
    assert_eq!(errors(&loaded), Vec::<String>::new());
    let before = texts(&loaded);
    let id = setting_move::find(&loaded.db, loaded.project, "large_order_fee").expect("found");
    let plan = setting_move::plan(&loaded.db, loaded.project, &id).expect("planned");
    let stopped = setting_move::apply(&mut loaded, &plan).expect_err("undone");
    let rules: Vec<&str> = stopped.problems.iter().map(|d| d.rule.id).collect();
    assert_eq!(rules, ["layout.unused_declaration"]);
    assert_eq!(texts(&loaded), before);
}

#[test]
fn restore_puts_the_texts_back() {
    let mut loaded = loaded(Layout::Project, ACCOUNTS);
    let before = texts(&loaded);
    let moved = moved(&mut loaded, "security.max_login_attempts");
    setting_move::restore(&mut loaded, &moved);
    assert_eq!(texts(&loaded), before);
}

#[test]
fn a_read_in_a_backfill_is_refused() {
    // A backfill takes literals until computed backfills (chunk 5.6), so
    // the project doesn't check; the plan refuses the read all the same.
    let files: &[(&str, &str)] = &[
        APP,
        (
            "domains/notes/types/stars.jadpo",
            "type Stars = Int {\n    min: 1\n    max: 5\n}\n",
        ),
        (
            "domains/notes/entities/note.jadpo",
            "use notes/types { Stars }\n\nentity Note {\n    id: Uuid\n    stars: Stars\n}\n",
        ),
        (
            "shared/libraries/ratings.jadpo",
            "use notes/types { Stars }\n\nlibrary ratings {\n    constant usual_stars: Stars = 3\n}\n",
        ),
        (
            "database/migrations/20261020T1401_add_stars/changes.jadpo",
            "use shared/libraries { ratings }\n\nmigration {\n    Note.stars: {\n        backfill: ratings.usual_stars\n    }\n}\n",
        ),
    ];
    let loaded = loaded(Layout::Project, files);
    assert_eq!(
        refused(&loaded, "ratings.usual_stars"),
        ["configuration.move_read_unsupported: `ratings.usual_stars` is read in a backfill"]
    );
}
