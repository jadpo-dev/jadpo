//! `jadpo package verify --serve` (area 11, "The verifier"; chunk R6):
//! what starts [`jadpo_verifier::service`], and what ties it to this `jadpo`.
//!
//! It refuses to start rather than run a package's code with less isolation
//! than the sandbox gives: without the credential, without a sandbox, or on a
//! machine whose sandbox isn't good enough to serve with. Everything is
//! checked before it listens.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use jadpo_diag::{Decider, Diagnostic, rule};
use jadpo_term::{Block, ColorChoice, Environment, Json, Screen, Stream, Style};
use jadpo_verifier::git::GitPolicy;
use jadpo_verifier::sandbox::Sandbox;
use jadpo_verifier::service::{Options, SHORTEST_TOKEN, Service};
use jadpo_verifier::verification::{CHECKS, Verifier};

use crate::attestation;
use crate::package::arguments;
use crate::releases::{self, Cache, Https, Release, Report};
use crate::run::{Call, Reply, Session, finish};

/// The variable the registry's credential comes in, a secret where it's
/// deployed.
pub const TOKEN: &str = "JADPO_VERIFIER_TOKEN";

/// Where it listens unless told otherwise: this machine alone, since HTTPS
/// comes from what stands in front of it (`fly.toml`).
const LISTEN: &str = "127.0.0.1:8080";

/// The registry that keeps previous releases' archives.
const REGISTRY: &str = "https://jadpo.dev/registry/v1/";

/// The most a previous release's archive may be: what an archive unpacks
/// to at most, and its headers.
const LONGEST_ARCHIVE: usize = 128 * 1024 * 1024;

/// What `jadpo package verify --serve` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierArgs {
    pub listen: String,
    pub registry: String,
    pub json: bool,
    pub choice: ColorChoice,
}

/// Why it won't start.
fn start_failed(message: &str, reason: String, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::CLI_VERIFIER_START_FAILED, message.to_string())
        .reason(reason)
        .next_for(next, Decider::Human)
}

/// The credential and the sandbox, or why it can't serve.
fn ready(env: &Environment) -> Result<(String, Sandbox), Diagnostic> {
    let Some(token) = env.var(TOKEN).filter(|token| token.len() >= SHORTEST_TOKEN) else {
        return Err(start_failed(
            "`jadpo package verify --serve` has no credential",
            format!(
                "It answers only the registry, which proves itself with a credential in `{TOKEN}`, of at least {SHORTEST_TOKEN} characters."
            ),
            "Set JADPO_VERIFIER_TOKEN to the secret the registry holds",
        ));
    };
    let sandbox = Sandbox::detect().map_err(|why| {
        start_failed(
            "`jadpo package verify --serve` has no sandbox to run packages in",
            format!(
                "A package's tests are code from anyone, and they run only in a sandbox with no network: {why}."
            ),
            "Run it from its container image, which has bubblewrap, or install bubblewrap on Linux",
        )
    })?;
    if !sandbox.is_isolating() {
        return Err(start_failed(
            "`jadpo package verify --serve` serves only on Linux",
            format!(
                "A package's tests are code from anyone, and {} here shares this machine's processes and can't limit their memory.",
                sandbox.name()
            ),
            "Run it from its container image",
        ));
    }
    Ok((token.to_string(), sandbox))
}

/// The flags, and whether it could start.
pub(crate) fn reply(call: &Call) -> Reply {
    let usage = "Run `jadpo package verify --serve [--listen <address>] [--registry <url>]`";
    if let Some(extra) = call.arguments.get(1) {
        return Reply::failed(arguments(
            format!("`jadpo package verify --serve` takes no folder, not `{extra}`"),
            "Each request to it names a repository and a commit.".to_string(),
            usage.to_string(),
        ));
    }
    if call.value("--previous").is_some() || call.flag("--no-run") {
        return Reply::failed(arguments(
            "`--serve` can't be given with `--previous` or `--no-run`".to_string(),
            "Each request to it names its previous release, and it runs both passes itself."
                .to_string(),
            usage.to_string(),
        ));
    }
    let registry = call.value("--registry").unwrap_or(REGISTRY).to_string();
    if !registry.starts_with("https://") || !registry.ends_with('/') {
        return Reply::failed(arguments(
            format!("`{registry}` isn't a registry's address"),
            "Previous releases' archives are fetched from it, over HTTPS, and its address ends in `/`.".to_string(),
            usage.to_string(),
        ));
    }
    if let Err(diagnostic) = ready(&call.environment) {
        return Reply::Failed(Box::new(diagnostic));
    }
    Reply::Session(Session::Verifier(Box::new(VerifierArgs {
        listen: call.value("--listen").unwrap_or(LISTEN).to_string(),
        registry,
        json: false,
        choice: ColorChoice::Auto,
    })))
}

