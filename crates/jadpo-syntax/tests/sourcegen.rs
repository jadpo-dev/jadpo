//! Generates `SyntaxKind` and the typed AST from `jadpo.ungram`
//! (docs/compiler-runtime.md §2.2) and fails when the committed files in
//! `src/generated/` differ from what the grammar produces. When it fails it
//! has already rewritten them: run it again and commit the result.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ungrammar::{Grammar, Rule};

/// Punctuation: the text as written and the kind's name.
const PUNCTUATION: &[(&str, &str)] = &[
    ("{", "L_BRACE"),
    ("}", "R_BRACE"),
    ("(", "L_PAREN"),
    (")", "R_PAREN"),
    ("[", "L_BRACK"),
    ("]", "R_BRACK"),
    ("<", "L_ANGLE"),
    (">", "R_ANGLE"),
    (",", "COMMA"),
    (":", "COLON"),
    (".", "DOT"),
    ("...", "DOT3"),
    ("=", "EQ"),
    ("==", "EQ2"),
    ("!=", "NEQ"),
    ("<=", "LTEQ"),
    (">=", "GTEQ"),
    ("+", "PLUS"),
    ("-", "MINUS"),
    ("*", "STAR"),
    ("/", "SLASH"),
    ("%", "PERCENT"),
    ("->", "THIN_ARROW"),
    ("=>", "FAT_ARROW"),
    ("?", "QUESTION"),
    ("_", "UNDERSCORE"),
    ("{{", "L_DBRACE"),
    ("}}", "R_DBRACE"),
];

/// Words that can never be names (area 1: control-flow and declaration
/// words are fully reserved).
const KEYWORDS: &[&str] = &[
    "action",
    "and",
    "assert",
    "attempt",
    "constant",
    "decline",
    "else",
    "emit",
    "entity",
    "fails",
    "false",
    "fixture",
    "for",
    "function",
    "if",
    "in",
    "library",
    "match",
    "mut",
    "none",
    "not",
    "or",
    "propagate",
    "public",
    "query",
    "reject",
    "return",
    "routes",
    "schedule",
    "subscriber",
    "true",
    "type",
    "var",
];

/// Words that are keywords only where the grammar expects them and names
/// everywhere else.
const CONTEXTUAL_KEYWORDS: &[&str] = &[
    "asc", "desc", "end", "include", "matches", "nearest", "package", "provide", "resource",
    "rule", "seed", "test", "use",
];

/// Tokens whose text varies.
const LITERALS: &[&str] = &[
    "ident",
    "int_number",
    "decimal_number",
    "unit_number",
    "string",
    "path_segment",
    "template_text",
];

/// `SKIPPED` is text a habit from another language left behind that the
/// parser reads as nothing (a `;` at the end of a line, `await`), so the
/// tree is the one the Jadpo form gives.
const TRIVIA: &[&str] = &["WHITESPACE", "COMMENT", "DOC_COMMENT", "SKIPPED"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn upper_snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

fn lower_snake(name: &str) -> String {
    upper_snake(name).to_lowercase()
}

/// The kind name for a token written in the grammar.
fn token_kind(text: &str) -> String {
    if let Some((_, name)) = PUNCTUATION.iter().find(|(t, _)| *t == text) {
        return (*name).to_string();
    }
    if KEYWORDS.contains(&text) || CONTEXTUAL_KEYWORDS.contains(&text) {
        return format!("{}_KW", text.to_uppercase());
    }
    if LITERALS.contains(&text) {
        return text.to_uppercase();
    }
    panic!("token `{text}` is not in sourcegen's token table");
}

/// The name of a token's accessor.
fn token_accessor(text: &str) -> String {
    let kind = token_kind(text).to_lowercase();
    format!("{kind}_token")
}

#[derive(Debug)]
enum Field {
    Token {
        name: String,
        kinds: Vec<String>,
    },
    Node {
        name: String,
        ty: String,
        many: bool,
        nth: Option<usize>,
        /// An earlier field whose type could also be read as this one
        /// (`FieldExpr = Expr '.' NameRef`, where a `NameRef` is an `Expr`):
        /// this field is the first child of its type after that one.
        after: Option<String>,
    },
}

#[derive(Debug)]
enum NodeDef {
    Struct { name: String, fields: Vec<Field> },
    Enum { name: String, variants: Vec<String> },
}

struct Collector<'g> {
    grammar: &'g Grammar,
    /// (label, kind, many) in order.
    items: Vec<(Option<String>, Item)>,
}

