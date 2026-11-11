//! Where else a `File` goes (area 16, "Attachments" and "What a `File`
//! holds"; area 11, "Types"; chunk B14): a bakery sends a supplier their
//! sheets back by email, every file of a list attached, and the bytes it
//! read of one beside them; and it sends a sheet to Acme's API, whose
//! operation's input holds a `File`, which goes out as its bytes in
//! Base64.
//!
//! The email's tests run through `jadpo test` here, and compiled with the
//! interpreter's records in the gate's compiled step (`compiled.rs`,
//! "files sent elsewhere"); the provider's body is read here from a
//! provider started on this machine. Nothing reaches a network.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use jadpo_term::Environment;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const ACME: &str = "shared/providers/acme/v1";

/// The bakery: its sheets, the email that sends them back, Acme's API
/// with `sandbox` as its sandbox's address, and the tests every executor
/// runs.
fn files_at(sandbox: &str) -> Vec<(String, String)> {
    let owned = |path: &str, text: &str| (path.to_string(), text.to_string());
    vec![
        owned(
            "app.jadpo",
            "application {\n    display_name: \"Bakery\"\n    connections: {\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/bakery/sheets\"\n        }\n        mail: {\n            driver: Driver.smtp\n            host: \"smtp.bakery.example\"\n        }\n        acme: {\n            package: Acme.v1\n            domains: [bakery]\n            credential: config.acme_key\n            environment: config.acme_environment\n        }\n    }\n    email: {\n        connection: mail\n        from: \"hello@bakery.example\"\n    }\n}\n\nlocales {\n    default: Locale.en_gb\n    supported: [Locale.en_gb]\n}\n",
        ),
        owned(
            "config.jadpo",
            "config {\n    acme_key: Text {\n        secret: true\n    }\n    acme_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n",
        ),
        owned(
            &format!("{ACME}/package.jadpo"),
            "/// Acme's sheets API.\npackage Acme.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            format!("{ACME}/provider.jadpo"),
            format!(
                "/// How Acme is reached.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n}}\n"
            ),
        ),
        owned(
            &format!("{ACME}/types/upload.jadpo"),
            "/// A sheet to file, and the sheets it replaces.\npublic type Upload = Object {\n    title: Text\n    sheet: File\n    replaces: List<File>\n}\n",
        ),
        owned(
            &format!("{ACME}/types/filed.jadpo"),
            "/// What Acme filed.\npublic type Filed = Object {\n    id: Text\n}\n",
        ),
        owned(
            &format!("{ACME}/resources/uploads.jadpo"),
            "public resource uploads {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/uploads\"\n        input: Upload\n        output: Filed\n        changes: false\n        source: PackageSource.written\n    }\n}\n",
        ),
        owned(
            "domains/bakery/types/sheet_text.jadpo",
            "/// A price sheet as a supplier sends it.\ntype SheetText = Text {\n    min_length: 1\n    max_length: 100000\n}\n",
        ),
        owned(
            "domains/bakery/failures/sheet_not_kept.jadpo",
            "/// The sheets store had no room for a sheet.\ntype SheetNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the sheet.\"\n}\n",
        ),
        owned(
            "domains/bakery/emails/sheets_email.jadpo",
            "/// A supplier's sheets, sent back to them.\ntype SheetsEmail = EmailMessage {\n    sheets: List<File>\n    whole: FileContents?\n}\n",
        ),
        owned(
            "domains/bakery/emails/sheets_email.txt",
            "Your sheets are attached.\n",
        ),
        owned(
            "domains/bakery/translations/en_gb.jadpo",
            "translations {\n    sheets_email: \"Your sheets\"\n}\n",
        ),
        owned(
            "domains/bakery/entities/mailing.jadpo",
            "/// A supplier's sheets sent back to them.\nentity Mailing {\n    id: Uuid\n    address: Email\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
        ),
        owned(
            "domains/bakery/actions/keep_sheet.jadpo",
            "use app { sheets }\nuse bakery/failures { SheetNotKept }\nuse bakery/types { SheetText }\n\n/// Keeps a price sheet.\naction keep_sheet(written: SheetText) fails SheetNotKept -> File {\n    return attempt sheets.put(encoding.to_utf8(written), FileType.csv) {\n        storage_full: SheetNotKept\n    }\n}\n",
        ),
        owned(
            "domains/bakery/actions/keep_terms.jadpo",
            "use app { sheets }\nuse bakery/failures { SheetNotKept }\nuse bakery/types { SheetText }\n\n/// Keeps a supplier's terms.\naction keep_terms(written: SheetText) fails SheetNotKept -> File {\n    return attempt sheets.put(encoding.to_utf8(written), FileType.pdf) {\n        storage_full: SheetNotKept\n    }\n}\n",
        ),
        owned(
            "domains/bakery/actions/send_sheets.jadpo",
            "use app { sheets }\nuse bakery/emails { SheetsEmail }\nuse bakery/entities { Mailing }\n\n/// Sends a supplier their sheets, and the terms' bytes as read.\naction send_sheets(address: Email, prices: File, terms: File) -> Mailing {\n    var whole: FileContents = sheets.read(terms)\n    var mailing: Mailing = Mailing.create {\n        set: {\n            address: address\n        }\n    }\n    emit SheetsEmail {\n        to: address\n        locale: Locale.en_gb\n        sheets: [prices, terms]\n        whole: whole\n    }\n    return mailing\n}\n",
        ),
        owned(
            "domains/bakery/actions/send_none.jadpo",
            "use bakery/emails { SheetsEmail }\nuse bakery/entities { Mailing }\n\n/// Sends a supplier none of their sheets.\naction send_none(address: Email) -> Mailing {\n    var mailing: Mailing = Mailing.create {\n        set: {\n            address: address\n        }\n    }\n    emit SheetsEmail {\n        to: address\n        locale: Locale.en_gb\n        sheets: []\n        whole: none\n    }\n    return mailing\n}\n",
        ),
        owned(
            "domains/bakery/actions/send_sheets.test.jadpo",
            "use bakery/actions { keep_sheet, keep_terms, send_none }\nuse bakery/entities { Mailing }\n\ntest \"each file a field holds goes as an attachment\" {\n    var prices: File = attempt keep_sheet(\"flour,1.20\")\n    var terms: File = attempt keep_terms(\"%PDF-1.7 terms\")\n    var mailing: Mailing = send_sheets(\"millers@example.test\", prices, terms)\n    assert mailing.address == \"millers@example.test\"\n    var sent: List<SentEmail> = email.sent\n    assert list.count(sent) == 1\n    assert list.all(sent, item -> item.attachments == [\"sheets-1.csv\", \"sheets-2.pdf\", \"whole.pdf\"])\n}\n\ntest \"an empty list attaches nothing\" {\n    var mailing: Mailing = send_none(\"millers@example.test\")\n    assert mailing.address == \"millers@example.test\"\n    var sent: List<SentEmail> = email.sent\n    assert list.count(sent) == 1\n    assert list.all(sent, item -> list.count(item.attachments) == 0)\n}\n",
        ),
        owned(
            "domains/bakery/actions/file_sheet.jadpo",
            "use app { acme }\nuse shared/providers/acme\n\n/// Files a sheet with Acme, with the one it replaces.\naction file_sheet(sheet: File, old: File) -> Acme.Filed {\n    return acme.uploads.create(Acme.Upload {\n        title: \"prices\"\n        sheet: sheet\n        replaces: [old]\n    })\n}\n",
        ),
        owned(
            "domains/bakery/actions/file_sheet.test.jadpo",
            "use app { acme }\nuse bakery/actions { keep_sheet }\nuse shared/providers/acme\n\ntest \"a sheet goes to Acme as its bytes\" {\n    fakes: {\n        acme.uploads.create: [Acme.Filed { id: \"f_1\" }]\n    }\n    var sheet: File = attempt keep_sheet(\"flour,1.20\")\n    var filed: Acme.Filed = file_sheet(sheet, sheet)\n    assert filed.id == \"f_1\"\n}\n",
        ),
    ]
}

