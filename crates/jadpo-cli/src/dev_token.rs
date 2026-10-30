//! External tokens in `jadpo dev` (area 8, "External tokens in tests and
//! `jadpo dev`"; area 14, `jadpo dev`; chunk A3). A session runs a local
//! issuer in the configured `external_token` issuer's place, its key made
//! for the session, and keeps the key in [`DEV_ISSUER`] while it runs, so
//! `jadpo dev token <user-id>` in another terminal signs a token for that
//! user's subject: a bearer token for a frontend or `curl`, without the
//! provider. The file is under `build/`, which `jadpo dev` keeps out of
//! git, readable by its owner alone, and removed when the session stops.

use std::path::Path;

use jadpo_diag::{Diagnostic, rule};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::Host;
use jadpo_runtime::config::ConfigValues;
use jadpo_runtime::http::local_issuer::{LOCAL_TOKEN_SECONDS, LocalIssuer};
use jadpo_term::Json;

use crate::run::{Call, Reply};

/// Where a running `jadpo dev` keeps its local issuer's key, with the
/// issuer and audience its tokens carry.
pub(crate) const DEV_ISSUER: &str = "build/dev/issuer.json";

/// The session's local issuer, its key from this machine's randomness.
pub(crate) fn new_issuer() -> Option<LocalIssuer> {
    LocalIssuer::new(&|bytes: &mut [u8]| jadpo_host_native::NativeHost::new().random(bytes))
}

/// The configured issuer and audience when the program signs in with
/// `external_token`, as `config` gives them.
fn configured(prepared: &Prepared, config: &ConfigValues) -> Option<(String, String)> {
    let external = prepared
        .plan()
        .routes()
        .identity
        .as_ref()?
        .external_token
        .as_ref()?;
    Some((
        config.text(&external.issuer)?.to_string(),
        config.text(&external.audience)?.to_string(),
    ))
}

/// Whether the program signs in with `external_token`.
pub(crate) fn takes_external_tokens(prepared: &Prepared) -> bool {
    prepared
        .plan()
        .routes()
        .identity
        .as_ref()
        .is_some_and(|identity| identity.external_token.is_some())
}

/// Writes [`DEV_ISSUER`] for the version being served, or removes it when
/// that version takes no external token.
pub(crate) fn remember(
    root: &Path,
    prepared: &Prepared,
    config: &ConfigValues,
    issuer: &LocalIssuer,
) {
    let path = root.join(DEV_ISSUER);
    let Some((configured_issuer, audience)) = configured(prepared, config) else {
        let _ = std::fs::remove_file(&path);
        return;
    };
    let document = serde_json::json!({
        "pid": std::process::id(),
        "key": issuer.seed(),
        "issuer": configured_issuer,
        "audience": audience,
    });
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = write_private(&path, &format!("{document}\n"));
}

/// Writes `text` to `path`, readable and writable by its owner alone.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let _ = std::fs::remove_file(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

/// Removes what [`remember`] wrote, when the session stops.
pub(crate) fn forget(root: &Path) {
    let _ = std::fs::remove_file(root.join(DEV_ISSUER));
}

/// What a running session's [`DEV_ISSUER`] holds.
struct Kept {
    issuer: LocalIssuer,
    configured_issuer: String,
    audience: String,
}

fn kept(root: &Path) -> Option<Kept> {
    let text = std::fs::read_to_string(root.join(DEV_ISSUER)).ok()?;
    let document: serde_json::Value = serde_json::from_str(&text).ok()?;
    let text = |name: &str| document.get(name)?.as_str().map(str::to_string);
    Some(Kept {
        issuer: LocalIssuer::from_seed(&text("key")?)?,
        configured_issuer: text("issuer")?,
        audience: text("audience")?,
    })
}

fn refused(headline: String, reason: &str, next: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CLI_DEV_ARGUMENTS, headline)
            .reason(reason)
            .next(next),
    )
}

const NEXT: &str = "Run `jadpo dev token <user-id>`, with the user's `id`";

