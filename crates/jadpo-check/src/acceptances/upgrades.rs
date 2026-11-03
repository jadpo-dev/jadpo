//! The review of a package upgrade (area 18, "Package upgrades"; chunk
//! V4): what the app used of each package on the before side, compared
//! with the package now, matched by package, resource and operation with
//! the version ignored, so a call moved from one version's connection to
//! the next is compared rather than listed as removed and added.
//!
//! Each side is read from text alone, its sources and its packages'
//! surfaces, so `jadpo check` needs no build of the base and `jadpo package
//! update` reads the same review from the tree it is about to change.
//! Use is read from the names written: `Postcodes.uk.parse`, or
//! `couriers.pickups.create` through a connection. A connection the app's
//! code never writes, or that `application { }` names elsewhere (`email: {
//! connection: mail }`), serves the app through a capability, so all its
//! operations count as used. A name in a string counts too: reading too
//! much as used asks a person about more, never less.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::record::{self as format, Entry};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode};

/// What one side of an upgrade holds.
#[derive(Clone, Debug, Default)]
pub struct Side {
    /// Each installed package's surface, by `AcmeCouriers.v1`.
    surfaces: BTreeMap<String, Vec<Entry>>,
    /// Each connection's package: `couriers` is `AcmeCouriers.v1`.
    connections: BTreeMap<String, String>,
    /// The connections `application { }` names outside `connections:`.
    named: BTreeSet<String>,
    /// Every dotted name the app's code writes.
    chains: BTreeSet<Vec<String>>,
}

/// One change a person must accept.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// As an acceptance lists it: `AcmeCouriers.Booking.driver_phone`.
    pub subject: String,
    /// `PackageChange.field_removed`.
    pub change: String,
    /// The package as it is now, the surface the change shows in:
    /// `AcmeCouriers.v2`.
    pub package: String,
    /// The surface's entry it shows in, when one still has it.
    pub heading: Option<String>,
    /// In plain words, for the approval report.
    pub said: String,
    /// What the app uses that it came through: `bookings.get`, or `host`.
    pub through: String,
    /// What happened: `field removed`.
    pub what: String,
    /// What it happened to: `Booking.driver_phone`.
    pub detail: String,
}

impl Finding {
    /// The mark of its line in the review: `+`, `-` or `~`.
    pub fn mark(&self) -> char {
        match self.what.as_str() {
            "new failure" => '+',
            "removed" | "field removed" => '-',
            _ => '~',
        }
    }
}

/// `AcmeCouriers.v1` as `AcmeCouriers`.
fn code_of(package: &str) -> &str {
    package.rsplit_once('.').map_or(package, |(code, _)| code)
}

/// Whether a project-relative path is what [`Side::read`] takes: the app's
/// own source, tests and acceptances aside, or an installed surface.
pub fn read_for_a_review(path: &str) -> bool {
    let packages = format!("{}/", jadpo_db::PACKAGES);
    if path.starts_with(&packages) {
        return path.ends_with(&format!("/{}", format::SURFACE_FILE));
    }
    path.ends_with(".jadpo")
        && !path.ends_with(".test.jadpo")
        && !path.starts_with("acceptances/")
        && !path.starts_with("build/")
}

impl Side {
    /// One side, from its files by project-relative path.
    pub fn read<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Side {
        let mut side = Side::default();
        for (path, text) in files {
            if !read_for_a_review(path) {
                continue;
            }
            if path.ends_with(format::SURFACE_FILE) {
                let entries =
                    format::read(&format::normalise(text), &format::SURFACE).unwrap_or_default();
                let package = entries
                    .iter()
                    .find_map(|entry| entry.heading.strip_prefix("package "));
                if let Some(package) = package {
                    side.surfaces.insert(package.to_string(), entries.clone());
                }
                continue;
            }
            if path == jadpo_db::PROJECT_FILE {
                side.read_application(text);
            }
            chains_in(text, &mut side.chains);
        }
        side
    }

    /// `app.jadpo`'s connections to packages, and which it names elsewhere.
    fn read_application(&mut self, text: &str) {
        let parse = jadpo_syntax::parse_file(text);
        let application = parse
            .syntax()
            .children()
            .filter_map(ast::SingletonDecl::cast)
            .find(|decl| {
                decl.keyword()
                    .is_some_and(|word| word.text() == "application")
            });
        let Some(application) = application else {
            return;
        };
        let mut elsewhere = String::new();
        for (name, member) in crate::tree::members(application.member_list()) {
            if crate::tree::name_text(&name).as_deref() != Some("connections") {
                elsewhere.push_str(&crate::tree::written(member.syntax()));
                elsewhere.push(' ');
                continue;
            }
            for (connection, inner) in crate::tree::members(member.block()) {
                let package = crate::tree::members(inner.block())
                    .into_iter()
                    .find(|(key, _)| crate::tree::name_text(key).as_deref() == Some("package"))
                    .and_then(|(_, value)| value.value())
                    .map(|value| crate::tree::written(value.syntax()));
                if let (Some(connection), Some(package)) =
                    (crate::tree::name_text(&connection), package)
                {
                    self.connections.insert(connection, package);
                }
            }
        }
        let words: BTreeSet<&str> = elsewhere
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .collect();
        self.named = self
            .connections
            .keys()
            .filter(|connection| words.contains(connection.as_str()))
            .cloned()
            .collect();
    }