#[derive(Debug, Clone)]
enum Item {
    Token(String),
    Tokens(Vec<String>),
    Node(String, bool),
}

impl Collector<'_> {
    fn walk(&mut self, rule: &Rule, label: Option<&str>, many: bool) {
        match rule {
            Rule::Labeled { label, rule } => self.walk(rule, Some(label), many),
            Rule::Node(node) => {
                let name = self.grammar[*node].name.clone();
                self.items
                    .push((label.map(str::to_string), Item::Node(name, many)));
            }
            Rule::Token(token) => {
                let text = self.grammar[*token].name.clone();
                if !many {
                    self.items
                        .push((label.map(str::to_string), Item::Token(text)));
                }
            }
            Rule::Seq(rules) => {
                for rule in rules {
                    self.walk(rule, None, many);
                }
            }
            Rule::Alt(rules) => {
                let tokens: Option<Vec<String>> = rules
                    .iter()
                    .map(|rule| match rule {
                        Rule::Token(token) => Some(self.grammar[*token].name.clone()),
                        _ => None,
                    })
                    .collect();
                match (tokens, label) {
                    (Some(tokens), Some(label)) => {
                        self.items
                            .push((Some(label.to_string()), Item::Tokens(tokens)));
                    }
                    _ => {
                        for rule in rules {
                            self.walk(rule, None, many);
                        }
                    }
                }
            }
            Rule::Opt(rule) => self.walk(rule, label, many),
            Rule::Rep(rule) => self.walk(rule, label, true),
        }
    }
}

