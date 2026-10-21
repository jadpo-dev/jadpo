//! The route trie (chunk 8.1k; docs/compiler-runtime.md §2.5, "The route
//! trie"): every path the HTTP component answers, built once from the
//! route table and the program's webhooks as its plan is loaded, and
//! walked without allocating anything for each request.
//!
//! A node's children are its fixed segments, in byte order, and one
//! placeholder. A request's path is walked fixed child first, so of the
//! routes for its method the one with fixed text earliest wins, as it
//! always has; every route the path matches gives its method to `Allow`.
//! Health's two paths and each webhook's are entries of the trie too, on
//! paths of fixed segments only, and answered whatever the method, ahead
//! of any route a placeholder would match there.
//!
//! A segment is compared percent-decoded, byte by byte, without decoding
//! it into a new string; a path with a segment whose escapes are malformed
//! or don't decode to UTF-8 is [`Found::Unreadable`]. Each route's label
//! (`POST /todos`) is made once, and a response shares it.

use std::sync::Arc;

use jadpo_std::HttpMethod;
use jadpo_std::http::{HEALTH_LIVE, HEALTH_READY, RouteTable, Segment};
use jadpo_std::provider::WebhookPlan;

/// What a request's path and method find in the trie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// A segment's escapes are malformed or don't decode to UTF-8 text.
    Unreadable,
    /// `/health/live`, or `/health/ready` when `ready`.
    Health { ready: bool },
    /// The webhook at this place in the program's webhooks.
    Webhook(u32),
    /// No route has this path.
    NotFound,
    /// Routes have this path: `route` the one for the method, by its
    /// number in the route table, if one answers it, and `allow` every
    /// method they answer between them.
    Route { route: Option<u32>, allow: Methods },
}

/// A set of the methods routes are written with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Methods(u8);

impl Methods {
    fn bit(method: HttpMethod) -> u8 {
        1 << (method as u8)
    }

    pub fn contains(self, method: HttpMethod) -> bool {
        self.0 & Methods::bit(method) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The `Allow` header's value: these methods, `HEAD` with `GET`, and
    /// `OPTIONS`, in alphabetical order (`GET, HEAD, OPTIONS, POST`).
    pub fn allow(self) -> String {
        let get = self.contains(HttpMethod::Get);
        [
            ("DELETE", self.contains(HttpMethod::Delete)),
            ("GET", get),
            ("HEAD", get),
            ("OPTIONS", true),
            ("PATCH", self.contains(HttpMethod::Patch)),
            ("POST", self.contains(HttpMethod::Post)),
            ("PUT", self.contains(HttpMethod::Put)),
        ]
        .into_iter()
        .filter(|(_, allowed)| *allowed)
        .map(|(word, _)| word)
        .collect::<Vec<_>>()
        .join(", ")
    }
}

/// What a path of fixed segments only is, answered before any route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Own {
    Health { ready: bool },
    Webhook(u32),
}

#[derive(Debug, Default)]
struct Node {
    /// Each fixed segment's child, by the segment's text, in byte order.
    fixed: Vec<(Box<str>, u32)>,
    /// The placeholder's child.
    param: Option<u32>,
    /// What a path ending here is, when the runtime answers it itself.
    own: Option<Own>,
    /// The route for each method a path ending here answers, by number.
    routes: [Option<u32>; 5],
    methods: Methods,
}

/// Every path a program answers: see the module.
#[derive(Debug)]
pub struct Routes {
    /// The root first.
    nodes: Vec<Node>,
    /// Each route's label, by its number.
    labels: Vec<Arc<str>>,
    /// Each webhook's label, by its place.
    webhooks: Vec<Arc<str>>,
    /// `GET /health/live` and `GET /health/ready`.
    health: [Arc<str>; 2],
}

impl Default for Routes {
    fn default() -> Routes {
        Routes::new(&RouteTable::default(), &[])
    }
}