/// Serves until the process is stopped.
pub fn serve(args: &VerifierArgs, env: &Environment) -> u8 {
    let refuse = |diagnostic: Diagnostic| -> u8 {
        let output = finish(
            Some("package"),
            Reply::Failed(Box::new(diagnostic)),
            env,
            args.json,
            args.choice,
        );
        let _ = std::io::stdout().write_all(output.stdout.as_bytes());
        let _ = std::io::stderr().write_all(output.stderr.as_bytes());
        output.code
    };
    let (token, sandbox) = match ready(env) {
        Ok(ready) => ready,
        Err(diagnostic) => return refuse(diagnostic),
    };
    let jadpo = match std::env::current_exe().and_then(std::fs::canonicalize) {
        Ok(jadpo) => jadpo,
        Err(error) => {
            return refuse(start_failed(
                "`jadpo package verify --serve` can't find its own `jadpo`",
                format!("It runs itself in the sandbox: {error}."),
                "Run it from its container image",
            ));
        }
    };
    let name = sandbox.name();
    let verifier = Verifier {
        jadpo,
        sandbox,
        git: GitPolicy::https(),
        scratch: std::env::temp_dir(),
        limits: CHECKS,
        archives: archives(args.registry.clone()),
        releases: lowest_release(env),
    };
    let options = Options {
        log: Arc::new(|line: String| {
            let _ = writeln!(std::io::stdout(), "{line}");
        }),
        ..Options::default()
    };
    let service = match Service::start(&args.listen, &token, verifier, options) {
        Ok(service) => service,
        Err(why) => {
            return refuse(start_failed(
                "`jadpo package verify --serve` couldn't start",
                format!("Listening failed: {why}."),
                "Give another address with `--listen`, or stop what holds that one",
            ));
        }
    };
    let address = service.address().to_string();
    let style = Style::detect(env, args.json, args.choice, Stream::Stdout);
    let started = if args.json {
        Json::object([
            ("listening", Json::from(address.as_str())),
            ("jadpo", Json::from(crate::help::VERSION)),
            ("sandbox", Json::from(name)),
        ])
        .to_document()
    } else {
        Screen::new()
            .with(Block::header(
                "jadpo package verify --serve",
                [
                    format!("answering the registry at {address}"),
                    format!("checks with Jadpo {} in {name}", crate::help::VERSION),
                    "ctrl-c stops it".to_string(),
                ],
            ))
            .render(&style)
    };
    let _ = std::io::stdout().write_all(started.as_bytes());
    let _ = std::io::stdout().flush();
    service.wait();
    0
}

/// Fetches a previous release's archive from the registry; the verifier
/// checks it against its digest.
fn archives(registry: String) -> jadpo_verifier::verification::Archives {
    Arc::new(move |digest| {
        let url = format!("{registry}archives/{digest}");
        crate::download::get_all(&url, LONGEST_ARCHIVE)
    })
}

/// Nothing to tell anyone of fetching a release: nobody is watching.
struct Quiet;

impl Report for Quiet {
    fn fetching(&self, _release: Release) {}
    fn progress(&self, _done: u64, _total: Option<u64>) {}
}

/// The newest release of a line, fetched and checked as a project's own
/// `jadpo` would be.
fn lowest_release(env: &Environment) -> jadpo_verifier::verification::Releases {
    let home: Option<PathBuf> =
        crate::package_cache::home(env).map(|home| std::path::absolute(&home).unwrap_or(home));
    Arc::new(move |line| {
        let home = home.as_ref()?;
        let (major, minor) = line.split_once('.')?;
        let line = (major.parse().ok()?, minor.parse().ok()?);
        let cache = Cache::new(home, crate::toolchain::HOST);
        let release = match cache.newest_of(line) {
            Some(release) => release,
            None => releases::newest(&Https, line).ok().flatten()?,
        };
        cache
            .install(release, &Https, &attestation::Sigstore::JADPO, &Quiet)
            .ok()
    })
}