    /// The versions held of `code`, newest last.
    fn versions(&self, code: &str) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .surfaces
            .keys()
            .map(String::as_str)
            .filter(|package| code_of(package) == code)
            .collect();
        out.sort_by_key(|package| {
            package
                .rsplit_once('.')
                .map(|(_, label)| {
                    label
                        .trim_start_matches('v')
                        .split('_')
                        .map(|part| part.parse::<u64>().unwrap_or(0))
                        .collect::<Vec<u64>>()
                })
                .unwrap_or_default()
        });
        out
    }

    /// What the app writes of each package: its declarations by package,
    /// the whole of a connection serving it through a capability included.
    fn written(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut written_connections = BTreeSet::new();
        for chain in &self.chains {
            let Some((first, rest)) = chain.split_first() else {
                continue;
            };
            if let Some(package) = self.connections.get(first) {
                if let Some(name) = self.longest(package, rest) {
                    out.entry(package.clone()).or_default().insert(name);
                    written_connections.insert(first.as_str());
                }
                continue;
            }
            let held = self.versions(first);
            if held.is_empty() {
                continue;
            }
            // `Postcodes.v3.uk.parse` while two versions are held; a
            // declaration that looks like a version is the declaration.
            let through = rest
                .split_first()
                .map(|(label, after)| (format!("{first}.{label}"), after));
            match through {
                Some((package, after)) if self.surfaces.contains_key(&package) => {
                    if let Some(name) = self.longest(&package, after) {
                        out.entry(package).or_default().insert(name);
                    }
                }
                _ => {
                    for package in held {
                        if let Some(name) = self.longest(package, rest) {
                            out.entry(package.to_string()).or_default().insert(name);
                        }
                    }
                }
            }
        }
        for (connection, package) in &self.connections {
            if self.named.contains(connection) || !written_connections.contains(connection.as_str())
            {
                let names =
                    self.surfaces.get(package).into_iter().flatten().filter_map(
                        |entry| match entry.heading.split_once(' ') {
                            Some(("operation" | "action", name)) => Some(name.to_string()),
                            _ => None,
                        },
                    );
                out.entry(package.clone()).or_default().extend(names);
            }
        }
        out
    }

    /// The longest start of `chain` that `package` declares.
    fn longest(&self, package: &str, chain: &[String]) -> Option<String> {
        let entries = self.surfaces.get(package)?;
        (1..=chain.len()).rev().find_map(|count| {
            let name = chain[..count].join(".");
            entry_of(entries, &name).map(|_| name)
        })
    }

    /// What the app uses of `package`: what it writes, and the types and
    /// failures those reach.
    pub fn used(&self, package: &str) -> BTreeSet<String> {
        let Some(entries) = self.surfaces.get(package) else {
            return BTreeSet::new();
        };
        let mut seen = BTreeSet::new();
        let mut waiting: Vec<String> = self
            .written()
            .remove(package)
            .unwrap_or_default()
            .into_iter()
            .collect();
        while let Some(name) = waiting.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(entry) = entry_of(entries, &name) {
                waiting.extend(reached(entries, entry));
            }
        }
        seen
    }

    /// The packages a connection reaches.
    fn connected(&self) -> BTreeSet<&str> {
        self.connections.values().map(String::as_str).collect()
    }
}

/// Every dotted name in `text`, its comments left out. A string is read
/// as well, for what it interpolates.
fn chains_in(text: &str, out: &mut BTreeSet<Vec<String>>) {
    let word =
        |text: &str| !text.is_empty() && text.chars().all(|c| c.is_alphanumeric() || c == '_');
    let mut chain: Vec<String> = Vec::new();
    let mut after_dot = false;
    let parse = jadpo_syntax::parse_file(text);
    for token in parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if token.kind().is_trivia() {
            continue;
        }
        let written = token.text();
        if written == "." && !chain.is_empty() {
            after_dot = true;
            continue;
        }
        if word(written) {
            if !after_dot && !chain.is_empty() {
                out.insert(std::mem::take(&mut chain));
            }
            chain.push(written.to_string());
            after_dot = false;
            continue;
        }
        if !chain.is_empty() {
            out.insert(std::mem::take(&mut chain));
        }
        after_dot = false;
        if token.kind() == SyntaxKind::STRING {
            chains_in_string(written, out);
        }
    }
    if !chain.is_empty() {
        out.insert(chain);
    }
}

