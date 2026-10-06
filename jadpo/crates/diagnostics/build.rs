use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let crates = manifest.parent().expect("crates directory");
    let mut codes = BTreeSet::new();
    visit_emitters(crates, &manifest, &mut codes);
    let mut values = String::new();
    for code in &codes {
        writeln!(values, "    {code:?},").expect("write generated catalogue entry");
    }
    let repository = manifest
        .ancestors()
        .nth(3)
        .expect("diagnostics crate should be inside the repository");
    let fixture_root = repository.join("tests/compile");
    println!("cargo:rerun-if-changed={}", fixture_root.display());
    let mut fixtures = BTreeSet::new();
    visit_fixtures(repository, &fixture_root, &mut fixtures);
    visit_rust_test_fixtures(repository, crates, &mut fixtures);
    let mut fixture_values = String::new();
    for (code, fixture) in fixtures {
        writeln!(fixture_values, "    ({code:?}, {fixture:?}),")
            .expect("write generated catalogue fixture entry");
    }
    let output = format!(
        "pub const CATALOGUE_CODES: &[&str] = &[\n{values}];\npub const CATALOGUE_FIXTURES: &[(&str, &str)] = &[\n{fixture_values}];\n"
    );
    let destination =
        PathBuf::from(env::var("OUT_DIR").expect("output directory")).join("catalogue_codes.rs");
    fs::write(destination, output).expect("write generated diagnostic catalogue code list");
}

fn visit_rust_test_fixtures(
    repository: &Path,
    path: &Path,
    fixtures: &mut BTreeSet<(String, String)>,
) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut entries = entries
        .map(|entry| entry.expect("test evidence directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            if is_build_directory(&entry) || entry.ends_with("crates/diagnostics") {
                continue;
            }
            visit_rust_test_fixtures(repository, &entry, fixtures);
            continue;
        }
        if !entry.extension().is_some_and(|extension| extension == "rs")
            || entry.ends_with("diagnostics/src/lib.rs")
        {
            continue;
        }
        println!("cargo:rerun-if-changed={}", entry.display());
        let source = fs::read_to_string(&entry)
            .unwrap_or_else(|error| panic!("read {}: {error}", entry.display()));
        let path = entry
            .strip_prefix(repository)
            .expect("test within repository")
            .to_string_lossy()
            .replace('\\', "/");
        for (code, name) in test_references(&source) {
            fixtures.insert((code, format!("{path}#{name}")));
        }
    }
}

fn visit_fixtures(repository: &Path, path: &Path, fixtures: &mut BTreeSet<(String, String)>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut entries = entries
        .map(|entry| entry.expect("fixture directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            visit_fixtures(repository, &entry, fixtures);
            continue;
        }
        if !entry.to_string_lossy().ends_with(".expect.json") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", entry.display());
        let source = fs::read_to_string(&entry)
            .unwrap_or_else(|error| panic!("read {}: {error}", entry.display()));
        let fixture = entry
            .strip_prefix(repository)
            .expect("fixture should be beneath repository")
            .to_string_lossy()
            .replace('\\', "/");
        for code in fixture_codes(&source) {
            fixtures.insert((code, fixture.clone()));
        }
    }
}

fn visit_emitters(path: &Path, diagnostics_crate: &Path, codes: &mut BTreeSet<String>) {
    let mut entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            if entry == diagnostics_crate
                || is_build_directory(&entry)
                || entry.file_name().is_some_and(|name| name == "tests")
            {
                continue;
            }
            visit_emitters(&entry, diagnostics_crate, codes);
        } else if entry.extension().is_some_and(|extension| extension == "rs") {
            println!("cargo:rerun-if-changed={}", entry.display());
            let source = fs::read_to_string(&entry)
                .unwrap_or_else(|error| panic!("read {}: {error}", entry.display()));
            codes.extend(emitted_codes(&source));
        }
    }
}