/// `jadpo dev token <user-id>`: a bearer token for that user's subject
/// from the running session's local issuer, alone on stdout so it can be
/// put straight into a header; with `--json`, the token, the subject and
/// when it expires.
pub(crate) fn token(call: &Call) -> Reply {
    if let Some(extra) = call.arguments.get(2) {
        return refused(
            format!("`jadpo dev token` takes one user's id, not `{extra}` too"),
            "It signs one token, for one user.",
            NEXT,
        );
    }
    let Some(written) = call.arguments.get(1) else {
        return refused(
            "`jadpo dev token` needs a user's id".to_string(),
            "It signs a token for that user's subject, from the local issuer `jadpo dev` runs in the provider's place.",
            NEXT,
        );
    };
    let Some(user) = jadpo_std::standard::storage::uuid_bytes(written) else {
        return refused(
            format!("`{written}` isn't a user's id"),
            "A user's id is a UUID, as the user entity's `id: Uuid` holds it.",
            NEXT,
        );
    };
    let cwd = call
        .cwd
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_needed("dev token");
    };
    let loaded = jadpo_db::load(&root);
    if jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .any(jadpo_check::blocks_running)
    {
        return refused(
            "The project has errors".to_string(),
            "A token is for the user entity this version of the project declares, and it doesn't check.",
            "Run `jadpo check`, fix what it finds, then run this again",
        );
    }
    let program = Program::new(Prepared::checked(&loaded.db, loaded.project));
    let plan = program.runtime_plan();
    let Some(store) = jadpo_runtime::http::local_issuer::dev_store(&plan) else {
        return refused(
            "This app takes no external tokens".to_string(),
            "`jadpo dev token` signs a token as `external_token:` takes one, and `authentication { }` declares none.",
            "Sign in another way, or add `external_token:` to `authentication { }`",
        );
    };
    let Some(kept) = kept(&root) else {
        return refused(
            "No `jadpo dev` is running in this project".to_string(),
            "The token is signed by the local issuer a running `jadpo dev` holds; its key lasts only as long as the session.",
            "Start `jadpo dev` in another terminal, then run this again",
        );
    };
    let address = match crate::schedules::url_of(call, &root, &loaded, store) {
        Ok(Some(address)) => address,
        Ok(None) => {
            return refused(
                format!("The database `{store}` can't be found"),
                "The user is read from the database `jadpo dev` serves, and no variable, `--url` or running session gives its address.",
                "Run this beside `jadpo dev`, or add `--url postgresql://…`",
            );
        }
        Err(reply) => return reply,
    };
    let unreadable = |why: String| {
        refused(
            format!("The database `{store}` couldn't be read"),
            &format!("Reading the user failed: {why}."),
            "Check `jadpo dev` is still running, then run this again",
        )
    };
    let now = crate::schedules::now(call);
    let runtime = match jadpo_host_native::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => return unreadable(error.to_string()),
    };
    let database = match address.open() {
        Ok(database) => database,
        Err(error) => return unreadable(crate::schedules::store_error(&error)),
    };
    let subject = match runtime.block_on(jadpo_runtime::http::local_issuer::dev_subject(
        &plan, &database, user, now,
    )) {
        Ok(Some(subject)) => subject,
        Ok(None) => {
            return refused(
                format!("No user has the id `{written}`"),
                "No record of the user entity has that id, or its lifecycle no longer shows it.",
                "Use the `id` of a user in the database `jadpo dev` serves",
            );
        }
        Err(error) => return unreadable(crate::schedules::store_error(&error)),
    };
    let Some(token) = kept
        .issuer
        .token(&kept.configured_issuer, &kept.audience, &subject, now)
    else {
        return unreadable("the token couldn't be signed".to_string());
    };
    let expires = (now.div_euclid(1000) + LOCAL_TOKEN_SECONDS) * 1000;
    Reply::raw(
        format!("{token}\n"),
        Json::object([
            ("token", Json::from(token.as_str())),
            ("subject", Json::from(subject.as_str())),
            (
                "expiresAt",
                Json::from(jadpo_std::format_instant(expires).as_str()),
            ),
        ]),
    )
}