/// The dotted names in a string's text.
fn chains_in_string(text: &str, out: &mut BTreeSet<Vec<String>>) {
    let piece = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    for part in text.split(|c: char| !piece(c)) {
        let chain: Vec<String> = part
            .split('.')
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect();
        if chain.len() > 1 {
            out.insert(chain);
        }
    }
}

/// The name a surface entry's heading gives: `uk.parse`, `UkPostcode`.
fn name_of(entry: &Entry) -> Option<&str> {
    let (word, rest) = entry.heading.split_once(' ')?;
    match word {
        "operation" | "action" | "function" | "message" | "failure" | "recipe" | "type" => {
            Some(rest.split(" = ").next().unwrap_or(rest))
        }
        _ => None,
    }
}

fn entry_of<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|entry| name_of(entry) == Some(name))
}

/// `operation`, `type`.
fn word_of(entry: &Entry) -> &str {
    entry.heading.split(' ').next().unwrap_or("")
}

/// A type's base: `Object`, `Enum`, `Text`.
fn base_of(entry: &Entry) -> Option<&str> {
    entry.heading.split_once(" = ").map(|(_, base)| base)
}

/// Whether the entry's `name: value` lines are fields, not a refined
/// type's rules.
fn holds_fields(entry: &Entry) -> bool {
    match word_of(entry) {
        "type" => matches!(base_of(entry), Some("Object" | "Input" | "Output")),
        "failure" | "message" => true,
        _ => false,
    }
}

/// `name: value`, when a line is one: not a description or a quoted
/// message that merely holds a colon.
fn field_line(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.split_once(": ")?;
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then_some((name, value))
}

/// An entry's `name: value` lines.
fn fields(entry: &Entry) -> BTreeMap<&str, &str> {
    entry
        .members
        .iter()
        .filter_map(|member| field_line(&member.line))
        .collect()
}

/// A callable's lines that name what it takes and answers: `input Form`.
const SHAPE: [&str; 3] = ["input ", "output ", "wire_output "];

/// The types and failures an entry names.
fn reached(entries: &[Entry], entry: &Entry) -> Vec<String> {
    let mut lines: Vec<&str> = Vec::new();
    for member in &entry.members {
        let line = member.line.as_str();
        let names_types = SHAPE.iter().any(|start| line.starts_with(start))
            || line.starts_with("signature ")
            || line.starts_with("fails ")
            || line.starts_with("after_effect ")
            || line.starts_with("variant ");
        if names_types {
            lines.push(line);
        } else if holds_fields(entry)
            && let Some((_, value)) = field_line(line)
        {
            lines.push(value);
        }
        lines.extend(member.nested.iter().map(String::as_str));
    }
    let mut out = Vec::new();
    for line in lines {
        for word in line.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if entries.iter().any(|entry| {
                matches!(word_of(entry), "type" | "failure") && name_of(entry) == Some(word)
            }) {
                out.push(word.to_string());
            }
        }
    }
    out
}

/// A callable's lines an app relies on beyond its failures and the types
/// they reach: what leaves its description, file and provenance out.
fn relied_on(entry: &Entry) -> Vec<&str> {
    const LOOSE: [&str; 6] = [
        "///",
        "file ",
        "source ",
        "confirmed ",
        "fails ",
        "after_effect ",
    ];
    entry
        .members
        .iter()
        .map(|member| member.line.as_str())
        .filter(|line| !LOOSE.iter().any(|loose| line.starts_with(loose)))
        .collect()
}

fn fails_of(entry: &Entry) -> BTreeSet<&str> {
    entry
        .members
        .iter()
        .filter_map(|member| {
            let line = member.line.as_str();
            line.strip_prefix("fails ")
                .or_else(|| line.strip_prefix("after_effect "))
        })
        .collect()
}

/// A failure's code, as the app and the review show it.
fn failure_code<'a>(entries: &'a [Entry], name: &'a str) -> &'a str {
    entry_of(entries, name)
        .and_then(|entry| {
            entry
                .members
                .iter()
                .find_map(|member| member.line.strip_prefix("code "))
        })
        .unwrap_or(name)
}

/// One comparison: `was` against `now`, of one package's code.
struct Pair<'a> {
    code: &'a str,
    package: &'a str,
    was: &'a [Entry],
    now: &'a [Entry],
}

