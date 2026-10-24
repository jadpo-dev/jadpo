//! Native deployables, built and run against live PostgreSQL (roadmap
//! chunks 8.1o and 8.1o2; docs/compiler-runtime.md §2.6, "A native
//! deployable"): `jadpo build` writes teaching example 23, the repair
//! café, as one binary in `build/target/app/`, holding none of the testing
//! machinery. Started with its settings from the environment and nothing
//! else, it refuses to start without its database's address and against a
//! database that hasn't applied its last migration, naming each in a
//! `runtime.startup_failed` event that carries the build stamp; once the
//! database is up to date it starts, books a repair through `POST
//! /repairs`, waiting for the subscribers that find it a bench, answers its
//! health check, and on SIGTERM stops accepting, drains and ends.
//!
//! Then the café declares two deployables (chunk 8.1o2): a kiosk serving
//! the `main` API and running the subscribers, and an office serving a
//! `staff` API and running a schedule that sweeps every second. `jadpo
//! build` writes both, each beside only its own API's document and each
//! compiled from only what it reaches (chunk 8.1o3): the kiosk's binary
//! holds no sweeping, the office's no booking. The kiosk books a repair
//! but doesn't know the staff's route and, ticking, runs no schedule; the
//! office lists the repair's notes but doesn't book, and runs its
//! schedule, logging each tick as `runtime.ticked` with the store it
//! couldn't work in once a table it sweeps is gone (chunk 8.4c). Once
//! `schedules: { trigger: ScheduleTrigger.external }`
//! leaves its ticks to another scheduler (chunk 8.2g), the office's server
//! never ticks, and `office tick` reads its settings, runs the schedule due
//! once, logs `runtime.ticked` and ends.
//!
//! A deployable carries the locale data of its app's supported locales
//! only (chunk 8.2c; area 10, "Locales"): a shop supporting `en_gb` and
//! `fr_fr` is built with CLDR's words for those and not for German or
//! Spanish, and answers exactly what `jadpo dev`, holding every locale,
//! answers: dates, zones, relative times, money, currencies, numbers,
//! units, a language's name, a plural message, a locale it doesn't list
//! written in one it does, a list sorted in Swedish, which it doesn't
//! list, in its default's order (chunk 8.4i), and Turkish capitals, which
//! take any locale. Since chunk 8.4a it answers its limits as `jadpo
//! dev` does too, on its workers' own stacks: a thousand frames of a
//! plain recursion and of a heavy one, four hundred inline functions
//! running at once, a level more of each the `recursion_limit` fault, the
//! server serving on; and work that never waits, in an action's loops, a
//! function's and inline functions, stopped at its one-second deadline.
//!
//! Two projects share one build of the runtime (chunk 8.4g;
//! docs/compiler-runtime.md §2.5, "Keeping rustc time down"): two small
//! greeters built into one cache, the second compiling none of the
//! runtime the first compiled, each binary holding its own greeting and
//! never the other's however their builds alternate, and neither the
//! path of its project nor of the cache.
//!
//! The projects are kept beside the build's own folders
//! (`target/jadpo-compiled/deployable/`), and their builds in the cache
//! there (`target/jadpo-compiled/cache/`, the shop in one of its own, as
//! no other project here carries its locales), so a second run compiles
//! only what changed. The database is the one `JADPO_TEST_DATABASE_URL` names
//! or one started from the machine's PostgreSQL tools;
//! `JADPO_SKIP_POSTGRES=1` skips it, and the release builds with it.
//! Nothing leaves this machine.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use serde_json::Value;

use crate::support::probes;

/// Makes the folder `to` hold what `from` holds, over what is there, but
/// `build/` and `.jadpo/`, which a run keeps: what an earlier run added is
/// removed, and a file left as it was keeps its time, so cargo builds only
/// what changed.
pub(crate) fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folders");
    for entry in fs::read_dir(to).expect("a folder") {
        let entry = entry.expect("an entry");
        let name = entry.file_name();
        if (name == ".jadpo" || name == "build") && to.join("app.jadpo").exists() {
            continue;
        }
        if !from.join(&name).exists() {
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path).expect("removed");
            } else {
                fs::remove_file(&path).expect("removed");
            }
        }
    }
    for entry in fs::read_dir(from).expect("a folder") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            if entry.file_name() != ".jadpo" && entry.file_name() != "build" {
                copy_folder(&path, &target);
            }
        } else if fs::read(&target).ok() != fs::read(&path).ok() {
            fs::copy(&path, &target).expect("a copy");
        }
    }
}

/// Teaching example 23, copied where its build is kept between runs.
fn repair_cafe() -> PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/deployable/23-repair-cafe");
    copy_folder(&examples.join("23-repair-cafe"), &root);
    root
}

/// A shop supporting `en_gb` and `fr_fr` whose one route shows its text
/// in the locale asked for, over every kind of locale data the standard
/// library reads; and, from chunk 8.4a, the routes that probe a
/// deployable's limits ([`probes`]), whose actions its policy
/// ([`shop`]) opens to anyone.
const SHOP: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {}

