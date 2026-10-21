//! The code generator (docs/compiler-runtime.md §2.1 and §2.5, "Compiled
//! code"; roadmap chunk 8.1f): a checked project written as one Rust crate
//! from its plan, for compiled deployables and the differential run.
//!
//! [`generate`] builds each file with `quote!`, parses it into `syn`'s
//! syntax tree, so output that doesn't parse is the generator's error and
//! never rustc's, and prints it with prettyplease. The crate is:
//!
//! - `Cargo.toml`: a library depending on the runtime crates and the
//!   shared leaf only, by path, with a `testing` feature turning on
//!   theirs;
//! - `plan.bin` and `plan-testing.bin`: the runtime plan's canonical bytes
//!   without and with its dev and test section, which the crate holds as
//!   a constant and decodes once as it starts;
//! - `src/lib.rs`: the frame table, each callable by number, each file
//!   write's row by its callable and site, and `program()`, the compiled
//!   program on this thread;
//! - `src/<module>.rs`: a module per domain, and one per top folder and
//!   `app` for the project's root, holding each callable's function: a
//!   plain `fn` for a function, an `async fn` for an action, fixture, test
//!   or seed, all over the runtime's values (`jadpo_value::Value`), as the
//!   interpreter's are.
//!
//! A body form not compiled yet is refused by name and place
//! ([`Refusal`]), every one of them, rather than written as code that
//! faults when a test reaches it. [`cargo`] builds what it writes.

mod body;
pub mod cargo;
mod names;

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use jadpo_plan::Plan;
use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::body::{Emitter, Shape, number, testing_only};

/// What the crate is called and where its dependencies are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The package's name.
    pub name: String,
    /// The folder holding `jadpo-std`, `jadpo-value` and `jadpo-runtime`,
    /// as the crate's manifest names it: relative to the crate's own
    /// folder, or absolute.
    pub runtime: String,
}

/// The crate's files, by path from its folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Generated {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Generated {
    /// Writes the crate into `folder`, each file only when it differs from
    /// what is there, so cargo builds again only what changed.
    pub fn write(&self, folder: &Path) -> std::io::Result<()> {
        for (path, bytes) in &self.files {
            let target = folder.join(path);
            if std::fs::read(&target).is_ok_and(|there| there == *bytes) {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, bytes)?;
        }
        Ok(())
    }
}

/// A body form the generator doesn't compile yet, where it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// The form, as written: `` `for` ``, `` `list.count` ``.
    pub form: String,
    pub file: String,
    pub line: u32,
    /// The callable it is in, as a frame names it.
    pub callable: String,
}

impl fmt::Display for Refusal {
    /// `` `for` isn't compiled yet: main.jadpo:3, in function total ``.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} isn't compiled yet: {}:{}, in {}",
            self.form, self.file, self.line, self.callable
        )
    }
}

