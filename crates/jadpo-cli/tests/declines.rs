//! `jadpo declines list` and `clear` (area 14; area 1, "Declining a
//! judgement call", chunk 5.4b): every decline with its rule, place and
//! reason, narrowed by `--rule` and a path, as text and `--json`; and
//! removing them, so each finding comes back for `jadpo check`.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("declines-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

const ROUNDING: &str = r#"use shared/types { Coins }

library rounding {
    /// The amount, capped at a thousand pence.
    function capped(amount: Coins) -> Coins {
        if (amount > Coins(1000)) {
            return Coins(1000)
        }
        return amount
    }

    /// The basket total, capped for the till.
    decline library.similar_exists: "Caps the basket for the till, which may change on its own"
    function cap_total(total: Coins) -> Coins {
        if (total > Coins(1000)) {
            return Coins(1000)
        }
        return total
    }
}
"#;

const FEES: &str = r#"use shared/types { Coins }

library fees {
    constant small_order_fee: Coins = 250
    decline constant.similar_exists: "A courier's charge that matches the fee today"
    constant courier_charge: Coins = 250
}
"#;

const SALES: &str = r#"use shared/types { Coins }
use shared/libraries { rounding }
use sales/libraries { fees }

action basket_total(price: Coins) -> Coins {
    return rounding.cap_total(
        rounding.capped(Coins(price + fees.small_order_fee + fees.courier_charge))
    )
}
"#;

fn shop(name: &str) -> PathBuf {
    let root = folder(name);
    for (path, text) in [
        ("app.jadpo", "application {}\n"),
        (
            "shared/types/coins.jadpo",
            "type Coins = Int {\n    min: 0\n}\n",
        ),
        ("shared/libraries/rounding.jadpo", ROUNDING),
        ("domains/sales/libraries/fees.jadpo", FEES),
        ("domains/sales/actions/basket_total.jadpo", SALES),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    root
}

#[test]
fn the_shop_checks_with_its_declines() {
    let root = shop("checks");
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.stdout, "checked: 5 files\n");
    assert_eq!(jadpo(&root, &["fmt", "--check"]).code, 0);
}

#[test]
fn list_shows_each_decline_with_its_rule_place_and_reason() {
    let root = shop("list");
    let output = jadpo(&root, &["declines", "list"]);
    assert_eq!(
        output.stdout,
        "Declines
  constant.similar_exists  domains/sales/libraries/fees.jadpo:5
    A courier's charge that matches the fee today
  library.similar_exists   shared/libraries/rounding.jadpo:13
    Caps the basket for the till, which may change on its own

2 declines: 2 rules, 2 files
"
    );
    assert_eq!(output.code, 0);
}

#[test]
fn list_narrows_by_rule_and_path_and_says_when_nothing_matches() {
    let root = shop("filters");
    let by_rule = jadpo(
        &root,
        &["declines", "list", "--rule", "library.similar_exists"],
    );
    assert!(
        by_rule
            .stdout
            .contains("shared/libraries/rounding.jadpo:13")
    );
    assert!(!by_rule.stdout.contains("fees.jadpo"));
    let by_path = jadpo(&root.join("domains"), &["declines", "list", "sales"]);
    assert!(
        by_path
            .stdout
            .contains("domains/sales/libraries/fees.jadpo:5")
    );
    assert!(!by_path.stdout.contains("rounding"));
    let none = jadpo(
        &root,
        &[
            "declines",
            "list",
            "shared",
            "--rule",
            "constant.similar_exists",
        ],
    );
    assert_eq!(
        none.stdout,
        "No declines: rule constant.similar_exists, in shared\n"
    );
    assert_eq!(none.code, 0);
}

#[test]
fn list_as_json_gives_each_reason_whole() {
    let root = shop("json");
    let output = jadpo(&root, &["declines", "list", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["ok"], true);
    assert_eq!(
        document["data"]["declines"][1],
        serde_json::json!({
            "rule": "library.similar_exists",
            "path": "shared/libraries/rounding.jadpo",
            "line": 13,
            "reason": "Caps the basket for the till, which may change on its own",
            "on": "rounding.cap_total"
        })
    );
    assert_eq!(document["data"]["rules"], 2);
    assert_eq!(document["data"]["files"], 2);
}

#[test]
fn clear_removes_the_lines_so_check_shows_each_finding_again() {
    let root = shop("clear");
    let output = jadpo(
        &root,
        &["declines", "clear", "--rule", "library.similar_exists"],
    );
    assert_eq!(
        output.stdout,
        "Removed 1 decline from 1 file

next: jadpo check  shows each finding again, to fix or decline
"
    );
    assert_eq!(output.code, 0);
    let rounding =
        fs::read_to_string(root.join("shared/libraries/rounding.jadpo")).expect("rounding");
    assert_eq!(
        rounding,
        ROUNDING.replace(
            "    decline library.similar_exists: \"Caps the basket for the till, which may change on its own\"\n",
            ""
        )
    );
    assert_eq!(
        fs::read_to_string(root.join("domains/sales/libraries/fees.jadpo")).expect("fees"),
        FEES
    );
    let check = jadpo(&root, &["check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&check.stdout).expect("JSON");
    let rules: Vec<&str> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|d| d["rule"].as_str())
        .collect();
    assert_eq!(rules, ["library.similar_exists"]);
    assert_eq!(document["diagnostics"][0]["declinable"], true);
    assert_eq!(check.code, 1);
    let again = jadpo(
        &root,
        &["declines", "clear", "--rule", "library.similar_exists"],
    );
    assert_eq!(
        again.stdout,
        "No declines to remove: rule library.similar_exists\n"
    );
}

#[test]
fn a_wrong_action_rule_or_path_is_a_command_error() {
    let root = shop("errors");
    let missing = jadpo(&root, &["declines"]);
    assert!(missing.stderr.contains("rule: cli.declines_arguments"));
    assert_eq!(missing.code, 2);
    let slip = jadpo(&root, &["declines", "lsit"]);
    assert!(slip.stderr.contains("Did you mean `list`?"));
    let rule = jadpo(
        &root,
        &["declines", "list", "--rule", "library.similar_exist"],
    );
    assert!(rule.stderr.contains("Use `--rule library.similar_exists`"));
    let outside = jadpo(&root, &["declines", "list", ".."]);
    assert!(outside.stderr.contains("`..` isn't in this project"));
    assert_eq!(outside.code, 2);
}

#[test]
fn clear_stops_on_a_file_that_does_not_parse() {
    let root = shop("broken");
    fs::write(
        root.join("domains/sales/actions/basket_total.jadpo"),
        "action basket_total( {\n",
    )
    .expect("write");
    let output = jadpo(&root, &["declines", "clear"]);
    assert!(
        output.stdout.contains("declines stopped"),
        "{}",
        output.stdout
    );
    assert_eq!(
        fs::read_to_string(root.join("shared/libraries/rounding.jadpo")).expect("rounding"),
        ROUNDING
    );
}