locales {
    default: Locale.en_gb
    supported: [Locale.en_gb, Locale.fr_fr]
}
"#,
    ),
    (
        "api/main.jadpo_record",
        r#"// Generated by Jadpo from the routes of API main; update it with `jadpo api update`, never by hand.
record api 1

GET /openapi.json
    generated openapi
    authentication none
    status 200

POST /shown
    authentication none
    input ShowRequest (body)
    output ShopText
    status 200

output ShopText
    count: PresentationText
    currency: PresentationText
    day_month: PresentationText
    in_canadian_french: PresentationText
    in_german: PresentationText
    language: PresentationText
    parcels: PresentationText
    placed: PresentationText
    placed_short: PresentationText
    price: PresentationText
    share: PresentationText
    since: PresentationText
    swedish_order: List<Text { max_length: 80 }>
    took: PresentationText
    turkish_capitals: Text
    weekday: PresentationText

input ShowRequest
    reader: Locale
"#,
    ),
    (
        "translations/fr_fr.jadpo",
        r#"translations {
    jadpo: {
        deadline_exceeded: "Le délai est dépassé."
        idempotency_key_reused: "Cette clé a déjà servi."
        internal_fault: "Une erreur interne est survenue."
        invalid_input: "Les données sont invalides."
        invalid_request: "La requête est invalide."
        method_not_allowed: "Cette méthode n'est pas permise."
        rate_limited: "Trop de requêtes."
        request_too_large: "La requête est trop grande."
        route_not_found: "Cette adresse n'existe pas."
        unavailable: "Le service est indisponible."
    }
}
"#,
    ),
    (
        "domains/shop/types/show_request.jadpo",
        r#"/// What the shop's text is shown for: its reader's locale.
type ShowRequest = Input {
    reader: Locale
}
"#,
    ),
    (
        "domains/shop/types/day_month.jadpo",
        r#"/// A day and its month, as "27 Sept".
type DayMonth = TimeFormat {
    day: ComponentStyle.numeric
    month: ComponentStyle.short
}
"#,
    ),
    (
        "domains/shop/types/product_name.jadpo",
        r#"/// A product's name on a shelf.
type ProductName = Text {
    max_length: 80
}
"#,
    ),
    (
        "domains/shop/types/shelf_label.jadpo",
        r#"/// A product's name in capitals, for a shelf edge.
type ShelfLabel = Text {}
"#,
    ),
    (
        "domains/shop/types/shop_text.jadpo",
        r#"use shop/types { ProductName, ShelfLabel }

/// The shop's text for one reader.
type ShopText = Output {
    placed: PresentationText
    placed_short: PresentationText
    day_month: PresentationText
    since: PresentationText
    price: PresentationText
    currency: PresentationText
    share: PresentationText
    count: PresentationText
    took: PresentationText
    weekday: PresentationText
    language: PresentationText
    parcels: PresentationText
    in_german: PresentationText
    in_canadian_french: PresentationText
    swedish_order: List<ProductName>
    turkish_capitals: ShelfLabel
}
"#,
    ),
    (
        "domains/shop/translations/parcels.jadpo",
        r#"/// How many parcels an order has.
type Parcels = Message {
    count: PluralCount
}
"#,
    ),
    (
        "domains/shop/translations/en_gb.jadpo",
        r#"translations {
    parcels: {
        one: "{count} parcel"
        other: "{count} parcels"
    }
}
"#,
    ),
    (
        "domains/shop/translations/fr_fr.jadpo",
        r#"translations {
    parcels: {
        one: "{count} colis"
        many: "{count} colis"
        other: "{count} colis"
    }
}
"#,
    ),
    (
        "domains/shop/actions/show.jadpo",
        r#"use shop/types { DayMonth, ProductName, ShelfLabel, ShopText }

/// A locale saved before the shop stopped listing it.
function saved_german() -> Locale {
    return Locale.de_de
}

/// The shop's text in the reader's locale, over every kind of locale data.
action show(reader: Locale) -> ShopText {
    var placed_at: Instant = "2026-09-24T13:30:05Z"
    var german: Locale = saved_german()
    var canadian: Locale = Locale.fr_ca
    var oat: ProductName = "Oat"
    var zopf: ProductName = "Zopf"
    var oland: ProductName = "Ölandsbröd"
    return ShopText {
        placed: text.format(
            placed_at,
            zone: Zone.europe_paris,
            locale: reader,
            format: TimeFormat.date_time_full
        )
        placed_short: text.format(
            placed_at,
            zone: Zone.asia_tokyo,
            locale: reader,
            format: TimeFormat.date_time_short
        )
        day_month: text.format(placed_at, zone: Zone.europe_paris, locale: reader, format: DayMonth)
        since: temporal.format_relative(
            placed_at,
            now: "2026-09-25T09:00:00Z",
            zone: Zone.europe_paris,
            locale: reader
        )
        price: text.format(money.from_minor(123450, Currency.eur), locale: reader)
        currency: text.format(Currency.jpy, locale: reader)
        share: text.format(12.5%, locale: reader)
        count: text.format(1234567.891, locale: reader)
        took: text.format(90m, locale: reader)
        weekday: text.format(Weekday.thursday, locale: reader)
        language: text.format(Locale.de_de, locale: reader)
        parcels: translations.shop.parcels(count: 3, locale: reader)
        in_german: text.format(
            placed_at,
            zone: Zone.europe_paris,
            locale: german,
            format: TimeFormat.date_long
        )
        in_canadian_french: text.format(
            placed_at,
            zone: Zone.america_toronto,
            locale: canadian,
            format: TimeFormat.date_long
        )
        swedish_order: list.sort_by([zopf, oland, oat], name -> name, locale: Locale.sv_se)
        turkish_capitals: ShelfLabel(text.upper("istanbul", locale: Locale.tr_tr))
    }
}
"#,
    ),
    (
        "domains/shop/actions/show.test.jadpo",
        r#"use shop/types { ShopText }

test "the shop's text in British English" {
    var shown: ShopText = show(Locale.en_gb)
    assert shown.placed == "Thursday, 24 September 2026 at 15:30:05 Central European Summer Time"
    assert shown.placed_short == "24/09/2026, 22:30"
    assert shown.day_month == "24 Sept"
    assert shown.since == "Yesterday at 15:30"
    assert shown.price == "€1,234.50"
    assert shown.currency == "Japanese Yen"
    assert shown.share == "12.5%"
    assert shown.count == "1,234,567.891"
    assert shown.took == "1 hour, 30 minutes"
    assert shown.weekday == "Thursday"
    assert shown.language == "German (Germany)"
    assert shown.parcels == "3 parcels"
}

test "the shop's text in French" {
    var shown: ShopText = show(Locale.fr_fr)
    assert shown.placed == "jeudi 24 septembre 2026 à 15:30:05 heure d’été d’Europe centrale"
    assert shown.since == "Hier à 15:30"
    assert shown.currency == "yen japonais"
    assert shown.weekday == "jeudi"
    assert shown.language == "allemand (Allemagne)"
    assert shown.parcels == "3 colis"
}

test "a locale the shop doesn't list is written in one it does" {
    var shown: ShopText = show(Locale.en_gb)
    assert shown.in_german == "24 September 2026"
    assert shown.in_canadian_french == "24 septembre 2026"
}

test "sorting in a locale the shop doesn't list uses one it does, and capitals take any" {
    var shown: ShopText = show(Locale.en_gb)
    assert shown.swedish_order == ["Oat", "Ölandsbröd", "Zopf"]
    assert shown.turkish_capitals == "İSTANBUL"
}
"#,
    ),
    (
        "domains/shop/routes/shown.jadpo",
        r#"use shop/types { ShopText, ShowRequest }
use shop/actions { show }

/// The shop's text for a reader.
routes Shown {
    prefix: /shown

    /// The text in the locale the request names.
    POST / {
        authentication: RouteAuthentication.none
        input: ShowRequest
        output: ShopText
        run: show(input.reader)
    }
}
"#,
    ),
];