/// The project every executor runs, for `compiled.rs`.
pub(crate) fn files() -> Vec<(String, String)> {
    files_at("https://sandbox.acme.example")
}

fn project(name: &str, files: Vec<(String, String)>) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("file-sends-b14-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        write(&root, &path, &text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn files_sent_elsewhere_pass_their_tests() {
    let root = project("tests", files());
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}

/// The project checked, with Acme's sandbox at `sandbox`.
fn program(name: &str, sandbox: &str) -> Program {
    let root = project(name, files_at(sandbox));
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:#?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "ACME_KEY" => Some("acme_test_key".into()),
            "ACME_ENVIRONMENT" => Some("sandbox".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    Program::new(prepared).with_config(&values)
}

/// A `File` in an operation's input goes out as its bytes in Base64, read
/// from its store as the call is made, wherever the input holds it; a
/// file whose bytes are gone from its store is a fault, and nothing is
/// sent.
#[test]
fn a_file_goes_to_a_provider_as_its_bytes() {
    let provider = FakeProvider::start();
    let program = program("provider", &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    let files = Rc::new(MemoryFiles::new());
    let sheet = files.keep("sheets", b"flour,1.20\n", "csv");
    let old = files.keep("sheets", b"%PDF-1.7 old", "pdf");
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(MemoryHost::new())
        .with("acme", driver)
        .with("sheets", Rc::clone(&files) as Rc<dyn Capability>);
    let call = |args: Vec<Value>| {
        let mut ctx = Ctx::of(
            &program,
            &host,
            Env {
                seed: Some(7),
                clock: Clock::TEST,
            },
        );
        let id = program.plan().find("file_sheet").expect("the action");
        runtime.block_on(program.run(&mut ctx, id, args))
    };
    provider.answer(200, r#"{"id": "f_1"}"#);
    let outcome = call(vec![sheet.clone(), old.clone()]);
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].json(),
        serde_json::json!({
            "title": "prices",
            "sheet": jadpo_value::encoding::to_base64(b"flour,1.20\n"),
            "replaces": [jadpo_value::encoding::to_base64(b"%PDF-1.7 old")],
        })
    );
    // Neither the store nor the key goes out.
    let key = jadpo_runtime::files::stored(&sheet).expect("a file").key;
    assert!(!sent[0].body.contains(&key), "{}", sent[0].body);

    let gone = jadpo_runtime::files::file_value(
        "sheets",
        &jadpo_std::files::key_for(&[9; 16]),
        None,
        4,
        "csv",
        None,
    );
    let refused = call(vec![gone, old]);
    assert!(
        matches!(&refused, Outcome::Fault(fault) if format!("{:?}", fault.kind).contains("no longer in its store")),
        "{refused:?}"
    );
    assert_eq!(provider.requests().len(), 1, "nothing more was sent");
}