fn is_diagnostic_code(value: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "CLI_", "CONFIG_", "POLICY_", "TEST_", "DATA_", "EFFECT_", "FAIL_", "FMT_", "INDEX_",
        "JADPO_", "LSP_", "MIG_", "MOD_", "ROUTE_", "RUNTIME_", "SEM_", "SYN_", "TYPE_",
    ];
    value != "JADPO_DEBUG_TARGET_STACKS"
        && value != "JADPO_ASCII"
        && value != "JADPO_DEPLOYMENT_PORT"
        && PREFIXES.iter().any(|prefix| {
            value
                .strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty())
        })
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

// A compiler source module named `target` is not Cargo's build directory.
pub fn is_build_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "target")
        && path
            .parent()
            .is_some_and(|parent| parent.join("Cargo.toml").is_file())
}

struct Codes {
    values: BTreeSet<String>,
    production: bool,
}
impl Codes {
    fn tokens(&mut self, tokens: proc_macro2::TokenStream) {
        for token in tokens {
            match token {
                proc_macro2::TokenTree::Group(group) => self.tokens(group.stream()),
                proc_macro2::TokenTree::Literal(literal) => {
                    if let Ok(value) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                        self.string(&value);
                    }
                }
                _ => {}
            }
        }
    }
    fn string(&mut self, value: &syn::LitStr) {
        let value = value.value();
        if is_diagnostic_code(&value) {
            self.values.insert(value);
        }
    }
}
// Evaluate only what is known when compiling production code (`test = false`).
// Other cfg flags remain unknown, so any(test, unix) must still be inventoried.
fn production_cfg(meta: &syn::Meta) -> Option<bool> {
    use syn::parse::Parser;
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Some(false),
        syn::Meta::List(list) => {
            let nested = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .ok()?;
            let values = nested.iter().map(production_cfg).collect::<Vec<_>>();
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|value| *value == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|value| *value == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|value| !value)
            } else {
                None
            }
        }
        _ => None,
    }
}
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Meta>()
                    .ok()
                    .and_then(|meta| production_cfg(&meta))
                    == Some(false))
    })
}
impl<'ast> syn::visit::Visit<'ast> for Codes {
    fn visit_lit_str(&mut self, value: &'ast syn::LitStr) {
        self.string(value);
    }
    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        self.tokens(value.tokens.clone());
    }
    fn visit_attribute(&mut self, _: &'ast syn::Attribute) {}
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        // Nested helper declarations are not executed just because a test exists.
        if self.production && !test_only(&item.attrs) {
            syn::visit::visit_item_fn(self, item);
        }
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if !self.production || !test_only(&item.attrs) {
            syn::visit::visit_item_mod(self, item);
        }
    }
}
pub fn emitted_codes(source: &str) -> BTreeSet<String> {
    use syn::visit::Visit;
    let file = syn::parse_file(source).expect("parse Rust diagnostic source");
    let mut codes = Codes {
        values: BTreeSet::new(),
        production: true,
    };
    codes.visit_file(&file);
    codes.values
}

// References identify a bounded test body, not proof of assertion execution or
// diagnostic reachability. Those still require executing and reviewing tests.
pub fn test_references(source: &str) -> BTreeSet<(String, String)> {
    use syn::visit::Visit;
    struct Tests {
        values: BTreeSet<(String, String)>,
    }
    impl<'ast> Visit<'ast> for Tests {
        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            if item.attrs.iter().any(|attr| attr.path().is_ident("test"))
                && !item.attrs.iter().any(|attr| attr.path().is_ident("ignore"))
            {
                let mut codes = Codes {
                    values: BTreeSet::new(),
                    production: false,
                };
                codes.visit_block(&item.block);
                self.values.extend(
                    codes
                        .values
                        .into_iter()
                        .map(|code| (code, item.sig.ident.to_string())),
                );
            } else {
                syn::visit::visit_item_fn(self, item);
            }
        }
    }
    let mut tests = Tests {
        values: BTreeSet::new(),
    };
    tests.visit_file(&syn::parse_file(source).expect("parse Rust test source"));
    tests.values
}

pub fn fixture_codes(source: &str) -> BTreeSet<String> {
    let fixture: serde_json::Value = serde_json::from_str(source).expect("parse compile fixture");
    fixture
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|diagnostic| diagnostic.get("code").and_then(serde_json::Value::as_str))
        .filter(|code| is_diagnostic_code(code))
        .map(str::to_owned)
        .collect()
}