/// The shop with its probes, written where its build is kept between
/// runs, a file left as it was keeping its time, its API's record brought
/// up to date.
fn shop() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/deployable/locale-shop");
    let policy = format!(
        "use probe\nuse shop\n\npolicy {{\n    actions: {{\n{}        shop.show: [Access.public]\n    }}\n}}\n",
        probes::POLICY
    );
    let files = SHOP
        .iter()
        .chain(probes::FILES)
        .copied()
        .chain([("policy.jadpo", policy.as_str())]);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            fs::write(&path, text).expect("written");
        }
    }
    jadpo(&root, &["api", "update"]);
    root
}

/// What a probe answered: its status, its error's code when it gave one,
/// and how long it took.
type Probed = (u16, Option<String>, Duration);

/// Each walk of [`probes::WALKS`] and each spin of [`probes::SPINS`] sent
/// by `send` (a method, a path and a body, giving the status and the
/// body), each answered as it should be: a walk within the limits with
/// its depth, one past them `internal_fault`; a spin at its one-second
/// deadline with `deadline_exceeded`, well before its work would end.
/// What each answered, in order.
fn probed(send: impl Fn(&str, &str, &str) -> (u16, String)) -> Vec<Probed> {
    let code = |body: &str| {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|body| body["error"]["code"].as_str().map(str::to_string))
    };
    let mut answered = Vec::new();
    for (path, depth, expected) in probes::WALKS {
        let started = std::time::Instant::now();
        let (status, body) = send("POST", path, &probes::walk(*depth));
        match expected {
            Ok(()) => {
                assert_eq!(status, 200, "{path} {depth}: {body}");
                let walked: Value = serde_json::from_str(&body).expect("JSON");
                assert_eq!(walked["depth"], depth.to_string(), "{path} {depth}");
            }
            Err(expected) => {
                assert_eq!(status, 500, "{path} {depth}: {body}");
                assert_eq!(code(&body).as_deref(), Some(*expected), "{path} {depth}");
            }
        }
        answered.push((status, code(&body), started.elapsed()));
    }
    for path in probes::SPINS {
        let started = std::time::Instant::now();
        let (status, body) = send("POST", path, &probes::spin());
        let took = started.elapsed();
        assert_eq!(status, 504, "{path}: {body}");
        assert_eq!(code(&body).as_deref(), Some("deadline_exceeded"), "{path}");
        assert!(
            took < Duration::from_secs(10),
            "{path} stopped at its one-second deadline, not when its work ended: {took:?}"
        );
        answered.push((status, code(&body), took));
    }
    answered
}

/// A connection the test server gives, as the URL a `PostgresUrl` holds:
/// the disposable server answers with `host=… dbname=…` pairs, an
/// external one with a URL. A database named on the admin connection's
/// pairs is the last `dbname=`, as libpq reads it.
fn as_url(connection: &str) -> String {
    if connection.starts_with("postgres") {
        return connection.to_string();
    }
    let part = |key: &str| {
        connection
            .split_whitespace()
            .rev()
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('\'').to_string())
            .unwrap_or_default()
    };
    format!(
        "postgresql://{}@/{}?host={}&port={}",
        part("user"),
        part("dbname"),
        part("host"),
        part("port")
    )
}

/// The migrations of the project at `root`, store `main`'s, in order.
pub(crate) fn migrations(root: &Path) -> Vec<jadpo_std::Migration> {
    let loaded = jadpo_db::load(root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    steps.get("main").cloned().expect("store main's migrations")
}

/// Applies `migrations` to the database at `connection`.
pub(crate) fn apply(connection: &str, migrations: &[jadpo_std::Migration]) {
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    let database = jadpo_host_native::postgres(connection).expect("a store");
    runtime
        .block_on(jadpo_runtime::migrate(&database, migrations))
        .expect("applied");
}

/// The deployable at `binary` started with only `variables` in its
/// environment, and its log lines as they come.
fn start(binary: &Path, variables: &[(&str, &str)]) -> (Child, Receiver<Value>) {
    start_with(binary, &[], variables)
}

/// The deployable at `binary` started with `arguments` and only
/// `variables` in its environment, and its log lines as they come.
fn start_with(
    binary: &Path,
    arguments: &[&str],
    variables: &[(&str, &str)],
) -> (Child, Receiver<Value>) {
    let mut command = Command::new(binary);
    command
        .args(arguments)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    for (name, value) in variables {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("it starts");
    let stdout = child.stdout.take().expect("its output");
    let (lines, read) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let event: Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| panic!("a log line is one JSON object: {line}"));
            if lines.send(event).is_err() {
                break;
            }
        }
    });
    (child, read)
}

/// The next log event but a tick's, within `seconds`: a server logs each
/// tick its timer runs (chunk 8.4c), whenever the timer comes round.
fn next(lines: &Receiver<Value>, seconds: u64) -> Value {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let event = lines
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("a log line in time");
        if event["event"] != "runtime.ticked" {
            return event;
        }
    }
}

/// The first tick logged that `wanted` holds of, within `seconds`,
/// passing over any other line.
fn ticked(lines: &Receiver<Value>, seconds: u64, wanted: impl Fn(&Value) -> bool) -> Value {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let event = lines
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("a tick logged in time");
        if event["event"] == "runtime.ticked" && wanted(&event) {
            return event;
        }
    }
}