impl Pair<'_> {
    fn finding(
        &self,
        subject: String,
        change: &str,
        heading: Option<&Entry>,
        through: &str,
        (what, detail): (&str, String),
        said: String,
    ) -> Finding {
        Finding {
            subject,
            change: format!("PackageChange.{change}"),
            package: self.package.to_string(),
            heading: heading.map(|entry| entry.heading.clone()),
            said,
            through: through.to_string(),
            what: what.to_string(),
            // `Booking  field removed  driver_phone`: once is enough.
            detail: detail
                .strip_prefix(&format!("{through}."))
                .map_or_else(|| detail.clone(), str::to_string),
        }
    }

    /// A declaration the app writes, and what it reaches.
    fn declaration(&self, name: &str, out: &mut Vec<Finding>, seen: &mut BTreeSet<String>) {
        let code = self.code;
        let Some(was) = entry_of(self.was, name) else {
            return;
        };
        let Some(now) = entry_of(self.now, name) else {
            let (change, what) = if matches!(word_of(was), "operation" | "action" | "function") {
                ("operation_removed", "removed")
            } else {
                ("declaration_removed", "removed")
            };
            out.push(self.finding(
                format!("{code}.{name}"),
                change,
                None,
                name,
                (what, String::new()),
                format!("the app uses `{code}.{name}`, and the package no longer has it"),
            ));
            return;
        };
        match word_of(was) {
            "operation" | "action" | "function" => self.callable(name, was, now, out),
            _ => {}
        }
        self.walk(name, was, out, seen);
    }

    fn callable(&self, name: &str, was: &Entry, now: &Entry, out: &mut Vec<Finding>) {
        let code = self.code;
        let old_fails = fails_of(was);
        for failure in fails_of(now) {
            if old_fails.contains(failure) {
                continue;
            }
            let shown = failure_code(self.now, failure);
            out.push(self.finding(
                format!("{code}.{name}.{shown}"),
                "failure_added",
                Some(now),
                name,
                ("new failure", shown.to_string()),
                format!("`{code}.{name}` can now fail with `{shown}`"),
            ));
        }
        let (old, new) = (relied_on(was), relied_on(now));
        if old != new {
            let gone: Vec<&str> = old.iter().copied().filter(|l| !new.contains(l)).collect();
            let came: Vec<&str> = new.iter().copied().filter(|l| !old.contains(l)).collect();
            let detail = match (gone.first(), came.first()) {
                (Some(gone), Some(came)) => format!("{gone} → {came}"),
                (Some(gone), None) => format!("no longer {gone}"),
                (None, Some(came)) => came.to_string(),
                (None, None) => String::new(),
            };
            out.push(self.finding(
                format!("{code}.{name}"),
                "operation_changed",
                Some(now),
                name,
                ("changed", detail.clone()),
                format!("`{code}.{name}` of the package changed: {detail}"),
            ));
        }
    }

    /// The types and failures `entry` reaches, each compared once.
    fn walk(
        &self,
        through: &str,
        entry: &Entry,
        out: &mut Vec<Finding>,
        seen: &mut BTreeSet<String>,
    ) {
        if matches!(word_of(entry), "type" | "failure" | "message")
            && let Some(name) = name_of(entry)
            && seen.insert(name.to_string())
        {
            self.shape(through, name, entry, out);
        }
        for reached_name in reached(self.was, entry) {
            if seen.contains(&reached_name) {
                continue;
            }
            if let Some(next) = entry_of(self.was, &reached_name) {
                self.walk(through, next, out, seen);
            }
        }
    }

    /// One type, failure or message: its fields, rules and variants.
    fn shape(&self, through: &str, name: &str, was: &Entry, out: &mut Vec<Finding>) {
        let code = self.code;
        // Gone altogether: what named it shows as changed.
        let Some(now) = entry_of(self.now, name) else {
            return;
        };
        let removed = |field: &str, what_of: String| {
            self.finding(
                format!("{code}.{name}.{field}"),
                "field_removed",
                Some(now),
                through,
                ("field removed", format!("{name}.{field}")),
                what_of,
            )
        };
        if holds_fields(was) && holds_fields(now) {
            let (old, new) = (fields(was), fields(now));
            for (field, ty) in &old {
                match new.get(field) {
                    None => out.push(removed(
                        field,
                        format!(
                            "the app uses `{code}.{name}`, whose field `{field}` is gone"
                        ),
                    )),
                    Some(now_ty) if now_ty != ty => out.push(self.finding(format!("{code}.{name}.{field}"), "field_retyped", Some(now), through, ("field retyped", format!("{name}.{field}  {ty} → {now_ty}")), format!(
                            "`{code}.{name}.{field}`, which the app uses, was `{ty}` and is now `{now_ty}`"
                        ))),
                    Some(_) => {}
                }
            }
        }
        if word_of(was) == "type" && word_of(now) == "type" {
            self.type_itself(through, name, was, now, out);
            let variants = |entry: &'_ Entry| -> BTreeMap<String, Vec<String>> {
                entry
                    .members
                    .iter()
                    .filter_map(|member| {
                        Some((
                            member.line.strip_prefix("variant ")?.to_string(),
                            member.nested.clone(),
                        ))
                    })
                    .collect()
            };
            let (old, new) = (variants(was), variants(now));
            for (variant, fields) in &old {
                match new.get(variant) {
                    None => out.push(removed(
                        variant,
                        format!(
                            "the app uses `{code}.{name}`, whose variant `{variant}` is gone"
                        ),
                    )),
                    Some(now_fields) if now_fields != fields => out.push(self.finding(format!("{code}.{name}.{variant}"), "field_retyped", Some(now), through, ("field retyped", format!("{name}.{variant}")), format!(
                            "the app uses `{code}.{name}`, whose variant `{variant}` holds other fields"
                        ))),
                    Some(_) => {}
                }
            }
        }
        if word_of(was) == "failure" && word_of(now) == "failure" {
            let line = |entry: &'_ Entry, start: &str| -> Option<String> {
                entry
                    .members
                    .iter()
                    .find_map(|member| member.line.strip_prefix(start).map(str::to_string))
            };
            for start in ["code ", "kind "] {
                if line(was, start) != line(now, start) {
                    let (from, to) = (
                        line(was, start).unwrap_or_default(),
                        line(now, start).unwrap_or_default(),
                    );
                    out.push(self.finding(format!("{code}.{name}"), "field_retyped", Some(now), through, ("field retyped", format!("{name}  {from} → {to}")), format!("failure `{code}.{name}`, which the app uses, was `{from}` and is now `{to}`")));
                    break;
                }
            }
        }
    }

    /// A type's base or a refined type's rules.
    fn type_itself(
        &self,
        through: &str,
        name: &str,
        was: &Entry,
        now: &Entry,
        out: &mut Vec<Finding>,
    ) {
        let code = self.code;
        let rules = |entry: &Entry| -> Vec<String> {
            if holds_fields(entry) || base_of(entry) == Some("Enum") {
                return Vec::new();
            }
            let mut lines: Vec<String> = fields(entry)
                .iter()
                .map(|(rule, value)| format!("{rule}: {value}"))
                .collect();
            lines.sort();
            lines
        };
        let shown = |entry: &Entry| -> String {
            let base = base_of(entry).unwrap_or("");
            let rules = rules(entry);
            if rules.is_empty() {
                base.to_string()
            } else {
                format!("{base} {{ {} }}", rules.join(", "))
            }
        };
        if shown(was) != shown(now) {
            let (from, to) = (shown(was), shown(now));
            out.push(self.finding(
                format!("{code}.{name}"),
                "field_retyped",
                Some(now),
                through,
                ("field retyped", format!("{name}  {from} → {to}")),
                format!("`{code}.{name}`, which the app uses, was `{from}` and is now `{to}`"),
            ));
        }
    }

    /// A provider's hosts and credential scheme.
    fn provider(&self, out: &mut Vec<Finding>) {
        let code = self.code;
        let provider = |entries: &'_ [Entry]| -> Option<Entry> {
            entries
                .iter()
                .find(|entry| entry.heading == "provider")
                .cloned()
        };
        let (Some(was), Some(now)) = (provider(self.was), provider(self.now)) else {
            return;
        };
        let lines = |entry: &Entry, start: &str| -> Vec<String> {
            entry
                .members
                .iter()
                .filter(|member| member.line.starts_with(start))
                .map(|member| member.line.clone())
                .collect()
        };
        let hosts = |entry: &Entry| -> BTreeSet<String> {
            lines(entry, "host ")
                .iter()
                .filter_map(|line| line.rsplit(' ').next().map(str::to_string))
                .collect()
        };
        let (old, new) = (hosts(&was), hosts(&now));
        if old != new {
            let list = |set: Vec<&String>| {
                set.iter()
                    .map(|host| host.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let gone = list(old.difference(&new).collect());
            let came = list(new.difference(&old).collect());
            let (detail, said) = match (gone.is_empty(), came.is_empty()) {
                (false, false) => (
                    format!("{gone} → {came}"),
                    format!("`{code}` now reaches `{came}` instead of `{gone}`"),
                ),
                (true, _) => (
                    format!("now also {came}"),
                    format!("`{code}` now also reaches `{came}`"),
                ),
                (_, true) => (
                    format!("no longer {gone}"),
                    format!("`{code}` no longer reaches `{gone}`"),
                ),
            };
            out.push(self.finding(
                format!("{code}.host"),
                "host_changed",
                Some(&now),
                "host",
                ("changed", detail),
                said,
            ));
        }
        // `credential header` then, for that scheme, `credential header x-key`.
        let scheme = |entry: &Entry| -> String {
            lines(entry, "credential ")
                .last()
                .map(|line| line.trim_start_matches("credential ").to_string())
                .unwrap_or_default()
        };
        let (from, to) = (scheme(&was), scheme(&now));
        if from != to {
            out.push(self.finding(
                format!("{code}.credential"),
                "credential_changed",
                Some(&now),
                "credential",
                ("changed", format!("{from} → {to}")),
                format!("`{code}` now takes its credential as `{to}` rather than `{from}`"),
            ));
        }
    }
}