impl Routes {
    /// The trie of `table`'s routes, health's paths and `webhooks`'.
    pub fn new(table: &RouteTable, webhooks: &[WebhookPlan]) -> Routes {
        let mut routes = Routes {
            nodes: vec![Node::default()],
            labels: table
                .routes
                .iter()
                .map(|route| Arc::from(route.label()))
                .collect(),
            webhooks: webhooks
                .iter()
                .map(|plan| {
                    let path: String = plan
                        .segments
                        .iter()
                        .map(|text| format!("/{text}"))
                        .collect();
                    Arc::from(format!("POST {path}"))
                })
                .collect(),
            health: [
                Arc::from("GET /health/live"),
                Arc::from("GET /health/ready"),
            ],
        };
        // Health first, then each webhook in order: the first to take a
        // path keeps it, as the first found always answered.
        for (path, ready) in [(&HEALTH_LIVE[..], false), (&HEALTH_READY[..], true)] {
            let at = routes.fixed_path(path.iter().copied());
            if let Some(node) = routes.nodes.get_mut(at) {
                node.own.get_or_insert(Own::Health { ready });
            }
        }
        for (number, plan) in (0u32..).zip(webhooks) {
            let at = routes.fixed_path(plan.segments.iter().map(String::as_str));
            if let Some(node) = routes.nodes.get_mut(at) {
                node.own.get_or_insert(Own::Webhook(number));
            }
        }
        for (number, route) in (0u32..).zip(&table.routes) {
            let mut at = 0;
            for segment in &route.segments {
                at = match segment {
                    Segment::Fixed(text) => routes.fixed_child(at, text),
                    Segment::Param { .. } => routes.param_child(at),
                };
            }
            // Two routes alike for a method: the later answers, as it did
            // when the route table was searched in order.
            if let Some(node) = routes.nodes.get_mut(at)
                && let Some(slot) = node.routes.get_mut(route.method as usize)
            {
                *slot = Some(number);
                node.methods.0 |= Methods::bit(route.method);
            }
        }
        routes
    }

    fn fixed_path<'t>(&mut self, path: impl Iterator<Item = &'t str>) -> usize {
        path.fold(0, |at, text| self.fixed_child(at, text))
    }

    fn fixed_child(&mut self, at: usize, text: &str) -> usize {
        let next = self.nodes.len();
        let Some(node) = self.nodes.get_mut(at) else {
            return at;
        };
        match node
            .fixed
            .binary_search_by(|(fixed, _)| fixed.as_ref().cmp(text))
        {
            Ok(found) => node
                .fixed
                .get(found)
                .map_or(at, |(_, child)| *child as usize),
            Err(place) => {
                node.fixed.insert(place, (Box::from(text), next as u32));
                self.nodes.push(Node::default());
                next
            }
        }
    }

    fn param_child(&mut self, at: usize) -> usize {
        let next = self.nodes.len();
        let Some(node) = self.nodes.get_mut(at) else {
            return at;
        };
        if let Some(child) = node.param {
            return child as usize;
        }
        node.param = Some(next as u32);
        self.nodes.push(Node::default());
        next
    }

    /// What `method` at `path` (as sent: percent-encoded, without its
    /// query string) finds. `HEAD` finds what `GET` does.
    pub fn find(&self, method: &str, path: &str) -> Found {
        if !segments(path).all(readable) {
            return Found::Unreadable;
        }
        let method = match method {
            "HEAD" => Some(HttpMethod::Get),
            other => HttpMethod::from_word(other),
        };
        let mut walk = Walk {
            method,
            own: None,
            route: None,
            allow: Methods::default(),
        };
        self.walk(0, segments(path), true, &mut walk);
        match walk.own {
            Some(Own::Health { ready }) => Found::Health { ready },
            Some(Own::Webhook(at)) => Found::Webhook(at),
            None if walk.allow.is_empty() => Found::NotFound,
            None => Found::Route {
                route: walk.route,
                allow: walk.allow,
            },
        }
    }

    /// Visits every node `rest` reaches from node `at`, fixed children
    /// before the placeholder's, so the first route found for the method
    /// has fixed text earliest. `fixed` is whether every segment so far
    /// matched fixed text.
    fn walk(&self, at: usize, mut rest: Segments<'_>, fixed: bool, walk: &mut Walk) {
        let Some(node) = self.nodes.get(at) else {
            return;
        };
        let Some(segment) = rest.next() else {
            // A path of fixed segments only is visited first, so what the
            // runtime answers there is found before any route.
            if fixed && node.own.is_some() {
                walk.own = node.own;
                return;
            }
            walk.allow.0 |= node.methods.0;
            if walk.route.is_none()
                && let Some(method) = walk.method
            {
                walk.route = node.routes.get(method as usize).copied().flatten();
            }
            return;
        };
        if let Some(child) = self.fixed_of(node, segment) {
            self.walk(child, rest.clone(), fixed, walk);
            if walk.own.is_some() {
                return;
            }
        }
        if let Some(child) = node.param {
            self.walk(child as usize, rest, false, walk);
        }
    }

    /// The child of `node` whose fixed text is `raw` percent-decoded.
    fn fixed_of(&self, node: &Node, raw: &str) -> Option<usize> {
        let found = if raw.contains('%') {
            node.fixed
                .binary_search_by(|(fixed, _)| fixed.bytes().cmp(Decoded(raw.as_bytes())))
        } else {
            node.fixed
                .binary_search_by(|(fixed, _)| fixed.as_ref().cmp(raw))
        };
        found
            .ok()
            .and_then(|place| node.fixed.get(place))
            .map(|(_, child)| *child as usize)
    }

    /// Route number `route`'s label, `POST /todos`.
    pub fn label(&self, route: u32) -> Option<Arc<str>> {
        self.labels.get(route as usize).cloned()
    }

    /// Webhook `at`'s label, `POST /webhooks/payments`.
    pub fn webhook_label(&self, at: u32) -> Option<Arc<str>> {
        self.webhooks.get(at as usize).cloned()
    }

    /// `GET /health/ready` when `ready`, else `GET /health/live`.
    pub fn health_label(&self, ready: bool) -> Arc<str> {
        let [live, ready_label] = &self.health;
        Arc::clone(if ready { ready_label } else { live })
    }
}