/// Waits for `child` to end, within `seconds`: its exit code.
fn ended(child: &mut Child, seconds: u64) -> Option<i32> {
    for _ in 0..seconds * 20 {
        if let Some(status) = child.try_wait().expect("its status") {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    panic!("the deployable didn't end within {seconds} s");
}

/// One HTTP/1.1 exchange with the server at `address`: the status and the
/// body.
fn exchange(address: &str, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("it accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("sent");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("an answer");
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("an HTTP answer: {answer}"));
    let body = answer
        .split_once("\r\n\r\n")
        .map_or(String::new(), |(_, body)| body.to_string());
    (status, body)
}

/// What a symbol naming `names`, a path in a crate, holds, legacy and v0
/// mangling alike: each name its length, then its text (as
/// `tools/deployable.py` reads them).
fn path(names: &[&str]) -> Vec<u8> {
    names
        .iter()
        .map(|name| format!("{}{name}", name.len()))
        .collect::<String>()
        .into_bytes()
}

fn holds(binary: &[u8], symbol: &[u8]) -> bool {
    binary.windows(symbol.len()).any(|window| window == symbol)
}

/// The stamp's part of an event, as the build said it of `deployable`,
/// one of the deployables it built.
fn stamped(event: &Value, built: &Value, deployable: &Value) {
    for key in ["commit", "modified", "compiler"] {
        assert_eq!(event[key], built[key], "{key} in {event}");
    }
    for key in ["build", "deployable", "target"] {
        assert_eq!(event[key], deployable[key], "{key} in {event}");
    }
}

/// The deployable `name` a build's data lists.
fn deployable<'a>(built: &'a Value, name: &str) -> &'a Value {
    built["deployables"]
        .as_array()
        .expect("deployables")
        .iter()
        .find(|deployable| deployable["deployable"] == name)
        .unwrap_or_else(|| panic!("`{name}` built: {built}"))
}

/// The rows `sql` gives against the database at `connection`, each cell
/// as text.
pub(crate) fn rows(connection: &str, sql: &str) -> Vec<Vec<String>> {
    use jadpo_runtime::Database;
    let store = jadpo_host_native::postgres(connection).expect("a store");
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begun");
        let rows = transaction.query(sql, &[]).await.expect("queried");
        transaction.commit().await.expect("committed");
        rows.into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| match value {
                        jadpo_std::SqlValue::Text(text) => text,
                        jadpo_std::SqlValue::Int8(n) => n.to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

/// How many rows the table `table` holds.
pub(crate) fn count(connection: &str, table: &str) -> i64 {
    rows(connection, &format!("SELECT count(*) FROM {table}"))[0][0]
        .parse()
        .expect("a count")
}

/// The cache folder the tests' builds share, as a user's builds share
/// theirs (chunk 8.4g): `XDG_CACHE_HOME` for `jadpo build`.
pub(crate) fn shared_cache() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/cache")
}

/// The environment `jadpo` is run in at `root`: nothing but the cache.
pub(crate) fn at(root: &Path, cache: &Path) -> Environment {
    Environment::empty()
        .with_cwd(root)
        .with_var("XDG_CACHE_HOME", &cache.to_string_lossy())
}

/// `jadpo` with `args` in `root`, which must succeed.
fn jadpo(root: &Path, args: &[&str]) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let output = run(&args, &at(root, &shared_cache()));
    assert_eq!(
        output.code, 0,
        "{args:?}: {}{}",
        output.stdout, output.stderr
    );
    output
}

/// What the café adds to declare its kiosk and its office: a `staff` API
/// with a route listing a repair's notes, and a schedule sweeping every
/// second, with the migration that keeps its state and the API's record.
fn declare(root: &Path) {
    let files = [
        (
            "app.jadpo",
            "application {\n    apis: {\n        main: {\n            prefix: /\n        }\n        staff: {\n            prefix: /staff\n        }\n    }\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n    events: {\n        store: main\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n            runs: [Trigger.subscribers]\n            target: Target.native\n        }\n        office: {\n            serves: [staff]\n            runs: [Trigger.schedules]\n            target: Target.native\n        }\n    }\n}\n",
        ),
        (
            "domains/cafe/types/pinned_note.jadpo",
            "use cafe/types { ThanksText }\n\n/// A thank-you note as the staff see it.\ntype PinnedNote = Output {\n    note: ThanksText\n}\n",
        ),
        (
            "domains/cafe/queries/notes_for.jadpo",
            "use cafe/entities { Repair, ThankYouNote }\nuse cafe/types { PinnedNote }\n\nquery notes_for(repair_id: Repair.id) -> List<PinnedNote> {\n    from: ThankYouNote\n    where: ThankYouNote.repair_id == repair_id\n    order: [ThankYouNote.id asc]\n    limit: 20\n}\n",
        ),
        (
            "domains/cafe/routes/staff.jadpo",
            "use cafe/entities { Repair }\nuse cafe/types { PinnedNote }\nuse cafe/queries { notes_for }\n\n/// The staff's view of the café.\nroutes Staff {\n    prefix: /staff\n    api: staff\n\n    /// The notes pinned up for a repair.\n    GET /{repair_id: Repair.id} {\n        authentication: RouteAuthentication.none\n        output: List<PinnedNote>\n        run: notes_for(repair_id)\n    }\n}\n",
        ),
        (
            "domains/cafe/actions/sweep_up.jadpo",
            "/// Sweeps the floor, which leaves nothing behind.\naction sweep_up() -> Unit {\n    assert 1 == 1\n}\n",
        ),
        (
            "domains/cafe/schedules/sweep.jadpo",
            "use cafe/actions { sweep_up }\n\n/// Sweeps the floor every second.\nschedule sweep {\n    every: 1s\n    run: sweep_up()\n}\n",
        ),
    ];
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            fs::write(&path, text).expect("written");
        }
    }
    jadpo(root, &["migrate", "new", "run_schedules"]);
    jadpo(root, &["api", "update"]);
}

