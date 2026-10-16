//! Translations (area 10, chunk 5.3b) from the command line: a project with
//! `translations/` checks and tests, its mistakes show a code frame in the
//! JSON file, `jadpo dev` writes the text in the locale a request names, and
//! a save of a translation file reruns exactly the tests that reach it.
//!
//! The project has no entities, so no database is needed.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_db::Database;
use jadpo_test::watch::{Edit, EditKind, Event, Watch};
use jadpo_test::{Options, Ran};

use super::http::{Dev, jadpo, tempdir, write};

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.fr_fr, Locale.ar_eg]\n}\n",
    ),
    (
        "policy.jadpo",
        "use shop\n\npolicy {\n    actions: {\n        shop.show_cart: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/shop/types/order_number.jadpo",
        "/// An order's number.\ntype OrderNumber = Text {\n    max_length: 20\n}\n",
    ),
    (
        "domains/shop/types/item_count.jadpo",
        "/// How many items.\ntype ItemCount = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/shop/types/cart_request.jadpo",
        "use shop/types { ItemCount, OrderNumber }\n\n/// A cart to describe.\ntype CartRequest = Input {\n    number: OrderNumber\n    count: ItemCount\n    locale: Locale\n}\n",
    ),
    (
        "domains/shop/types/cart_text.jadpo",
        "/// What a customer is told.\ntype CartText = Output {\n    shipped: PresentationText\n    cart: PresentationText\n}\n",
    ),
    (
        "domains/shop/actions/show_cart.jadpo",
        r#"use shop/types { CartText, ItemCount, OrderNumber }

/// The cart, in the customer's language.
action show_cart(number: OrderNumber, count: ItemCount, locale: Locale) -> CartText {
    return CartText {
        shipped: translations.order_shipped(locale: locale, order_number: number)
        cart: translations.items_in_cart(locale: locale, count: count)
    }
}
"#,
    ),
    (
        "domains/shop/actions/show_cart.test.jadpo",
        r#"test "the cart is described in each language" {
    assert show_cart("A-17", 1, Locale.en_gb).cart == "1 item in your cart"
    assert show_cart("A-17", 3, Locale.en_gb).cart == "3 items in your cart"
    assert show_cart("A-17", 0, Locale.fr_fr).cart == "0 article dans votre panier"
}

test "an order's shipping is told in French" {
    assert show_cart("A-17", 1, Locale.fr_fr).shipped == "Votre commande A-17 a été expédiée."
}
"#,
    ),
    (
        "domains/shop/routes/cart.jadpo",
        r#"use shop/types { CartRequest, CartText }
use shop/actions { show_cart }

routes Cart {
    prefix: /cart

    POST / {
        authentication: RouteAuthentication.none
        input: CartRequest
        output: CartText
        run: show_cart(input.number, input.count, input.locale)
    }
}
"#,
    ),
    (
        "tests/arithmetic.test.jadpo",
        "test \"one and one are two\" {\n    assert 1 + 1 == 2\n}\n",
    ),
    (
        "translations/en_gb.json",
        r#"{
  "order_shipped": "Your order {order_number} has shipped.",
  "items_in_cart": "{count, plural, one {# item} other {# items}} in your cart"
}
"#,
    ),
    (
        "translations/fr_fr.json",
        r#"{
  "order_shipped": "Votre commande {order_number} a été expédiée.",
  "items_in_cart": "{count, plural, one {# article} many {# d'articles} other {# articles}} dans votre panier"
}
"#,
    ),
    (
        "translations/ar_eg.json",
        r#"{
  "order_shipped": "تم شحن طلبك {order_number}.",
  "items_in_cart": "{count, plural, zero {لا عناصر} one {عنصر واحد} two {عنصران} few {# عناصر} many {# عنصرًا} other {# عنصر}} في سلتك"
}
"#,
    ),
];