struct Walk {
    method: Option<HttpMethod>,
    own: Option<Own>,
    route: Option<u32>,
    allow: Methods,
}

/// A path's segments, as sent: empty ones left out.
pub(crate) type Segments<'p> = std::iter::Filter<std::str::Split<'p, char>, fn(&&'p str) -> bool>;

/// `path`'s segments, as sent, empty ones left out.
pub(crate) fn segments(path: &str) -> Segments<'_> {
    fn written(segment: &&str) -> bool {
        !segment.is_empty()
    }
    path.split('/').filter(written as fn(&&str) -> bool)
}

/// The byte an escape at the start of `bytes` stands for, as
/// [`super::percent_decode`] reads one: `None` when it is malformed.
fn escape(bytes: &[u8]) -> Option<u8> {
    let hex = std::str::from_utf8(bytes.get(1..3)?).ok()?;
    u8::from_str_radix(hex, 16).ok()
}

/// A segment's bytes, percent-decoded as they are read; a malformed escape
/// reads as its `%`, which [`readable`] has refused first.
struct Decoded<'s>(&'s [u8]);

impl Iterator for Decoded<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        let (&first, rest) = self.0.split_first()?;
        if first == b'%'
            && let Some(byte) = escape(self.0)
        {
            self.0 = self.0.get(3..).unwrap_or_default();
            return Some(byte);
        }
        self.0 = rest;
        Some(first)
    }
}

