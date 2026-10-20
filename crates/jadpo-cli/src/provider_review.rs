//! What `jadpo check` and `jadpo package import` show of the scoped
//! review and the unconfirmed operations (area 11, "Mapping imported
//! types to standard types", layers 4 and 5; chunk 7.2e2).
//!
//! The screen, designed from the CLI design language and critiqued with
//! the design skills before it was built: nothing at all while every
//! type the app's calls reach is reviewed and every call is confirmed,
//! since a passing run is one line; otherwise a bold "To review" section,
//! the one instruction for the agent, then each package with the types
//! still to review listed and the reviewed ones counted; and a bold "Not
//! yet confirmed" section, the instruction with `jadpo package record`
//! whole on its first line, then the calls as the app writes them. Both
//! are agent work, not problems, so the footer counts them muted, before
//! the files, and the outcome stays a success. `--json` gives every call
//! and every type, reviewed or not.

use jadpo_check::review::ProviderReview;
use jadpo_term::{Block, Fact, Json};

use crate::package_import::{WIDTH, wrap};

/// What the agent does with a type to review, as `jadpo check` says it.
const REVIEW: &str = "Go through each type against the provider's documentation, then add it to `reviewed:` in `imported:` in its package's `provider.jadpo` (agent)";

/// What the agent does with an unconfirmed call.
const CONFIRM: &str = "Record each with `jadpo package record`, which checks a sandbox answer against its types (agent)";

/// `text` wrapped, each line at `indent`.
fn indented(text: &str, indent: &str) -> Vec<String> {
    wrap(text, WIDTH - indent.len())
        .into_iter()
        .map(|line| format!("{indent}{line}"))
        .collect()
}

/// The "To review" lines for `reviews`, each package's outstanding types
/// and how many of its reached types are reviewed; empty when none is
/// outstanding.
fn review_lines(reviews: &[&ProviderReview]) -> Vec<String> {
    let mut lines = Vec::new();
    for review in reviews {
        let outstanding: Vec<&str> = review.outstanding().collect();
        if outstanding.is_empty() {
            continue;
        }
        let done = review.types.len() - outstanding.len();
        lines.extend(indented(
            &format!(
                "{}  {}; {done} of {} reviewed",
                review.package,
                outstanding.join(", "),
                review.types.len()
            ),
            "  ",
        ));
    }
    lines
}

/// The calls no recording confirms, as the app writes them.
fn unconfirmed(reviews: &[&ProviderReview]) -> Vec<String> {
    reviews
        .iter()
        .flat_map(|review| review.unconfirmed())
        .map(|call| call.call.clone())
        .collect()
}

/// The sections `jadpo check` shows before its footer.
pub fn check_blocks(reviews: &[ProviderReview]) -> Vec<Block> {
    let reviews: Vec<&ProviderReview> = reviews.iter().collect();
    let mut blocks = Vec::new();
    let types = review_lines(&reviews);
    if !types.is_empty() {
        let mut lines = wrap(REVIEW, WIDTH);
        lines.extend(types);
        blocks.push(Block::Section {
            heading: "To review".to_string(),
            lines,
        });
    }
    let calls = unconfirmed(&reviews);
    if !calls.is_empty() {
        blocks.push(confirm_block(&calls));
    }
    blocks
}

fn confirm_block(calls: &[String]) -> Block {
    let mut lines = wrap(CONFIRM, WIDTH);
    lines.extend(indented(&calls.join(", "), "  "));
    Block::Section {
        heading: "Not yet confirmed".to_string(),
        lines,
    }
}

/// The footer's counts of what is left: types to review, calls
/// unconfirmed.
pub fn facts(reviews: &[ProviderReview]) -> Vec<Fact> {
    let types: usize = reviews.iter().map(|r| r.outstanding().count()).sum();
    let calls: usize = reviews.iter().map(|r| r.unconfirmed().count()).sum();
    let mut out = Vec::new();
    if types > 0 {
        out.push(Fact::plain(format!("{types} to review")));
    }
    if calls > 0 {
        out.push(Fact::plain(format!("{calls} unconfirmed")));
    }
    out
}

/// Every own integration's calls and reached types, for `--json`.
pub fn json(reviews: &[ProviderReview]) -> Json {
    Json::array(reviews.iter().map(|review| {
        Json::object([
            ("package", Json::from(review.package.as_str())),
            ("folder", Json::from(review.folder.as_str())),
            ("imported", Json::from(review.imported)),
            (
                "calls",
                Json::array(review.calls.iter().map(|call| {
                    Json::object([
                        ("call", Json::from(call.call.as_str())),
                        ("connection", Json::from(call.connection.as_str())),
                        ("operation", Json::from(call.operation.as_str())),
                        ("confirmed", Json::from(call.confirmed)),
                    ])
                })),
            ),
            (
                "types",
                Json::array(review.types.iter().map(|(name, reviewed)| {
                    Json::object([
                        ("name", Json::from(name.as_str())),
                        ("reviewed", Json::from(*reviewed)),
                    ])
                })),
            ),
        ])
    }))
}

/// The scoped review instruction an import ends with (area 11's fourth
/// layer), for the package it wrote: the types the app's calls reach
/// still to review, or, while the app calls none of its operations, when
/// the review starts; then the calls not yet confirmed.
pub fn import_blocks(review: Option<&ProviderReview>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let what = "use the standard type wherever one fits, add a decoding wherever a format is undocumented (seconds, cents, comma-separated lists), and mark which operations change something and which are idempotent";
    let called = review.is_some_and(|review| !review.calls.is_empty());
    let mut lines = if called {
        wrap(
            &format!(
                "Go through each type the app's calls reach against the provider's documentation: {what}; then add the type to `reviewed:` in `imported:` (agent)"
            ),
            WIDTH,
        )
    } else {
        wrap(
            &format!(
                "Once the app calls its operations, go through each type they reach against the provider's documentation: {what}; then add the type to `reviewed:` in `imported:`. `jadpo check` lists them (agent)"
            ),
            WIDTH,
        )
    };
    if let Some(review) = review {
        let outstanding: Vec<&str> = review.outstanding().collect();
        if called && outstanding.is_empty() {
            // Every type the calls reach is reviewed.
            return review_free(review);
        }
        if !outstanding.is_empty() {
            lines.extend(indented(&outstanding.join(", "), "  "));
        }
    }
    blocks.push(Block::Section {
        heading: "To review".to_string(),
        lines,
    });
    if let Some(review) = review {
        let calls = unconfirmed(&[review]);
        if !calls.is_empty() {
            blocks.push(confirm_block(&calls));
        }
    }
    blocks
}

/// An import whose reached types are all reviewed shows only the calls
/// still to confirm.
fn review_free(review: &ProviderReview) -> Vec<Block> {
    let calls = unconfirmed(&[review]);
    if calls.is_empty() {
        Vec::new()
    } else {
        vec![confirm_block(&calls)]
    }
}