fn project() -> (tempdir::Dir, PathBuf) {
    let dir = tempdir::Dir::new("translations");
    let root = dir.path().join("shop");
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [&["api", "update"][..], &["check"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    (dir, root)
}

fn cart(dev: &Dev, locale: &str, count: u32) -> super::http::Reply {
    dev.send(
        "POST",
        "/cart",
        &[],
        &format!(r#"{{"number":"A-17","count":{count},"locale":"{locale}"}}"#),
    )
}

#[test]
fn a_project_with_translations_checks_and_its_tests_pass() {
    let (_dir, root) = project();
    let done = jadpo(&root, &["test"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    assert!(done.stdout.contains("3 passed"), "{}", done.stdout);
}

#[test]
fn the_text_is_written_in_the_locale_a_request_names() {
    let (_dir, root) = project();
    let dev = Dev::start_without_database(&root);
    for (locale, count, shipped, shown) in [
        (
            "en-GB",
            1,
            "Your order A-17 has shipped.",
            "1 item in your cart",
        ),
        (
            "en-GB",
            1200,
            "Your order A-17 has shipped.",
            "1,200 items in your cart",
        ),
        (
            "fr-FR",
            2,
            "Votre commande A-17 a été expédiée.",
            "2 articles dans votre panier",
        ),
        ("ar-EG", 3, "تم شحن طلبك A-17.", "\u{663} عناصر في سلتك"),
        ("ar-EG", 2, "تم شحن طلبك A-17.", "عنصران في سلتك"),
    ] {
        let reply = cart(&dev, locale, count);
        assert_eq!(reply.status, 200, "{locale}: {reply:?}");
        assert_eq!(
            reply.json(),
            serde_json::json!({ "shipped": shipped, "cart": shown }),
            "{locale} {count}"
        );
    }
}

#[test]
fn a_mistake_in_a_translation_file_shows_its_line_in_that_file() {
    let (_dir, root) = project();
    write(
        &root,
        "translations/fr_fr.json",
        "{\n  \"order_shipped\": \"Votre commande {order_number} a été expédiée.\"\n}\n",
    );
    let done = jadpo(&root, &["check"]);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    let out = format!("{}{}", done.stdout, done.stderr);
    for expected in [
        "error: `translations/fr_fr.json` has no translation for `items_in_cart`",
        " --> translations/fr_fr.json:1:1",
        "rule: translation.key_missing",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }

    write(
        &root,
        "translations/fr_fr.json",
        FILES[11]
            .1
            .replace("{order_number} a", "{order_number a")
            .as_str(),
    );
    let done = jadpo(&root, &["check"]);
    assert_eq!(done.code, 1);
    let out = format!("{}{}", done.stdout, done.stderr);
    assert!(
        out.contains("--> translations/fr_fr.json:2:36")
            && out.contains("rule: translation.message_invalid"),
        "{out}"
    );
    // Nothing runs while a file is wrong.
    let done = jadpo(&root, &["test"]);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
}

// --- watching ----------------------------------------------------------------------------

fn folder() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("translations-watch");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    for (path, text) in FILES {
        write(&dir, path, text);
    }
    dir
}

fn saved(path: &str) -> Vec<Edit> {
    vec![Edit::new(path, EditKind::Saved)]
}

fn ran(event: &Event) -> BTreeSet<String> {
    match &event.ran {
        Some(Ran::Done(run)) => run
            .results
            .iter()
            .map(|result| result.test.name.clone())
            .collect(),
        None => BTreeSet::new(),
        other => panic!("expected a run, got {other:?}"),
    }
}

fn bodies(executions: &[String]) -> usize {
    executions
        .iter()
        .filter(|query| query.starts_with("typed_body("))
        .count()
}

#[test]
fn a_save_of_a_translation_file_reruns_the_tests_that_reach_its_key() {
    let root = folder();
    let (mut watch, first) = Watch::start_with(
        Options {
            cwd: root.clone(),
            seed: Some(1),
            ..Options::default()
        },
        Database::recording(),
    )
    .expect("starts");
    assert_eq!(first.reached, None);
    assert_eq!(ran(&first).len(), 3);
    watch.executions();

    // The words of `order_shipped` in French: the tests of the action that
    // shows it rerun, the arithmetic does not, and no body is checked again.
    let text = fs::read_to_string(root.join("translations/fr_fr.json")).expect("read");
    write(
        &root,
        "translations/fr_fr.json",
        &text.replace("a été expédiée", "est partie"),
    );
    let event = watch
        .apply(saved("translations/fr_fr.json"))
        .expect("a change");
    let both: BTreeSet<String> = [
        "an order's shipping is told in French".to_string(),
        "the cart is described in each language".to_string(),
    ]
    .into();
    assert_eq!(ran(&event), both);
    assert_eq!(bodies(&watch.executions()), 0);

    // A save of the same text changes nothing.
    let text = fs::read_to_string(root.join("translations/fr_fr.json")).expect("read");
    write(&root, "translations/fr_fr.json", &text);
    assert!(watch.apply(saved("translations/fr_fr.json")).is_none());

    // The count's plural in the default locale: the other test.
    let text = fs::read_to_string(root.join("translations/en_gb.json")).expect("read");
    write(
        &root,
        "translations/en_gb.json",
        &text.replace("# items}", "# things}"),
    );
    let event = watch
        .apply(saved("translations/en_gb.json"))
        .expect("a change");
    assert_eq!(ran(&event), both);
}