/// Whether segment `raw`'s escapes are well formed and decode to UTF-8,
/// checked a few bytes at a time without decoding it into a string.
pub(crate) fn readable(raw: &str) -> bool {
    if !raw.contains('%') {
        return true;
    }
    let bytes = raw.as_bytes();
    let mut held = [0u8; 64];
    let mut len = 0;
    let mut at = 0;
    while let Some(&first) = bytes.get(at) {
        let byte = if first == b'%' {
            let Some(byte) = bytes.get(at..).and_then(escape) else {
                return false;
            };
            at += 3;
            byte
        } else {
            at += 1;
            first
        };
        if let Some(slot) = held.get_mut(len) {
            *slot = byte;
        }
        len += 1;
        if len == held.len() {
            match std::str::from_utf8(&held) {
                Ok(_) => len = 0,
                // A character cut off at the end: kept for the next bytes.
                Err(error) if error.error_len().is_none() => {
                    let valid = error.valid_up_to();
                    held.copy_within(valid.., 0);
                    len -= valid;
                }
                Err(_) => return false,
            }
        }
    }
    held.get(..len)
        .is_some_and(|rest| std::str::from_utf8(rest).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::http::{Authentication, CodecId, RoutePlan, Run};
    use jadpo_std::provider::SignatureScheme;

    fn route(method: HttpMethod, path: &str) -> RoutePlan {
        let segments: Vec<Segment> = path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| match segment.strip_prefix('{') {
                Some(name) => Segment::Param {
                    name: name.trim_end_matches('}').to_string(),
                    codec: CodecId(0),
                },
                None => Segment::Fixed(segment.to_string()),
            })
            .collect();
        RoutePlan {
            key: format!("route {} {path}", method.word()),
            method,
            segments,
            api: 0,
            input: None,
            output: None,
            status: jadpo_std::SuccessStatus::Ok,
            deadline_ms: 10_000,
            run: Run::Action(0),
            settle: None,
            stream: false,
            keys: None,
            args: Vec::new(),
            authentication: Authentication::Public,
            caller: None,
            file: "routes.jadpo".into(),
            line: 1,
        }
    }

    fn webhook(path: &[&str]) -> WebhookPlan {
        WebhookPlan {
            connection: path.last().unwrap_or(&"").to_string(),
            segments: path.iter().map(|text| text.to_string()).collect(),
            scheme: SignatureScheme::Stripe,
            handshake: None,
            secret: 0,
            event_id: "id".into(),
            event_type: "type".into(),
            occurred_at: None,
            store: "main".into(),
            events: Vec::new(),
        }
    }

    fn table() -> RouteTable {
        use HttpMethod::*;
        RouteTable {
            routes: vec![
                route(Get, "/todos"),
                route(Post, "/todos"),
                route(Get, "/todos/{todo_id}"),
                route(Delete, "/todos/{todo_id}"),
                route(Get, "/todos/archived"),
                route(Get, "/{a}/b"),
                route(Get, "/a/{b}"),
                route(Put, "/a/b"),
                route(Get, "/"),
                route(Get, "/webhooks/{name}"),
                route(Patch, "/x/{y}/z"),
                route(Get, "/x/y/{z}"),
            ],
            ..RouteTable::default()
        }
    }

    /// How the route table was searched before the trie (chunks 4.1 to
    /// 8.1a): every route whose path matches, the method's with fixed
    /// text earliest answering.
    fn searched(table: &RouteTable, method: &str, path: &str) -> Found {
        let Some(written) = super::super::split_path(path) else {
            return Found::Unreadable;
        };
        let matches = |route: &RoutePlan| {
            route.segments.len() == written.len()
                && route
                    .segments
                    .iter()
                    .zip(&written)
                    .all(|(segment, written)| match segment {
                        Segment::Fixed(text) => text == written,
                        Segment::Param { .. } => !written.is_empty(),
                    })
        };
        let mask = |route: &RoutePlan| {
            route.segments.iter().take(64).fold(0u64, |mask, segment| {
                (mask << 1) | u64::from(matches!(segment, Segment::Fixed(_)))
            })
        };
        let matching: Vec<(u32, &RoutePlan)> = (0u32..)
            .zip(&table.routes)
            .filter(|(_, route)| matches(route))
            .collect();
        if matching.is_empty() {
            return Found::NotFound;
        }
        let mut allow = Methods::default();
        for (_, route) in &matching {
            allow.0 |= Methods::bit(route.method);
        }
        let method = match method {
            "HEAD" => Some(HttpMethod::Get),
            other => HttpMethod::from_word(other),
        };
        let route = method.and_then(|method| {
            matching
                .iter()
                .filter(|(_, route)| route.method == method)
                .max_by_key(|(_, route)| mask(route))
                .map(|(number, _)| *number)
        });
        Found::Route { route, allow }
    }

    #[test]
    fn every_path_finds_what_searching_the_table_found() {
        let table = table();
        let routes = Routes::new(&table, &[]);
        let paths = [
            "/",
            "/todos",
            "/todos/",
            "//todos",
            "/todos/archived",
            "/todos/arch%69ved",
            "/todos/7",
            "/todos/7/more",
            "/a/b",
            "/a/c",
            "/c/b",
            "/a",
            "/b",
            "/x/y/z",
            "/x/q/z",
            "/x/y/q",
            "/nothing",
            "/todos/%E2%9C%93",
            "/a%2Fb",
        ];
        for method in [
            "GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE",
        ] {
            for path in paths {
                assert_eq!(
                    routes.find(method, path),
                    searched(&table, method, path),
                    "{method} {path}"
                );
            }
        }
    }

    #[test]
    fn a_fixed_segment_beats_a_placeholder_the_earliest_first() {
        let table = table();
        let routes = Routes::new(&table, &[]);
        let label = |method, path| match routes.find(method, path) {
            Found::Route {
                route: Some(route), ..
            } => routes.label(route).map(|label| label.to_string()),
            _ => None,
        };
        assert_eq!(
            label("GET", "/todos/archived").as_deref(),
            Some("GET /todos/archived")
        );
        assert_eq!(
            label("GET", "/todos/7").as_deref(),
            Some("GET /todos/{todo_id}")
        );
        assert_eq!(label("GET", "/a/b").as_deref(), Some("GET /a/{b}"));
        assert_eq!(label("GET", "/c/b").as_deref(), Some("GET /{a}/b"));
        assert_eq!(label("HEAD", "/").as_deref(), Some("GET /"));
        // Every route the path matches answers `Allow`.
        let Found::Route { route, allow } = routes.find("DELETE", "/todos/archived") else {
            panic!("no route");
        };
        assert_eq!(
            routes.label(route.unwrap()).as_deref(),
            Some("DELETE /todos/{todo_id}")
        );
        assert_eq!(allow.allow(), "DELETE, GET, HEAD, OPTIONS");
        let Found::Route { route, allow } = routes.find("POST", "/a/b") else {
            panic!("no route");
        };
        assert_eq!(route, None);
        assert_eq!(allow.allow(), "GET, HEAD, OPTIONS, PUT");
        assert_eq!(routes.find("GET", "/nothing"), Found::NotFound);
    }

    #[test]
    fn a_segment_that_isnt_text_is_unreadable() {
        let routes = Routes::new(&table(), &[]);
        for path in [
            "/bad/%zz",
            "/todos/%ff",
            "/todos/%E2%9C",
            "/todos/%",
            "/todos/a%2",
        ] {
            assert_eq!(routes.find("GET", path), Found::Unreadable, "{path}");
        }
        // A character cut off where the bytes are checked, and one that isn't.
        let long = format!("/todos/{}%E2%9C%93", "a".repeat(62));
        assert!(matches!(routes.find("GET", &long), Found::Route { .. }));
        let cut = format!("/todos/{}%E2%9C", "a".repeat(62));
        assert_eq!(routes.find("GET", &cut), Found::Unreadable);
    }

    #[test]
    fn health_and_webhooks_are_answered_whatever_the_method() {
        let routes = Routes::new(&table(), &[webhook(&["webhooks", "payments"])]);
        assert_eq!(
            routes.find("GET", "/health/live"),
            Found::Health { ready: false }
        );
        assert_eq!(
            routes.find("DELETE", "/health/ready"),
            Found::Health { ready: true }
        );
        assert_eq!(routes.find("GET", "/health"), Found::NotFound);
        // A webhook's path beats a route's placeholder there.
        assert_eq!(routes.find("POST", "/webhooks/payments"), Found::Webhook(0));
        assert_eq!(
            routes.find("GET", "/webhooks/pay%6Dents"),
            Found::Webhook(0)
        );
        assert!(matches!(
            routes.find("GET", "/webhooks/other"),
            Found::Route { route: Some(9), .. }
        ));
        assert_eq!(
            routes.webhook_label(0).as_deref(),
            Some("POST /webhooks/payments")
        );
        assert_eq!(&*routes.health_label(true), "GET /health/ready");
    }
}