fn is_enum(grammar: &Grammar, rule: &Rule) -> Option<Vec<String>> {
    match rule {
        Rule::Alt(rules) => rules
            .iter()
            .map(|rule| match rule {
                Rule::Node(node) => Some(grammar[*node].name.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    }
}

/// Whether a node of type `ty` can be read as `wider`: the same type, or a
/// variant of the enum `wider`, directly or through another enum.
fn covers(grammar: &Grammar, wider: &str, ty: &str) -> bool {
    if wider == ty {
        return true;
    }
    let Some(node) = grammar.iter().find(|node| grammar[*node].name == wider) else {
        return false;
    };
    is_enum(grammar, &grammar[node].rule)
        .is_some_and(|variants| variants.iter().any(|variant| covers(grammar, variant, ty)))
}

fn lower(grammar: &Grammar) -> Vec<NodeDef> {
    let mut defs = Vec::new();
    for node in grammar.iter() {
        let data = &grammar[node];
        if let Some(variants) = is_enum(grammar, &data.rule) {
            defs.push(NodeDef::Enum {
                name: data.name.clone(),
                variants,
            });
            continue;
        }
        let mut collector = Collector {
            grammar,
            items: Vec::new(),
        };
        collector.walk(&data.rule, None, false);
        // A node type that appears more than once without labels is a list.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for (label, item) in &collector.items {
            if let (None, Item::Node(ty, _)) = (label, item) {
                *counts.entry(ty.clone()).or_default() += 1;
            }
        }
        let mut fields = Vec::new();
        let mut earlier: Vec<String> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut nth: BTreeMap<String, usize> = BTreeMap::new();
        let labelled_types: BTreeMap<String, usize> =
            collector
                .items
                .iter()
                .fold(BTreeMap::new(), |mut acc, (label, item)| {
                    if let (Some(_), Item::Node(ty, _)) = (label, item) {
                        *acc.entry(ty.clone()).or_default() += 1;
                    }
                    acc
                });
        for (label, item) in collector.items {
            match item {
                Item::Token(text) => {
                    let name = label.unwrap_or_else(|| token_accessor(&text));
                    if seen.insert(name.clone()) {
                        fields.push(Field::Token {
                            name,
                            kinds: vec![token_kind(&text)],
                        });
                    }
                }
                Item::Tokens(texts) => {
                    let name = label.expect("token alternatives are labelled");
                    if seen.insert(name.clone()) {
                        fields.push(Field::Token {
                            name,
                            kinds: texts.iter().map(|t| token_kind(t)).collect(),
                        });
                    }
                }
                Item::Node(ty, many) => {
                    let duplicated = labelled_types.get(&ty).copied().unwrap_or(0) > 1;
                    let many = many || (label.is_none() && counts[&ty] > 1);
                    let name = match &label {
                        Some(label) => label.clone(),
                        None if many => format!("{}s", lower_snake(&ty)),
                        None if ty == "Type" => "ty".to_string(),
                        None => lower_snake(&ty),
                    };
                    let index = if duplicated {
                        let next = nth.entry(ty.clone()).or_default();
                        let index = *next;
                        *next += 1;
                        Some(index)
                    } else {
                        None
                    };
                    let after = if many || index.is_some() {
                        None
                    } else {
                        earlier
                            .iter()
                            .find(|wider| *wider != &ty && covers(grammar, wider, &ty))
                            .cloned()
                    };
                    earlier.push(ty.clone());
                    if seen.insert(name.clone()) {
                        fields.push(Field::Node {
                            name,
                            ty,
                            many,
                            nth: index,
                            after,
                        });
                    }
                }
            }
        }
        defs.push(NodeDef::Struct {
            name: data.name.clone(),
            fields,
        });
    }
    defs
}

fn generate_kinds(defs: &[NodeDef]) -> String {
    let mut out = String::new();
    out.push_str(
        "//! Generated by `cargo test -p jadpo-syntax --test integration sourcegen::` from `jadpo.ungram`. Do not edit.\n\n",
    );
    out.push_str("#![allow(bad_style, missing_docs, unreachable_pub, clippy::all)]\n\n");
    let mut kinds: Vec<String> = vec!["EOF".into()];
    kinds.extend(PUNCTUATION.iter().map(|(_, name)| (*name).to_string()));
    kinds.extend(
        KEYWORDS
            .iter()
            .map(|kw| format!("{}_KW", kw.to_uppercase())),
    );
    kinds.extend(
        CONTEXTUAL_KEYWORDS
            .iter()
            .map(|kw| format!("{}_KW", kw.to_uppercase())),
    );
    kinds.extend(LITERALS.iter().map(|lit| lit.to_uppercase()));
    kinds.extend(TRIVIA.iter().map(|t| (*t).to_string()));
    kinds.push("ERROR".into());
    for def in defs {
        if let NodeDef::Struct { name, .. } = def {
            kinds.push(upper_snake(name));
        }
    }
    out.push_str("/// Every token and node kind in a Jadpo syntax tree.\n");
    out.push_str("#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]\n#[repr(u16)]\npub enum SyntaxKind {\n");
    for kind in &kinds {
        let _ = writeln!(out, "    {kind},");
    }
    out.push_str("}\n\n");
    out.push_str("use self::SyntaxKind::*;\n\n");
    let _ = writeln!(out, "const ALL: [SyntaxKind; {}] = [", kinds.len());
    for kind in &kinds {
        let _ = writeln!(out, "    {kind},");
    }
    out.push_str("];\n\n");
    out.push_str("impl SyntaxKind {\n");
    out.push_str("    /// The kind with this raw value; `ERROR` for a value no kind has.\n");
    out.push_str("    pub fn from_raw(raw: u16) -> SyntaxKind {\n        ALL.get(usize::from(raw)).copied().unwrap_or(ERROR)\n    }\n\n");
    out.push_str("    pub fn is_trivia(self) -> bool {\n        matches!(self, WHITESPACE | COMMENT | DOC_COMMENT | SKIPPED)\n    }\n\n");
    out.push_str("    /// Reserved words, which can never be names.\n    pub fn is_keyword(self) -> bool {\n        matches!(self, ");
    out.push_str(
        &KEYWORDS
            .iter()
            .map(|kw| format!("{}_KW", kw.to_uppercase()))
            .collect::<Vec<_>>()
            .join(" | "),
    );
    out.push_str(")\n    }\n\n");
    out.push_str("    pub fn is_punctuation(self) -> bool {\n        matches!(self, ");
    out.push_str(
        &PUNCTUATION
            .iter()
            .map(|(_, name)| (*name).to_string())
            .collect::<Vec<_>>()
            .join(" | "),
    );
    out.push_str(")\n    }\n\n");
    out.push_str("    /// The reserved word spelt `text`.\n    pub fn from_keyword(text: &str) -> Option<SyntaxKind> {\n        let kind = match text {\n");
    for kw in KEYWORDS {
        let _ = writeln!(out, "            \"{kw}\" => {}_KW,", kw.to_uppercase());
    }
    out.push_str("            _ => return None,\n        };\n        Some(kind)\n    }\n\n");
    out.push_str("    /// The contextual keyword spelt `text`.\n    pub fn from_contextual_keyword(text: &str) -> Option<SyntaxKind> {\n        let kind = match text {\n");
    for kw in CONTEXTUAL_KEYWORDS {
        let _ = writeln!(out, "            \"{kw}\" => {}_KW,", kw.to_uppercase());
    }
    out.push_str("            _ => return None,\n        };\n        Some(kind)\n    }\n\n");
    out.push_str("    /// The fixed text of a punctuation or keyword kind.\n    pub fn text(self) -> Option<&'static str> {\n        let text = match self {\n");
    for (text, name) in PUNCTUATION {
        let _ = writeln!(out, "            {name} => \"{text}\",");
    }
    for kw in KEYWORDS.iter().chain(CONTEXTUAL_KEYWORDS) {
        let _ = writeln!(out, "            {}_KW => \"{kw}\",", kw.to_uppercase());
    }
    out.push_str("            _ => return None,\n        };\n        Some(text)\n    }\n}\n\n");
    out.push_str("/// The kind of a token, by its text: `T![\"{\"]`, `T![type]`.\n#[macro_export]\nmacro_rules! T {\n");
    for (text, name) in PUNCTUATION {
        let _ = writeln!(out, "    [{:?}] => {{ $crate::SyntaxKind::{name} }};", text);
    }
    for kw in KEYWORDS.iter().chain(CONTEXTUAL_KEYWORDS) {
        let _ = writeln!(
            out,
            "    [{kw}] => {{ $crate::SyntaxKind::{}_KW }};",
            kw.to_uppercase()
        );
    }
    for lit in LITERALS {
        let _ = writeln!(
            out,
            "    [{lit}] => {{ $crate::SyntaxKind::{} }};",
            lit.to_uppercase()
        );
    }
    out.push_str("}\n");
    out
}

fn generate_nodes(defs: &[NodeDef]) -> String {
    let enums: BTreeMap<&str, &Vec<String>> = defs
        .iter()
        .filter_map(|def| match def {
            NodeDef::Enum { name, variants } => Some((name.as_str(), variants)),
            NodeDef::Struct { .. } => None,
        })
        .collect();
    let mut out = String::new();
    out.push_str(
        "//! Generated by `cargo test -p jadpo-syntax --test integration sourcegen::` from `jadpo.ungram`. Do not edit.\n\n",
    );
    out.push_str("#![allow(clippy::all)]\n\n");
    out.push_str("use crate::SyntaxKind::{self, *};\nuse crate::ast::{AstChildren, AstNode, support};\nuse crate::{SyntaxNode, SyntaxToken};\n\n");
    for def in defs {
        match def {
            NodeDef::Struct { name, fields } => {
                let kind = upper_snake(name);
                let _ = writeln!(
                    out,
                    "#[derive(Debug, Clone, PartialEq, Eq, Hash)]\npub struct {name} {{\n    pub(crate) syntax: SyntaxNode,\n}}\n"
                );
                let _ = writeln!(
                    out,
                    "impl AstNode for {name} {{\n    fn can_cast(kind: SyntaxKind) -> bool {{\n        kind == {kind}\n    }}\n    fn cast(syntax: SyntaxNode) -> Option<Self> {{\n        if Self::can_cast(syntax.kind()) {{ Some(Self {{ syntax }}) }} else {{ None }}\n    }}\n    fn syntax(&self) -> &SyntaxNode {{\n        &self.syntax\n    }}\n}}\n"
                );
                if fields.is_empty() {
                    continue;
                }
                let _ = writeln!(out, "impl {name} {{");
                for field in fields {
                    match field {
                        Field::Token { name, kinds } if kinds.len() == 1 => {
                            let _ = writeln!(
                                out,
                                "    pub fn {name}(&self) -> Option<SyntaxToken> {{\n        support::token(&self.syntax, {})\n    }}",
                                kinds[0]
                            );
                        }
                        Field::Token { name, kinds } => {
                            let _ = writeln!(
                                out,
                                "    pub fn {name}(&self) -> Option<SyntaxToken> {{\n        support::token_in(&self.syntax, &[{}])\n    }}",
                                kinds.join(", ")
                            );
                        }
                        Field::Node {
                            name,
                            ty,
                            many: true,
                            ..
                        } => {
                            let _ = writeln!(
                                out,
                                "    pub fn {name}(&self) -> AstChildren<{ty}> {{\n        support::children(&self.syntax)\n    }}"
                            );
                        }
                        Field::Node {
                            name,
                            ty,
                            nth: Some(n),
                            ..
                        } => {
                            let _ = writeln!(
                                out,
                                "    pub fn {name}(&self) -> Option<{ty}> {{\n        support::children(&self.syntax).nth({n})\n    }}"
                            );
                        }
                        Field::Node {
                            name,
                            ty,
                            after: Some(wider),
                            ..
                        } => {
                            let _ = writeln!(
                                out,
                                "    pub fn {name}(&self) -> Option<{ty}> {{\n        support::child_after(&self.syntax, {wider}::can_cast)\n    }}"
                            );
                        }
                        Field::Node { name, ty, .. } => {
                            let _ = writeln!(
                                out,
                                "    pub fn {name}(&self) -> Option<{ty}> {{\n        support::child(&self.syntax)\n    }}"
                            );
                        }
                    }
                }
                out.push_str("}\n\n");
            }
            NodeDef::Enum { name, variants } => {
                let _ = writeln!(
                    out,
                    "#[derive(Debug, Clone, PartialEq, Eq, Hash)]\npub enum {name} {{"
                );
                for variant in variants {
                    let _ = writeln!(out, "    {variant}({variant}),");
                }
                out.push_str("}\n\n");
                let can_cast: Vec<String> = variants
                    .iter()
                    .map(|variant| {
                        if enums.contains_key(variant.as_str()) {
                            format!("{variant}::can_cast(kind)")
                        } else {
                            format!("kind == {}", upper_snake(variant))
                        }
                    })
                    .collect();
                let _ = writeln!(
                    out,
                    "impl AstNode for {name} {{\n    fn can_cast(kind: SyntaxKind) -> bool {{\n        {}\n    }}",
                    can_cast.join(" || ")
                );
                out.push_str(
                    "    fn cast(syntax: SyntaxNode) -> Option<Self> {\n        let kind = syntax.kind();\n",
                );
                for variant in variants {
                    if enums.contains_key(variant.as_str()) {
                        let _ = writeln!(
                            out,
                            "        if {variant}::can_cast(kind) {{\n            return {variant}::cast(syntax).map(Self::{variant});\n        }}"
                        );
                    } else {
                        let _ = writeln!(
                            out,
                            "        if kind == {} {{\n            return Some(Self::{variant}({variant} {{ syntax }}));\n        }}",
                            upper_snake(variant)
                        );
                    }
                }
                out.push_str("        None\n    }\n    fn syntax(&self) -> &SyntaxNode {\n        match self {\n");
                for variant in variants {
                    let _ = writeln!(out, "            Self::{variant}(it) => it.syntax(),");
                }
                out.push_str("        }\n    }\n}\n\n");
                for variant in variants {
                    let _ = writeln!(
                        out,
                        "impl From<{variant}> for {name} {{\n    fn from(node: {variant}) -> {name} {{\n        {name}::{variant}(node)\n    }}\n}}\n"
                    );
                }
            }
        }
    }
    out
}

fn rustfmt(text: &str) -> String {
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rustfmt runs (rust-toolchain.toml installs it)");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        stdin.write_all(text.as_bytes()).expect("write to rustfmt");
    }
    let output = child.wait_with_output().expect("rustfmt finishes");
    assert!(
        output.status.success(),
        "rustfmt failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("UTF-8")
}

/// Writes `text` to `path` if it differs, and says so.
fn ensure(path: &Path, text: &str) -> bool {
    let current = std::fs::read_to_string(path).unwrap_or_default();
    if current == text {
        return true;
    }
    std::fs::write(path, text).expect("write generated file");
    false
}

#[test]
fn generated_ast_matches_the_grammar() {
    let text = std::fs::read_to_string(crate_dir().join("jadpo.ungram")).expect("grammar");
    let grammar: Grammar = text.parse().expect("jadpo.ungram parses");
    let defs = lower(&grammar);
    let kinds = rustfmt(&generate_kinds(&defs));
    let nodes = rustfmt(&generate_nodes(&defs));
    let generated = crate_dir().join("src/generated");
    let kinds_same = ensure(&generated.join("kinds.rs"), &kinds);
    let nodes_same = ensure(&generated.join("nodes.rs"), &nodes);
    assert!(
        kinds_same && nodes_same,
        "src/generated/ was stale and has been rewritten from jadpo.ungram: rerun the tests and commit it"
    );
}