#[cfg(unix)]
#[test]
fn a_teaching_example_is_built_refuses_to_start_out_of_date_serves_and_drains_then_as_two_deployables()
 {
    if !jadpo_test::live() {
        return;
    }
    let root = repair_cafe();
    let output = run(
        &["build".to_string(), "--json".to_string()],
        &at(&root, &shared_cache()),
    );
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let app = deployable(built, "app");
    assert_eq!(
        built["deployables"].as_array().map(Vec::len),
        Some(1),
        "{built}"
    );
    assert_eq!(app["target"], "native", "{built}");
    assert_eq!(built["compiler"], env!("CARGO_PKG_VERSION"), "{built}");
    let id = app["build"].as_str().expect("a build id");
    assert!(
        id.len() == 14 && id.starts_with("b_") && id[2..].chars().all(|c| c.is_ascii_hexdigit()),
        "{id}"
    );
    assert_eq!(app["binary"], "build/target/app/app", "{built}");
    assert_eq!(built["settings"], serde_json::json!(["DATABASE_URL"]));
    let binary = root.join("build/target/app/app");
    assert!(
        root.join("build/target/app/openapi/main.json").is_file(),
        "the API's document is beside the binary"
    );
    assert!(root.join("build/rust/app/rust-toolchain.toml").is_file());
    // The manifest `jadpo check` wrote names the same build.
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(root.join("build/manifest.json")).expect("read"))
            .expect("JSON");
    assert_eq!(manifest["deployables"]["app"]["build"], id, "{manifest}");

    // Built without the `testing` feature: it reaches none of the fakes,
    // recordings, crash points, records or a test's steps, but it is a
    // deployable, with the runtime's HTTP component and the driver factory.
    let bytes = fs::read(&binary).expect("the binary");
    for absent in [
        path(&["jadpo_runtime", "fakes"]),
        path(&["ai", "fake"]),
        path(&["capability", "Faked"]),
        path(&["jadpo_runtime", "recorded"]),
        path(&["jadpo_runtime", "prompt_recordings"]),
        path(&["jadpo_runtime", "crashes"]),
        path(&["jadpo_runtime", "record"]),
        path(&["jadpo_runtime", "test_steps"]),
    ] {
        assert!(
            !holds(&bytes, &absent),
            "it holds {}",
            String::from_utf8_lossy(&absent)
        );
    }
    for present in [
        path(&["jadpo_runtime", "http"]),
        path(&["jadpo_drivers", "factory"]),
    ] {
        assert!(
            holds(&bytes, &present),
            "it lacks {}",
            String::from_utf8_lossy(&present)
        );
    }

    // Without its database's address it doesn't start, naming the
    // setting and its variable.
    let (mut child, lines) = start(&binary, &[("HOST", "127.0.0.1"), ("PORT", "0")]);
    let failed = next(&lines, 30);
    assert_eq!(failed["event"], "runtime.startup_failed", "{failed}");
    stamped(&failed, built, app);
    assert_eq!(
        failed["problems"][0]["problem"], "setting_missing",
        "{failed}"
    );
    assert_eq!(
        failed["problems"][0]["variable"], "DATABASE_URL",
        "{failed}"
    );
    assert_eq!(ended(&mut child, 30), Some(1));

    // Against a database that hasn't applied the last migration, it
    // doesn't start either, naming it.
    let server = jadpo_test::postgres::Server::start().expect("a server");
    let connection = server.create_database("deployable").expect("a database");
    let url = as_url(&connection);
    let migrations = migrations(&root);
    let (last, earlier) = migrations.split_last().expect("migrations");
    apply(&connection, earlier);
    let settings = [
        ("DATABASE_URL", url.as_str()),
        ("HOST", "127.0.0.1"),
        ("PORT", "0"),
    ];
    let (mut child, lines) = start(&binary, &settings);
    let failed = next(&lines, 60);
    assert_eq!(failed["event"], "runtime.startup_failed", "{failed}");
    let problem = &failed["problems"][0];
    assert_eq!(problem["problem"], "migrations_pending", "{failed}");
    assert_eq!(problem["store"], "main", "{failed}");
    assert_eq!(
        problem["pending"],
        serde_json::json!([last.name]),
        "{failed}"
    );
    assert_eq!(ended(&mut child, 30), Some(1));

    // Up to date, it starts, serves and ticks.
    apply(&connection, &migrations);
    let (mut child, lines) = start(&binary, &settings);
    let started = next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    stamped(&started, built, app);
    assert_eq!(started["unbuilt"], serde_json::json!([]), "{started}");
    let address = started["address"]
        .as_str()
        .expect("its address")
        .to_string();
    let (status, body) = exchange(
        &address,
        "POST",
        "/repairs",
        r#"{"item":"lamp","symptom":"flickers when the shade is touched"}"#,
    );
    assert!(status == 201 || status == 202, "{status}: {body}");
    let ticket: Value = serde_json::from_str(&body).expect("a ticket");
    assert_eq!(ticket["item"], "lamp", "{ticket}");
    let (status, body) = exchange(&address, "GET", "/health/ready", "");
    assert_eq!(status, 200, "{body}");

    // SIGTERM while a booking is on its way: the request it is reading
    // is still answered, then it drains and ends.
    let body = r#"{"item":"bike","symptom":"the chain slips on every hill"}"#;
    let (head, rest) = body.split_at(20);
    let mut in_flight = TcpStream::connect(&address).expect("it accepts");
    in_flight
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a timeout");
    write!(
        in_flight,
        "POST /repairs HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{head}",
        body.len()
    )
    .expect("sent");
    in_flight.flush().expect("sent");
    std::thread::sleep(Duration::from_millis(300));
    let stopped = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success());
    std::thread::sleep(Duration::from_millis(300));
    in_flight.write_all(rest.as_bytes()).expect("the rest sent");
    let mut answer = String::new();
    in_flight.read_to_string(&mut answer).expect("an answer");
    assert!(
        answer.starts_with("HTTP/1.1 201") || answer.starts_with("HTTP/1.1 202"),
        "{answer}"
    );
    let last = next(&lines, 30);
    assert_eq!(last["event"], "runtime.stopped", "{last}");
    stamped(&last, built, app);
    assert_eq!(ended(&mut child, 30), Some(0));
    assert!(
        TcpStream::connect(&address).is_err(),
        "nothing listens once it has stopped"
    );

    // Declared: a kiosk serving `main` and running the subscribers, and
    // an office serving `staff` and running the schedules, each built
    // beside only its own API's document.
    declare(&root);
    let output = jadpo(&root, &["build", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let kiosk = deployable(built, "kiosk");
    let office = deployable(built, "office");
    assert_eq!(kiosk["serves"], serde_json::json!(["main"]), "{built}");
    assert_eq!(kiosk["runs"], serde_json::json!(["subscribers"]), "{built}");
    assert_eq!(office["serves"], serde_json::json!(["staff"]), "{built}");
    assert_eq!(office["runs"], serde_json::json!(["schedules"]), "{built}");
    assert_ne!(kiosk["build"], office["build"]);
    assert_eq!(
        kiosk["openapi"],
        serde_json::json!(["build/target/kiosk/openapi/main.json"]),
        "{built}"
    );
    assert_eq!(
        office["openapi"],
        serde_json::json!(["build/target/office/openapi/staff.json"]),
        "{built}"
    );
    // Each is compiled from only what it reaches (chunk 8.1o3): the
    // kiosk holds the booking but not the office's sweeping, the office
    // the other way round.
    let kiosk_binary = fs::read(root.join("build/target/kiosk/kiosk")).expect("the kiosk");
    let office_binary = fs::read(root.join("build/target/office/office")).expect("the office");
    assert!(holds(&kiosk_binary, b"action book_repair"));
    assert!(!holds(&kiosk_binary, b"action sweep_up"));
    assert!(holds(&office_binary, b"action sweep_up"));
    assert!(!holds(&office_binary, b"action book_repair"));
    apply(&connection, &self::migrations(&root));

    // The kiosk books, waiting for its subscribers, but has no staff
    // route; it ticks, and runs no schedule.
    let (mut child, lines) = start(&root.join("build/target/kiosk/kiosk"), &settings);
    let started = next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    stamped(&started, built, kiosk);
    let address = started["address"]
        .as_str()
        .expect("its address")
        .to_string();
    let (status, body) = exchange(
        &address,
        "POST",
        "/repairs",
        r#"{"item":"toaster","symptom":"pops up before the bread browns"}"#,
    );
    assert!(status == 201 || status == 202, "{status}: {body}");
    let ticket: Value = serde_json::from_str(&body).expect("a ticket");
    let repair = ticket["id"].as_str().expect("its id").to_string();
    let (status, body) = exchange(&address, "GET", &format!("/staff/{repair}"), "");
    assert_eq!(status, 404, "{body}");
    std::thread::sleep(Duration::from_millis(2_500));
    assert_eq!(
        count(&connection, "jadpo_schedules"),
        0,
        "the kiosk ran a schedule"
    );
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert_eq!(next(&lines, 30)["event"], "runtime.stopped");
    assert_eq!(ended(&mut child, 30), Some(0));

    // The office lists the repair's notes but doesn't book, and runs its
    // schedule each second.
    let (mut child, lines) = start(&root.join("build/target/office/office"), &settings);
    let started = next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    stamped(&started, built, office);
    let address = started["address"]
        .as_str()
        .expect("its address")
        .to_string();
    let (status, body) = exchange(&address, "GET", &format!("/staff/{repair}"), "");
    assert_eq!(status, 200, "{body}");
    let (status, body) = exchange(
        &address,
        "POST",
        "/repairs",
        r#"{"item":"lamp","symptom":"hums when it is switched on"}"#,
    );
    assert_eq!(status, 404, "{body}");
    let mut ran = 0;
    for _ in 0..80 {
        ran = count(&connection, "jadpo_schedule_runs");
        if ran > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(ran > 0, "the office ran no schedule");
    // Each tick its timer runs is logged, as `office tick` logs its one,
    // by the schedule's name (chunk 8.4c).
    let line = ticked(&lines, 30, |line| {
        line["ran"].as_array().is_some_and(|ran| !ran.is_empty())
    });
    stamped(&line, built, office);
    assert_eq!(line["ran"][0]["schedule"], "sweep", "{line}");
    assert_eq!(line["problems"], serde_json::json!([]), "{line}");
    assert_eq!(line["unbuilt"], serde_json::json!([]), "{line}");
    // A store the tick can't work in is logged too: here the table of
    // idempotency keys every tick sweeps, gone for a moment.
    rows(
        &connection,
        "ALTER TABLE jadpo_idempotency_keys RENAME TO jadpo_idempotency_keys_away",
    );
    let line = ticked(&lines, 30, |line| {
        line["problems"]
            .as_array()
            .is_some_and(|problems| !problems.is_empty())
    });
    rows(
        &connection,
        "ALTER TABLE jadpo_idempotency_keys_away RENAME TO jadpo_idempotency_keys",
    );
    let problem = &line["problems"][0];
    assert_eq!(problem["problem"], "store_failed", "{line}");
    assert_eq!(problem["store"], "main", "{line}");
    assert!(
        problem["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("`main` couldn't be ticked: ")),
        "{line}"
    );
    assert!(
        !line.to_string().contains(&url),
        "the log holds its address: {line}"
    );
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert_eq!(next(&lines, 30)["event"], "runtime.stopped");
    assert_eq!(ended(&mut child, 30), Some(0));

    // Ticked by another scheduler (area 12, question 16; chunk 8.2g): the
    // office's server never ticks, and `office tick` runs one tick and
    // ends, as often as the build says.
    let app = fs::read_to_string(root.join("app.jadpo")).expect("read");
    let external = app.replace(
        "    events: {\n        store: main\n    }\n",
        "    events: {\n        store: main\n    }\n    schedules: {\n        trigger: ScheduleTrigger.external\n    }\n",
    );
    assert_ne!(app, external);
    fs::write(root.join("app.jadpo"), external).expect("written");
    let output = jadpo(&root, &["build", "office", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let office = deployable(built, "office");
    assert_eq!(
        office["tick"],
        serde_json::json!({ "command": "build/target/office/office tick", "every_ms": 1000 }),
        "{built}"
    );
    let binary = root.join("build/target/office/office");
    let before = count(&connection, "jadpo_schedule_runs");
    let (mut child, lines) = start(&binary, &settings);
    let started = next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    assert_eq!(started["trigger"], "external", "{started}");
    std::thread::sleep(Duration::from_millis(2_500));
    assert_eq!(
        count(&connection, "jadpo_schedule_runs"),
        before,
        "the office's server ticked"
    );
    Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert_eq!(next(&lines, 30)["event"], "runtime.stopped");
    assert_eq!(ended(&mut child, 30), Some(0));

    // A tick reads its settings as serving does.
    let (mut child, lines) = start_with(&binary, &["tick"], &[]);
    let failed = next(&lines, 30);
    assert_eq!(failed["event"], "runtime.startup_failed", "{failed}");
    assert_eq!(
        failed["problems"][0]["variable"], "DATABASE_URL",
        "{failed}"
    );
    assert_eq!(ended(&mut child, 30), Some(1));

    // One tick runs the schedule due, says so, and ends.
    let (mut child, lines) = start_with(&binary, &["tick"], &[("DATABASE_URL", url.as_str())]);
    let ticked = ticked(&lines, 60, |_| true);
    assert_eq!(ticked["event"], "runtime.ticked", "{ticked}");
    stamped(&ticked, built, office);
    assert_eq!(ticked["ran"][0]["schedule"], "sweep", "{ticked}");
    assert_eq!(ticked["ran"][0]["outcome"], "succeeded", "{ticked}");
    assert_eq!(ticked["problems"], serde_json::json!([]), "{ticked}");
    assert_eq!(ended(&mut child, 30), Some(0));
    assert_eq!(count(&connection, "jadpo_schedule_runs"), before + 1);

    // A database that takes the connection and never answers holds
    // neither a start nor a tick past SIGTERM (chunk 8.4d).
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("a port on this machine");
    let silent = format!(
        "postgres://app@127.0.0.1:{}/app",
        silent.local_addr().expect("its address").port()
    );
    for arguments in [&[][..], &["tick"][..]] {
        let (mut child, lines) = start_with(
            &binary,
            arguments,
            &[
                ("DATABASE_URL", silent.as_str()),
                ("HOST", "127.0.0.1"),
                ("PORT", "0"),
            ],
        );
        std::thread::sleep(Duration::from_millis(1_000));
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .expect("kill runs");
        let stopped = next(&lines, 5);
        assert_eq!(
            stopped["event"], "runtime.stopped",
            "{arguments:?}: {stopped}"
        );
        assert_eq!(ended(&mut child, 5), Some(0), "{arguments:?}");
    }

    // Anything else is no command.
    let (mut child, lines) = start_with(&binary, &["ticks"], &[]);
    let failed = next(&lines, 30);
    assert_eq!(
        failed["problems"][0]["problem"], "command_unknown",
        "{failed}"
    );
    assert_eq!(ended(&mut child, 30), Some(2));
}

#[cfg(unix)]
#[test]
fn a_deployable_carries_its_supported_locales_and_answers_what_dev_answers() {
    if !jadpo_test::live() {
        return;
    }
    let root = shop();
    let tested = run(&["test".to_string()], &Environment::empty().with_cwd(&root));
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);

    // `jadpo dev`, with every locale, answers first.
    let readers = ["en-GB", "fr-FR"];
    let body = |reader: &str| format!(r#"{{"reader":"{reader}"}}"#);
    let dev = super::http::Dev::start_without_database(&root);
    let expected: Vec<Value> = readers
        .iter()
        .map(|reader| {
            let reply = dev.send("POST", "/shown", &[], &body(reader));
            assert_eq!(reply.status, 200, "{reader}: {reply:?}");
            reply.json()
        })
        .collect();
    // Its limits, probed (chunk 8.4a): the interpreter's, which the
    // deployable's compiled code must match below.
    let dev_probed = probed(|method, path, body| {
        let reply = dev.send(method, path, &[], body);
        (reply.status, reply.body().to_string())
    });
    drop(dev);
    assert_eq!(expected[0]["weekday"], "Thursday", "{}", expected[0]);
    assert_eq!(expected[1]["weekday"], "jeudi", "{}", expected[1]);

    let cache = root.with_file_name("locale-shop-cache");
    let output = run(
        &["build".to_string(), "--json".to_string()],
        &at(&root, &cache),
    );
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);

    // The data it was built with, the one locale set in its cache, holds
    // English and French month names, and no other language's.
    let carried: Vec<PathBuf> = within(&cache.join("jadpo/builds"), 3)
        .into_iter()
        .filter(|folder| {
            folder
                .parent()
                .is_some_and(|parent| parent.ends_with("locales"))
        })
        .collect();
    assert_eq!(carried.len(), 1, "{carried:?}");
    // A cargo cache restore can leave the digest folder but prune mod.rs.
    // Build again against the same cargo target: repair must happen before
    // ICU4X's include! reads its generated data, not merely before export.
    fs::remove_file(carried[0].join("mod.rs")).expect("pruned locale module");
    let rebuilt = run(
        &["build".to_string(), "--json".to_string()],
        &at(&root, &cache),
    );
    assert_eq!(rebuilt.code, 0, "{}{}", rebuilt.stdout, rebuilt.stderr);
    assert!(carried[0].join("mod.rs").is_file());
    let months = fs::read_to_string(carried[0].join("datetime_names_month_gregorian_v1.rs.data"))
        .expect("the carried month names");
    for word in ["September", "septembre"] {
        assert!(months.contains(word), "{word} carried");
    }
    for word in ["septiembre", "settembre", "Oktober"] {
        assert!(!months.contains(word), "{word} not carried");
    }
    // Nor does the binary: Thursday is there in English and French, not
    // in German or Spanish.
    let binary = root.join("build/target/app/app");
    let bytes = fs::read(&binary).expect("the binary");
    for word in ["Thursday", "jeudi"] {
        assert!(holds(&bytes, word.as_bytes()), "{word} in the binary");
    }
    for word in ["Donnerstag", "jueves"] {
        assert!(!holds(&bytes, word.as_bytes()), "{word} not in the binary");
    }

    // It answers each reader exactly as `jadpo dev` did.
    let (mut child, lines) = start(&binary, &[("HOST", "127.0.0.1"), ("PORT", "0")]);
    let started = next(&lines, 60);
    assert_eq!(started["event"], "runtime.started", "{started}");
    let address = started["address"]
        .as_str()
        .expect("its address")
        .to_string();
    for (reader, expected) in readers.iter().zip(&expected) {
        let (status, answer) = exchange(&address, "POST", "/shown", &body(reader));
        assert_eq!(status, 200, "{reader}: {answer}");
        let answer: Value = serde_json::from_str(&answer).expect("JSON");
        assert_eq!(&answer, expected, "{reader}");
    }
    // Its limits answer as `jadpo dev`'s did, on its workers' own stacks
    // (chunk 8.4a): a thousand frames of any of the walks, four hundred
    // inline functions, and a level more of each the `recursion_limit`
    // fault, logged as such, the server serving on; and work that never
    // waits stopped at its deadline, in loops and in inline functions.
    let deployable_probed = probed(|method, path, body| exchange(&address, method, path, body));
    let answers = |probed: &[Probed]| {
        probed
            .iter()
            .map(|(status, code, _)| (*status, code.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(answers(&deployable_probed), answers(&dev_probed));
    let mut limits = 0;
    while limits < 3 {
        let event = next(&lines, 30);
        if event["classification"] == "recursion_limit" {
            limits += 1;
        }
    }
    let (status, _) = exchange(&address, "POST", "/shown", &body("en-GB"));
    assert_eq!(status, 200, "it serves on");
    let stopped = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(stopped.success());
    assert_eq!(ended(&mut child, 30), Some(0));
}

/// The folders `depth` levels below `folder`, but those whose names start
/// with a dot.
fn within(folder: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry.expect("an entry").path();
        let hidden = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if !path.is_dir() || hidden {
            continue;
        }
        if depth == 1 {
            found.push(path);
        } else {
            found.extend(within(&path, depth - 1));
        }
    }
    found
}

/// A small project whose one route greets with `words`, written at `name`
/// beside the other projects, its API's record brought up to date.
fn greeter(name: &str, words: &str) -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/deployable")
        .join(name);
    let action = format!(
        "use greeting/types {{ Hello, Words }}\n\n/// Greets whoever asks.\naction greet() -> Hello {{\n    return Hello {{\n        words: Words(\"{words}\")\n    }}\n}}\n"
    );
    let files = [
        ("app.jadpo", "application {}\n"),
        (
            "policy.jadpo",
            "use greeting\n\npolicy {\n    actions: {\n        greeting.greet: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/greeting/types/words.jadpo",
            "/// What the greeting says.\ntype Words = Text {}\n",
        ),
        (
            "domains/greeting/types/hello.jadpo",
            "use greeting/types { Words }\n\n/// A greeting.\ntype Hello = Output {\n    words: Words\n}\n",
        ),
        ("domains/greeting/actions/greet.jadpo", action.as_str()),
        (
            "domains/greeting/routes/greeting.jadpo",
            "use greeting/types { Hello }\nuse greeting/actions { greet }\n\n/// The greeting.\nroutes Greeting {\n    prefix: /greeting\n\n    /// Greets.\n    POST / {\n        authentication: RouteAuthentication.none\n        output: Hello\n        run: greet()\n    }\n}\n",
        ),
    ];
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            fs::write(&path, text).expect("written");
        }
    }
    jadpo(&root, &["api", "update"]);
    root
}

/// The runtime's libraries in the shared cache's native targets, each
/// with when it was written.
fn runtime_libraries() -> Vec<(PathBuf, std::time::SystemTime)> {
    let mut found = Vec::new();
    for target in within(&shared_cache().join("jadpo/builds"), 3) {
        let deps = target.join("release/deps");
        let native = target
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("native-"));
        let Ok(entries) = fs::read_dir(&deps) else {
            continue;
        };
        if !native {
            continue;
        }
        for entry in entries {
            let path = entry.expect("an entry").path();
            let name = path.file_name().expect("a name").to_string_lossy();
            if name.starts_with("libjadpo_runtime-") && name.ends_with(".rlib") {
                let written = fs::metadata(&path)
                    .and_then(|meta| meta.modified())
                    .expect("a time");
                found.push((path, written));
            }
        }
    }
    found
}

/// Two projects built into one cache share one build of the runtime
/// (chunk 8.4g; docs/compiler-runtime.md §2.5, "Keeping rustc time
/// down"): the second compiles none of what the first compiled of it,
/// neither keeps a target or locale data of its own, and each binary
/// holds its own greeting, never the other's, however their builds
/// alternate, though both build a deployable `app` from packages of the
/// same names; and neither binary holds its project's path or the
/// cache's, so where a project is built doesn't change what it builds.
#[cfg(unix)]
#[test]
fn projects_share_one_build_of_the_runtime_and_never_each_others_program() {
    if !jadpo_test::live() {
        return;
    }
    let one = greeter("greeter-one", "Hello from one");
    let two = greeter("greeter-two", "Hello from two");
    jadpo(&one, &["build"]);
    let compiled = runtime_libraries();
    assert!(!compiled.is_empty(), "the runtime compiled in the cache");
    jadpo(&two, &["build"]);
    let after = runtime_libraries();
    for library in &compiled {
        assert!(after.contains(library), "{library:?} compiled again");
    }
    // The first again, changing nothing: its program is the one the
    // other's build didn't leave behind.
    jadpo(&one, &["build"]);
    let cache = shared_cache();
    for (root, own, other) in [
        (&one, "Hello from one", "Hello from two"),
        (&two, "Hello from two", "Hello from one"),
    ] {
        for leftover in ["build/rust/target", "build/rust/cldr"] {
            assert!(!root.join(leftover).exists(), "{leftover} kept");
        }
        let binary = fs::read(root.join("build/target/app/app")).expect("the binary");
        assert!(holds(&binary, own.as_bytes()), "{own} in its binary");
        assert!(!holds(&binary, other.as_bytes()), "{other} in its binary");
        for folder in [root.as_path(), cache.as_path()] {
            let path = folder.to_string_lossy();
            assert!(!holds(&binary, path.as_bytes()), "{path} in its binary");
        }
    }
}
