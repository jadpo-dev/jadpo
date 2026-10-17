//! Go to definition from a translation call (area 10, "Tooling"; chunk
//! 5.4d): the call of `translations.<domain>.<message>(...)` lands on the
//! message's type and on its wording in each language file.

use std::path::{Path, PathBuf};

use jadpo_lsp::Server;
use lsp_server::{Message, Request, RequestId};
use serde_json::{Value, json};

const ACTION: &str = "domains/shop/actions/describe.jadpo";

const DESCRIBE: &str = "use shop/types { Described, ParcelCount }\n\n/// What a customer is told.\naction describe(count: ParcelCount, reader_locale: Locale) -> Described {\n    return Described {\n        parcels: translations.shop.parcels(count: count, locale: reader_locale)\n    }\n}\n";

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in [
        (
            "app.jadpo",
            "application {}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb, Locale.de_de]\n}\n",
        ),
        (
            "domains/shop/types/parcel_count.jadpo",
            "/// How many parcels.\ntype ParcelCount = Int {\n    min: 0\n}\n",
        ),
        (
            "domains/shop/types/described.jadpo",
            "/// What a customer is told.\ntype Described = Output {\n    parcels: PresentationText\n}\n",
        ),
        (ACTION, DESCRIBE),
        (
            "domains/shop/translations/parcels.jadpo",
            "/// How many parcels an order has.\ntype Parcels = Message {\n    count: PluralCount\n}\n",
        ),
        (
            "domains/shop/translations/en_gb.jadpo",
            "translations {\n    parcels: {\n        one: \"{count} parcel\"\n        other: \"{count} parcels\"\n    }\n}\n",
        ),
        (
            "domains/shop/translations/de_de.jadpo",
            "translations {\n    parcels: {\n        one: \"{count} Paket\"\n        other: \"{count} Pakete\"\n    }\n}\n",
        ),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root.canonicalize().expect("real path")
}

fn start(root: &Path) -> Server {
    let params = serde_json::from_value(json!({
        "processId": null,
        "rootUri": format!("file://{}", root.display()),
        "capabilities": {},
    }))
    .expect("initialize params");
    Server::initialize(&params, root).0
}

fn definition(server: &mut Server, root: &Path, line: u32, character: u32) -> Value {
    let answer = server.handle(Message::Request(Request::new(
        RequestId::from(1),
        "textDocument/definition".to_string(),
        json!({
            "textDocument": {"uri": format!("file://{}", root.join(ACTION).display())},
            "position": {"line": line, "character": character},
        }),
    )));
    let Some(Message::Response(response)) = answer.first() else {
        panic!("{answer:?}");
    };
    response.response_result.clone().unwrap_or(Value::Null)
}

fn place(root: &Path, location: &Value) -> (String, u64, u64, u64) {
    (
        location["uri"]
            .as_str()
            .expect("uri")
            .trim_start_matches(&format!("file://{}/", root.display()))
            .to_string(),
        location["range"]["start"]["line"].as_u64().expect("line"),
        location["range"]["start"]["character"].as_u64().expect("start"),
        location["range"]["end"]["character"].as_u64().expect("end"),
    )
}

#[test]
fn a_call_goes_to_its_message_and_every_wording() {
    let root = project("lsp-translation-definition");
    let mut server = start(&root);
    // The call is on line 5: `parcels` starts after `translations.shop.`.
    let column = DESCRIBE
        .lines()
        .nth(5)
        .and_then(|line| line.find("parcels(").map(|at| at as u32 + 2))
        .expect("the call");
    let found = definition(&mut server, &root, 5, column);
    let places: Vec<(String, u64, u64, u64)> = found
        .as_array()
        .unwrap_or_else(|| panic!("a list of places: {found}"))
        .iter()
        .map(|location| place(&root, location))
        .collect();
    assert_eq!(
        places,
        [
            // The type `Parcels`.
            ("domains/shop/translations/parcels.jadpo".to_string(), 1, 5, 12),
            // Its wording, in each language file.
            ("domains/shop/translations/de_de.jadpo".to_string(), 1, 4, 11),
            ("domains/shop/translations/en_gb.jadpo".to_string(), 1, 4, 11),
        ]
    );
    // Any part of the call's name does the same; its arguments do not.
    let on_domain = definition(&mut server, &root, 5, column - 6);
    assert_eq!(on_domain, found);
    let on_argument = definition(&mut server, &root, 5, column + 20);
    assert_ne!(on_argument, found);
}