/// Every change that needs a person, sorted, between `before` and `after`.
pub fn review(before: &Side, after: &Side) -> Vec<Finding> {
    let mut out = Vec::new();
    let written_before = before.written();
    let written_after = after.written();
    let codes: BTreeSet<&str> = before.surfaces.keys().map(|p| code_of(p)).collect();
    for code in codes {
        let holds = after.versions(code);
        // Removed altogether: that is `jadpo package remove`, not an upgrade.
        let Some(newest) = holds.last().copied() else {
            continue;
        };
        for (package_was, was) in before
            .surfaces
            .iter()
            .filter(|(package, _)| code_of(package) == code)
        {
            // What each version has been compared with, once each.
            let mut seen: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
            let staying = holds.contains(&package_was.as_str());
            for name in written_before.get(package_was).into_iter().flatten() {
                // Where the app now writes it, else the same version while
                // it is held, else the newest.
                let moved: Vec<&str> = holds
                    .iter()
                    .copied()
                    .filter(|held| {
                        written_after
                            .get(*held)
                            .is_some_and(|names| names.contains(name))
                    })
                    .collect();
                let targets = if !moved.is_empty() {
                    moved
                } else if staying {
                    vec![package_was.as_str()]
                } else {
                    vec![newest]
                };
                for package in targets {
                    if let Some(now) = after.surfaces.get(package) {
                        let pair = Pair {
                            code,
                            package,
                            was,
                            now,
                        };
                        pair.declaration(name, &mut out, seen.entry(package).or_default());
                    }
                }
            }
            if before.connected().contains(package_was.as_str()) {
                let reached = after.connected();
                let mut targets: Vec<&str> = holds
                    .iter()
                    .copied()
                    .filter(|held| reached.contains(held))
                    .collect();
                if targets.is_empty() {
                    targets.push(if staying {
                        package_was.as_str()
                    } else {
                        newest
                    });
                }
                for package in targets {
                    if let Some(now) = after.surfaces.get(package) {
                        let pair = Pair {
                            code,
                            package,
                            was,
                            now,
                        };
                        pair.provider(&mut out);
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.subject == b.subject && a.change == b.change);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(package: &str, kind: &str, body: &str) -> String {
        format!(
            "{}package {package}\n    file package.jadpo\n    kind {kind}\n    version 1.0.0\n\n{body}",
            format::header(&format::SURFACE, format::SURFACE_FROM),
        )
    }

    const OLD: &str = "provider\n    file provider.jadpo\n    protocol json_http\n    host live https://api.acme.example\n    credential bearer\n\noperation bookings.get\n    file resources/bookings.jadpo\n    request GET /bookings\n    input Query\n    output Booking\n\noperation bookings.create\n    file resources/bookings.jadpo\n    request POST /bookings\n    input Query\n    output Booking\n    changes\n\noperation bookings.cancel\n    file resources/bookings.jadpo\n    request POST /cancel\n    output Booking\n\ntype Query = Object\n    file types/query.jadpo\n    id: Text\n\ntype Booking = Object\n    file types/booking.jadpo\n    driver_phone: Text\n    weight: Int\n    status: Status\n\ntype Status = Enum\n    file types/status.jadpo\n    variant open\n    variant closed\n\ntype Unused = Object\n    file types/unused.jadpo\n    gone: Text\n\nfailure AddressUnverifiable\n    file failures/address.jadpo\n    kind invalid_value\n    code address_unverifiable\n";

    fn side<'a>(files: &[(&'a str, &'a str)]) -> Side {
        Side::read(files.iter().copied())
    }

    const APP_V1: &str = "application {\n    connections: {\n        couriers: {\n            package: AcmeCouriers.v1\n        }\n    }\n}\n";
    const APP_V2: &str = "application {\n    connections: {\n        couriers_v1: {\n            package: AcmeCouriers.v1\n        }\n        couriers: {\n            package: AcmeCouriers.v2\n        }\n    }\n}\n";
    const CALLS: &str = "/// Calls.\naction book() {\n    // couriers.bookings.cancel is not called.\n    couriers.bookings.get(q)\n    couriers.bookings.create(q)\n}\n";

    fn subjects(findings: &[Finding]) -> Vec<(String, String)> {
        findings
            .iter()
            .map(|finding| (finding.subject.clone(), finding.change.clone()))
            .collect()
    }

    fn pairs(entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(subject, change)| (subject.to_string(), format!("PackageChange.{change}")))
            .collect()
    }

    fn upgrade(now: &str) -> Vec<Finding> {
        let old = surface("AcmeCouriers.v1", "provider", OLD);
        let new = surface("AcmeCouriers.v1", "provider", now);
        let at = "packages/acme_couriers/v1/surface.jadpo_record";
        let before = side(&[
            ("app.jadpo", APP_V1),
            ("domains/a/b.jadpo", CALLS),
            (at, &old),
        ]);
        let after = side(&[
            ("app.jadpo", APP_V1),
            ("domains/a/b.jadpo", CALLS),
            (at, &new),
        ]);
        review(&before, &after)
    }

    #[test]
    fn nothing_changed_needs_nobody() {
        assert_eq!(upgrade(OLD), Vec::new());
    }

    #[test]
    fn a_new_failure_on_a_used_operation_needs_a_person() {
        let now = OLD.replace(
            "request POST /bookings\n    input Query\n    output Booking\n    changes\n",
            "request POST /bookings\n    input Query\n    output Booking\n    changes\n    fails AddressUnverifiable\n",
        );
        let found = upgrade(&now);
        assert_eq!(
            subjects(&found),
            pairs(&[(
                "AcmeCouriers.bookings.create.address_unverifiable",
                "failure_added"
            )])
        );
        assert_eq!(
            found[0].said,
            "`AcmeCouriers.bookings.create` can now fail with `address_unverifiable`"
        );
        assert_eq!(found[0].through, "bookings.create");
        assert_eq!(found[0].detail, "address_unverifiable");
        assert_eq!(found[0].mark(), '+');
    }

    #[test]
    fn a_used_field_removed_or_retyped_needs_a_person_and_a_field_nothing_reaches_doesnt() {
        let now = OLD
            .replace(
                "    driver_phone: Text\n    weight: Int\n",
                "    weight: Text\n",
            )
            .replace("    gone: Text\n", "");
        assert_eq!(
            subjects(&upgrade(&now)),
            pairs(&[
                ("AcmeCouriers.Booking.driver_phone", "field_removed"),
                ("AcmeCouriers.Booking.weight", "field_retyped"),
            ])
        );
    }

    #[test]
    fn a_new_optional_field_or_an_uncalled_operation_needs_nobody() {
        let now = OLD
            .replace("    weight: Int\n", "    weight: Int\n    note: Text?\n")
            .replace("    request POST /cancel\n", "    request POST /v2/cancel\n")
            .replace("operation bookings.cancel\n", "operation bookings.list\n    file resources/bookings.jadpo\n    request GET /all\n\noperation bookings.cancel\n");
        assert_eq!(upgrade(&now), Vec::new());
    }

    #[test]
    fn a_comment_is_not_a_use() {
        let now = OLD.replace(
            "operation bookings.cancel\n    file resources/bookings.jadpo\n    request POST /cancel\n    output Booking\n\n",
            "",
        );
        assert_eq!(upgrade(&now), Vec::new());
    }

    #[test]
    fn a_used_operation_removed_needs_a_person_even_with_no_call_left() {
        let now = OLD.replace(
            "operation bookings.get\n    file resources/bookings.jadpo\n    request GET /bookings\n    input Query\n    output Booking\n\n",
            "",
        );
        let found = upgrade(&now);
        assert_eq!(
            subjects(&found),
            pairs(&[("AcmeCouriers.bookings.get", "operation_removed")])
        );
    }

    #[test]
    fn a_variant_gone_needs_a_person_and_one_added_does_not() {
        let gone = OLD.replace("    variant closed\n", "");
        assert_eq!(
            subjects(&upgrade(&gone)),
            pairs(&[("AcmeCouriers.Status.closed", "field_removed")])
        );
        let added = OLD.replace(
            "    variant closed\n",
            "    variant closed\n    variant held\n",
        );
        assert_eq!(upgrade(&added), Vec::new());
    }

    #[test]
    fn a_host_or_a_credential_scheme_changed_needs_a_person() {
        let now = OLD
            .replace("https://api.acme.example", "https://api.eu.acme.example")
            .replace(
                "credential bearer",
                "credential header\n    credential header x-key",
            );
        let found = upgrade(&now);
        assert_eq!(
            subjects(&found),
            pairs(&[
                ("AcmeCouriers.credential", "credential_changed"),
                ("AcmeCouriers.host", "host_changed"),
            ])
        );
        assert_eq!(
            found[1].said,
            "`AcmeCouriers` now reaches `https://api.eu.acme.example` instead of `https://api.acme.example`"
        );
    }

    #[test]
    fn the_version_is_ignored_when_a_call_moves_to_the_next_connection() {
        let old = surface("AcmeCouriers.v1", "provider", OLD);
        let next = surface(
            "AcmeCouriers.v2",
            "provider",
            &OLD.replace("    driver_phone: Text\n", ""),
        );
        let (v1, v2) = (
            "packages/acme_couriers/v1/surface.jadpo_record",
            "packages/acme_couriers/v2/surface.jadpo_record",
        );
        let before = side(&[
            ("app.jadpo", APP_V1),
            ("domains/a/b.jadpo", CALLS),
            (v1, &old),
        ]);
        // Both held, the calls now through the new version's connection.
        let after = side(&[
            ("app.jadpo", APP_V2),
            ("domains/a/b.jadpo", CALLS),
            (v1, &old),
            (v2, &next),
        ]);
        let found = review(&before, &after);
        assert_eq!(
            subjects(&found),
            pairs(&[("AcmeCouriers.Booking.driver_phone", "field_removed")])
        );
        assert_eq!(found[0].package, "AcmeCouriers.v2");
        // And with the old version gone from the lock.
        let after = side(&[
            ("app.jadpo", &APP_V1.replace("v1", "v2")),
            ("domains/a/b.jadpo", CALLS),
            (v2, &next),
        ]);
        assert_eq!(
            subjects(&review(&before, &after)),
            pairs(&[("AcmeCouriers.Booking.driver_phone", "field_removed")])
        );
    }

    #[test]
    fn a_connection_the_code_never_writes_uses_every_operation() {
        let old = surface(
            "Resend.v1",
            "provider",
            "operation emails.send\n    file r.jadpo\n    request POST /emails\n",
        );
        let new = surface(
            "Resend.v1",
            "provider",
            "operation emails.send\n    file r.jadpo\n    request POST /v2/emails\n",
        );
        let app = "application {\n    connections: {\n        mail: {\n            package: Resend.v1\n        }\n    }\n    email: {\n        connection: mail\n    }\n}\n";
        let at = "packages/resend/v1/surface.jadpo_record";
        let found = review(
            &side(&[("app.jadpo", app), (at, &old)]),
            &side(&[("app.jadpo", app), (at, &new)]),
        );
        assert_eq!(
            subjects(&found),
            pairs(&[("Resend.emails.send", "operation_changed")])
        );
    }

    #[test]
    fn a_package_removed_altogether_is_not_an_upgrade() {
        let old = surface("AcmeCouriers.v1", "provider", OLD);
        let at = "packages/acme_couriers/v1/surface.jadpo_record";
        let before = side(&[
            ("app.jadpo", APP_V1),
            ("domains/a/b.jadpo", CALLS),
            (at, &old),
        ]);
        assert_eq!(
            review(&before, &side(&[("app.jadpo", "application {}\n")])),
            Vec::new()
        );
    }

    #[test]
    fn a_library_is_read_through_its_name_and_a_tests_use_is_not_counted() {
        let body = "function uk.parse\n    file l.jadpo\n    signature (a: Text) -> UkPostcode\n\nfunction uk.tidy\n    file l.jadpo\n    signature (a: Text) -> Text\n\ntype UkPostcode = Text\n    file t.jadpo\n    max_length: 8\n";
        let old = surface("Postcodes.v2", "library", body);
        let new = surface(
            "Postcodes.v2",
            "library",
            &body
                .replace("max_length: 8", "max_length: 6")
                .replace("signature (a: Text) -> Text", "signature (b: Int) -> Int"),
        );
        let code = "action a() {\n    return Postcodes.uk.parse(x)\n}\n";
        let test = "test \"t\" {\n    Postcodes.uk.tidy(x)\n}\n";
        let at = "packages/postcodes/v2/surface.jadpo_record";
        let files = |surface: &str| -> Side {
            Side::read([
                ("domains/a/a.jadpo", code),
                ("domains/a/a.test.jadpo", test),
                (at, surface),
            ])
        };
        let found = review(&files(&old), &files(&new));
        assert_eq!(
            subjects(&found),
            pairs(&[("Postcodes.UkPostcode", "field_retyped")])
        );
        assert_eq!(found[0].through, "uk.parse");
        assert_eq!(
            found[0].said,
            "`Postcodes.UkPostcode`, which the app uses, was `Text { max_length: 8 }` and is now `Text { max_length: 6 }`"
        );
    }
}
