//! Translations as messages (area 10, chunk 5.4c) from the command line: a
//! project with messages and language files checks and tests, its mistakes
//! show a code frame in the language file, `jadpo dev` writes the text in
//! the locale a request names, and a save of a language file reruns exactly
//! the tests that reach it.
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
action show_cart(number: OrderNumber, count: ItemCount, reader_locale: Locale) -> CartText {
    return CartText {
        shipped: translations.shop.order_shipped(order_number: number, locale: reader_locale)
        cart: translations.shop.items_in_cart(count: count, locale: reader_locale)
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
        "domains/shop/translations/order_shipped.jadpo",
        "use shop/types { OrderNumber }\n\n/// Tells a customer their order has shipped.\ntype OrderShipped = Message {\n    order_number: OrderNumber\n}\n",
    ),
    (
        "domains/shop/translations/items_in_cart.jadpo",
        "/// How many items a cart holds.\ntype ItemsInCart = Message {\n    count: PluralCount\n}\n",
    ),
    (
        "domains/shop/translations/en_gb.jadpo",
        r#"translations {
    order_shipped: "Your order {order_number} has shipped."
    items_in_cart: {
        one: "{count} item in your cart"
        other: "{count} items in your cart"
    }
}
"#,
    ),
    (
        "domains/shop/translations/fr_fr.jadpo",
        r#"translations {
    order_shipped: "Votre commande {order_number} a été expédiée."
    items_in_cart: {
        one: "{count} article dans votre panier"
        many: "{count} d'articles dans votre panier"
        other: "{count} articles dans votre panier"
    }
}
"#,
    ),
    (
        "domains/shop/translations/ar_eg.jadpo",
        r#"translations {
    order_shipped: "تم شحن طلبك {order_number}."
    items_in_cart: {
        zero: "لا عناصر في سلتك"
        one: "عنصر واحد في سلتك"
        two: "عنصران في سلتك"
        few: "{count} عناصر في سلتك"
        many: "{count} عنصرًا في سلتك"
        other: "{count} عنصر في سلتك"
    }
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
fn a_mistake_in_a_language_file_shows_its_line_in_that_file() {
    let (_dir, root) = project();
    let fr = "domains/shop/translations/fr_fr.jadpo";
    write(
        &root,
        fr,
        "translations {\n    order_shipped: \"Votre commande {order_number} a été expédiée.\"\n}\n",
    );
    let done = jadpo(&root, &["check"]);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    let out = format!("{}{}", done.stdout, done.stderr);
    for expected in [
        "error: `items_in_cart` has no wording in `domains/shop/translations/fr_fr.jadpo`",
        " --> domains/shop/translations/fr_fr.jadpo:1:1",
        "rule: translation.wording_missing",
    ] {
        assert!(out.contains(expected), "missing `{expected}`:\n{out}");
    }

    write(
        &root,
        fr,
        FILES[13]
            .1
            .replace("{order_number} a", "{order_number a")
            .as_str(),
    );
    let done = jadpo(&root, &["check"]);
    assert_eq!(done.code, 1);
    let out = format!("{}{}", done.stdout, done.stderr);
    assert!(
        out.contains("--> domains/shop/translations/fr_fr.jadpo:2:36")
            && out.contains("rule: translation.wording_invalid"),
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
    let text =
        fs::read_to_string(root.join("domains/shop/translations/fr_fr.jadpo")).expect("read");
    write(
        &root,
        "domains/shop/translations/fr_fr.jadpo",
        &text.replace("a été expédiée", "est partie"),
    );
    let event = watch
        .apply(saved("domains/shop/translations/fr_fr.jadpo"))
        .expect("a change");
    let both: BTreeSet<String> = [
        "an order's shipping is told in French".to_string(),
        "the cart is described in each language".to_string(),
    ]
    .into();
    assert_eq!(ran(&event), both);
    assert_eq!(bodies(&watch.executions()), 0);

    // A save of the same text changes nothing.
    let text =
        fs::read_to_string(root.join("domains/shop/translations/fr_fr.jadpo")).expect("read");
    write(&root, "domains/shop/translations/fr_fr.jadpo", &text);
    assert!(
        watch
            .apply(saved("domains/shop/translations/fr_fr.jadpo"))
            .is_none()
    );

    // The count's plural in the default locale: the other test.
    let text =
        fs::read_to_string(root.join("domains/shop/translations/en_gb.jadpo")).expect("read");
    write(
        &root,
        "domains/shop/translations/en_gb.jadpo",
        &text.replace("{count} items in your cart", "{count} things in your cart"),
    );
    let event = watch
        .apply(saved("domains/shop/translations/en_gb.jadpo"))
        .expect("a change");
    assert_eq!(ran(&event), both);
}