/// Why no crate was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Body forms not compiled yet, in callable and site order.
    Refused(Vec<Refusal>),
    /// The plan had no canonical bytes, which no plan lacks.
    Plan(String),
    /// A file the generator wrote didn't parse as Rust: its own mistake.
    Unparsed { file: String, error: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Refused(refusals) => {
                for (at, refusal) in refusals.iter().enumerate() {
                    if at > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{refusal}")?;
                }
                Ok(())
            }
            Error::Plan(error) => write!(f, "the plan has no canonical bytes: {error}"),
            Error::Unparsed { file, error } => {
                write!(f, "the generated {file} isn't Rust: {error}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// The crate `plan`, a checked project's, compiles to.
pub fn generate(plan: &Plan, options: &Options) -> Result<Generated, Error> {
    let callables = plan.callables();
    let numbers = 0..u32::try_from(callables.len()).unwrap_or(u32::MAX);
    // Each callable's module, and its function as another module names it.
    let modules: Vec<proc_macro2::Ident> = callables
        .iter()
        .map(|callable| quote::format_ident!("{}", names::module(&callable.file)))
        .collect();
    let paths: Vec<TokenStream> = numbers
        .clone()
        .zip(callables)
        .zip(&modules)
        .map(|((number, callable), module)| {
            let function = names::function(number, callable);
            quote!(crate::#module::#function)
        })
        .collect();
    let mut refusals = Vec::new();
    let mut code: BTreeMap<String, Vec<TokenStream>> = BTreeMap::new();
    let mut table = Vec::new();
    for ((number, callable), module) in numbers.zip(callables).zip(&modules) {
        let shape = Shape::of(callable.kind);
        let function = names::function(number, callable);
        let tokens = Emitter {
            plan,
            callable,
            number,
            shape,
            paths: &paths,
            refusals: &mut refusals,
            label: None,
            recording: false,
            handling: Vec::new(),
            inline: None,
        }
        .callable(number, &function);
        code.entry(module.to_string()).or_default().push(tokens);
        let entry = names::entry(number);
        let variant = match shape {
            Shape::Function => quote!(Function),
            Shape::Action => quote!(Action),
            Shape::Body => quote!(Body),
        };
        let cfg = if testing_only(callable.kind) {
            quote!(#[cfg(feature = "testing")])
        } else {
            TokenStream::new()
        };
        let n = number_literal(number);
        table.push(quote!(#cfg #n => Some(Callable::#variant(#module::#entry)),));
    }
    // Each file write's row, a function of its own in its callable's
    // module, by the callable and the site of its body (chunk 6.4b2).
    let mut rows = Vec::new();
    let mut written = std::collections::BTreeSet::new();
    for batch in plan.batches() {
        let jadpo_std::batch::BatchEach::Row(row) = &batch.each else {
            continue;
        };
        let (Some(callable), Some(module)) = (
            callables.get(row.callable as usize),
            modules.get(row.callable as usize),
        ) else {
            continue;
        };
        if !written.insert((row.callable, row.body)) {
            continue;
        }
        let name = quote::format_ident!("row{}_{}", row.callable, row.body);
        let tokens = Emitter {
            plan,
            callable,
            number: row.callable,
            shape: Shape::Function,
            paths: &paths,
            refusals: &mut refusals,
            label: None,
            recording: false,
            handling: Vec::new(),
            inline: None,
        }
        .row(row, &name);
        code.entry(module.to_string()).or_default().push(tokens);
        let cfg = if testing_only(callable.kind) {
            quote!(#[cfg(feature = "testing")])
        } else {
            TokenStream::new()
        };
        let (at, body) = (number(row.callable), number(row.body));
        rows.push(quote!(#cfg (#at, #body) => Some(#module::#name),));
    }
    if !refusals.is_empty() {
        return Err(Error::Refused(refusals));
    }
    let mut files = BTreeMap::new();
    files.insert("Cargo.toml".to_string(), manifest(options).into_bytes());
    let runtime = plan.runtime();
    files.insert(
        "plan.bin".to_string(),
        runtime
            .encode_deployable()
            .map_err(|error| Error::Plan(error.to_string()))?,
    );
    files.insert(
        "plan-testing.bin".to_string(),
        runtime
            .encode()
            .map_err(|error| Error::Plan(error.to_string()))?,
    );
    let module_names: Vec<proc_macro2::Ident> = code
        .keys()
        .map(|module| quote::format_ident!("{}", module))
        .collect();
    files.insert(
        "src/lib.rs".to_string(),
        printed("src/lib.rs", root(plan, &module_names, &table, &rows))?,
    );
    for (module, functions) in code {
        let path = format!("src/{module}.rs");
        let doc = Literal::string(&format!(
            " The callables of `{module}`, compiled. Generated by Jadpo; don't edit it."
        ));
        let tokens = quote! {
            #![doc = #doc]

            use jadpo_runtime::compiled::{self as rt, Args, Program, Ran, Stop, Truth};
            use jadpo_runtime::operators::Operator;
            use jadpo_runtime::{Ctx, LocalFuture};
            use jadpo_value::{Rounding, Value};

            #(#functions)*
        };
        files.insert(path.clone(), printed(&path, tokens)?);
    }
    Ok(Generated { files })
}

fn number_literal(n: u32) -> Literal {
    number(n)
}

/// `tokens` parsed as a Rust file and printed.
fn printed(file: &str, tokens: TokenStream) -> Result<Vec<u8>, Error> {
    let parsed: syn::File = syn::parse2(tokens).map_err(|error| Error::Unparsed {
        file: file.to_string(),
        error: error.to_string(),
    })?;
    Ok(prettyplease::unparse(&parsed).into_bytes())
}

/// The crate's root: its modules, the plan's bytes, the frame table, each
/// callable by number, each file write's row and the program.
fn root(
    plan: &Plan,
    modules: &[proc_macro2::Ident],
    table: &[TokenStream],
    rows: &[TokenStream],
) -> TokenStream {
    let places = plan.callables().iter().map(|callable| {
        let label = Literal::string(&callable.label);
        let file = Literal::string(&callable.file);
        let line = number(callable.line);
        let sites = callable
            .body
            .sites
            .iter()
            .zip(&callable.sites)
            .map(|(site, decoded)| {
                let (line, start, end, anchor) = (
                    number(site.line),
                    number(site.start),
                    number(site.end),
                    number(decoded.anchor),
                );
                quote!([#line, #start, #end, #anchor])
            });
        quote!(Place { label: #label, file: #file, line: #line, sites: &[#(#sites),*] })
    });
    quote! {
        #![doc = " A Jadpo project compiled (docs/compiler-runtime.md §2.5). Generated by Jadpo; don't edit it."]
        #![allow(
            unused_mut,
            unused_variables,
            unused_assignments,
            unused_imports,
            unreachable_code,
            clippy::all
        )]

        #(pub mod #modules;)*

        use jadpo_runtime::compiled::{Callable, Code, Place, Program, Row, Shared};

        /// The runtime plan's canonical bytes, decoded once as the program
        /// starts; a build with the `testing` feature holds its dev and
        /// test section too.
        #[cfg(not(feature = "testing"))]
        const PLAN: &[u8] = include_bytes!("../plan.bin");
        #[cfg(feature = "testing")]
        const PLAN: &[u8] = include_bytes!("../plan-testing.bin");

        /// Each callable's label, file, line and sites, by number.
        const PLACES: &[Place] = &[#(#places),*];

        static SHARED: Shared = Shared::new();

        static CODE: Code = Code {
            plan: PLAN,
            places: PLACES,
            callable,
            row,
            shared: &SHARED,
        };

        /// Each callable by number.
        fn callable(n: u32) -> Option<Callable> {
            match n {
                #(#table)*
                _ => None,
            }
        }

        /// Each file write's row, by its callable's number and its body's
        /// site.
        fn row(callable: u32, body: u32) -> Option<Row> {
            match (callable, body) {
                #(#rows)*
                _ => None,
            }
        }

        /// The compiled program on this thread; its plan's bytes refused
        /// when they aren't a plan this runtime reads.
        pub fn program() -> Result<Program, jadpo_std::Unreadable> {
            Program::new(&CODE)
        }
    }
}

/// The crate's manifest.
fn manifest(options: &Options) -> String {
    let runtime = options.runtime.trim_end_matches('/');
    let path = |name: &str| format!("{{ path = \"{runtime}/{name}\" }}");
    format!(
        "# Generated by Jadpo from a checked project's plan (docs/compiler-runtime.md §2.5); don't edit it.\n\
         [package]\n\
         name = \"{name}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\
         \n\
         [lib]\n\
         path = \"src/lib.rs\"\n\
         \n\
         [features]\n\
         testing = [\"jadpo-runtime/testing\", \"jadpo-std/testing\"]\n\
         \n\
         [dependencies]\n\
         jadpo-runtime = {runtime_crate}\n\
         jadpo-std = {std_crate}\n\
         jadpo-value = {value_crate}\n",
        name = options.name,
        runtime_crate = path("jadpo-runtime"),
        std_crate = path("jadpo-std"),
        value_crate = path("jadpo-value"),
    )
}
